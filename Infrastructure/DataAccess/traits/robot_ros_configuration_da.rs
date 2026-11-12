use anyhow::Result;
use robot_common_lib::ros::Topic;

pub trait RobotRosConfigurationDATrait: Send + Sync {
    fn topics(&self, id: i64) -> Result<Vec<Topic>>;
}
