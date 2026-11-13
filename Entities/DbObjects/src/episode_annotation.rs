use crate::{DbBaseObject, DbObject, DbTableAttribute, Resource};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EpisodeAnnotation {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub episode_id: Option<i64>,
    pub start_time_ns: Option<i64>,
    pub end_time_ns: Option<i64>,
    pub label: Option<String>,
    pub note: Option<String>,
}
impl DbObject for EpisodeAnnotation {
    const RESOURCE: Resource = Resource::EpisodeAnnotation;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::EPISODE_ANNOTATIONS,
        fields: &[
            ("episodeId", "episode_id"),
            ("startTimeNs", "start_time_ns"),
            ("endTimeNs", "end_time_ns"),
            ("label", "label"),
            ("note", "note"),
        ],
        child: None,
        keys: &["id"],
    };
}
