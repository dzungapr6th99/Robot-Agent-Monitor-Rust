use crate::{DbBaseObject, DbObject, DbTableAttribute, Resource};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct JointGroupMembers {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub joint_group_id: Option<i64>,
    pub joint_name: Option<String>,
    pub sort_order: i64,
}
impl DbObject for JointGroupMembers {
    const RESOURCE: Resource = Resource::JointGroupMembers;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::JOINT_GROUP_MEMBERS,
        fields: &[
            ("jointGroupId", "joint_group_id"),
            ("jointName", "joint_name"),
            ("sortOrder", "sort_order"),
        ],
        child: None,
        keys: &["jointGroupId", "jointName"],
    };
}
