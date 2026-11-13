use anyhow::Result;
use robot_db_objects::{DbObject, Record};

pub trait BaseBLTrait<T: DbObject>: Send + Sync {
    fn insert(&self, data: Record) -> Result<i64>;
    fn insert_many(&self, records: &[Record]) -> Result<usize>;
    fn update(&self, data: Record) -> Result<i64>;
    fn delete(&self, data: Record) -> Result<i64>;
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<T>>;
    fn query(&self, ids: &[i64]) -> Result<Vec<T>>;
}
