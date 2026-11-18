#[cfg(not(feature = "ros2"))]
pub mod standard_messages;
#[cfg(feature = "ros2")]
pub use ros_env::{
    builtin_interfaces::msg::Time,
    diagnostic_msgs::msg::{DiagnosticArray, DiagnosticStatus, KeyValue},
    geometry_msgs::msg::{
        Point, Pose, PoseWithCovariance, Quaternion, Transform, TransformStamped, Twist,
        TwistWithCovariance, Vector3,
    },
    nav_msgs::msg::Odometry,
    rosgraph_msgs::msg::Clock,
    sensor_msgs::msg::JointState,
    std_msgs::msg::Header,
    tf2_msgs::msg::TFMessage,
};
use serde::Serialize;
#[cfg(not(feature = "ros2"))]
pub use standard_messages::*;

#[cfg(feature = "ros2")]
pub trait RosMessageType:
    rosidl_runtime_rs::Message + Clone + Send + Sync + Serialize + serde::de::DeserializeOwned + 'static
{
    const TYPE_NAME: &'static str;
    fn validate(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

#[cfg(not(feature = "ros2"))]
pub trait RosMessageType:
    Clone + Send + Sync + Serialize + serde::de::DeserializeOwned + 'static
{
    const TYPE_NAME: &'static str;
    fn validate(&self) -> Result<(), &'static str> {
        Ok(())
    }
}

impl RosMessageType for Clock {
    const TYPE_NAME: &'static str = "rosgraph_msgs/msg/Clock";
    fn validate(&self) -> Result<(), &'static str> {
        if self.clock.nanosec >= 1_000_000_000 {
            return Err("Invalid ROS clock nanosec");
        }
        Ok(())
    }
}
impl RosMessageType for JointState {
    const TYPE_NAME: &'static str = "sensor_msgs/msg/JointState";
    fn validate(&self) -> Result<(), &'static str> {
        for values in [&self.position, &self.velocity, &self.effort] {
            if !values.is_empty() && values.len() != self.name.len() {
                return Err("JointState arrays must match name length or be empty");
            }
        }
        valid_header(&self.header)
    }
}
impl RosMessageType for DiagnosticStatus {
    const TYPE_NAME: &'static str = "diagnostic_msgs/msg/DiagnosticStatus";
    fn validate(&self) -> Result<(), &'static str> {
        if self.level > Self::STALE {
            return Err("Invalid diagnostic status level");
        }
        Ok(())
    }
}
impl RosMessageType for DiagnosticArray {
    const TYPE_NAME: &'static str = "diagnostic_msgs/msg/DiagnosticArray";
    fn validate(&self) -> Result<(), &'static str> {
        valid_header(&self.header)?;
        for status in &self.status {
            status.validate()?;
        }
        Ok(())
    }
}
impl RosMessageType for Odometry {
    const TYPE_NAME: &'static str = "nav_msgs/msg/Odometry";
    fn validate(&self) -> Result<(), &'static str> {
        valid_header(&self.header)
    }
}
impl RosMessageType for TFMessage {
    const TYPE_NAME: &'static str = "tf2_msgs/msg/TFMessage";
    fn validate(&self) -> Result<(), &'static str> {
        for transform in &self.transforms {
            valid_header(&transform.header)?;
        }
        Ok(())
    }
}
fn valid_header(header: &Header) -> Result<(), &'static str> {
    if header.stamp.nanosec >= 1_000_000_000 {
        return Err("Invalid ROS timestamp nanosec");
    }
    Ok(())
}

/// JSON conversion happens at the monitor boundary; Rust services use their typed handles directly.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RosMessageEvent {
    pub topic: String,
    pub message_type: String,
    pub data: serde_json::Value,
}
impl RosMessageEvent {
    pub fn from_message<T: RosMessageType>(
        topic: String,
        message: T,
    ) -> Result<Self, serde_json::Error> {
        Ok(Self {
            topic,
            message_type: T::TYPE_NAME.into(),
            data: serde_json::to_value(message)?,
        })
    }
}
