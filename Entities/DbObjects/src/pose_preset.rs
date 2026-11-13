use crate::{DbBaseObject, DbObject, DbTableAttribute, PosePresetJoint, Resource};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PosePreset {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub robot_configuration_id: Option<i64>,
    pub name: Option<String>,
    pub pose_preset_joints: Option<Vec<PosePresetJoint>>,
}
impl DbObject for PosePreset {
    const RESOURCE: Resource = Resource::PosePreset;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::POSE_PRESETS,
        fields: &[
            ("robotConfigurationId", "robot_configuration_id"),
            ("name", "name"),
        ],
        child: Some((
            "posePresetJoints",
            Resource::PosePresetJoint,
            "posePresetId",
        )),
        keys: &["id"],
    };
}
