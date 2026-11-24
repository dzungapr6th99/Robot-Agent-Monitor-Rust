use crate::base_da::BaseDA;
use crate::dapper_extensions::{DapperExtensions, log_sql};
use crate::traits::{DbManagementTrait, VlaModelSchemaTrait};
use anyhow::Result;
use robot_db_objects::VlaModel;
use rusqlite::{Connection, ToSql, types::Value as Sql};

// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type VlaModelDA = BaseDA<VlaModel>;

const CREATE_TABLE_SQL: &str = "
CREATE TABLE IF NOT EXISTS vla_models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    robot_configuration_id INTEGER NOT NULL REFERENCES robot_configurations(id),
    code TEXT NOT NULL,
    display_name TEXT NOT NULL,
    model_type TEXT NOT NULL,
    version TEXT NOT NULL,
    description TEXT,
    format_version INTEGER NOT NULL DEFAULT 1,
    package_sha256 TEXT NOT NULL,
    package_bytes INTEGER NOT NULL,
    storage_path TEXT NOT NULL,
    manifest_json TEXT NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    UNIQUE (robot_configuration_id, code, version)
);
CREATE INDEX IF NOT EXISTS idx_vla_models_robot_configuration_id
    ON vla_models (robot_configuration_id);
";

pub fn ensure_vla_models_table(connection: &Connection) -> Result<bool> {
    robot_common_lib::logged_result!("ensure_vla_models_table", {
        log_sql(CREATE_TABLE_SQL.trim(), std::iter::empty());
        connection.execute_batch(CREATE_TABLE_SQL)?;
        Ok(true)
    })
}

fn table_exists(connection: &Connection) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'vla_models')",
        [],
        |row| row.get(0),
    )?)
}

impl VlaModelSchemaTrait for BaseDA<VlaModel> {
    fn ensure_table(&self) -> Result<bool> {
        robot_common_lib::logged_result!("ensure_table", {
            let db = self.db_management.get_connection(true)?;
            ensure_vla_models_table(&db)
        })
    }

    fn count_by_configuration(&self, configuration_id: i64) -> Result<i64> {
        robot_common_lib::logged_result!("count_by_configuration", {
            let db = self.db_management.get_connection(false)?;
            if !table_exists(&db)? {
                return Ok(0);
            }
            let sql_text = "SELECT COUNT(*) FROM vla_models WHERE robot_configuration_id = ?1";
            log_sql(sql_text, ["?1"]);
            Ok(db.query_row(sql_text, [configuration_id], |row| row.get(0))?)
        })
    }

    fn search(&self, configuration_id: Option<i64>, query: Option<&str>) -> Result<Vec<VlaModel>> {
        robot_common_lib::logged_result!("search_vla_models", {
            let mut db = self.db_management.get_connection(false)?;
            if !table_exists(&db)? {
                return Ok(Vec::new());
            }
            let transaction = db.transaction()?;
            let mut conditions = Vec::new();
            let mut params: Vec<(String, Sql)> = Vec::new();
            if let Some(config_id) = configuration_id {
                conditions.push("robot_configuration_id = :config_id".to_owned());
                params.push((":config_id".into(), Sql::Integer(config_id)));
            }
            if let Some(q) = query.map(str::trim).filter(|q| !q.is_empty()) {
                let escaped = q
                    .to_lowercase()
                    .replace('\\', "\\\\")
                    .replace('%', "\\%")
                    .replace('_', "\\_");
                let pattern = format!("%{escaped}%");
                conditions.push(
                    "(LOWER(code) LIKE :q ESCAPE '\\' \
                     OR LOWER(display_name) LIKE :q ESCAPE '\\' \
                     OR LOWER(version) LIKE :q ESCAPE '\\' \
                     OR LOWER(model_type) LIKE :q ESCAPE '\\')"
                        .to_owned(),
                );
                params.push((":q".into(), Sql::Text(pattern)));
            }
            let where_clause = if conditions.is_empty() {
                String::new()
            } else {
                format!(" WHERE {}", conditions.join(" AND "))
            };
            let sql_text = format!("SELECT rowid FROM vla_models{where_clause} ORDER BY id DESC");
            log_sql(&sql_text, params.iter().map(|(name, _)| name.as_str()));
            let ids = {
                let bindings: Vec<(&str, &dyn ToSql)> = params
                    .iter()
                    .map(|(name, val)| (name.as_str(), val as &dyn ToSql))
                    .collect();
                let mut statement = transaction.prepare(&sql_text)?;
                statement
                    .query_map(bindings.as_slice(), |row| row.get::<_, i64>(0))?
                    .collect::<rusqlite::Result<Vec<_>>>()?
            };
            let records = transaction.query::<VlaModel>(&ids)?;
            transaction.commit()?;
            Ok(records)
        })
    }

    fn exists_version(
        &self,
        connection: &Connection,
        configuration_id: i64,
        code: &str,
        version: &str,
    ) -> Result<bool> {
        robot_common_lib::logged_result!("exists_version", {
            let sql_text = "SELECT EXISTS (SELECT 1 FROM vla_models WHERE robot_configuration_id = ?1 AND code = ?2 AND version = ?3)";
            log_sql(sql_text, ["?1", "?2", "?3"]);
            Ok(connection.query_row(sql_text, (configuration_id, code, version), |row| {
                row.get(0)
            })?)
        })
    }

    fn update_storage_path(
        &self,
        connection: &Connection,
        id: i64,
        storage_path: &str,
    ) -> Result<usize> {
        robot_common_lib::logged_result!("update_storage_path", {
            let sql_text = "UPDATE vla_models SET storage_path = ?1, updated_at = CURRENT_TIMESTAMP WHERE id = ?2";
            log_sql(sql_text, ["?1", "?2"]);
            Ok(connection.execute(sql_text, (storage_path, id))?)
        })
    }
}
