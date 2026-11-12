use anyhow::Result;
use rusqlite::Connection;

pub trait DbManagementTrait: Send + Sync {
    fn get_connection(&self, writable: bool) -> Result<Connection>;
}
