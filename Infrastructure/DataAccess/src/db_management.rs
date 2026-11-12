use crate::traits::DbManagementTrait;
use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use std::{path::PathBuf, time::Duration};

pub struct DbManagement(pub PathBuf);

impl DbManagementTrait for DbManagement {
    fn get_connection(&self, writable: bool) -> Result<Connection> {
        robot_common_lib::logged_result!("get_connection", {
            let flags = if writable {
                OpenFlags::SQLITE_OPEN_READ_WRITE
            } else {
                OpenFlags::SQLITE_OPEN_READ_ONLY
            };
            let connection = Connection::open_with_flags(&self.0, flags)?;
            connection.busy_timeout(Duration::from_secs(5))?;
            connection.pragma_update(None, "foreign_keys", "ON")?;
            Ok(connection)
        })
    }
}
