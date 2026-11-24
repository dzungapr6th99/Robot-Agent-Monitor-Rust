use anyhow::Result;
use robot_common_lib::{
    Upload, ValidationError,
    urdf_storage::UrdfStorage,
    vla::{VlaError, VlaInference, VlaObservation},
    vla_package::VlaManifest,
};
use robot_crud_business::{
    RobotConfigurationBL, SensorBL, VlaModelBL,
    traits::{RobotConfigurationBLTrait, VlaModelBLTrait},
};
use robot_data_access::{
    RobotConfigurationDA, SensorDA, VlaModelDA,
    db_management::DbManagement,
    traits::VlaModelSchemaTrait,
};
use robot_db_objects::Record;
use robot_third_party_client::traits::VlaRuntimeTrait;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fmt::Write as _,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
use zip::write::SimpleFileOptions;

struct MockRuntime {
    loaded: Mutex<Option<String>>,
}

impl MockRuntime {
    fn new() -> Self {
        Self {
            loaded: Mutex::new(None),
        }
    }
}

impl VlaRuntimeTrait for MockRuntime {
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
    async fn infer(&self, _: &str, _: &VlaObservation) -> Result<VlaInference> {
        Ok(VlaInference {
            actions: vec![],
            done: true,
        })
    }
}

const SAMPLE_URDF: &str = r#"<?xml version="1.0"?>
<robot name="yellowarm">
  <link name="base_link"/>
  <link name="link1"/>
  <link name="link2"/>
  <link name="fixed_tip"/>
  <joint name="joint1" type="revolute">
    <parent link="base_link"/>
    <child link="link1"/>
  </joint>
  <joint name="joint2" type="continuous">
    <parent link="link1"/>
    <child link="link2"/>
  </joint>
  <joint name="tip_fixed" type="fixed">
    <parent link="link2"/>
    <child link="fixed_tip"/>
  </joint>
</robot>"#;

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hex = String::with_capacity(64);
    for b in Sha256::digest(bytes) {
        let _ = write!(&mut hex, "{b:02x}");
    }
    hex
}

fn build_zip(
    path: &Path,
    code: &str,
    version: &str,
    joints: &[&str],
    camera: Option<&str>,
) {
    let model_bytes = b"mock-onnx-content";
    let mut inputs = vec![json!({"name": "state", "kind": "joint_position", "dtype": "float32"})];
    if let Some(cam) = camera {
        inputs.push(json!({
            "name": "image_front",
            "kind": "image",
            "camera": cam,
            "width": 8,
            "height": 8,
            "encoding": "rgb8",
            "layout": "NCHW",
            "scale": "0_1"
        }));
    }
    let manifest = json!({
        "formatVersion": 1,
        "code": code,
        "name": format!("Model {code}"),
        "modelType": "smolvla",
        "version": version,
        "robot": { "jointNames": joints },
        "onnx": { "file": "model.onnx" },
        "inputs": inputs,
        "outputs": [
            { "name": "actions", "kind": "joint_position_chunk", "dtype": "float32" }
        ],
        "files": { "model.onnx": sha256_hex(model_bytes) }
    })
    .to_string();

    let file = File::create(path).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    let opts = SimpleFileOptions::default();
    zip.start_file("manifest.json", opts).unwrap();
    zip.write_all(manifest.as_bytes()).unwrap();
    zip.start_file("model.onnx", opts).unwrap();
    zip.write_all(model_bytes).unwrap();
    zip.finish().unwrap();
}

struct Env {
    root: PathBuf,
    config_bl: Arc<RobotConfigurationBL>,
    model_bl: VlaModelBL<MockRuntime>,
    runtime: Arc<MockRuntime>,
    active_model_id: Arc<Mutex<Option<i64>>>,
    config_id: i64,
}

