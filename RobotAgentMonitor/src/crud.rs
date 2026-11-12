use robot_api_object::crud::{
    CreateRequest, CreateResponse, DeleteRequest, QueryResponse, UpdateRequest, UpdateResponse,
};
use robot_crud_business::traits::CrudServicesTrait;
use std::sync::Arc;

use axum::{
    Json,
    extract::{FromRequest, Multipart, Path, Request, State},
    http::{StatusCode, Uri},
    response::{IntoResponse, Response},
};
use robot_common_lib::{Upload, ValidationError};
use robot_db_objects::{Record, Resource};
use serde_json::{Value, json};

use crate::HttpServer;

pub(crate) async fn create(
    State(app): State<Arc<HttpServer>>,
    Path(resource): Path<String>,
    request: Request,
) -> Response {
    let Some(resource) = Resource::from_route(&resource) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let (envelope, urdf) = if resource == Resource::RobotConfiguration {
        match Multipart::from_request(request, &()).await {
            Ok(form) => match read_form(form, app.crud.max_urdf_bytes()).await {
                Ok(parts) => parts,
                Err(response) => return *response,
            },
            Err(error) => return error.into_response(),
        }
    } else {
        match Json::<CreateRequest<Record>>::from_request(request, &()).await {
            Ok(Json(envelope)) => (envelope, None),
            Err(error) => return error.into_response(),
        }
    };
    let Some(data) = envelope.data else {
        return if resource == Resource::RobotConfiguration {
            write_response(
                envelope.request_id,
                true,
                None,
                -100100001,
                "Data must not be null",
                false,
            )
        } else {
            (StatusCode::BAD_REQUEST, "Data is null").into_response()
        };
    };
    let result = tokio::task::spawn_blocking(move || app.crud.insert(resource, data, urdf)).await;
    complete_write(envelope.request_id, true, "Error when insert data", result)
}

pub(crate) async fn update(
    State(app): State<Arc<HttpServer>>,
    Path(resource): Path<String>,
    Json(request): Json<UpdateRequest<Record>>,
) -> Response {
    let Some(resource) = Resource::from_route(&resource) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let result = tokio::task::spawn_blocking(move || {
        app.crud.update(resource, request.data.unwrap_or_default())
    })
    .await;
    complete_write(request.request_id, false, "Error when update data", result)
}

pub(crate) async fn delete(
    State(app): State<Arc<HttpServer>>,
    Path(resource): Path<String>,
    Json(request): Json<DeleteRequest<Record>>,
) -> Response {
    let Some(resource) = Resource::from_route(&resource) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let result = tokio::task::spawn_blocking(move || {
        app.crud.delete(resource, request.data.unwrap_or_default())
    })
    .await;
    complete_write(request.request_id, false, "Error when delete data", result)
}

pub(crate) async fn query(
    State(app): State<Arc<HttpServer>>,
    Path(resource): Path<String>,
    uri: Uri,
) -> Response {
    let Some(resource) = Resource::from_route(&resource) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let ids = url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes())
        .filter(|(key, _)| {
            key.eq_ignore_ascii_case("ids") || key.to_ascii_lowercase().starts_with("ids[")
        })
        .map(|(_, value)| value.parse::<i32>().map(i64::from))
        .collect::<Result<Vec<_>, _>>();
    let Ok(ids) = ids else {
        return (StatusCode::BAD_REQUEST, "Invalid ids").into_response();
    };
    if ids.is_empty() {
        return query_error(1, "Success");
    }
    let result = tokio::task::spawn_blocking(move || app.crud.query(resource, &ids)).await;
    match result {
        Ok(Ok(records)) if !records.is_empty() => Json(records).into_response(),
        Ok(Ok(_)) => query_error(-999, "Error when search data"),
        result => {
            eprintln!("CRUD query failed: {result:?}");
            query_error(-999, "Service get error")
        }
    }
}

