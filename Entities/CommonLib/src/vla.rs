use anyhow::{Result, ensure};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

pub struct VlaOptions {
    pub base_url: String,
    pub access_token: String,
    /// Total command deadline, including observations, inference and publishing.
    pub timeout_ms: u64,
    pub command_topic: String,
    pub state_topic: String,
    pub max_steps: u32,
    pub step_period_ms: u64,
}

impl Default for VlaOptions {
    fn default() -> Self {
        Self {
            base_url: "http://127.0.0.1:8091/".into(),
            access_token: String::new(),
            timeout_ms: 600_000,
            command_topic: "/yellowarm_sim/joint_command".into(),
            state_topic: "/yellowarm_sim/joint_states".into(),
            max_steps: 1000,
            step_period_ms: 100,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct VlaProfile {
    pub id: String,
    pub model: String,
    pub version: String,
}

pub fn parse_profiles(json: &str) -> Result<Vec<VlaProfile>> {
    let profiles: Vec<VlaProfile> = serde_json::from_str(json)?;
    let mut ids = HashSet::new();
    for profile in &profiles {
        ensure!(
            !profile.id.trim().is_empty() && !profile.version.trim().is_empty(),
            "Empty VLA profile id or version"
        );
        ensure!(
            matches!(profile.model.as_str(), "smolvla" | "groot"),
            "Unsupported VLA model"
        );
        ensure!(ids.insert(&profile.id), "Duplicate VLA profile id");
    }
    Ok(profiles)
}

/// Wire contract shared by the HTTP adapter and business layer.
#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VlaInference {
    pub actions: Vec<VlaJoints>,
    pub done: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VlaError {
    InvalidRequest,
    ProfileNotFound,
    Busy,
    Unavailable,
    Timeout,
}
impl std::fmt::Display for VlaError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::InvalidRequest => "Invalid VLA request",
            Self::ProfileNotFound => "VLA profile not found",
            Self::Busy => "A VLA command is already running",
            Self::Unavailable => "ROS or VLA is unavailable",
            Self::Timeout => "VLA command timed out",
        })
    }
}
impl std::error::Error for VlaError {}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn profiles_validate_at_configuration_boundary() {
        let valid = r#"[{"id":"arm","model":"smolvla","version":"v1"}]"#;
        assert_eq!(parse_profiles(valid).unwrap().len(), 1);
        assert!(parse_profiles(&valid.replace("smolvla", "unknown")).is_err());
        assert!(
            parse_profiles(&format!(
                "[{},{}]",
                &valid[1..valid.len() - 1],
                &valid[1..valid.len() - 1]
            ))
            .is_err()
        );
        assert!(parse_profiles(&valid.replace("\"version\"", "\"extra\"")).is_err());
        assert!(parse_profiles("[]").unwrap().is_empty());
    }
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
}
