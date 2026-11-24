use anyhow::Result;
use robot_db_objects::VlaModel;
use std::path::Path;

pub trait VlaModelBLTrait: Send + Sync {
    fn ensure_table(&self) -> Result<bool>;
    fn cleanup_staging(&self) -> Result<usize>;
    fn install(&self, configuration_id: i64, zip_path: &Path) -> Result<i64>;
    fn list(&self, configuration_id: Option<i64>, query: Option<&str>) -> Result<Vec<VlaModel>>;
    fn get(&self, id: i64) -> Result<Option<VlaModel>>;
    fn update(
        &self,
        id: i64,
        display_name: Option<String>,
        description: Option<String>,
    ) -> Result<VlaModel>;
    fn delete(&self, id: i64) -> Result<bool>;
}
