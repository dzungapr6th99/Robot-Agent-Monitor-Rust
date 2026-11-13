use crate::traits::BaseBLTrait;
use anyhow::Result;
use robot_data_access::traits::BaseDATrait;
use robot_db_objects::{DbObject, Record};
use std::sync::Arc;

pub struct BaseBL<T: DbObject> {
    pub data_access: Arc<dyn BaseDATrait<T>>,
}

impl<T: DbObject> BaseBL<T> {
    pub fn new(data_access: Arc<dyn BaseDATrait<T>>) -> Self {
        Self { data_access }
    }
}

impl<T: DbObject> BaseBLTrait<T> for BaseBL<T> {
    fn insert(&self, data: Record) -> Result<i64> {
        robot_common_lib::logged_result!("insert", { self.data_access.insert(&data) })
    }
    fn insert_many(&self, records: &[Record]) -> Result<usize> {
        robot_common_lib::logged_result!("insert_many", { self.data_access.insert_many(records) })
    }
    fn update(&self, data: Record) -> Result<i64> {
        robot_common_lib::logged_result!("update", { self.data_access.update(&data) })
    }
    fn delete(&self, data: Record) -> Result<i64> {
        robot_common_lib::logged_result!("delete", { self.data_access.delete(&data) })
    }
    fn list(&self, limit: u32, offset: u32) -> Result<Vec<T>> {
        robot_common_lib::logged_result!("list", { self.data_access.list(limit, offset) })
    }
    fn query(&self, ids: &[i64]) -> Result<Vec<T>> {
        robot_common_lib::logged_result!("query", { self.data_access.query(ids) })
    }
}
