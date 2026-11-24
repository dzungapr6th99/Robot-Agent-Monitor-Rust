use crate::{HttpServer, http::check_origin};
use axum::{
    Json,
    extract::{Multipart, Path, Query, State, rejection::JsonRejection},
    http::{HeaderMap, StatusCode},
};
use robot_common_lib::vla::{VlaError, VlaRuntimeStatus};
use robot_crud_business::traits::{VlaBLTrait, VlaModelBLTrait};
use robot_db_objects::VlaModel;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    fs::{self, File},
    io::Write,
    sync::Arc,
};

type HttpError = (StatusCode, Json<Value>);

fn error(status: StatusCode, message: &str) -> HttpError {
    (status, Json(json!({"message": message})))
}

fn map_vla_error(failure: anyhow::Error) -> HttpError {
    let kind = failure
        .downcast_ref::<VlaError>()
        .copied()
        .unwrap_or(VlaError::Unavailable);
    let status = match kind {
        VlaError::InvalidRequest | VlaError::InvalidPackage => StatusCode::BAD_REQUEST,
        VlaError::ModelNotFound => StatusCode::NOT_FOUND,
        VlaError::Busy | VlaError::InUse | VlaError::WrongRobot | VlaError::Duplicate => {
            StatusCode::CONFLICT
        }
        VlaError::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
        VlaError::Timeout => StatusCode::GATEWAY_TIMEOUT,
    };
    let msg = failure.to_string();
    let text = if msg.is_empty() {
        kind.to_string()
    } else {
        msg
    };
    error(status, &text)
}

fn model_to_json(model: VlaModel) -> Value {
    let manifest: Option<Value> = model
        .manifest_json
        .as_deref()
        .and_then(|raw| serde_json::from_str(raw).ok());
    let mut value = serde_json::to_value(model).unwrap_or_else(|_| json!({}));
    if let Some(obj) = value.as_object_mut() {
        obj.insert(
            "manifest".into(),
            manifest.unwrap_or(Value::Null),
        );
    }
    value
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ListQuery {
    robot_configuration_id: Option<String>,
    q: Option<String>,
}

pub(crate) async fn list_models(
    State(app): State<Arc<HttpServer>>,
    Query(query): Query<ListQuery>,
) -> Result<Json<Vec<Value>>, HttpError> {
    let config_id = match query
        .robot_configuration_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(raw) => Some(
            raw.parse::<i64>()
                .map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid robotConfigurationId"))?,
        ),
        None => None,
    };
    let vla_models = Arc::clone(&app.vla_models);
    let models = tokio::task::spawn_blocking(move || vla_models.list(config_id, query.q.as_deref()))
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Worker failed"))?
        .map_err(map_vla_error)?;
    Ok(Json(models.into_iter().map(model_to_json).collect()))
}

pub(crate) async fn get_model(
    State(app): State<Arc<HttpServer>>,
    Path(id): Path<i64>,
) -> Result<Json<Value>, HttpError> {
    let vla_models = Arc::clone(&app.vla_models);
    let model = tokio::task::spawn_blocking(move || vla_models.get(id))
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Worker failed"))?
        .map_err(map_vla_error)?;
    match model {
        Some(m) => Ok(Json(model_to_json(m))),
        None => Err(error(StatusCode::NOT_FOUND, "VLA model not found")),
    }
}

