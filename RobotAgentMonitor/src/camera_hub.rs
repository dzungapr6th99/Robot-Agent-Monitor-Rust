use robot_crud_business::traits::CameraBLTrait;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail, ensure};
use axum::{
    Json,
    extract::{
        Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::Response,
};
use robot_crud_business::camera_bl::{CameraCommand, CameraQuery};
use serde::Deserialize;
use serde_json::{Value, json};

use crate::{
    HttpServer,
    auth::{Claims, bearer},
};

const SEPARATOR: char = '\u{001e}';

#[derive(Deserialize)]
pub(crate) struct ConnectionQuery {
    access_token: Option<String>,
    id: Option<String>,
}

struct PendingConnection {
    peer: String,
    subject: Option<String>,
    expires: Instant,
}

#[derive(Default)]
pub(crate) struct Negotiations(tokio::sync::Mutex<HashMap<String, PendingConnection>>);

pub(crate) async fn upgrade(
    State(app): State<Arc<HttpServer>>,
    Query(query): Query<ConnectionQuery>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    check_origin(&app, &headers)?;
    let claims = app
        .authorize(
            bearer(&headers).or(query.access_token.as_deref()),
            "camera.view",
        )
        .await?;
    let peer = if let Some(token) = query.id {
        let mut pending = app.negotiations.0.lock().await;
        let valid = pending.get(&token).is_some_and(|connection| {
            connection.expires > Instant::now() && connection.subject == claims.sub
        });
        if !valid {
            return Err(StatusCode::NOT_FOUND);
        }
        pending.remove(&token).ok_or(StatusCode::NOT_FOUND)?.peer
    } else {
        uuid::Uuid::new_v4().simple().to_string()
    };
    Ok(ws
        .max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .on_upgrade(move |mut socket| async move {
            if app.camera.connect(peer.clone()).await.is_err() {
                return;
            }
            if let Err(error) = run(&app, &mut socket, &peer, &claims).await {
                robot_common_lib::operation_log::error(
                    module_path!(),
                    format_args!("Camera connection failed: {error:#}"),
                );
            }
            app.camera.disconnect(&peer).await;
        }))
}

pub(crate) async fn negotiate(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
) -> Result<Json<Value>, StatusCode> {
    check_origin(&app, &headers)?;
    let claims = app.authorize(bearer(&headers), "camera.view").await?;
    let now = Instant::now();
    let mut pending = app.negotiations.0.lock().await;
    pending.retain(|_, entry| entry.expires > now);
    if pending.len() >= 128 {
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    let peer = uuid::Uuid::new_v4().simple().to_string();
    let token = uuid::Uuid::new_v4().simple().to_string();
    pending.insert(
        token.clone(),
        PendingConnection {
            peer: peer.clone(),
            subject: claims.sub,
            expires: now + Duration::from_secs(30),
        },
    );
    Ok(Json(json!({
        "negotiateVersion": 1,
        "connectionId": peer,
        "connectionToken": token,
        "availableTransports": [{"transport":"WebSockets", "transferFormats":["Text"]}]
    })))
}

fn check_origin(app: &HttpServer, headers: &HeaderMap) -> Result<(), StatusCode> {
    let mut origins = headers.get_all("origin").iter();
    let Some(origin) = origins.next() else {
        return Ok(());
    };
    if origins.next().is_some() {
        return Err(StatusCode::FORBIDDEN);
    }
    let origin = origin.to_str().map_err(|_| StatusCode::FORBIDDEN)?;
    let same_origin = headers
        .get("host")
        .and_then(|host| host.to_str().ok())
        .is_some_and(|host| origin == format!("https://{host}"));
    if !same_origin && !app.origins.iter().any(|allowed| allowed == origin) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(())
}

async fn run(app: &HttpServer, socket: &mut WebSocket, peer: &str, claims: &Claims) -> Result<()> {
    let frame = tokio::time::timeout(Duration::from_secs(5), socket.recv())
        .await?
        .context("Missing SignalR handshake")??;
    let Message::Text(handshake) = frame else {
        bail!("Invalid handshake");
    };
    let (hello, remaining) = handshake
        .split_once(SEPARATOR)
        .context("Incomplete handshake")?;
    let hello: Value = serde_json::from_str(hello)?;
    if hello["protocol"] != "json" || hello["version"] != 1 {
        send(socket, json!({"error":"Unsupported SignalR protocol"})).await?;
        return Ok(());
    }
    send(socket, json!({})).await?;

    let mut events = app.camera.subscribe();
    let mut shutdown = app.shutdown.clone();
    let mut keepalive = tokio::time::interval(Duration::from_secs(5));
    let mut last_received = tokio::time::Instant::now();
    if !remaining.is_empty() && !receive(app, socket, peer, claims, remaining).await? {
        return Ok(());
    }
    loop {
        if *shutdown.borrow() || claims.expired() {
            break;
        }
        tokio::select! {
            _ = shutdown.changed() => break,
            _ = keepalive.tick() => {
                if last_received.elapsed() > Duration::from_secs(30) { break; }
                send(socket, json!({"type":6})).await?;
            }
            event = events.recv() => {
                if let Ok(stream) = event {
                    send(socket, json!({"type":1, "target":"CameraStreamChanged", "arguments":[stream]})).await?;
                }
            }
            frame = socket.recv() => match frame {
                Some(Ok(Message::Text(text))) => {
                    last_received = tokio::time::Instant::now();
                    if !receive(app, socket, peer, claims, &text).await? { break; }
                }
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => {}
                _ => break,
            }
        }
    }
    send(socket, json!({"type":7})).await
}

async fn send(socket: &mut WebSocket, value: Value) -> Result<()> {
    tokio::time::timeout(
        Duration::from_secs(5),
        socket.send(Message::Text(format!("{value}{SEPARATOR}").into())),
    )
    .await??;
    Ok(())
}

async fn receive(
    app: &HttpServer,
    socket: &mut WebSocket,
    peer: &str,
    claims: &Claims,
    text: &str,
) -> Result<bool> {
    ensure!(text.ends_with(SEPARATOR), "Incomplete SignalR message");
    for record in text.split_terminator(SEPARATOR) {
        if claims.expired() {
            return Ok(false);
        }
        let value: Value = serde_json::from_str(record)?;
        match value["type"].as_u64() {
            Some(6) => continue,
            Some(7) => return Ok(false),
            Some(1) => {}
            _ => bail!("Unsupported SignalR message"),
        }
        let target = value["target"].as_str().unwrap_or("");
        let arguments = value["arguments"]
            .as_array()
            .context("Missing invocation arguments")?;
        let argument = arguments.first().unwrap_or(&Value::Null);
        let result = invoke(app, peer, claims, target, argument).await;
        if let Some(id) = value["invocationId"].as_str() {
            let completion = match result {
                Ok(result) => json!({"type":3, "invocationId":id, "result":result}),
                Err(error) => {
                    eprintln!("Camera invocation {target} failed: {error}");
                    json!({"type":3, "invocationId":id, "error":"Camera invocation failed"})
                }
            };
            send(socket, completion).await?;
        }
    }
    Ok(true)
}

async fn invoke(
    app: &HttpServer,
    peer: &str,
    claims: &Claims,
    target: &str,
    argument: &Value,
) -> Result<Value> {
    let query = match target {
        "GetResolutions" => Some(CameraQuery::Resolutions),
        "GetStreams" => Some(CameraQuery::Streams),
        "GetHealth" => Some(CameraQuery::Health),
        "GetRtpCapabilities" => Some(CameraQuery::RtpCapabilities),
        _ => None,
    };
    if let Some(query) = query {
        return app.camera.query(query).await;
    }
    let camera = || {
        argument
            .as_str()
            .map(str::to_owned)
            .context("Missing camera")
    };
    let command = match target {
        "StartStream" => CameraCommand::Start {
            camera: argument["camera"]
                .as_str()
                .context("Missing camera")?
                .into(),
            resolution: argument["resolution"].as_str().unwrap_or("HD").into(),
        },
        "StopStream" => CameraCommand::Stop(camera()?),
        "CreateTransport" => CameraCommand::CreateTransport,
        "ConnectTransport" => CameraCommand::ConnectTransport(argument["dtlsParameters"].clone()),
        "Consume" => CameraCommand::Consume {
            camera: argument["camera"]
                .as_str()
                .context("Missing camera")?
                .into(),
            capabilities: argument["rtpCapabilities"].clone(),
        },
        "Resume" => CameraCommand::Resume(camera()?),
        "LeaveCamera" => CameraCommand::Leave(camera()?),
        _ => bail!("Unknown camera method"),
    };
    ensure!(
        !command.requires_manage() || claims.allows("camera.manage"),
        "Camera permission denied"
    );
    app.camera.execute(peer, command).await
}
