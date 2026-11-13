use crate::{DbBaseObject, DbObject, DbTableAttribute, Resource, RobotConfiguration};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RobotProfile {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub name: Option<String>,
    #[serde(rename = "description", alias = "descriptions")]
    pub descriptions: Option<String>,
    pub robot_configurations: Option<Vec<RobotConfiguration>>,
}
impl DbObject for RobotProfile {
    const RESOURCE: Resource = Resource::RobotProfile;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::ROBOT_PROFILES,
        fields: &[("name", "name"), ("description", "description")],
        child: Some((
            "robotConfigurations",
            Resource::RobotConfiguration,
            "robotProfileId",
        )),
        keys: &["id"],
    };
}
