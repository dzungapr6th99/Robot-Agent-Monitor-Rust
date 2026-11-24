use crate::{
    RobotConfigurationBL, SensorBL,
    robot_configuration_bl::extract_non_fixed_joints,
    traits::{RobotConfigurationBLTrait, VlaModelBLTrait},
    vla_camera::resolve_camera_topics,
};
use anyhow::{Context, Result};
use robot_common_lib::{
    vla::{VlaError, vla_err},
    vla_package,
};
use robot_data_access::{
    VlaModelDA,
    dapper_extensions::DapperExtensions,
    traits::{BaseDATrait, DbManagementTrait, VlaModelSchemaTrait},
};
use robot_db_objects::{Record, VlaModel};
use robot_third_party_client::traits::VlaRuntimeTrait;
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

pub struct VlaModelBL<Rt> {
    pub data_access: Arc<VlaModelDA>,
    pub robot_configurations: Arc<RobotConfigurationBL>,
    pub sensors: Arc<SensorBL>,
    pub runtime: Arc<Rt>,
    pub data_root: PathBuf,
    pub max_unpacked_bytes: u64,
    pub active_model_id: Arc<Mutex<Option<i64>>>,
}

impl<Rt: VlaRuntimeTrait> VlaModelBL<Rt> {
    fn staging_root(&self) -> PathBuf {
        self.data_root.join("vla").join(".staging")
    }

    fn remove_model_dir(&self, storage_path: &str) {
        let dir = self.data_root.join(storage_path);
        if let (Ok(target), Ok(root)) = (dir.canonicalize(), self.data_root.canonicalize())
            && target.starts_with(&root)
            && target != root
        {
            let _ = fs::remove_dir_all(target);
        }
    }
}

impl<Rt: VlaRuntimeTrait> VlaModelBLTrait for VlaModelBL<Rt> {
    fn ensure_table(&self) -> Result<bool> {
        robot_common_lib::logged_result!("vla_model_ensure_table", {
            self.data_access.ensure_table()
        })
    }

