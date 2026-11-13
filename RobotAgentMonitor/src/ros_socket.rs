use crate::{
    HttpServer, camera_hub,
    http::{PublishRequest, publish},
};
use axum::{
    extract::{
        State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode},
    response::Response,
};
use robot_crud_business::traits::RobotRosBLTrait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::{sync::broadcast::error::RecvError, time};

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum ClientFrame {
    Publish {
        id: u32,
        #[serde(flatten)]
        request: PublishRequest,
    },
}

pub(crate) async fn upgrade(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
    upgrade: WebSocketUpgrade,
) -> Result<Response, StatusCode> {
    camera_hub::check_origin(&app, &headers)?;
    Ok(upgrade
        .max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .on_upgrade(move |socket| run(app, socket)))
}

async fn run(app: Arc<HttpServer>, mut socket: WebSocket) {
    let mut messages = app.ros.subscribe_messages();
    let mut shutdown = app.shutdown.clone();
    let mut status = time::interval(Duration::from_secs(5));
    status.set_missed_tick_behavior(time::MissedTickBehavior::Skip);

    while !*shutdown.borrow() {
        let frame = tokio::select! {
            _ = shutdown.changed() => break,
            _ = status.tick() => json!({"type": "status", "data": app.ros.status()}),
            message = messages.recv() => match message {
                Ok(data) => json!({"type": "ros_message", "data": data}),
                Err(RecvError::Lagged(count)) => json!({"type": "messages_dropped", "count": count}),
                Err(RecvError::Closed) => break,
            },
            frame = socket.recv() => match frame {
                Some(Ok(Message::Text(text))) => match serde_json::from_str(&text) {
                    Ok(ClientFrame::Publish { id, request }) => {
                        let result = publish(&app, request).await;
                        let status = result.unwrap_or_else(|status| status);
                        json!({"type": "publish_result", "id": id, "status": status.as_u16()})
                    }
                    _ => json!({"type": "error", "message": "Invalid ROS WebSocket request"}),
                },
                Some(Ok(Message::Ping(_) | Message::Pong(_))) => continue,
                Some(Ok(Message::Binary(_))) => json!({"type": "error", "message": "Expected JSON text"}),
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
            },
        };
        if !send(&mut socket, frame).await {
            break;
        }
    }
}

async fn send(socket: &mut WebSocket, frame: Value) -> bool {
    matches!(
        time::timeout(
            Duration::from_secs(3),
            socket.send(Message::Text(frame.to_string().into()))
        )
        .await,
        Ok(Ok(()))
    )
}
