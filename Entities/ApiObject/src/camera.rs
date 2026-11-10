use serde::Serialize;
#[derive(Clone, Copy, PartialEq, Serialize)]
pub struct CameraResolution {
    pub preset: &'static str,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CameraStreamState {
    pub camera: String,
    pub active: Option<bool>,
    pub resolution: Option<CameraResolution>,
    pub producer_id: Option<String>,
    pub last_error: Option<&'static str>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}
