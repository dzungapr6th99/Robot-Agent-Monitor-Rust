use crate::{DbBaseObject, DbObject, DbTableAttribute, Resource};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct VlaModel {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub robot_configuration_id: Option<i64>,
    pub code: Option<String>,
    pub display_name: Option<String>,
    pub model_type: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub format_version: Option<i64>,
    pub package_sha256: Option<String>,
    pub package_bytes: Option<i64>,
    pub storage_path: Option<String>,
    pub manifest_json: Option<String>,
}

impl DbObject for VlaModel {
    const RESOURCE: Resource = Resource::VlaModel;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::VLA_MODELS,
        fields: &[
            ("robotConfigurationId", "robot_configuration_id"),
            ("code", "code"),
            ("displayName", "display_name"),
            ("modelType", "model_type"),
            ("version", "version"),
            ("description", "description"),
            ("formatVersion", "format_version"),
            ("packageSha256", "package_sha256"),
            ("packageBytes", "package_bytes"),
            ("storagePath", "storage_path"),
            ("manifestJson", "manifest_json"),
        ],
        child: None,
        keys: &["id"],
    };
}
