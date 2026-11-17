use anyhow::Result;
use robot_common_lib::{
    ros::{Direction, Topic},
    vla::*,
};
use robot_crud_business::{
    robot_ros_bl::RosStatus,
    traits::{RobotRosBLTrait, VlaBLTrait},
    vla_bl::VlaBL,
};
use robot_ros_bridge::{RosMessageEvent, RosMessageType};
use robot_third_party_client::traits::VlaClientTrait;
use std::{
    sync::{
        Arc,
        atomic::{AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, broadcast};

struct Client {
    pending: bool,
    invalid: bool,
    entered: Arc<Notify>,
}
impl VlaClientTrait for Client {
    async fn infer(&self, _: &str, _: &str, observation: &VlaJoints) -> Result<VlaInference> {
        self.entered.notify_one();
        if self.pending {
            return std::future::pending().await;
        }
        Ok(VlaInference {
            actions: (0..4)
                .map(|_| VlaJoints {
                    name: if self.invalid {
                        vec!["wrong_joint".into()]
                    } else {
                        observation.name.clone()
                    },
                    position: vec![0.25],
                })
                .collect(),
            done: true,
        })
    }
}
struct Ros {
    events: broadcast::Sender<RosMessageEvent>,
    count: AtomicU32,
    moved: Notify,
}
impl RobotRosBLTrait for Ros {
    async fn register<T: RosMessageType>(&mut self) -> Result<()> {
        Ok(())
    }
    fn validate_monitor_topics(&self) -> Result<()> {
        Ok(())
    }
    fn status(&self) -> RosStatus {
        let topic = |name: &str, direction| Topic {
            name: name.into(),
            message_type: "sensor_msgs/msg/JointState".into(),
            direction,
            best_effort: false,
            transient_local: false,
        };
        RosStatus {
            running: true,
            subscriptions: vec![topic("/state", Direction::Subscribe)],
            publishers: vec![topic("/command", Direction::Publish)],
            traffic: vec![],
        }
    }
    fn subscribe_messages(&self) -> broadcast::Receiver<RosMessageEvent> {
        self.events.subscribe()
    }
    async fn publish(&self, topic: &str, kind: &str, data: serde_json::Value) -> Result<()> {
        assert_eq!(topic, "/command");
        assert_eq!(kind, "sensor_msgs/msg/JointState");
        assert_eq!(
            data,
            serde_json::json!({"name":["joint"],"position":[0.25]})
        );
        self.count.fetch_add(1, Ordering::SeqCst);
        self.moved.notify_one();
        Ok(())
    }
}
fn fixture(
    pending: bool,
    invalid: bool,
    max_steps: u32,
    timeout_ms: u64,
) -> (
    Arc<VlaBL<Client, Ros>>,
    Arc<Ros>,
    Arc<Notify>,
    tokio::task::JoinHandle<()>,
) {
    let ros = Arc::new(Ros {
        events: broadcast::channel(16).0,
        count: AtomicU32::new(0),
        moved: Notify::new(),
    });
    let entered = Arc::new(Notify::new());
    let bl = Arc::new(
        VlaBL::new(
            Client {
                pending,
                invalid,
                entered: entered.clone(),
            },
            ros.clone(),
            vec![VlaProfile {
                id: "arm".into(),
                model: "smolvla".into(),
                version: "v1".into(),
            }],
            VlaOptions {
                command_topic: "/command".into(),
                state_topic: "/state".into(),
                max_steps,
                timeout_ms,
                step_period_ms: 20,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let sender = ros.events.clone();
    let pump = tokio::spawn(async move {
        loop {
            tokio::time::sleep(Duration::from_millis(2)).await;
            let _ = sender.send(RosMessageEvent {
                topic: "/state".into(),
                message_type: "sensor_msgs/msg/JointState".into(),
                data: serde_json::json!({"name":["joint"],"position":[0.0]}),
            });
        }
    });
    (bl, ros, entered, pump)
}

#[tokio::test]
async fn invalid_requests_and_unknown_profiles_do_not_move() {
    let (bl, ros, _, pump) = fixture(false, false, 10, 1000);
    assert_eq!(
        bl.run("missing", "move").await.unwrap_err().downcast_ref(),
        Some(&VlaError::ProfileNotFound)
    );
    for task in [" ".to_owned(), "x".repeat(2049)] {
        assert_eq!(
            bl.run("arm", &task).await.unwrap_err().downcast_ref(),
            Some(&VlaError::InvalidRequest)
        );
    }
    assert_eq!(ros.count.load(Ordering::SeqCst), 0);
    assert!(!bl.stop());
    pump.abort();
}
#[tokio::test]
async fn done_returns_actual_published_steps() {
    let (bl, ros, _, pump) = fixture(false, false, 10, 1000);
    assert_eq!(bl.run("arm", "move").await.unwrap(), 4);
    assert_eq!(ros.count.load(Ordering::SeqCst), 4);
    assert!(!bl.stop());
    pump.abort();
}
#[tokio::test]
async fn caps_publishing_at_max_steps() {
    let (bl, ros, _, pump) = fixture(false, false, 2, 1000);
    assert_eq!(bl.run("arm", "move").await.unwrap(), 2);
    assert_eq!(ros.count.load(Ordering::SeqCst), 2);
    pump.abort();
}
#[tokio::test]
async fn busy_and_stop_cancel_pending_inference() {
    let (bl, ros, entered, pump) = fixture(true, false, 10, 1000);
    let worker = bl.clone();
    let run = tokio::spawn(async move { worker.run("arm", "move").await });
    entered.notified().await;
    assert_eq!(
        bl.run("arm", "move again")
            .await
            .unwrap_err()
            .downcast_ref(),
        Some(&VlaError::Busy)
    );
    assert!(bl.stop());
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(100), run)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        0
    );
    assert_eq!(ros.count.load(Ordering::SeqCst), 0);
    assert!(!bl.stop());
    pump.abort();
}
#[tokio::test]
async fn stop_prevents_further_steps_and_releases_run() {
    let (bl, ros, _, pump) = fixture(false, false, 10, 1000);
    let worker = bl.clone();
    let run = tokio::spawn(async move { worker.run("arm", "move").await });
    ros.moved.notified().await;
    assert!(bl.stop());
    assert_eq!(run.await.unwrap().unwrap(), 1);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(ros.count.load(Ordering::SeqCst), 1);
    assert_eq!(bl.run("arm", "again").await.unwrap(), 4);
    pump.abort();
}
#[tokio::test]
async fn rejects_unknown_action_joint_before_publishing() {
    let (bl, ros, _, pump) = fixture(false, true, 10, 1000);
    assert_eq!(
        bl.run("arm", "move").await.unwrap_err().downcast_ref(),
        Some(&VlaError::Unavailable)
    );
    assert_eq!(ros.count.load(Ordering::SeqCst), 0);
    pump.abort();
}
#[tokio::test]
async fn deadline_and_dropped_request_release_exclusivity() {
    let (bl, _, entered, pump) = fixture(true, false, 10, 50);
    assert_eq!(
        bl.run("arm", "move").await.unwrap_err().downcast_ref(),
        Some(&VlaError::Timeout)
    );
    assert!(!bl.stop());
    let worker = bl.clone();
    let run = tokio::spawn(async move { worker.run("arm", "move").await });
    entered.notified().await;
    run.abort();
    let _ = run.await;
    assert!(!bl.stop());
    pump.abort();
}
