pub mod robot_ros;
pub use robot_ros::RobotRos;
pub use robot_ros2_interface_message as messages;
pub use robot_ros2_interface_message::{RosMessageEvent, RosMessageType};

#[cfg(feature = "ros2")]
pub use rclrs::rosidl_runtime_rs::{Message, Service as ServiceType};
#[cfg(feature = "ros2")]
pub use rclrs::{self, Client, Node, Publisher, QoSProfile, Service, Subscription};

pub mod camera_ros;
pub use camera_ros::CameraRos;

#[path = "../traits/mod.rs"]
pub mod traits;
