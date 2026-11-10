use robot_common_lib::ros::{Direction, RosTopicConfiguration};
use robot_crud_business::robot_ros_configuration::RobotRosConfiguration;
use std::path::Path;

#[test]
fn deployment_matches_the_actual_isaac_ros_graph() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let config: RosTopicConfiguration = serde_json::from_str(
        &std::fs::read_to_string(root.join("deploy/yellowarm-sim/ros-topics.json")).unwrap(),
    )
    .unwrap();
    let topics = config.into_topics().unwrap();
    let scene = std::fs::read_to_string(
        root.join("../../robots/yellowarm_orca_dual/usd/YellowArm_Two_ORCA_ros.usda"),
    )
    .unwrap();
    assert_eq!(topics.len(), 3);
    for topic in &topics {
        assert!(scene.contains(&format!("inputs:topicName = \"{}\"", topic.name)));
    }
    let state = topics
        .iter()
        .find(|topic| topic.name.ends_with("/joint_states"))
        .unwrap();
    assert_eq!(state.message_type, "sensor_msgs/msg/JointState");
    assert_eq!(state.direction, Direction::Subscribe);
    assert!(state.best_effort);
    let clock = topics
        .iter()
        .find(|topic| topic.name.ends_with("/clock"))
        .unwrap();
    assert_eq!(clock.message_type, "rosgraph_msgs/msg/Clock");
    assert_eq!(clock.direction, Direction::Subscribe);
    assert_eq!(
        topics
            .iter()
            .find(|topic| topic.name.ends_with("/joint_command"))
            .unwrap()
            .direction,
        Direction::Publish
    );
}
#[test]
fn deployment_and_database_topics_merge_without_duplicate_subscriptions() {
    let json =
        r#"{"subscriptions":[{"name":"/state","messageType":"sensor_msgs/msg/JointState"}]}"#;
    let first: RosTopicConfiguration = serde_json::from_str(json).unwrap();
    let second: RosTopicConfiguration = serde_json::from_str(json).unwrap();
    assert_eq!(
        RobotRosConfiguration::merge_topics(
            first
                .into_topics()
                .unwrap()
                .into_iter()
                .chain(second.into_topics().unwrap())
        )
        .unwrap()
        .len(),
        1
    );
    let conflict: RosTopicConfiguration = serde_json::from_str(
        r#"{"subscriptions":[{"name":"/state","messageType":"rosgraph_msgs/msg/Clock"}]}"#,
    )
    .unwrap();
    let base: RosTopicConfiguration = serde_json::from_str(json).unwrap();
    assert!(
        RobotRosConfiguration::merge_topics(
            base.into_topics()
                .unwrap()
                .into_iter()
                .chain(conflict.into_topics().unwrap())
        )
        .is_err()
    );
}
