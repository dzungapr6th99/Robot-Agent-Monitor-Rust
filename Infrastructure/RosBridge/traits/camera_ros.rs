use anyhow::Result;
use std::future::Future;

pub trait CameraRosTrait: Send + Sync {
    fn running(&self) -> bool;
    fn encoder(
        &self,
        camera: &str,
        action: &str,
        width: u32,
        height: u32,
    ) -> impl Future<Output = Result<String>> + Send;
}
