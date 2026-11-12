//! Read-only end-to-end probe through the same rclrs + BL path as the backend.
use anyhow::{Context, Result, ensure};
use robot_common_lib::ros::{Direction, RosTopicConfiguration};
use robot_crud_business::{robot_ros_bl::RobotRosBL, traits::RobotRosBLTrait};
use robot_ros_bridge::{
    RobotRos,
    messages::{Clock, JointState},
};
use serde_json::json;
use std::{
    env,
    sync::Arc,
    time::{Duration, Instant},
};
#[tokio::main]
async fn main() -> Result<()> {
    let domain: u16 = env::var("ROS_DOMAIN_ID")
        .unwrap_or_else(|_| "117".into())
        .parse()?;
    let path = env::var("ROS_TOPICS_FILE")
        .unwrap_or_else(|_| "deploy/yellowarm-sim/ros-topics.json".into());
    let config: RosTopicConfiguration = serde_json::from_str(&std::fs::read_to_string(path)?)?;
    let topics = config
        .into_topics()?
        .into_iter()
        .filter(|topic| topic.direction == Direction::Subscribe)
        .collect();
    let ros = Arc::new(RobotRos::start(domain, "/")?);
    let mut monitor = RobotRosBL::new(ros, topics);
    let mut events = monitor.subscribe_messages();
    monitor.register::<JointState>().await?;
    monitor.register::<Clock>().await?;
    monitor.validate_monitor_topics()?;
    let expected: usize = env::var("ROS_EXPECTED_JOINTS")
        .unwrap_or_else(|_| "48".into())
        .parse()?;
    let seconds: u64 = env::var("ROS_PROBE_TIMEOUT_SECONDS")
        .unwrap_or_else(|_| "60".into())
        .parse()?;
    let deadline = Instant::now() + Duration::from_secs(seconds);
    let mut states = 0u64;
    let mut clocks = 0u64;
    let mut joint_count = 0;
    while states < 5 || clocks < 5 {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .context("Timed out waiting for Isaac Sim joint state and clock")?;
        let event = tokio::time::timeout(remaining, events.recv())
            .await
            .context("Timed out waiting for ROS messages")??;
        match event.message_type.as_str() {
            "sensor_msgs/msg/JointState" => {
                let message: JointState = serde_json::from_value(event.data)?;
                ensure!(
                    message.name.len() == expected,
                    "Expected {expected} joints, received {}",
                    message.name.len()
                );
                ensure!(
                    message.position.len() == expected
                        && message.position.iter().all(|value| value.is_finite()),
                    "Invalid joint positions"
                );
                joint_count = message.name.len();
                states += 1;
            }
            "rosgraph_msgs/msg/Clock" => {
                let _: Clock = serde_json::from_value(event.data)?;
                clocks += 1;
            }
            _ => {}
        }
    }
    let report = json!({"passed":true,"transport":"rclrs","domainId":domain,"jointCount":joint_count,"jointStateMessages":states,"clockMessages":clocks,"traffic":monitor.status().traffic});
    let report = serde_json::to_string_pretty(&report)?;
    if let Ok(path) = env::var("ROS_RECEIVE_REPORT") {
        std::fs::write(path, &report)?;
    }
    println!("{report}");
    Ok(())
}
