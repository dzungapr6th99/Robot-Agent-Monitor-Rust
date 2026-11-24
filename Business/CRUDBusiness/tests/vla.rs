use anyhow::Result;
use robot_common_lib::{
    ros::{Direction, Topic},
    vla::*,
    vla_package::VlaManifest,
};
use robot_crud_business::{
    SensorBL,
    robot_ros_bl::RosStatus,
    traits::{RobotRosBLTrait, VlaBLTrait, VlaCameraFeedTrait},
    vla_bl::VlaBL,
};
use robot_data_access::{
    SensorDA, VlaModelDA,
    db_management::DbManagement,
    traits::{BaseDATrait, VlaModelSchemaTrait},
};
use robot_db_objects::Record;
use robot_ros_bridge::{RosMessageEvent, RosMessageType};
use robot_third_party_client::traits::VlaRuntimeTrait;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Notify, broadcast};

struct FakeRuntime {
    pending: bool,
    invalid: bool,
    entered: Arc<Notify>,
    loaded: Mutex<Option<String>>,
    calls: Arc<AtomicU32>,
    repeat: bool,
    single_step: bool,
}

impl FakeRuntime {
    fn new(pending: bool, invalid: bool, entered: Arc<Notify>) -> Self {
        Self {
            pending,
            invalid,
            entered,
            loaded: Mutex::new(None),
            calls: Arc::new(AtomicU32::new(0)),
            repeat: false,
            single_step: false,
        }
    }
}

impl VlaRuntimeTrait for FakeRuntime {
    fn load(&self, _: &Path, _: &VlaManifest, sha: &str) -> Result<bool> {
        *self.loaded.lock().unwrap() = Some(sha.to_owned());
        Ok(true)
    }

    fn unload(&self) -> bool {
        self.loaded.lock().unwrap().take().is_some()
    }

    fn loaded(&self) -> Option<String> {
        self.loaded.lock().unwrap().clone()
    }

    fn providers(&self) -> Vec<String> {
        vec!["cpu".into()]
    }

    async fn infer(&self, _: &str, _observation: &VlaObservation) -> Result<VlaInference> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.entered.notify_one();
        if self.pending {
            return std::future::pending().await;
        }
        let steps = if self.single_step || self.repeat { 1 } else { 4 };
        Ok(VlaInference {
            actions: (0..steps)
                .map(|_| VlaJoints {
                    name: if self.invalid {
                        vec!["wrong_joint".into()]
                    } else {
                        vec!["joint".into()]
                    },
                    position: vec![0.25],
                })
                .collect(),
            done: !self.repeat,
        })
    }
}

struct FakeCamera {
    stale: AtomicBool,
}

impl FakeCamera {
    fn fresh() -> Self {
        Self {
            stale: AtomicBool::new(false),
        }
    }
}

impl VlaCameraFeedTrait for FakeCamera {
    fn start(&self, _: &[(String, String)]) -> Result<bool> {
        Ok(true)
    }

    fn latest(&self, camera: &str, _: Duration) -> Result<VlaImage> {
        if self.stale.load(Ordering::SeqCst) {
            return Err(vla_err(
                VlaError::Unavailable,
                format!("Camera '{camera}' frame is stale"),
            ));
        }
        Ok(VlaImage {
            width: 2,
            height: 2,
            encoding: "rgb8".into(),
            data: vec![128u8; 2 * 2 * 3],
        })
    }