    fn cleanup_staging(&self) -> Result<usize> {
        robot_common_lib::logged_result!("vla_model_cleanup_staging", {
            let root = self.staging_root();
            if !root.exists() {
                return Ok(0);
            }
            let mut removed = 0usize;
            for entry in fs::read_dir(&root)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    let _ = fs::remove_dir_all(&path);
                } else {
                    let _ = fs::remove_file(&path);
                }
                removed += 1;
            }
            Ok(removed)
        })
    }

    fn install(&self, configuration_id: i64, zip_path: &Path) -> Result<i64> {
        robot_common_lib::logged_result!("vla_model_install", {
            if configuration_id <= 0 {
                return Err(vla_err(
                    VlaError::InvalidRequest,
                    "robotConfigurationId must be positive",
                ));
            }
            let configuration = self
                .robot_configurations
                .query(&[configuration_id])?
                .into_iter()
                .next()
                .ok_or_else(|| {
                    vla_err(
                        VlaError::InvalidPackage,
                        format!("URDF configuration #{configuration_id} not found"),
                    )
                })?;
            let urdf_path = configuration.urdf_path.ok_or_else(|| {
                vla_err(
                    VlaError::InvalidPackage,
                    format!("URDF configuration #{configuration_id} has no stored URDF file"),
                )
            })?;

            let staging = self.staging_root().join(uuid::Uuid::new_v4().to_string());
            let result = (|| -> Result<i64> {
                // 1 & 2. Extract into staging and validate manifest & file sha256.
                let extracted =
                    vla_package::extract(zip_path, &staging, self.max_unpacked_bytes)
                        .map_err(|err| vla_err(VlaError::InvalidPackage, err.to_string()))?;

                // 3. Validate jointNames against non-fixed joints of the target URDF (G3).
                let urdf_xml = self
                    .robot_configurations
                    .urdf_storage
                    .read_urdf_text(&urdf_path)
                    .map_err(|err| {
                        vla_err(
                            VlaError::InvalidPackage,
                            format!("Cannot read URDF file for configuration #{configuration_id}: {err}"),
                        )
                    })?;
                let urdf_joints = extract_non_fixed_joints(&urdf_xml)
                    .map_err(|err| vla_err(VlaError::InvalidPackage, err.to_string()))?;
                for joint in &extracted.manifest.robot.joint_names {
                    if !urdf_joints.contains(joint) {
                        return Err(vla_err(
                            VlaError::InvalidPackage,
                            format!(
                                "Joint '{joint}' in VLA manifest is not a non-fixed joint in URDF #{configuration_id}"
                            ),
                        ));
                    }
                }

                // 4. Validate cameras declared in manifest exist in enabled sensors/streams (G3).
                let cameras = extracted.manifest.required_cameras();
                resolve_camera_topics(&self.sensors, configuration_id, &cameras)?;

                // 5. Probe ONNX runtime load on staging if runtime is built and idle.
                if self.runtime.is_available() {
                    let busy = self
                        .active_model_id
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .is_some();
                    if !busy && self.runtime.loaded().is_none() {
                        self.runtime
                            .load(&staging, &extracted.manifest, &extracted.package_sha256)
                            .map_err(|err| {
                                if err.downcast_ref::<VlaError>().is_some() {
                                    err
                                } else {
                                    vla_err(VlaError::InvalidPackage, err.to_string())
                                }
                            })?;
                        self.runtime.unload();
                    }
                }

                // 6, 7 & 8. Insert in transaction, move staging -> final dir, update storage_path, commit.
                let mut db = self.data_access.db_management.get_connection(true)?;
                let tx = db.transaction()?;
                if self.data_access.exists_version(
                    &tx,
                    configuration_id,
                    &extracted.manifest.code,
                    &extracted.manifest.version,
                )? {
                    return Err(vla_err(
                        VlaError::Duplicate,
                        format!(
                            "Model '{}@{}' already exists for URDF #{configuration_id}",
                            extracted.manifest.code, extracted.manifest.version
                        ),
                    ));
                }

                let record = Record::from_iter([
                    ("robotConfigurationId".into(), json!(configuration_id)),
                    ("code".into(), json!(extracted.manifest.code)),
                    ("displayName".into(), json!(extracted.manifest.name)),
                    ("modelType".into(), json!(extracted.manifest.model_type)),
                    ("version".into(), json!(extracted.manifest.version)),
                    ("description".into(), json!("")),
                    (
                        "formatVersion".into(),
                        json!(i64::from(extracted.manifest.format_version)),
                    ),
                    ("packageSha256".into(), json!(extracted.package_sha256)),
                    ("packageBytes".into(), json!(extracted.package_bytes)),
                    ("storagePath".into(), json!("pending")),
                    ("manifestJson".into(), json!(extracted.manifest_json)),
                ]);
                let id = tx.insert::<VlaModel>(&record)?;
                let relative = format!("vla/{configuration_id}/{id}");
                let target = self.data_root.join(&relative);
                let parent = target.parent().context("Missing target parent")?;
                fs::create_dir_all(parent)?;
                fs::rename(&staging, &target).with_context(|| {
                    format!("Failed to move staged model to {}", target.display())
                })?;

                let commit_result = (|| -> Result<()> {
                    self.data_access.update_storage_path(&tx, id, &relative)?;
                    tx.commit()?;
                    Ok(())
                })();
                if commit_result.is_err() {
                    let _ = fs::remove_dir_all(&target);
                }
                commit_result?;
                robot_common_lib::operation_log::info(
                    module_path!(),
                    format_args!(
                        "Installed VLA model #{id} for configuration #{configuration_id}"
                    ),
                );
                Ok(id)
            })();

            if result.is_err() && staging.exists() {
                let _ = fs::remove_dir_all(&staging);
            }
            result
        })
    }

    fn list(&self, configuration_id: Option<i64>, query: Option<&str>) -> Result<Vec<VlaModel>> {
        robot_common_lib::logged_result!("vla_model_list", {
            self.data_access.search(configuration_id, query)
        })
    }

    fn get(&self, id: i64) -> Result<Option<VlaModel>> {
        robot_common_lib::logged_result!("vla_model_get", {
            Ok(self.data_access.query(&[id])?.into_iter().next())
        })
    }

    fn update(
        &self,
        id: i64,
        display_name: Option<String>,
        description: Option<String>,
    ) -> Result<VlaModel> {
        robot_common_lib::logged_result!("vla_model_update", {
            let existing = self
                .get(id)?
                .ok_or_else(|| vla_err(VlaError::ModelNotFound, format!("VLA model #{id} not found")))?;
            let mut record = Record::new();
            record.insert("id".into(), json!(id));
            if let Some(name) = display_name {
                let trimmed = name.trim();
                if trimmed.is_empty() {
                    return Err(vla_err(
                        VlaError::InvalidRequest,
                        "displayName must not be empty",
                    ));
                }
                record.insert("displayName".into(), json!(trimmed));
            }
            if let Some(desc) = description {
                record.insert("description".into(), json!(desc));
            }
            if record.len() > 1 {
                self.data_access.update(&record)?;
            }
            Ok(self.get(id)?.unwrap_or(existing))
        })
    }

    fn delete(&self, id: i64) -> Result<bool> {
        robot_common_lib::logged_result!("vla_model_delete", {
            let model = self
                .get(id)?
                .ok_or_else(|| vla_err(VlaError::ModelNotFound, format!("VLA model #{id} not found")))?;
            let is_active = *self
                .active_model_id
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                == Some(id);
            let is_loaded = model.package_sha256.as_deref().is_some()
                && self.runtime.loaded().as_deref() == model.package_sha256.as_deref();
            if is_active || is_loaded {
                return Err(vla_err(
                    VlaError::InUse,
                    "Model đang được nạp hoặc đang chạy, không thể xóa",
                ));
            }

            let record = Record::from_iter([("id".into(), json!(id))]);
            let deleted = self.data_access.delete(&record)?;
            if deleted > 0
                && let Some(path) = model.storage_path.as_deref()
            {
                self.remove_model_dir(path);
            }
            Ok(deleted > 0)
        })
    }
}
