use anyhow::Result;
use robot_common_lib::Upload;
use robot_db_objects::{Record, RobotConfiguration};

pub trait RobotConfigurationBLTrait: Send + Sync {
    fn insert(&self, data: Record, upload: Option<Upload>) -> Result<i64>;
    fn asset_manifest(&self, id: i64) -> Result<Vec<String>>;
    fn read_asset(&self, id: i64, path: &str) -> Result<(Vec<u8>, &'static str)>;
    fn update(&self, data: Record) -> Result<i64>;
    fn delete(&self, data: Record) -> Result<i64>;
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<RobotConfiguration>>;
    fn query(&self, ids: &[i64]) -> Result<Vec<RobotConfiguration>>;
}
