use crate::robot_ros_bl::RosStatus;
use anyhow::Result;
use robot_ros_bridge::{RosMessageEvent, RosMessageType};
use serde_json::Value;
use std::future::Future;
use tokio::sync::broadcast;

pub trait RobotRosBLTrait: Send + Sync {
    /// Register a type exposed through the monitor HTTP API.
    fn register<T: RosMessageType>(&mut self) -> impl Future<Output = Result<()>> + Send;
    fn validate_monitor_topics(&self) -> Result<()>;
    fn status(&self) -> RosStatus;
    fn subscribe_messages(&self) -> broadcast::Receiver<RosMessageEvent>;
    fn publish(
        &self,
        topic: &str,
        message_type: &str,
        data: Value,
    ) -> impl Future<Output = Result<()>> + Send;
}