    fn stop(&self) -> bool {
        true
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

fn setup_db(name: &str, with_camera: bool) -> (PathBuf, Arc<VlaModelDA>, Arc<SensorBL>, i64) {
    let dir = std::env::temp_dir().join(format!(
        "vla-bl-test-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&dir).unwrap();
    let db_path = dir.join("test.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE robot_profiles (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE);
         CREATE TABLE robot_configurations (id INTEGER PRIMARY KEY AUTOINCREMENT, robot_profile_id INTEGER NOT NULL, name TEXT NOT NULL, urdf_path TEXT NOT NULL);
         CREATE TABLE sensors (id INTEGER PRIMARY KEY AUTOINCREMENT, robot_configuration_id INTEGER NOT NULL, code TEXT, display_name TEXT, sensor_type TEXT, is_enabled INTEGER);
         CREATE TABLE sensor_streams (id INTEGER PRIMARY KEY AUTOINCREMENT, sensor_id INTEGER NOT NULL, role TEXT, topic TEXT, data_type TEXT, is_enabled INTEGER);
         INSERT INTO robot_profiles (id, name) VALUES (1, 'YellowArm');
         INSERT INTO robot_configurations (id, robot_profile_id, name, urdf_path) VALUES (1, 1, 'cfg1', 'robots/1/1/arm.urdf');
         INSERT INTO robot_configurations (id, robot_profile_id, name, urdf_path) VALUES (2, 1, 'cfg2', 'robots/1/2/arm.urdf');",
    )
    .unwrap();
    if with_camera {
        conn.execute_batch(
            "INSERT INTO sensors (id, robot_configuration_id, code, display_name, sensor_type, is_enabled) VALUES (1, 1, 'front', 'Front Cam', 'camera', 1);
             INSERT INTO sensor_streams (id, sensor_id, role, topic, data_type, is_enabled) VALUES (1, 1, 'rgb', '/cam/front/image_raw', 'sensor_msgs/msg/Image', 1);",
        )
        .unwrap();
    }
    drop(conn);

    let db = Arc::new(DbManagement(db_path));
    let vla_da = Arc::new(VlaModelDA::new(Arc::clone(&db)));
    vla_da.ensure_table().unwrap();
    let sensors = Arc::new(SensorBL::new(Arc::new(SensorDA::new(Arc::clone(&db)))));

    let mut inputs = vec![json!({"name": "state", "kind": "joint_position", "dtype": "float32"})];
    if with_camera {
        inputs.push(json!({
            "name": "image_front",
            "kind": "image",
            "camera": "front",
            "width": 2,
            "height": 2,
            "encoding": "rgb8",
            "layout": "NCHW",
            "scale": "0_1"
        }));
    }
    let manifest = json!({
        "formatVersion": 1,
        "code": "arm-vla",
        "name": "Arm VLA",
        "modelType": "smolvla",
        "version": "v1",
        "robot": { "jointNames": ["joint"] },
        "onnx": { "file": "model.onnx" },
        "inputs": inputs,
        "outputs": [
            { "name": "actions", "kind": "joint_position_chunk", "dtype": "float32" },
            { "name": "done", "kind": "done_flag", "optional": true }
        ],
        "files": {}
    })
    .to_string();

    let model_id = vla_da
        .insert(&Record::from_iter([
            ("robotConfigurationId".into(), json!(1)),
            ("code".into(), json!("arm-vla")),
            ("displayName".into(), json!("Arm VLA")),
            ("modelType".into(), json!("smolvla")),
            ("version".into(), json!("v1")),
            ("description".into(), json!("")),
            ("formatVersion".into(), json!(1)),
            ("packageSha256".into(), json!("a".repeat(64))),
            ("packageBytes".into(), json!(100)),
            ("storagePath".into(), json!("vla/1/1")),
            ("manifestJson".into(), json!(manifest)),
        ]))
        .unwrap();

    (dir, vla_da, sensors, model_id)
}

type Fixture = (
    Arc<VlaBL<FakeRuntime, Ros, FakeCamera>>,
    Arc<Ros>,
    Arc<FakeCamera>,
    Arc<Notify>,
    i64,
    tokio::task::JoinHandle<()>,
    PathBuf,
);

fn fixture(
    name: &str,
    runtime: FakeRuntime,
    max_steps: u32,
    timeout_ms: u64,
    step_period_ms: u64,
    with_camera: bool,
) -> Fixture {
    let (dir, vla_da, sensors, model_id) = setup_db(name, with_camera);
    let ros = Arc::new(Ros {
        events: broadcast::channel(16).0,
        count: AtomicU32::new(0),
        moved: Notify::new(),
    });
    let entered = Arc::clone(&runtime.entered);
    let camera = Arc::new(FakeCamera::fresh());
    let bl = Arc::new(
        VlaBL::new(
            Arc::new(runtime),
            ros.clone(),
            camera.clone(),
            vla_da,
            sensors,
            dir.clone(),
            VlaOptions {
                command_topic: "/command".into(),
                state_topic: "/state".into(),
                max_steps,
                timeout_ms,
                step_period_ms,
                image_max_age_ms: 500,
                configuration_id: None,
            },
            Arc::default(),
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
    (bl, ros, camera, entered, model_id, pump, dir)
}

#[tokio::test]
async fn invalid_requests_unknown_models_and_wrong_robot_do_not_move() {
    let entered = Arc::new(Notify::new());
    let (bl, ros, _, _, model_id, pump, dir) =
        fixture("invalid", FakeRuntime::new(false, false, entered), 10, 1000, 20, false);
    assert_eq!(
        bl.run(1, 9999, "move").await.unwrap_err().downcast_ref(),
        Some(&VlaError::ModelNotFound)
    );
    assert_eq!(
        bl.run(2, model_id, "move")
            .await
            .unwrap_err()
            .downcast_ref(),
        Some(&VlaError::WrongRobot)
    );
    for task in [" ".to_owned(), "x".repeat(2049)] {
        assert_eq!(
            bl.run(1, model_id, &task)
                .await
                .unwrap_err()
                .downcast_ref(),
            Some(&VlaError::InvalidRequest)
        );
    }
    assert_eq!(ros.count.load(Ordering::SeqCst), 0);
    assert!(!bl.stop());
    pump.abort();
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn done_returns_actual_published_steps_and_updates_status() {
    let entered = Arc::new(Notify::new());
    let (bl, ros, _, _, model_id, pump, dir) =
        fixture("done", FakeRuntime::new(false, false, entered), 10, 1000, 20, true);
    assert_eq!(bl.run(1, model_id, "move").await.unwrap(), 4);
    assert_eq!(ros.count.load(Ordering::SeqCst), 4);
    let status = bl.status();
    assert_eq!(status.loaded_model_id, Some(model_id));
    assert_eq!(status.configuration_id, Some(1));
    assert!(!status.running);
    assert!(!bl.stop());
    pump.abort();
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn stale_camera_frame_is_rejected_with_unavailable() {
    let entered = Arc::new(Notify::new());
    let (bl, ros, camera, _, model_id, pump, dir) =
        fixture("stale", FakeRuntime::new(false, false, entered), 10, 1000, 20, true);
    camera.stale.store(true, Ordering::SeqCst);
    assert_eq!(
        bl.run(1, model_id, "move")
            .await
            .unwrap_err()
            .downcast_ref(),
        Some(&VlaError::Unavailable)
    );
    assert_eq!(ros.count.load(Ordering::SeqCst), 0);
    pump.abort();
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn final_action_returns_success_without_waiting_past_deadline() {
    let entered = Arc::new(Notify::new());
    let mut rt = FakeRuntime::new(false, false, entered);
    rt.single_step = true;
    let (bl, ros, _, _, model_id, pump, dir) = fixture("final", rt, 10, 500, 1000, false);
    let result = bl.run(1, model_id, "move").await;
    pump.abort();
    assert_eq!(ros.count.load(Ordering::SeqCst), 1);
    assert_eq!(result.unwrap(), 1);
    assert!(!bl.stop());
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn caps_publishing_at_max_steps() {
    let entered = Arc::new(Notify::new());
    let (bl, ros, _, _, model_id, pump, dir) =
        fixture("cap", FakeRuntime::new(false, false, entered), 2, 1000, 20, false);
    assert_eq!(bl.run(1, model_id, "move").await.unwrap(), 2);
    assert_eq!(ros.count.load(Ordering::SeqCst), 2);
    pump.abort();
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn unfinished_inference_batches_repeat_until_total_step_limit() {
    let entered = Arc::new(Notify::new());
    let mut rt = FakeRuntime::new(false, false, entered);
    rt.repeat = true;
    let calls = Arc::clone(&rt.calls);
    let (bl, ros, _, _, model_id, pump, dir) = fixture("repeat", rt, 3, 1000, 1, false);
    assert_eq!(bl.run(1, model_id, "move").await.unwrap(), 3);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(ros.count.load(Ordering::SeqCst), 3);
    assert!(!bl.stop());
    pump.abort();
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn busy_and_stop_cancel_pending_inference() {
    let entered = Arc::new(Notify::new());
    let (bl, ros, _, entered, model_id, pump, dir) =
        fixture("busy", FakeRuntime::new(true, false, entered), 10, 1000, 20, false);
    let worker = bl.clone();
    let run = tokio::spawn(async move { worker.run(1, model_id, "move").await });
    entered.notified().await;
    assert_eq!(
        bl.run(1, model_id, "move again")
            .await
            .unwrap_err()
            .downcast_ref(),
        Some(&VlaError::Busy)
    );
    assert!(bl.stop());
    assert_eq!(
        tokio::time::timeout(Duration::from_millis(200), run)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        0
    );
    assert_eq!(ros.count.load(Ordering::SeqCst), 0);
    assert!(!bl.stop());
    pump.abort();
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn stop_prevents_further_steps_and_releases_run() {
    let entered = Arc::new(Notify::new());
    let (bl, ros, _, _, model_id, pump, dir) =
        fixture("stop", FakeRuntime::new(false, false, entered), 10, 1000, 20, false);
    let worker = bl.clone();
    let run = tokio::spawn(async move { worker.run(1, model_id, "move").await });
    ros.moved.notified().await;
    assert!(bl.stop());
    assert_eq!(run.await.unwrap().unwrap(), 1);
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(ros.count.load(Ordering::SeqCst), 1);
    assert_eq!(bl.run(1, model_id, "again").await.unwrap(), 4);
    pump.abort();
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn rejects_unknown_action_joint_before_publishing() {
    let entered = Arc::new(Notify::new());
    let (bl, ros, _, _, model_id, pump, dir) =
        fixture("badjoint", FakeRuntime::new(false, true, entered), 10, 1000, 20, false);
    assert_eq!(
        bl.run(1, model_id, "move")
            .await
            .unwrap_err()
            .downcast_ref(),
        Some(&VlaError::Unavailable)
    );
    assert_eq!(ros.count.load(Ordering::SeqCst), 0);
    pump.abort();
    let _ = fs::remove_dir_all(dir);
}

#[tokio::test]
async fn deadline_and_dropped_request_release_exclusivity() {
    let entered = Arc::new(Notify::new());
    let (bl, _, _, entered, model_id, pump, dir) =
        fixture("timeout", FakeRuntime::new(true, false, entered), 10, 50, 20, false);
    assert_eq!(
        bl.run(1, model_id, "move")
            .await
            .unwrap_err()
            .downcast_ref(),
        Some(&VlaError::Timeout)
    );
    assert!(!bl.stop());
    entered.notified().await;
    let worker = bl.clone();
    let run = tokio::spawn(async move { worker.run(1, model_id, "move").await });
    entered.notified().await;
    run.abort();
    let _ = run.await;
    assert!(!bl.stop());
    pump.abort();
    let _ = fs::remove_dir_all(dir);
}
