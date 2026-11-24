use crate::{
    SensorBL,
    traits::{RobotRosBLTrait, VlaBLTrait, VlaCameraFeedTrait, VlaRunGuardTrait},
    vla_camera::resolve_camera_topics,
};
use anyhow::{Result, ensure};
use robot_common_lib::{
    vla::{VlaError, VlaJoints, VlaObservation, VlaOptions, VlaRuntimeStatus, vla_err},
    vla_package::VlaManifest,
};
use robot_data_access::{VlaModelDA, traits::BaseDATrait};
use robot_ros_bridge::RosMessageEvent;
use robot_third_party_client::traits::VlaRuntimeTrait;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{broadcast, watch};

const JOINT_STATE: &str = "sensor_msgs/msg/JointState";
type ActiveRun = Arc<Mutex<Option<watch::Sender<bool>>>>;
type LoadedMeta = Arc<Mutex<Option<(i64, i64)>>>;

pub struct VlaBL<Rt, R, Cam> {
    runtime: Arc<Rt>,
    ros: Arc<R>,
    camera: Arc<Cam>,
    vla_models: Arc<VlaModelDA>,
    sensors: Arc<SensorBL>,
    data_root: PathBuf,
    options: VlaOptions,
    active: ActiveRun,
    active_model_id: Arc<Mutex<Option<i64>>>,
    loaded_meta: LoadedMeta,
}

// Releases exclusivity even if the HTTP request future is dropped.
struct VlaRunGuard {
    active: ActiveRun,
    active_model_id: Arc<Mutex<Option<i64>>>,
}

impl VlaRunGuardTrait for VlaRunGuard {
    fn finish(&mut self) -> bool {
        *self
            .active_model_id
            .lock()
            .unwrap_or_else(|e| e.into_inner()) = None;
        self.active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
            .is_some()
    }
}

impl Drop for VlaRunGuard {
    fn drop(&mut self) {
        self.finish();
    }
}

impl<Rt: VlaRuntimeTrait + 'static, R: RobotRosBLTrait, Cam: VlaCameraFeedTrait> VlaBL<Rt, R, Cam> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        runtime: Arc<Rt>,
        ros: Arc<R>,
        camera: Arc<Cam>,
        vla_models: Arc<VlaModelDA>,
        sensors: Arc<SensorBL>,
        data_root: PathBuf,
        options: VlaOptions,
        active_model_id: Arc<Mutex<Option<i64>>>,
    ) -> Result<Self> {
        robot_common_lib::logged_result!("new_vla_business", {
            ensure!(
                options.timeout_ms > 0
                    && options.timeout_ms < 750_000
                    && options.max_steps > 0
                    && options.step_period_ms > 0
                    && options.image_max_age_ms > 0,
                "Invalid VLA limits"
            );
            Ok(Self {
                runtime,
                ros,
                camera,
                vla_models,
                sensors,
                data_root,
                options,
                active: Arc::default(),
                active_model_id,
                loaded_meta: Arc::default(),
            })
        })
    }

    async fn execute(
        &self,
        manifest: &VlaManifest,
        task: &str,
        published: &mut u32,
    ) -> Result<u32> {
        let cameras = manifest.required_cameras();
        let expected_joints = &manifest.robot.joint_names;
        loop {
            // Require a new state for each inference cycle, rather than reusing
            // messages queued while the previous motion batch was executing.
            let mut messages = self.ros.subscribe_messages();
            let observation =
                tokio::time::timeout(Duration::from_secs(5), self.observation(&mut messages, &cameras))
                    .await
                    .map_err(|_| VlaError::Timeout)??;
            let inference = self.runtime.infer(task, &observation).await?;
            ensure!(
                inference.done || !inference.actions.is_empty(),
                VlaError::Unavailable
            );
            // Validate the complete batch before moving the robot.
            for action in &inference.actions {
                action.validate().map_err(|_| VlaError::Unavailable)?;
                ensure!(
                    action.name.len() == expected_joints.len()
                        && action.name.iter().all(|name| {
                            expected_joints.contains(name)
                                && observation.joints.name.contains(name)
                        }),
                    VlaError::Unavailable
                );
            }
            let action_count = inference.actions.len();
            for (index, action) in inference.actions.into_iter().enumerate() {
                if *published == self.options.max_steps {
                    return Ok(*published);
                }
                self.ros
                    .publish(
                        &self.options.command_topic,
                        JOINT_STATE,
                        serde_json::json!({"name": action.name, "position": action.position}),
                    )
                    .await
                    .map_err(|_| VlaError::Unavailable)?;
                *published += 1;
                // The period separates motion steps; no delay is needed after
                // the final step of a completed instruction.
                if *published == self.options.max_steps
                    || (inference.done && index + 1 == action_count)
                {
                    return Ok(*published);
                }
                tokio::time::sleep(Duration::from_millis(self.options.step_period_ms)).await;
            }
            if inference.done {
                return Ok(*published);
            }
        }
    }

    async fn observation(
        &self,
        messages: &mut broadcast::Receiver<RosMessageEvent>,
        cameras: &[String],
    ) -> Result<VlaObservation> {
        let joints = loop {
            let mut event = match messages.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => return Err(VlaError::Unavailable.into()),
            };
            if event.topic != self.options.state_topic || event.message_type != JOINT_STATE {
                continue;
            }
            // Bounded draining keeps cancellation/deadline responsive even if
            // the producer continuously fills the broadcast channel.
            for _ in 0..256 {
                let newer = match messages.try_recv() {
                    Ok(newer) => newer,
                    Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(_) => break,
                };
                if newer.topic == self.options.state_topic && newer.message_type == JOINT_STATE {
                    event = newer;
                }
            }
            let state = &event.data;
            let joints: VlaJoints = serde_json::from_value(serde_json::json!({
                "name": state.get("name"), "position": state.get("position")
            }))
            .map_err(|_| VlaError::Unavailable)?;
            joints.validate().map_err(|_| VlaError::Unavailable)?;
            break joints;
        };

        let max_age = Duration::from_millis(self.options.image_max_age_ms);
        let mut images = HashMap::with_capacity(cameras.len());
        for camera in cameras {
            let frame = self.camera.latest(camera, max_age)?;
            images.insert(camera.clone(), frame);
        }

        Ok(VlaObservation { joints, images })
    }
}

