use crate::camera_bl::{CameraCommand, CameraQuery};
use anyhow::Result;
use robot_api_object::camera::CameraStreamState;
use serde_json::Value;
use std::future::Future;
use tokio::sync::broadcast;

pub trait CameraBLTrait: Send + Sync {
    fn subscribe(&self) -> broadcast::Receiver<CameraStreamState>;
    fn connect(&self, peer: String) -> impl Future<Output = Result<bool>> + Send;
    fn disconnect(&self, peer: &str) -> impl Future<Output = bool> + Send;
    fn retry_cleanup(&self) -> impl Future<Output = bool> + Send;
    fn shutdown(&self) -> impl Future<Output = bool> + Send;
    fn query(&self, query: CameraQuery) -> impl Future<Output = Result<Value>> + Send;
    fn execute(
        &self,
        peer: &str,
        command: CameraCommand,
    ) -> impl Future<Output = Result<Value>> + Send;
}
