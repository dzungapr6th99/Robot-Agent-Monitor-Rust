use crate::{DbBaseObject, DbObject, DbTableAttribute, Resource};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct JointInteface {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub robot_configuration_id: Option<i64>,
    pub name: Option<String>,
    pub operation: Option<String>,
    pub ros_endpoint: Option<String>,
    pub data_type: Option<String>,
    pub is_enabled: Option<bool>,
    pub is_default_command: Option<bool>,
}
impl DbObject for JointInteface {
    const RESOURCE: Resource = Resource::JointInteface;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::JOINT_INTERFACES,
        fields: &[
            "robotConfigurationId",
            "name",
            "operation",
            "rosEndpoint",
            "dataType",
            "isEnabled",
            "isDefaultCommand",
        ],
        child: None,
        keys: &["id"],
    };
}