impl<Rt: VlaRuntimeTrait + 'static, R: RobotRosBLTrait, Cam: VlaCameraFeedTrait> VlaBLTrait
    for VlaBL<Rt, R, Cam>
{
    fn topics_ready(&self) -> Result<()> {
        let status = self.ros.status();
        if !status.running {
            return Err(vla_err(
                VlaError::Unavailable,
                "ROS is disabled or unavailable",
            ));
        }
        let has_cmd = status.publishers.iter().any(|topic| {
            topic.name == self.options.command_topic && topic.message_type == JOINT_STATE
        });
        if !has_cmd {
            return Err(vla_err(
                VlaError::Unavailable,
                format!(
                    "VLA command topic '{}' is not a configured JointState publisher",
                    self.options.command_topic
                ),
            ));
        }
        let has_state = status.subscriptions.iter().any(|topic| {
            topic.name == self.options.state_topic && topic.message_type == JOINT_STATE
        });
        if !has_state {
            return Err(vla_err(
                VlaError::Unavailable,
                format!(
                    "VLA state topic '{}' is not a configured JointState subscription",
                    self.options.state_topic
                ),
            ));
        }
        Ok(())
    }

    fn status(&self) -> VlaRuntimeStatus {
        let running = self
            .active
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .is_some();
        let (loaded_model_id, configuration_id) = if self.runtime.loaded().is_some() {
            match *self.loaded_meta.lock().unwrap_or_else(|e| e.into_inner()) {
                Some((model_id, config_id)) => (Some(model_id), Some(config_id)),
                None => (None, self.options.configuration_id),
            }
        } else {
            (None, self.options.configuration_id)
        };
        VlaRuntimeStatus {
            loaded_model_id,
            configuration_id,
            running,
            providers: self.runtime.providers(),
        }
    }

    async fn run(&self, configuration_id: i64, model_id: i64, task: &str) -> Result<u32> {
        robot_common_lib::logged_result!(async "run_vla", {
            ensure!(
                configuration_id > 0
                    && model_id > 0
                    && !task.trim().is_empty()
                    && task.chars().count() <= 2048,
                VlaError::InvalidRequest
            );

            // 1. Lookup model.
            let model = self
                .vla_models
                .query(&[model_id])?
                .into_iter()
                .next()
                .ok_or(VlaError::ModelNotFound)?;

            // 2. Model must belong to configuration_id, and match ROBOT_CONFIGURATION_ID if set (G4).
            if model.robot_configuration_id != Some(configuration_id) {
                return Err(vla_err(
                    VlaError::WrongRobot,
                    format!(
                        "VLA model #{model_id} does not belong to URDF configuration #{configuration_id}"
                    ),
                ));
            }
            if let Some(bound_config) = self.options.configuration_id
                && bound_config != configuration_id
            {
                return Err(vla_err(
                    VlaError::WrongRobot,
                    format!(
                        "Requested URDF #{configuration_id} does not match agent ROBOT_CONFIGURATION_ID #{bound_config}"
                    ),
                ));
            }

            // Acquire single-run exclusivity before checking topics or loading models.
            let mut cancelled = {
                let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
                ensure!(active.is_none(), VlaError::Busy);
                let (sender, receiver) = watch::channel(false);
                *active = Some(sender);
                *self
                    .active_model_id
                    .lock()
                    .unwrap_or_else(|e| e.into_inner()) = Some(model_id);
                receiver
            };
            let _guard = VlaRunGuard {
                active: Arc::clone(&self.active),
                active_model_id: Arc::clone(&self.active_model_id),
            };

            // 3. Check ROS & JointState topics.
            self.topics_ready()?;

            // 4. Parse manifest, resolve cameras, load model if needed, start camera feed.
            let manifest_json = model
                .manifest_json
                .as_deref()
                .ok_or_else(|| vla_err(VlaError::InvalidPackage, "Model is missing manifestJson"))?;
            let manifest: VlaManifest = serde_json::from_str(manifest_json)
                .map_err(|err| vla_err(VlaError::InvalidPackage, format!("Corrupt manifest: {err}")))?;
            let storage_path = model
                .storage_path
                .as_deref()
                .ok_or_else(|| vla_err(VlaError::InvalidPackage, "Model is missing storagePath"))?;
            let package_sha = model
                .package_sha256
                .as_deref()
                .ok_or_else(|| vla_err(VlaError::InvalidPackage, "Model is missing packageSha256"))?;

            let cameras = manifest.required_cameras();
            let camera_topics = resolve_camera_topics(&self.sensors, configuration_id, &cameras)
                .map_err(|err| vla_err(VlaError::Unavailable, err.to_string()))?;

            if self.runtime.loaded().as_deref() != Some(package_sha) {
                self.camera.stop();
                self.runtime.unload();
                *self.loaded_meta.lock().unwrap_or_else(|e| e.into_inner()) = None;

                let runtime = Arc::clone(&self.runtime);
                let model_dir = self.data_root.join(storage_path);
                let manifest_owned = manifest.clone();
                let sha_owned = package_sha.to_owned();
                tokio::task::spawn_blocking(move || {
                    runtime.load(&model_dir, &manifest_owned, &sha_owned)
                })
                .await
                .map_err(|_| VlaError::Unavailable)??;

                *self.loaded_meta.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((model_id, configuration_id));
                if let Err(err) = self.camera.start(&camera_topics) {
                    self.runtime.unload();
                    *self.loaded_meta.lock().unwrap_or_else(|e| e.into_inner()) = None;
                    return Err(err);
                }
            }

            robot_common_lib::operation_log::info(
                module_path!(),
                format_args!(
                    "Starting VLA command (configuration #{configuration_id}, model #{model_id})"
                ),
            );
            let mut published = 0;
            tokio::select! {
                biased;
                _ = cancelled.changed() => Ok(published),
                result = tokio::time::timeout(
                    Duration::from_millis(self.options.timeout_ms),
                    self.execute(&manifest, task, &mut published)
                ) => result.map_err(|_| VlaError::Timeout)?,
            }
        })
    }

    fn stop(&self) -> bool {
        let active = self.active.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(sender) = active.as_ref() {
            sender.send_replace(true);
            true
        } else {
            false
        }
    }
}
