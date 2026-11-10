use std::sync::Arc;

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, State},
    http::{HeaderMap, StatusCode},
    routing::{get, post},
};
use robot_crud_business::{
    CrudServices,
    camera_bl::CameraBL,
    robot_ros_bl::{RobotRosBL, RosStatus},
};
use serde::Deserialize;
use std::{convert::Infallible, time::Duration};
use tokio::sync::broadcast::error::RecvError;

use crate::{
    auth::{Claims, TokenVerifier, bearer},
    camera_hub,
};

pub struct HttpServer {
    pub crud: Arc<CrudServices>,
    pub ros: Arc<RobotRosBL>,
    pub camera: Arc<CameraBL>,
    pub auth: Option<TokenVerifier>,
    pub origins: Vec<String>,
    pub shutdown: tokio::sync::watch::Receiver<bool>,
    pub(crate) negotiations: camera_hub::Negotiations,
}

impl HttpServer {
    pub fn new(
        crud: Arc<CrudServices>,
        ros: Arc<RobotRosBL>,
        camera: Arc<CameraBL>,
        auth: Option<TokenVerifier>,
        origins: Vec<String>,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Self {
        Self {
            crud,
            ros,
            camera,
            auth,
            origins,
            shutdown,
            negotiations: Default::default(),
        }
    }

    pub fn router(self) -> Router {
        let max = self.crud.max_urdf_bytes().saturating_add(64 * 1024);
        let cors = tower_http::cors::CorsLayer::new()
            .allow_origin(
                self.origins
                    .iter()
                    .filter(|origin| origin.as_str() != "*")
                    .filter_map(|origin| origin.parse().ok())
                    .collect::<Vec<_>>(),
            )
            .allow_credentials(true)
            .allow_methods([
                axum::http::Method::GET,
                axum::http::Method::POST,
                axum::http::Method::PUT,
                axum::http::Method::DELETE,
            ])
            .allow_headers([
                axum::http::header::AUTHORIZATION,
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderName::from_static("x-requested-with"),
                axum::http::HeaderName::from_static("x-signalr-user-agent"),
                axum::http::HeaderName::from_static("userid"),
            ]);
        Router::new()
            .route(
                "/api/v1/{resource}/create",
                post(crate::crud::create).layer(DefaultBodyLimit::max(max)),
            )
            .route(
                "/api/v1/{resource}/update",
                axum::routing::put(crate::crud::update),
            )
            .route(
                "/api/v1/{resource}/delete",
                axum::routing::delete(crate::crud::delete),
            )
            .route("/api/v1/{resource}/query", get(crate::crud::query))
            .route("/api/v1/{resource}/list", get(crate::monitor::list))
            .route("/api/health", get(crate::monitor::health))
            .route(
                "/api/v1/robot-configurations/{id}/manifest",
                get(crate::monitor::manifest),
            )
            .route(
                "/api/v1/robot-configurations/{id}/assets/{*path}",
                get(crate::monitor::asset),
            )
            .route("/hubs/camera/negotiate", post(camera_hub::negotiate))
            .route("/api/ros/status", get(ros_status))
            .route("/api/ros/messages", get(ros_messages))
            .route("/api/ros/publish", post(ros_publish))
            .route("/hubs/camera", get(camera_hub::upgrade))
            .layer(DefaultBodyLimit::max(64 * 1024))
            .layer(cors)
            .with_state(Arc::new(self))
    }

    pub(crate) async fn authorize(
        &self,
        token: Option<&str>,
        permission: &str,
    ) -> Result<Claims, StatusCode> {
        let verifier = self.auth.as_ref().ok_or(StatusCode::UNAUTHORIZED)?;
        let claims = verifier
            .verify(token.ok_or(StatusCode::UNAUTHORIZED)?)
            .await
            .map_err(|_| StatusCode::UNAUTHORIZED)?;
        if !claims.allows(permission) {
            return Err(StatusCode::FORBIDDEN);
        }
        Ok(claims)
    }
}

async fn ros_status(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
) -> Result<Json<RosStatus>, StatusCode> {
    app.authorize(bearer(&headers), "ros.view").await?;
    Ok(Json(app.ros.status()))
}

async fn ros_messages(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
) -> Result<impl axum::response::IntoResponse, StatusCode> {
    let claims = app.authorize(bearer(&headers), "ros.view").await?;
    let mut messages = app.ros.subscribe_messages();
    let mut shutdown = app.shutdown.clone();
    let stream = async_stream::stream! {
        let mut expiry = tokio::time::interval(Duration::from_secs(5));
        loop {
            if claims.expired() || *shutdown.borrow() { break; }
            tokio::select! {
                _ = shutdown.changed() => break,
                _ = expiry.tick() => {}
                message = messages.recv() => match message {
                    Ok(message) => {
                        match Event::default().event("ros_message").json_data(message) {
                            Ok(event) => yield Ok::<_, Infallible>(event),
                            Err(error) => eprintln!("ROS message JSON failed: {error}"),
                        }
                    }
                    Err(RecvError::Lagged(count)) => {
                        yield Ok(Event::default().event("messages_dropped").data(count.to_string()));
                    }
                    Err(RecvError::Closed) => break,
                }
            }
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(5))))
}

#[derive(Deserialize)]
struct PublishRequest {
    topic: String,
    #[serde(rename = "messageType")]
    message_type: String,
    data: serde_json::Value,
}

async fn ros_publish(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
    Json(request): Json<PublishRequest>,
) -> Result<StatusCode, StatusCode> {
    app.authorize(bearer(&headers), "ros.publish").await?;
    if !app.ros.status().running {
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    app.ros
        .publish(&request.topic, &request.message_type, request.data)
        .await
        .map_err(|error| {
            eprintln!("ROS publish failed: {error}");
            StatusCode::BAD_GATEWAY
        })?;
    Ok(StatusCode::NO_CONTENT)
}
