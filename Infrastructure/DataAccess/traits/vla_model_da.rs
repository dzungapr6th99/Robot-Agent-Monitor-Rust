use super::BaseDATrait;
use anyhow::Result;
use robot_db_objects::VlaModel;

pub trait VlaModelSchemaTrait: BaseDATrait<VlaModel> {
    fn ensure_table(&self) -> Result<bool>;
    fn count_by_configuration(&self, configuration_id: i64) -> Result<i64>;
    fn search(&self, configuration_id: Option<i64>, query: Option<&str>) -> Result<Vec<VlaModel>>;
    fn exists_version(
        &self,
        connection: &rusqlite::Connection,
        configuration_id: i64,
        code: &str,
        version: &str,
    ) -> Result<bool>;
    fn update_storage_path(
        &self,
        connection: &rusqlite::Connection,
        id: i64,
        storage_path: &str,
    ) -> Result<usize>;
}