fn complete_write(
    request_id: Option<String>,
    create: bool,
    message: &str,
    result: Result<anyhow::Result<i64>, tokio::task::JoinError>,
) -> Response {
    match result {
        Ok(Ok(count)) if count >= 1 => {
            write_response(request_id, create, Some(count), 1, "Success", true)
        }
        Ok(Err(error)) if error.is::<ValidationError>() => {
            let error = error.downcast::<ValidationError>().unwrap();
            write_response(request_id, create, None, error.0, error.1, false)
        }
        result => {
            eprintln!("CRUD command failed: {result:?}");
            write_response(
                request_id,
                create,
                create.then_some(-1),
                -999,
                message,
                false,
            )
        }
    }
}

fn write_response(
    request_id: Option<String>,
    create: bool,
    id: Option<i64>,
    code: i32,
    message: &str,
    ok: bool,
) -> Response {
    let status = if ok {
        StatusCode::OK
    } else {
        StatusCode::BAD_REQUEST
    };
    if create {
        (
            status,
            Json(CreateResponse {
                request_id,
                message: vec![message.into()],
                code: vec![code],
                record_id: id,
            }),
        )
            .into_response()
    } else {
        (
            status,
            Json(UpdateResponse {
                request_id,
                message: vec![message.into()],
                code: vec![code],
            }),
        )
            .into_response()
    }
}

fn query_error(code: i32, message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(QueryResponse::<Record> {
            request_id: None,
            code,
            message: Some(message.into()),
            datas: None,
        }),
    )
        .into_response()
}

async fn read_form(
    mut form: Multipart,
    max: usize,
) -> Result<(CreateRequest<Record>, Option<Upload>), Box<Response>> {
    let mut envelope = CreateRequest::default();
    let mut urdf = None;
    let mut assets = Vec::new();
    let mut total = 0usize;
    while let Some(mut field) = form
        .next_field()
        .await
        .map_err(|error| Box::new(error.into_response()))?
    {
        let key = field.name().unwrap_or("").to_ascii_lowercase();
        if (urdf.is_none() && (key == "attachfiles" || key.starts_with("attachfiles[")))
            || key == "assetfiles"
        {
            let filename = field.file_name().unwrap_or("").to_owned();
            let mut bytes = Vec::new();
            while let Some(chunk) = field
                .chunk()
                .await
                .map_err(|error| Box::new(error.into_response()))?
            {
                total = total.saturating_add(chunk.len());
                if total > max {
                    return Err(Box::new(write_response(
                        envelope.request_id,
                        true,
                        None,
                        -100100008,
                        "URDF file exceeds the allowed size",
                        false,
                    )));
                }
                bytes.extend_from_slice(&chunk);
            }
            if key == "assetfiles" {
                if assets.len() >= 499 {
                    return Err(Box::new(
                        (StatusCode::PAYLOAD_TOO_LARGE, "Too many package files").into_response(),
                    ));
                }
                assets.push(robot_common_lib::UploadAsset { filename, bytes });
            } else {
                urdf = Some(Upload {
                    filename,
                    bytes,
                    assets: Vec::new(),
                });
            }
        } else if key == "requestid" {
            envelope.request_id = Some(
                field
                    .text()
                    .await
                    .map_err(|error| Box::new(error.into_response()))?,
            );
        } else if key == "data" {
            let value = field
                .text()
                .await
                .map_err(|error| Box::new(error.into_response()))?;
            envelope.data = Some(serde_json::from_str(&value).map_err(|_| {
                Box::new((StatusCode::BAD_REQUEST, "Invalid Data").into_response())
            })?);
        } else if let Some(property) = key.strip_prefix("data.") {
            let text = field
                .text()
                .await
                .map_err(|error| Box::new(error.into_response()))?;
            let data = envelope.data.get_or_insert_with(Record::new);
            match property {
                "robotprofileid" => {
                    data.insert(
                        "robotProfileId".into(),
                        text.parse::<i64>()
                            .map(|id| json!(id))
                            .unwrap_or(Value::Null),
                    );
                }
                "name" => {
                    data.insert("name".into(), json!(text));
                }
                _ => {}
            }
        }
    }
    if let Some(upload) = &mut urdf {
        upload.assets = assets;
    }
    Ok((envelope, urdf))
}