pub(crate) async fn upload_model(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
    mut multipart: Multipart,
) -> Result<(StatusCode, Json<Value>), HttpError> {
    check_origin(&app, &headers).map_err(|s| error(s, "Origin is not allowed"))?;

    let staging_dir = app.vla_data_root.join("vla").join(".staging");
    fs::create_dir_all(&staging_dir)
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Cannot prepare upload staging"))?;
    let temp_zip = staging_dir.join(format!("upload-{}.zip", uuid::Uuid::new_v4()));

    let mut config_id: Option<i64> = None;
    let mut has_package = false;
    let max_bytes = app.vla_max_package_bytes;

    let stream_result: Result<(), HttpError> = async {
        while let Some(mut field) = multipart.next_field().await.map_err(|err| {
            let status = err.status();
            let message = if status == StatusCode::PAYLOAD_TOO_LARGE {
                "VLA package exceeds maximum allowed size"
            } else {
                "Invalid multipart upload"
            };
            error(status, message)
        })? {
            let name = field.name().unwrap_or("").to_owned();
            if name.eq_ignore_ascii_case("robotConfigurationId") {
                let text = field
                    .text()
                    .await
                    .map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid robotConfigurationId"))?;
                config_id = text.trim().parse::<i64>().ok();
            } else if name.eq_ignore_ascii_case("package") {
                if let Some(filename) = field.file_name()
                    && !filename.to_ascii_lowercase().ends_with(".zip")
                {
                    return Err(error(
                        StatusCode::BAD_REQUEST,
                        "VLA package file must have .zip extension",
                    ));
                }
                let mut file = File::create(&temp_zip).map_err(|_| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Cannot create staging file for upload",
                    )
                })?;
                let mut total = 0usize;
                while let Some(chunk) = field.chunk().await.map_err(|err| {
                    let status = err.status();
                    if status == StatusCode::PAYLOAD_TOO_LARGE {
                        error(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "VLA package exceeds maximum allowed size",
                        )
                    } else {
                        error(StatusCode::BAD_REQUEST, "Failed while reading package upload")
                    }
                })? {
                    total = total.saturating_add(chunk.len());
                    if total > max_bytes {
                        return Err(error(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            "VLA package exceeds maximum allowed size",
                        ));
                    }
                    file.write_all(&chunk).map_err(|_| {
                        error(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Failed writing package to staging storage",
                        )
                    })?;
                }
                file.sync_all().map_err(|_| {
                    error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Failed flushing package to staging storage",
                    )
                })?;
                has_package = total > 0;
            }
        }
        Ok(())
    }
    .await;

    if let Err(err) = stream_result {
        let _ = fs::remove_file(&temp_zip);
        return Err(err);
    }

    let Some(configuration_id) = config_id.filter(|&id| id > 0) else {
        let _ = fs::remove_file(&temp_zip);
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Valid robotConfigurationId is required",
        ));
    };
    if !has_package {
        let _ = fs::remove_file(&temp_zip);
        return Err(error(
            StatusCode::BAD_REQUEST,
            "Package zip file ('package') is required",
        ));
    }

    let vla_models = Arc::clone(&app.vla_models);
    let zip_for_install = temp_zip.clone();
    let installed = tokio::task::spawn_blocking(move || {
        let res = vla_models.install(configuration_id, &zip_for_install);
        let _ = fs::remove_file(&zip_for_install);
        match res {
            Ok(id) => vla_models.get(id).map(|m| (id, m)),
            Err(err) => Err(err),
        }
    })
    .await
    .map_err(|_| {
        let _ = fs::remove_file(&temp_zip);
        error(StatusCode::INTERNAL_SERVER_ERROR, "Install worker failed")
    })?
    .map_err(map_vla_error)?;

    let body = match installed.1 {
        Some(model) => model_to_json(model),
        None => json!({"id": installed.0}),
    };
    Ok((StatusCode::CREATED, Json(body)))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct UpdateModelRequest {
    display_name: Option<String>,
    description: Option<String>,
}

pub(crate) async fn update_model(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
    body: Result<Json<UpdateModelRequest>, JsonRejection>,
) -> Result<Json<Value>, HttpError> {
    check_origin(&app, &headers).map_err(|s| error(s, "Origin is not allowed"))?;
    let Json(req) =
        body.map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid update request body"))?;
    let vla_models = Arc::clone(&app.vla_models);
    let updated = tokio::task::spawn_blocking(move || {
        vla_models.update(id, req.display_name, req.description)
    })
    .await
    .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Worker failed"))?
    .map_err(map_vla_error)?;
    Ok(Json(model_to_json(updated)))
}

pub(crate) async fn delete_model(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
    Path(id): Path<i64>,
) -> Result<Json<Value>, HttpError> {
    check_origin(&app, &headers).map_err(|s| error(s, "Origin is not allowed"))?;
    let vla_models = Arc::clone(&app.vla_models);
    let deleted = tokio::task::spawn_blocking(move || vla_models.delete(id))
        .await
        .map_err(|_| error(StatusCode::INTERNAL_SERVER_ERROR, "Worker failed"))?
        .map_err(map_vla_error)?;
    Ok(Json(json!({"deleted": deleted, "id": id})))
}

pub(crate) async fn status(State(app): State<Arc<HttpServer>>) -> Json<VlaRuntimeStatus> {
    Json(app.vla.status())
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Command {
    robot_configuration_id: i64,
    model_id: i64,
    task: String,
}

pub(crate) async fn command(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
    body: Result<Json<Command>, JsonRejection>,
) -> Result<Json<Value>, HttpError> {
    check_origin(&app, &headers).map_err(|s| error(s, "Origin is not allowed"))?;
    let Json(command) =
        body.map_err(|_| error(StatusCode::BAD_REQUEST, "Invalid VLA request"))?;
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
        result = app.vla.run(command.robot_configuration_id, command.model_id, &command.task) => result,
    };
    result
        .map(|steps| Json(json!({"publishedSteps": steps})))
        .map_err(map_vla_error)
}

pub(crate) async fn stop(
    State(app): State<Arc<HttpServer>>,
    headers: HeaderMap,
) -> Result<Json<Value>, HttpError> {
    check_origin(&app, &headers).map_err(|s| error(s, "Origin is not allowed"))?;
    Ok(Json(json!({"stopping": app.vla.stop()})))
}
