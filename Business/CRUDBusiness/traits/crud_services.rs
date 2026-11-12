use anyhow::Result;
use robot_common_lib::Upload;
use robot_db_objects::{Record, Resource};

pub trait CrudServicesTrait: Send + Sync {
    fn max_urdf_bytes(&self) -> usize;
    fn insert(&self, resource: Resource, data: Record, upload: Option<Upload>) -> Result<i64>;
    fn update(&self, resource: Resource, data: Record) -> Result<i64>;
    fn delete(&self, resource: Resource, data: Record) -> Result<i64>;
    fn list(&self, resource: Resource, limit: u32, offset: u32) -> Result<Vec<serde_json::Value>>;
    fn query(&self, resource: Resource, ids: &[i64]) -> Result<Vec<serde_json::Value>>;
}
