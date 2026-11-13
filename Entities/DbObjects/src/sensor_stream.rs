use crate::{DbBaseObject, DbObject, DbTableAttribute, Resource};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SensorStream {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub sensor_id: Option<i64>,
    pub role: Option<String>,
    pub topic: Option<String>,
    pub data_type: Option<String>,
    pub is_enabled: Option<bool>,
}
impl DbObject for SensorStream {
    const RESOURCE: Resource = Resource::SensorStream;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::SENSOR_STREAMS,
        fields: &[
            ("sensorId", "sensor_id"),
            ("role", "role"),
            ("topic", "topic"),
            ("dataType", "data_type"),
            ("isEnabled", "is_enabled"),
        ],
        child: None,
        keys: &["id"],
    };
}
