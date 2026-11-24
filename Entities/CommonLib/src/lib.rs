use std::borrow::Cow;

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
pub struct ValidationError(pub i32, pub Cow<'static, str>);

impl ValidationError {
    pub const fn new(code: i32, message: &'static str) -> Self {
        Self(code, Cow::Borrowed(message))
    }

    pub fn owned(code: i32, message: String) -> Self {
        Self(code, Cow::Owned(message))
    }
}

impl std::fmt::Display for ValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.1)
    }
}
impl std::error::Error for ValidationError {}

pub mod const_data;
pub mod operation_log;
pub mod ros;
pub mod vla;
pub mod vla_package;
