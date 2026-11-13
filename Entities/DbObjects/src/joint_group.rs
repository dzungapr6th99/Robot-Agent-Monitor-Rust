use crate::{DbBaseObject, DbObject, DbTableAttribute, JointGroupMembers, Resource};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct JointGroup {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub robot_configuration_id: Option<i64>,
    pub name: Option<String>,
    pub display_name: Option<String>,
    pub sort_order: i64,
    pub joint_group_members: Option<Vec<JointGroupMembers>>,
}
impl DbObject for JointGroup {
    const RESOURCE: Resource = Resource::JointGroup;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::JOINT_GROUPS,
        fields: &[
            ("robotConfigurationId", "robot_configuration_id"),
            ("name", "name"),
            ("displayName", "display_name"),
            ("sortOrder", "sort_order"),
        ],
        child: Some((
            "jointGroupMembers",
            Resource::JointGroupMembers,
            "jointGroupId",
        )),
        keys: &["id"],
    };
}
