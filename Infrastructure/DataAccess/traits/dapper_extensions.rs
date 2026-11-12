use anyhow::Result;
use robot_db_objects::{DbObject, Record};

pub trait DapperExtensions {
    fn insert<T: DbObject>(&self, data: &Record) -> Result<i64>;
    fn update<T: DbObject>(&self, data: &Record) -> Result<i64>;
    fn delete<T: DbObject>(&self, data: &Record) -> Result<i64>;
    fn query<T: DbObject>(&self, ids: &[i64]) -> Result<Vec<T>>;
}
