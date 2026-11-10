use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Direction {
    Subscribe,
    Publish,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Topic {
    pub name: String,
    pub message_type: String,
    #[serde(skip)]
    pub direction: Direction,
    pub best_effort: bool,
    pub transient_local: bool,
}

pub fn default_topics() -> Vec<Topic> {
    [
        (
            "/diagnostics",
            "diagnostic_msgs/msg/DiagnosticArray",
            false,
            false,
        ),
        ("/joint_states", "sensor_msgs/msg/JointState", true, false),
        ("/odom", "nav_msgs/msg/Odometry", true, false),
        ("/tf", "tf2_msgs/msg/TFMessage", true, false),
        ("/tf_static", "tf2_msgs/msg/TFMessage", false, true),
    ]
    .into_iter()
    .map(|(name, message_type, best_effort, transient_local)| Topic {
        name: name.into(),
        message_type: message_type.into(),
        direction: Direction::Subscribe,
        best_effort,
        transient_local,
    })
    .collect()
}

/// Deployment endpoints; robot-specific names stay in configuration files.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RosTopicConfiguration {
    #[serde(default)]
    pub subscriptions: Vec<RosEndpoint>,
    #[serde(default)]
    pub publishers: Vec<RosEndpoint>,
}
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RosEndpoint {
    pub name: String,
    pub message_type: String,
    #[serde(default)]
    pub best_effort: bool,
    #[serde(default)]
    pub transient_local: bool,
}
impl RosTopicConfiguration {
    pub fn into_topics(self) -> anyhow::Result<Vec<Topic>> {
        self.subscriptions
            .into_iter()
            .map(|endpoint| (endpoint, Direction::Subscribe))
            .chain(
                self.publishers
                    .into_iter()
                    .map(|endpoint| (endpoint, Direction::Publish)),
            )
            .map(|(endpoint, direction)| {
                anyhow::ensure!(
                    endpoint.name.starts_with('/')
                        && !endpoint.name.chars().any(char::is_whitespace),
                    "ROS endpoint must be an absolute topic name: {}",
                    endpoint.name
                );
                anyhow::ensure!(
                    !endpoint.message_type.trim().is_empty(),
                    "ROS message type is required for {}",
                    endpoint.name
                );
                Ok(Topic {
                    name: endpoint.name,
                    message_type: endpoint.message_type,
                    direction,
                    best_effort: endpoint.best_effort,
                    transient_local: endpoint.transient_local,
                })
            })
            .collect()
    }
}
