use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct VlaOptions {
    /// Total command deadline, including observations, inference and publishing.
    pub timeout_ms: u64,
    pub command_topic: String,
    pub state_topic: String,
    pub max_steps: u32,
    pub step_period_ms: u64,
    pub image_max_age_ms: u64,
    pub configuration_id: Option<i64>,
}

impl Default for VlaOptions {
    fn default() -> Self {
        Self {
            timeout_ms: 600_000,
            command_topic: "/yellowarm_sim/joint_command".into(),
            state_topic: "/yellowarm_sim/joint_states".into(),
            max_steps: 1000,
            step_period_ms: 100,
            image_max_age_ms: 500,
            configuration_id: None,
        }
    }
}

/// Joint positions shared between ROS bridge, ONNX runtime and business layer.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VlaJoints {
    pub name: Vec<String>,
    pub position: Vec<f64>,
}

impl VlaJoints {
    pub fn validate(&self) -> Result<bool> {
        ensure!(
            !self.name.is_empty() && self.name.len() == self.position.len(),
            "Invalid VLA joint array lengths"
        );
        let mut names = HashSet::new();
        ensure!(
            self.name
                .iter()
                .all(|name| !name.trim().is_empty() && names.insert(name)),
            "Invalid VLA joint names"
        );
        ensure!(
            self.position.iter().all(|value| value.is_finite()),
            "Invalid VLA joint positions"
        );
        Ok(true)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VlaImage {
    pub width: u32,
    pub height: u32,
    pub encoding: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VlaObservation {
    pub joints: VlaJoints,
    pub images: HashMap<String, VlaImage>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VlaInference {
    pub actions: Vec<VlaJoints>,
    pub done: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VlaRuntimeStatus {
    pub loaded_model_id: Option<i64>,
    pub configuration_id: Option<i64>,
    pub running: bool,
    pub providers: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VlaError {
    InvalidRequest,
    InvalidPackage,
    ModelNotFound,
    Busy,
    InUse,
    WrongRobot,
    Duplicate,
    Unavailable,
    Timeout,
}

impl std::fmt::Display for VlaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidRequest => "Invalid VLA request",
            Self::InvalidPackage => "Invalid VLA package",
            Self::ModelNotFound => "VLA model not found",
            Self::Busy => "A VLA command is already running",
            Self::InUse => "VLA model is currently loaded or running",
            Self::WrongRobot => "VLA model does not match the active robot configuration",
            Self::Duplicate => "A VLA model with this code and version already exists for this URDF",
            Self::Unavailable => "ROS, camera or VLA runtime is unavailable",
            Self::Timeout => "VLA command timed out",
        })
    }
}
impl std::error::Error for VlaError {}

pub fn vla_err(kind: VlaError, message: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(kind).context(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_joint_actions() {
        let mut joints = VlaJoints {
            name: vec!["joint".into()],
            position: vec![0.0],
        };
        assert!(joints.validate().is_ok());
        joints.position[0] = f64::NAN;
        assert!(joints.validate().is_err());
        joints.position.clear();
        assert!(joints.validate().is_err());
    }

    #[test]
    fn vla_err_preserves_error_kind_and_message() {
        let err = vla_err(VlaError::InvalidPackage, "Missing manifest.json");
        assert_eq!(err.downcast_ref::<VlaError>(), Some(&VlaError::InvalidPackage));
        assert_eq!(err.to_string(), "Missing manifest.json");
    }
}
