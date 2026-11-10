use anyhow::{Result, bail};
use robot_common_lib::ros::{Direction, Topic};
use robot_data_access::robot_ros_configuration_da::RobotRosConfigurationDA;
use std::{collections::BTreeMap, sync::Arc};
pub struct RobotRosConfiguration(pub Arc<RobotRosConfigurationDA>);
impl RobotRosConfiguration {
    pub fn get_topics(&self, configuration_id: i64) -> Result<Vec<Topic>> {
        anyhow::ensure!(configuration_id > 0, "Invalid robot configuration ID");
        Self::merge_topics(self.0.topics(configuration_id)?)
    }
    pub fn merge_topics(topics: impl IntoIterator<Item = Topic>) -> Result<Vec<Topic>> {
        let mut endpoints = BTreeMap::<(Direction, String), Topic>::new();
        for topic in topics {
            let key = (topic.direction.clone(), topic.name.clone());
            if let Some(existing) = endpoints.get_mut(&key) {
                if existing.message_type != topic.message_type {
                    bail!("Conflicting ROS message types for {}", topic.name);
                }
                existing.best_effort |= topic.best_effort;
            } else {
                endpoints.insert(key, topic);
            }
        }
        Ok(endpoints.into_values().collect())
    }
}
