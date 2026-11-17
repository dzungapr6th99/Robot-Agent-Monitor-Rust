use crate::{HttpServer, http::check_origin};
use axum::{
    Json,
    extract::{State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use robot_common_lib::vla::{VlaError, VlaProfile};
use robot_crud_business::traits::VlaBLTrait;
use serde::Deserialize;
use serde_json::{Value, json};
use std::sync::Arc;

type HttpError = (StatusCode, Json<Value>);
fn error(status: StatusCode, message: &str) -> HttpError {
    (status, Json(json!({"message": message})))
}

pub(crate) async fn models(State(app): State<Arc<HttpServer>>) -> Json<Vec<VlaProfile>> {
    Json(app.vla.models())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Command {
    profile_id: String,
    task: String,
}

pub(crate) async fn command(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
    body: Result<Json<Command>, JsonRejection>,
) -> Result<Json<Value>, HttpError> {
    check_origin(&app, &headers).map_err(|s| error(s, "Origin is not allowed"))?;
    let Json(command) = body.map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid VLA request"))?;
    let mut shutdown = app.shutdown.clone();
    if *shutdown.borrow() {
        return Err(error(
            StatusCode::SERVICE_UNAVAILABLE,
            "Agent is shutting down",
        ));
    }
    let result = tokio::select! {
        biased;
        _ = shutdown.changed() => return Err(error(StatusCode::SERVICE_UNAVAILABLE, "Agent is shutting down")),
        result = app.vla.run(&command.profile_id, &command.task) => result,
    };
    result
        .map(|steps| Json(json!({"publishedSteps": steps})))
        .map_err(|failure| {
            let kind = failure
                .downcast_ref::<VlaError>()
                .copied()
                .unwrap_or(VlaError::Unavailable);
            let status = match kind {
                VlaError::InvalidRequest => StatusCode::BAD_REQUEST,
                VlaError::ProfileNotFound => StatusCode::NOT_FOUND,
                VlaError::Busy => StatusCode::CONFLICT,
                VlaError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
                VlaError::Timeout => StatusCode::GATEWAY_TIMEOUT,
            };
            error(status, &kind.to_string())
        })
}

pub(crate) async fn stop(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    check_origin(&app, &headers).map_err(|s| error(s, "Origin is not allowed"))?;
    Ok(Json(json!({"stopping": app.vla.stop()})))
}
