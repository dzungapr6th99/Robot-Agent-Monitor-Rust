#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Resource {
    RobotProfile,
    RobotConfiguration,
    JointInteface,
    Sensor,
    SensorStream,
    JointGroup,
    JointGroupMembers,
    PosePreset,
    PosePresetJoint,
    Episode,
    EpisodeAnnotation,
}

impl Resource {
    pub fn from_route(route: &str) -> Option<Self> {
        Some(match route {
            "robot-profiles" => Self::RobotProfile,
            "robot-configurations" => Self::RobotConfiguration,
            "joint-interfaces" => Self::JointInteface,
            "sensors" => Self::Sensor,
            "sensor-streams" => Self::SensorStream,
            "joint-groups" => Self::JointGroup,
            "joint-group-members" => Self::JointGroupMembers,
            "pose-presets" => Self::PosePreset,
            "pose-preset-joints" => Self::PosePresetJoint,
            "episodes" => Self::Episode,
            "episode-annotations" => Self::EpisodeAnnotation,
            _ => return None,
        })
    }
}

impl Resource {
    pub fn table(self) -> crate::DbTableAttribute {
        use crate::DbObject;
        match self {
            Self::RobotProfile => crate::RobotProfile::TABLE,
            Self::RobotConfiguration => crate::RobotConfiguration::TABLE,
            Self::JointInteface => crate::JointInteface::TABLE,
            Self::Sensor => crate::Sensor::TABLE,
            Self::SensorStream => crate::SensorStream::TABLE,
            Self::JointGroup => crate::JointGroup::TABLE,
            Self::JointGroupMembers => crate::JointGroupMembers::TABLE,
            Self::PosePreset => crate::PosePreset::TABLE,
            Self::PosePresetJoint => crate::PosePresetJoint::TABLE,
            Self::Episode => crate::Episode::TABLE,
            Self::EpisodeAnnotation => crate::EpisodeAnnotation::TABLE,
        }
    }
}
