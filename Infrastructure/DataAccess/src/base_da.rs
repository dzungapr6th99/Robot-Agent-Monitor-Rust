use crate::{dapper_extensions::DapperExtensions, db_management::DbManagement};
use anyhow::Result;
use robot_db_objects::{DbObject, Record};
use std::{marker::PhantomData, sync::Arc};

pub struct BaseDA<T: DbObject> {
    pub db_management: Arc<DbManagement>,
    entity: PhantomData<T>,
}

impl<T: DbObject> BaseDA<T> {
    pub fn new(db_management: Arc<DbManagement>) -> Self {
        Self {
            db_management,
            entity: PhantomData,
        }
    }

    pub fn insert(&self, data: &Record) -> Result<i64> {
        let mut db = self.db_management.get_connection(true)?;
        let transaction = db.transaction()?;
        let id = transaction.insert::<T>(data)?;
        transaction.commit()?;
        Ok(id)
    }

    pub fn update(&self, data: &Record) -> Result<i64> {
        self.db_management.get_connection(true)?.update::<T>(data)
    }

    pub fn delete(&self, data: &Record) -> Result<i64> {
        self.db_management.get_connection(true)?.delete::<T>(data)
    }

    pub fn list(&self, limit: u32, offset: u32) -> Result<Vec<T>> {
        let mut db = self.db_management.get_connection(false)?;
        let transaction = db.transaction()?;
        let ids = {
            let mut statement = transaction.prepare(&format!(
                "SELECT rowid FROM {} ORDER BY rowid LIMIT ?1 OFFSET ?2",
                T::TABLE.name
            ))?;
            statement
                .query_map([limit, offset], |row| row.get::<_, i64>(0))?
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        let records = transaction.query::<T>(&ids)?;
        transaction.commit()?;
        Ok(records)
    }

    pub fn query(&self, ids: &[i64]) -> Result<Vec<T>> {
        let mut db = self.db_management.get_connection(false)?;
        let transaction = db.transaction()?;
        let records = transaction.query::<T>(ids)?;
        transaction.commit()?;
        Ok(records)
    }
}
