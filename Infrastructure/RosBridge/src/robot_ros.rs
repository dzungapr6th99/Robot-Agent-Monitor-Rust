use anyhow::Result;

#[cfg(feature = "ros2")]
#[path = "robot_ros/rclrs_runtime.rs"]
mod rclrs_runtime;
#[cfg(feature = "ros2")]
use rclrs_runtime::RosRuntime;

#[derive(Default)]
pub struct RobotRos {
    #[cfg(feature = "ros2")]
    runtime: Option<RosRuntime>,
}

impl RobotRos {
    pub fn start(domain_id: u16, namespace: &str) -> Result<Self> {
        robot_common_lib::logged_result!("start", {
            #[cfg(feature = "ros2")]
            {
                Ok(Self {
                    runtime: Some(RosRuntime::start(domain_id, namespace)?),
                })
            }
            #[cfg(not(feature = "ros2"))]
            {
                let _ = (domain_id, namespace);
                anyhow::bail!(
                    "ROS support requires a sourced ROS 2 workspace and cargo build --features ros2"
                );
            }
        })
    }
}

#[cfg(not(feature = "ros2"))]
impl crate::traits::RobotRosTrait for RobotRos {
    fn running(&self) -> bool {
        false
    }
}
