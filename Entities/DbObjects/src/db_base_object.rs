use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DbBaseObject {
    pub id: i64,
    pub created_at: String,
    pub updated_at: String,
}

impl Default for DbBaseObject {
    fn default() -> Self {
        Self {
            id: 0,
            created_at: "0001-01-01T00:00:00".into(),
            updated_at: "0001-01-01T00:00:00".into(),
        }
    }
}
