use anyhow::Result;
use robot_db_objects::{DbObject, Record};

pub trait DapperExtensions {
    fn count<T: DbObject>(&self, filter: &Record) -> Result<i64>;
    fn get<T: DbObject>(&self, filter: &Record) -> Result<Vec<T>>;
    fn get_first_or_default<T: DbObject>(&self, filter: &Record) -> Result<Option<T>>;
    fn insert<T: DbObject>(&self, data: &Record) -> Result<i64>;
    /// Inserts the whole batch atomically and returns the number of inserted rows.
    /// All records must supply the same mapped columns. Omitted columns use database defaults.
    fn insert_many<T: DbObject>(&self, records: &[Record]) -> Result<usize>;
    fn update<T: DbObject>(&self, data: &Record) -> Result<i64>;
    fn delete<T: DbObject>(&self, data: &Record) -> Result<i64>;
    fn query<T: DbObject>(&self, ids: &[i64]) -> Result<Vec<T>>;
}
