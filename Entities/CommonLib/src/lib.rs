pub mod urdf_storage;

pub struct UploadAsset {
    pub filename: String,
    pub bytes: Vec<u8>,
}

pub struct Upload {
    pub filename: String,
    pub bytes: Vec<u8>,
    pub assets: Vec<UploadAsset>,
}
#[derive(Debug)]
pub struct ValidationError(pub i32, pub &'static str);

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.1)
    }
}
impl std::error::Error for ValidationError {}

pub mod const_data;
pub mod ros;

pub mod integrations;