fn setup(name: &str) -> Env {
    let root = std::env::temp_dir().join(format!(
        "vla-model-bl-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&root).unwrap();
    let db_path = root.join("monitor.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE robot_profiles (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL UNIQUE);
         CREATE TABLE robot_configurations (id INTEGER PRIMARY KEY AUTOINCREMENT, robot_profile_id INTEGER NOT NULL, name TEXT NOT NULL, urdf_path TEXT NOT NULL);
         CREATE TABLE joint_interfaces (id INTEGER PRIMARY KEY AUTOINCREMENT, robot_configuration_id INTEGER NOT NULL, name TEXT NOT NULL);
         CREATE TABLE sensors (id INTEGER PRIMARY KEY AUTOINCREMENT, robot_configuration_id INTEGER NOT NULL, code TEXT, display_name TEXT, sensor_type TEXT, is_enabled INTEGER);
         CREATE TABLE sensor_streams (id INTEGER PRIMARY KEY AUTOINCREMENT, sensor_id INTEGER NOT NULL, role TEXT, topic TEXT, data_type TEXT, is_enabled INTEGER);
         INSERT INTO robot_profiles (id, name) VALUES (1, 'YellowArm');",
    )
    .unwrap();
    drop(conn);

    let db = Arc::new(DbManagement(db_path.clone()));
    let vla_da = Arc::new(VlaModelDA::new(Arc::clone(&db)));
    vla_da.ensure_table().unwrap();

    let config_bl = Arc::new(RobotConfigurationBL {
        data_access: Arc::new(RobotConfigurationDA::new(Arc::clone(&db))),
        vla_models: Arc::clone(&vla_da),
        max_urdf_bytes: 1024 * 1024,
        urdf_storage: UrdfStorage(root.clone()),
    });

    let config_id = config_bl
        .insert(
            Record::from_iter([
                ("robotProfileId".into(), json!(1)),
                ("name".into(), json!("YellowArm v1")),
            ]),
            Some(Upload {
                filename: "yellowarm.urdf".into(),
                bytes: SAMPLE_URDF.as_bytes().to_vec(),
                assets: vec![],
            }),
        )
        .unwrap();

    // Add enabled camera sensor "front" with Image stream.
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO sensors (id, robot_configuration_id, code, display_name, sensor_type, is_enabled) VALUES (1, ?1, 'front', 'Front Camera', 'camera', 1)",
        [config_id],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO sensor_streams (id, sensor_id, role, topic, data_type, is_enabled) VALUES (1, 1, 'rgb', '/cam/front/image_raw', 'sensor_msgs/msg/Image', 1)",
        [],
    )
    .unwrap();
    drop(conn);

    let sensors = Arc::new(SensorBL::new(Arc::new(SensorDA::new(Arc::clone(&db)))));
    let runtime = Arc::new(MockRuntime::new());
    let active_model_id: Arc<Mutex<Option<i64>>> = Arc::default();
    let model_bl = VlaModelBL {
        data_access: vla_da,
        robot_configurations: Arc::clone(&config_bl),
        sensors,
        runtime: Arc::clone(&runtime),
        data_root: root.clone(),
        max_unpacked_bytes: 10 * 1024 * 1024,
        active_model_id: Arc::clone(&active_model_id),
    };

    Env {
        root,
        config_bl,
        model_bl,
        runtime,
        active_model_id,
        config_id,
    }
}

#[test]
fn install_search_update_in_use_and_urdf_delete_guard() {
    let env = setup("lifecycle");
    let zip_path = env.root.join("good.zip");
    build_zip(&zip_path, "yellowarm-smolvla", "1.0.0", &["joint1", "joint2"], Some("front"));

    // 1. Install valid package.
    let model_id = env.model_bl.install(env.config_id, &zip_path).unwrap();
    assert!(model_id > 0);
    let model = env.model_bl.get(model_id).unwrap().unwrap();
    assert_eq!(model.code.as_deref(), Some("yellowarm-smolvla"));
    assert!(env.root.join(model.storage_path.as_deref().unwrap()).is_dir());

    // 2. Duplicate version is rejected.
    let dup = env.model_bl.install(env.config_id, &zip_path).unwrap_err();
    assert_eq!(dup.downcast_ref::<VlaError>(), Some(&VlaError::Duplicate));

    // 3. Search and update metadata.
    assert_eq!(
        env.model_bl.list(Some(env.config_id), Some("1.0.0")).unwrap().len(),
        1
    );
    let updated = env
        .model_bl
        .update(model_id, Some("Renamed VLA".into()), Some("Pick cube".into()))
        .unwrap();
    assert_eq!(updated.display_name.as_deref(), Some("Renamed VLA"));
    assert_eq!(updated.description.as_deref(), Some("Pick cube"));

    // 4. Deleting active/running model returns InUse.
    *env.active_model_id.lock().unwrap() = Some(model_id);
    let err = env.model_bl.delete(model_id).unwrap_err();
    assert_eq!(err.downcast_ref::<VlaError>(), Some(&VlaError::InUse));
    *env.active_model_id.lock().unwrap() = None;

    // 5. Deleting URDF while VLA model exists is blocked with ValidationError.
    let del_err = env
        .config_bl
        .delete(Record::from_iter([("id".into(), json!(env.config_id))]))
        .unwrap_err();
    let val_err = del_err.downcast_ref::<ValidationError>().unwrap();
    assert!(val_err.1.contains("Xóa hết 1 model VLA"));

    // 6. Delete model, then delete URDF -> both DB and disk directories cleaned up.
    assert!(env.model_bl.delete(model_id).unwrap());
    assert!(!env.root.join(model.storage_path.as_deref().unwrap()).exists());

    let urdf_dir = env.root.join(format!("robots/1/{}", env.config_id));
    assert!(urdf_dir.is_dir());
    let deleted = env
        .config_bl
        .delete(Record::from_iter([("id".into(), json!(env.config_id))]))
        .unwrap();
    assert_eq!(deleted, 1);
    assert!(!urdf_dir.exists());

    let _ = fs::remove_dir_all(env.root);
}

#[test]
fn rejects_unknown_joint_or_missing_camera_without_leaving_staging_junk() {
    let env = setup("validation");

    // Unknown or fixed joint rejected.
    let bad_joint = env.root.join("bad-joint.zip");
    build_zip(&bad_joint, "arm-bad-joint", "1.0.0", &["joint1", "tip_fixed"], Some("front"));
    let err = env.model_bl.install(env.config_id, &bad_joint).unwrap_err();
    assert_eq!(err.downcast_ref::<VlaError>(), Some(&VlaError::InvalidPackage));

    // Missing camera sensor rejected.
    let bad_cam = env.root.join("bad-cam.zip");
    build_zip(&bad_cam, "arm-bad-cam", "1.0.0", &["joint1"], Some("wrist_missing"));
    let err = env.model_bl.install(env.config_id, &bad_cam).unwrap_err();
    assert_eq!(err.downcast_ref::<VlaError>(), Some(&VlaError::InvalidPackage));

    // Verify staging directory has no leftover subdirectories.
    let staging = env.root.join("vla/.staging");
    if staging.exists() {
        assert_eq!(fs::read_dir(staging).unwrap().count(), 0);
    }
    assert!(env.runtime.loaded().is_none());

    let _ = fs::remove_dir_all(env.root);
}
