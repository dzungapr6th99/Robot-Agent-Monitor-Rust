use crate::{DbBaseObject, DbObject, DbTableAttribute, EpisodeAnnotation, Resource};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Episode {
    #[serde(flatten)]
    pub base: DbBaseObject,
    pub robot_configuration_id: Option<i64>,
    pub name: Option<String>,
    pub task: Option<String>,
    pub status: Option<String>,
    pub result: Option<String>,
    pub recording_path: Option<String>,
    pub operator_note: Option<String>,
    pub started_at: Option<String>,
    pub ended_at: Option<String>,
    pub duration_ns: Option<i64>,
    pub episode_annotations: Option<Vec<EpisodeAnnotation>>,
}
impl DbObject for Episode {
    const RESOURCE: Resource = Resource::Episode;
    const TABLE: DbTableAttribute = DbTableAttribute {
        name: robot_common_lib::const_data::TableName::EPISODES,
        fields: &[
            "robotConfigurationId",
            "name",
            "task",
            "status",
            "result",
            "recordingPath",
            "operatorNote",
            "startedAt",
            "endedAt",
            "durationNs",
        ],
        child: Some((
            "episodeAnnotations",
            Resource::EpisodeAnnotation,
            "episodeId",
        )),
        keys: &["id"],
    };
}
