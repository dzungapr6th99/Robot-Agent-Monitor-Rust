use crate::traits::{RobotRosBLTrait, VlaBLTrait, VlaRunGuardTrait};
use anyhow::{Result, ensure};
use robot_common_lib::vla::{VlaError, VlaJoints, VlaOptions, VlaProfile};
use robot_ros_bridge::RosMessageEvent;
use robot_third_party_client::traits::VlaClientTrait;
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::sync::{broadcast, watch};

const JOINT_STATE: &str = "sensor_msgs/msg/JointState";
type ActiveRun = Arc<Mutex<Option<watch::Sender<bool>>>>;

pub struct VlaBL<C, R> {
    client: C,
    ros: Arc<R>,
    profiles: Vec<VlaProfile>,
    options: VlaOptions,
    active: ActiveRun,
}

// Releases exclusivity even if the HTTP request future is dropped.
struct VlaRunGuard(ActiveRun);
impl VlaRunGuardTrait for VlaRunGuard {
    fn finish(&mut self) -> bool {
        self.0
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

impl<C: VlaClientTrait, R: RobotRosBLTrait> VlaBL<C, R> {
    pub fn new(
        client: C,
        ros: Arc<R>,
        profiles: Vec<VlaProfile>,
        options: VlaOptions,
    ) -> Result<Self> {
        robot_common_lib::logged_result!("new_vla_business", {
            ensure!(
                options.timeout_ms > 0
                    && options.timeout_ms < 750_000
                    && options.max_steps > 0
                    && options.step_period_ms > 0,
                "Invalid VLA limits"
            );
            if !profiles.is_empty() {
                let status = ros.status();
                ensure!(
                    status
                        .publishers
                        .iter()
                        .any(|topic| topic.name == options.command_topic
                            && topic.message_type == JOINT_STATE),
                    "VLA command topic must be a configured JointState publisher"
                );
                ensure!(
                    status
                        .subscriptions
                        .iter()
                        .any(|topic| topic.name == options.state_topic
                            && topic.message_type == JOINT_STATE),
                    "VLA state topic must be a configured JointState subscription"
                );
            }
            Ok(Self {
                client,
                ros,
                profiles,
                options,
                active: Arc::default(),
            })
        })
    }

    async fn execute(&self, profile_id: &str, task: &str, published: &mut u32) -> Result<u32> {
        loop {
            // Require a new state for each inference cycle, rather than reusing
            // messages queued while the previous motion batch was executing.
            let mut messages = self.ros.subscribe_messages();
            let observation =
                tokio::time::timeout(Duration::from_secs(5), self.observation(&mut messages))
                    .await
                    .map_err(|_| VlaError::Timeout)??;
            let inference = self.client.infer(profile_id, task, &observation).await?;
            ensure!(
                inference.done || !inference.actions.is_empty(),
                VlaError::Unavailable
            );
            // Validate the complete batch before moving the robot.
            for action in &inference.actions {
                action.validate().map_err(|_| VlaError::Unavailable)?;
                ensure!(
                    action.name.len() == observation.name.len()
                        && action
                            .name
                            .iter()
                            .all(|name| observation.name.contains(name)),
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
    ) -> Result<VlaJoints> {
        loop {
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
            let observation: VlaJoints = serde_json::from_value(serde_json::json!({
                "name": state.get("name"), "position": state.get("position")
            }))
            .map_err(|_| VlaError::Unavailable)?;
            observation.validate().map_err(|_| VlaError::Unavailable)?;
            return Ok(observation);
        }
    }
}

impl<C: VlaClientTrait, R: RobotRosBLTrait> VlaBLTrait for VlaBL<C, R> {
    fn models(&self) -> Vec<VlaProfile> {
        self.profiles.clone()
    }

    async fn run(&self, profile_id: &str, task: &str) -> Result<u32> {
        robot_common_lib::logged_result!(async "run_vla", {
            ensure!(!task.trim().is_empty() && task.chars().count() <= 2048, VlaError::InvalidRequest);
            ensure!(self.profiles.iter().any(|p| p.id == profile_id), VlaError::ProfileNotFound);
            let mut cancelled = {
                let mut active = self.active.lock().unwrap_or_else(|e| e.into_inner());
                ensure!(active.is_none(), VlaError::Busy);
                ensure!(self.ros.status().running, VlaError::Unavailable);
                let (sender, receiver) = watch::channel(false);
                *active = Some(sender);
                receiver
            };
            let _guard = VlaRunGuard(Arc::clone(&self.active));
            robot_common_lib::operation_log::info(module_path!(), format_args!("Starting VLA command"));
            let mut published = 0;
            tokio::select! {
                biased;
                _ = cancelled.changed() => Ok(published),
                result = tokio::time::timeout(Duration::from_millis(self.options.timeout_ms),
                    self.execute(profile_id, task, &mut published)) => result.map_err(|_| VlaError::Timeout)?,
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
