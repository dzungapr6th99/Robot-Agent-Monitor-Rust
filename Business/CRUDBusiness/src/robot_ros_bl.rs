use anyhow::{Context, Result, ensure};
use robot_common_lib::ros::{Direction, Topic};
#[cfg(feature = "ros2")]
use robot_ros_bridge::{Publisher, Subscription};
use robot_ros_bridge::{RobotRos, RosMessageEvent, RosMessageType};
use serde::Serialize;
use serde_json::Value;
#[cfg(any(feature = "ros2", test))]
use std::time::{SystemTime, UNIX_EPOCH};
use std::{
    collections::{HashMap, HashSet},
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::broadcast;

#[derive(Serialize)]
pub struct RosStatus {
    pub running: bool,
    pub subscriptions: Vec<Topic>,
    pub publishers: Vec<Topic>,
    pub traffic: Vec<RosTopicTraffic>,
}

/// Receipt metadata only; message payloads remain in the live event stream.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RosTopicTraffic {
    pub topic: String,
    pub message_type: String,
    pub received_messages: u64,
    pub last_received_unix_ms: Option<u64>,
    pub last_message_age_ms: Option<u64>,
}
#[derive(Default)]
struct Receipt {
    count: u64,
    time: Option<Instant>,
    unix_ms: Option<u64>,
}
#[derive(Default)]
struct TrafficTracker(Mutex<HashMap<String, Receipt>>);
impl TrafficTracker {
    #[cfg(any(feature = "ros2", test))]
    fn observe(&self, topic: &str) -> bool {
        let mut receipts = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let receipt = receipts.entry(topic.into()).or_default();
        let first = receipt.count == 0;
        receipt.count = receipt.count.saturating_add(1);
        receipt.time = Some(Instant::now());
        receipt.unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map(|duration| duration.as_millis().min(u128::from(u64::MAX)) as u64);
        first
    }
    fn status(&self, topic: &Topic) -> RosTopicTraffic {
        let receipts = self.0.lock().unwrap_or_else(|error| error.into_inner());
        let receipt = receipts.get(&topic.name);
        RosTopicTraffic {
            topic: topic.name.clone(),
            message_type: topic.message_type.clone(),
            received_messages: receipt.map_or(0, |receipt| receipt.count),
            last_received_unix_ms: receipt.and_then(|receipt| receipt.unix_ms),
            last_message_age_ms: receipt
                .and_then(|receipt| receipt.time)
                .map(|instant| instant.elapsed().as_millis().min(u128::from(u64::MAX)) as u64),
        }
    }
}

type PublishFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

trait JsonPublisher: Send + Sync {
    fn message_type(&self) -> &'static str;
    fn publish(&self, data: Value) -> PublishFuture<'_>;
}
#[cfg(feature = "ros2")]
struct MonitorPublisher<T: RosMessageType> {
    publisher: Publisher<T>,
}
#[cfg(feature = "ros2")]
impl<T: RosMessageType> JsonPublisher for MonitorPublisher<T> {
    fn message_type(&self) -> &'static str {
        T::TYPE_NAME
    }
    fn publish(&self, data: Value) -> PublishFuture<'_> {
        Box::pin(async move {
            let message: T = serde_json::from_value(data)?;
            message.validate().map_err(anyhow::Error::msg)?;
            self.publisher.publish(&message)?;
            Ok(())
        })
    }
}

/// Monitoring/JSON registrations are independent of the generic ROS node.
pub struct RobotRosBL {
    ros: Arc<RobotRos>,
    topics: Vec<Topic>,
    registered_types: HashSet<&'static str>,
    publishers: HashMap<String, Box<dyn JsonPublisher>>,
    messages: broadcast::Sender<RosMessageEvent>,
    traffic: Arc<TrafficTracker>,
    #[cfg(feature = "ros2")]
    subscriptions: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

impl RobotRosBL {
    pub fn new(ros: Arc<RobotRos>, topics: Vec<Topic>) -> Self {
        Self {
            ros,
            topics,
            registered_types: HashSet::new(),
            publishers: HashMap::new(),
            messages: broadcast::channel(256).0,
            traffic: Arc::default(),
            #[cfg(feature = "ros2")]
            subscriptions: Vec::new(),
        }
    }

