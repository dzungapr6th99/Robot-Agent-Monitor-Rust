use anyhow::Result;
use robot_common_lib::ros::Topic;

pub trait RobotRosConfigurationTrait: Send + Sync {
    fn get_topics(&self, configuration_id: i64) -> Result<Vec<Topic>>;
}
