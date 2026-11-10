pub use robot_db_objects::Record;
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateRequest<T> {
    #[serde(alias = "RequestId")]
    pub request_id: Option<String>,
    #[serde(alias = "Data")]
    pub data: Option<T>,
}
pub type UpdateRequest<T> = CreateRequest<T>;
pub type DeleteRequest<T> = CreateRequest<T>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateResponse {
    pub request_id: Option<String>,
    pub message: Vec<String>,
    pub code: Vec<i32>,
    pub record_id: Option<i64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateResponse {
    pub request_id: Option<String>,
    pub message: Vec<String>,
    pub code: Vec<i32>,
}
pub type DeleteResponse = UpdateResponse;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryResponse<T> {
    pub request_id: Option<String>,
    pub message: Option<String>,
    pub code: i32,
    pub datas: Option<Vec<T>>,
}
