use crate::{DbBaseObject, DbObject, DbTableAttribute, Resource, SensorStream};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sensor {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub robot_configuration_id: Option<i64>,
    pub code: Option<String>,
    pub display_name: Option<String>,
    pub sensor_type: Option<String>,
    pub is_enabled: Option<bool>,
    pub sensor_streams: Option<Vec<SensorStream>>,
}
impl DbObject for Sensor {
    const RESOURCE: Resource = Resource::Sensor;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::SENSORS,
        fields: &[
            ("robotConfigurationId", "robot_configuration_id"),
            ("code", "code"),
            ("displayName", "display_name"),
            ("sensorType", "sensor_type"),
            ("isEnabled", "is_enabled"),
        ],
        child: Some(("sensorStreams", Resource::SensorStream, "sensorId")),
        keys: &["id"],
    };
}
