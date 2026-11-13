use crate::traits::BaseDATrait;
use crate::traits::DbManagementTrait;
use crate::{
    dapper_extensions::{DapperExtensions, log_sql, quote_identifier},
    db_management::DbManagement,
};
use anyhow::Result;
use robot_db_objects::{DbObject, Record};
use rusqlite::named_params;
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
}

impl<T: DbObject> BaseDATrait<T> for BaseDA<T> {
    fn insert(&self, data: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("insert", {
            let mut db = self.db_management.get_connection(true)?;
            let transaction = db.transaction()?;
            let id = transaction.insert::<T>(data)?;
            transaction.commit()?;
            Ok(id)
        })
    }

    fn insert_many(&self, records: &[Record]) -> Result<usize> {
        robot_common_lib::logged_result!("insert_many", {
            self.db_management
                .get_connection(true)?
                .insert_many::<T>(records)
        })
    }

    fn update(&self, data: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("update", {
            self.db_management.get_connection(true)?.update::<T>(data)
        })
    }

    fn delete(&self, data: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("delete", {
            self.db_management.get_connection(true)?.delete::<T>(data)
        })
    }

    fn list(&self, limit: u32, offset: u32) -> Result<Vec<T>> {
        robot_common_lib::logged_result!("list", {
            let mut db = self.db_management.get_connection(false)?;
            let transaction = db.transaction()?;
            let ids = {
                let sql_text = format!(
                    "SELECT rowid FROM {} ORDER BY rowid LIMIT :limit OFFSET :offset",
                    quote_identifier(T::TABLE.name)
                );
                log_sql(&sql_text, [":limit", ":offset"]);
                let mut statement = transaction.prepare(&sql_text)?;
                statement
                    .query_map(named_params! {":limit": limit, ":offset": offset}, |row| {
                        row.get::<_, i64>(0)
                    })?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            let records = transaction.query::<T>(&ids)?;
            transaction.commit()?;
            Ok(records)
        })
    }

    fn query(&self, ids: &[i64]) -> Result<Vec<T>> {
        robot_common_lib::logged_result!("query", {
            let mut db = self.db_management.get_connection(false)?;
            let transaction = db.transaction()?;
            let records = transaction.query::<T>(ids)?;
            transaction.commit()?;
            Ok(records)
        })
    }
}
