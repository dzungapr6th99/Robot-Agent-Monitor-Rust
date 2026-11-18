use robot_crud_business::traits::{CrudServicesTrait, RobotRosBLTrait};
use std::sync::Arc;

use axum::{
    Json, Router,
    extract::{DefaultBodyLimit, MatchedPath, Request, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
};
use robot_crud_business::{
    CrudServices,
    robot_ros_bl::{RobotRosBL, RosStatus},
};
use serde::Deserialize;

pub struct HttpServer {
    pub crud: Arc<CrudServices>,
    pub ros: Arc<RobotRosBL>,
    pub origins: Vec<String>,
    pub shutdown: tokio::sync::watch::Receiver<bool>,
}

impl HttpServer {
    pub fn new(
        crud: Arc<CrudServices>,
        ros: Arc<RobotRosBL>,
        origins: Vec<String>,
        shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> Self {
        Self {
            crud,
            ros,
            origins,
            shutdown,
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
            .route("/api/ros/status", get(ros_status))
            .route("/api/ros/ws", get(crate::ros_socket::upgrade))
            .route("/api/ros/publish", post(ros_publish))
            .layer(DefaultBodyLimit::max(64 * 1024))
            .layer(cors)
            .layer(middleware::from_fn(log_incoming_request))
            .with_state(Arc::new(self))
    }
}

async fn log_incoming_request(request: Request, next: Next) -> Response {
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or("<unmatched>", MatchedPath::as_str);
    robot_common_lib::operation_log::info(
        module_path!(),
        format_args!("Receiving HTTP {} {route}", request.method()),
    );
    next.run(request).await
}

async fn ros_status(State(app): State<Arc<HttpServer>>) -> Json<RosStatus> {
    Json(app.ros.status())
}

#[derive(Deserialize)]
pub(crate) struct PublishRequest {
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
    check_origin(&app, &headers)?;
    publish(&app, request).await
}

pub(crate) fn check_origin(app: &HttpServer, headers: &HeaderMap) -> Result<(), StatusCode> {
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

pub(crate) async fn publish(
    app: &HttpServer,
    request: PublishRequest,
) -> Result<StatusCode, StatusCode> {
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