    /// Register a type only when it must be exposed through the monitor HTTP API.
    pub async fn register<T: RosMessageType>(&mut self) -> Result<()> {
        ensure!(
            self.registered_types.insert(T::TYPE_NAME),
            "Monitor message type is already registered"
        );
        if !self.ros.running() {
            return Ok(());
        }
        #[cfg(feature = "ros2")]
        for topic in self
            .topics
            .iter()
            .filter(|topic| topic.message_type == T::TYPE_NAME)
        {
            let qos = RobotRos::qos(topic.best_effort, 10, topic.transient_local);
            match topic.direction {
                Direction::Publish => {
                    let publisher = self.ros.publisher::<T>(&topic.name, qos)?;
                    self.publishers
                        .insert(topic.name.clone(), Box::new(MonitorPublisher { publisher }));
                }
                Direction::Subscribe => {
                    let messages = self.messages.clone();
                    let traffic = Arc::clone(&self.traffic);
                    let topic_name = topic.name.clone();
                    let subscription: Subscription<T> =
                        self.ros
                            .subscriber::<T>(&topic.name, qos, move |message: T| {
                                if let Err(error) = message.validate() {
                                    eprintln!("Invalid ROS message on {topic_name}: {error}");
                                    return;
                                }
                                match RosMessageEvent::from_message(topic_name.clone(), message) {
                                    Ok(event) => {
                                        if traffic.observe(&topic_name) {
                                            eprintln!(
                                                "ROS received first message: {} [{}]",
                                                topic_name,
                                                T::TYPE_NAME
                                            );
                                        }
                                        let _ = messages.send(event);
                                    }
                                    Err(error) => eprintln!(
                                        "ROS JSON conversion failed on {topic_name}: {error}"
                                    ),
                                }
                            })?;
                    self.subscriptions.push(Box::new(subscription));
                }
            }
        }
        Ok(())
    }

    pub fn validate_monitor_topics(&self) -> Result<()> {
        if self.ros.running() {
            for topic in &self.topics {
                ensure!(
                    self.registered_types.contains(topic.message_type.as_str()),
                    "ROS monitor type is not registered: {}",
                    topic.message_type
                );
            }
        }
        Ok(())
    }

    pub fn status(&self) -> RosStatus {
        let (subscriptions, publishers) = self
            .topics
            .iter()
            .cloned()
            .partition(|topic| topic.direction == Direction::Subscribe);
        let traffic = self
            .topics
            .iter()
            .filter(|topic| topic.direction == Direction::Subscribe)
            .map(|topic| self.traffic.status(topic))
            .collect();
        RosStatus {
            traffic,
            running: self.ros.running(),
            subscriptions,
            publishers,
        }
    }

    pub fn subscribe_messages(&self) -> broadcast::Receiver<RosMessageEvent> {
        self.messages.subscribe()
    }

    pub async fn publish(&self, topic: &str, message_type: &str, data: Value) -> Result<()> {
        ensure!(self.ros.running(), "ROS is disabled or unavailable");
        let publisher = self
            .publishers
            .get(topic)
            .context("ROS monitor publisher is not configured")?;
        ensure!(
            publisher.message_type() == message_type,
            "Message type does not match the configured publisher"
        );
        tokio::time::timeout(Duration::from_secs(5), publisher.publish(data)).await??;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn traffic_counts_receipts_without_caching_payloads() {
        let tracker = TrafficTracker::default();
        let topic = Topic {
            name: "/test/state".into(),
            message_type: "sensor_msgs/msg/JointState".into(),
            direction: Direction::Subscribe,
            best_effort: true,
            transient_local: false,
        };
        assert_eq!(tracker.status(&topic).received_messages, 0);
        assert!(tracker.status(&topic).last_message_age_ms.is_none());
        assert!(tracker.observe(&topic.name));
        assert!(!tracker.observe(&topic.name));
        let status = tracker.status(&topic);
        assert_eq!(status.received_messages, 2);
        assert!(status.last_received_unix_ms.is_some());
        assert!(status.last_message_age_ms.is_some());
    }
}
