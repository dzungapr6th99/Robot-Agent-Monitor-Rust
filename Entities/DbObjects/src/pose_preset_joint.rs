use crate::{DbBaseObject, DbObject, DbTableAttribute, Resource};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PosePresetJoint {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub pose_preset_id: Option<i64>,
    pub joint_name: Option<String>,
    pub position: Option<f64>,
}
impl DbObject for PosePresetJoint {
    const RESOURCE: Resource = Resource::PosePresetJoint;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::POSE_PRESET_JOINTS,
        fields: &[
            ("posePresetId", "pose_preset_id"),
            ("jointName", "joint_name"),
            ("position", "position"),
        ],
        child: None,
        keys: &["posePresetId", "jointName"],
    };
}
