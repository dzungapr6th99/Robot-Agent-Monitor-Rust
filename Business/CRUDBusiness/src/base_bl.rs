use anyhow::Result;
use robot_data_access::base_da::BaseDA;
use robot_db_objects::{DbObject, Record};
use std::sync::Arc;

pub struct BaseBL<T: DbObject> {
    pub data_access: Arc<BaseDA<T>>,
}

impl<T: DbObject> BaseBL<T> {
    pub fn new(data_access: Arc<BaseDA<T>>) -> Self {
        Self { data_access }
    }

    pub fn insert(&self, data: Record) -> Result<i64> {
        self.data_access.insert(&data)
    }
    pub fn update(&self, data: Record) -> Result<i64> {
        self.data_access.update(&data)
    }
    pub fn delete(&self, data: Record) -> Result<i64> {
        self.data_access.delete(&data)
    }
    pub fn list(&self, limit: u32, offset: u32) -> Result<Vec<T>> {
        self.data_access.list(limit, offset)
    }
    pub fn query(&self, ids: &[i64]) -> Result<Vec<T>> {
        self.data_access.query(ids)
    }
}
