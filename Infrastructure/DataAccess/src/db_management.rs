use anyhow::Result;
use rusqlite::{Connection, OpenFlags};
use std::{path::PathBuf, time::Duration};

pub struct DbManagement(pub PathBuf);

impl DbManagement {
    pub fn get_connection(&self, writable: bool) -> Result<Connection> {
        let flags = if writable {
            OpenFlags::SQLITE_OPEN_READ_WRITE
        } else {
            OpenFlags::SQLITE_OPEN_READ_ONLY
        };
        let connection = Connection::open_with_flags(&self.0, flags)?;
        connection.busy_timeout(Duration::from_secs(5))?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }
}
