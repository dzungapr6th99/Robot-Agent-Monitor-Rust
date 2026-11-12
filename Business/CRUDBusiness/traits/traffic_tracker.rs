use crate::robot_ros_bl::RosTopicTraffic;
use robot_common_lib::ros::Topic;

pub(crate) trait TrafficTrackerTrait {
    #[cfg(any(feature = "ros2", test))]
    fn observe(&self, topic: &str) -> bool;
    fn status(&self, topic: &Topic) -> RosTopicTraffic;
}
