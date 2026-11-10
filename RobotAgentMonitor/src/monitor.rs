use crate::HttpServer;
use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};
use robot_db_objects::Resource;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize)]
pub(crate) struct Page {
    limit: Option<u32>,
    offset: Option<u32>,
}
pub(crate) async fn list(
    State(app): State<Arc<HttpServer>>,
    Path(resource): Path<String>,
    Query(page): Query<Page>,
) -> Response {
    let Some(resource) = Resource::from_route(&resource) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    match tokio::task::spawn_blocking(move || {
        app.crud.list(
            resource,
            page.limit.unwrap_or(200).clamp(1, 500),
            page.offset.unwrap_or(0),
        )
    })
    .await
    {
        Ok(Ok(rows)) => Json(rows).into_response(),
        error => {
            eprintln!("List failed: {error:?}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Cannot load records").into_response()
        }
    }
}
pub(crate) async fn health(State(app): State<Arc<HttpServer>>) -> Response {
    match tokio::task::spawn_blocking(move || app.crud.list(Resource::RobotProfile, 1, 0)).await {
        Ok(Ok(_)) => Json(serde_json::json!({"status":"ok"})).into_response(),
        _ => (StatusCode::SERVICE_UNAVAILABLE, "Database unavailable").into_response(),
    }
}
pub(crate) async fn asset(
    State(app): State<Arc<HttpServer>>,
    Path((id, path)): Path<(i64, String)>,
) -> Response {
    match tokio::task::spawn_blocking(move || app.crud.robot_configurations.read_asset(id, &path))
        .await
    {
        Ok(Ok((bytes, mime))) => (
            [
                (header::CONTENT_TYPE, mime),
                (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            ],
            bytes,
        )
            .into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}

pub(crate) async fn manifest(State(app): State<Arc<HttpServer>>, Path(id): Path<i64>) -> Response {
    match tokio::task::spawn_blocking(move || app.crud.robot_configurations.asset_manifest(id))
        .await
    {
        Ok(Ok(files)) => Json(files).into_response(),
        _ => StatusCode::NOT_FOUND.into_response(),
    }
}
