use crate::{DbBaseObject, DbObject, DbTableAttribute, JointGroup, JointInteface, Resource};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RobotConfiguration {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub robot_profile_id: Option<i64>,
    pub name: Option<String>,
    pub urdf_path: Option<String>,
    pub joint_intefaces: Option<Vec<JointInteface>>,
    pub joint_groups: Option<Vec<JointGroup>>,
}
impl DbObject for RobotConfiguration {
    const RESOURCE: Resource = Resource::RobotConfiguration;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::ROBOT_CONFIGURATIONS,
        fields: &[
            ("robotProfileId", "robot_profile_id"),
            ("name", "name"),
            ("urdfPath", "urdf_path"),
        ],
        child: Some((
            "jointIntefaces",
            Resource::JointInteface,
            "robotConfigurationId",
        )),
        keys: &["id"],
    };
}
