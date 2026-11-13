use anyhow::{Context, Result, ensure};
use robot_db_objects::{DbObject, DbTableAttribute, Record, Resource};
use rusqlite::{
    Connection, OptionalExtension, Row, params_from_iter,
    types::{Value as Sql, ValueRef},
};
use serde_json::{Value, json};

pub use crate::traits::DapperExtensions;

impl DapperExtensions for Connection {
    fn insert<T: DbObject>(&self, data: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("insert", { insert_record(self, T::RESOURCE, data) })
    }
    fn update<T: DbObject>(&self, data: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("update", {
            let resource = T::RESOURCE;
            let table = resource.table();
            let (where_clause, key) = identity(resource, data)?;
            let fields = values(&table, data, true)?;
            if fields.is_empty() {
                return Ok(0);
            }
            let assignments = fields
                .iter()
                .map(|(column, _)| format!("{column} = ?"))
                .collect::<Vec<_>>()
                .join(", ");
            let params = fields.into_iter().map(|(_, value)| value).chain(key);
            Ok(self.execute(
                &format!(
                    "UPDATE {} SET {assignments} WHERE {where_clause}",
                    table.name
                ),
                params_from_iter(params),
            )? as i64)
        })
    }

    fn delete<T: DbObject>(&self, data: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("delete", {
            let resource = T::RESOURCE;
            let table = resource.table();
            let (where_clause, key) = identity(resource, data)?;
            Ok(self.execute(
                &format!("DELETE FROM {} WHERE {where_clause}", table.name),
                params_from_iter(key),
            )? as i64)
        })
    }

    fn query<T: DbObject>(&self, ids: &[i64]) -> Result<Vec<T>> {
        robot_common_lib::logged_result!("query", {
            if ids.is_empty() {
                return Ok(Vec::new());
            }
            let meta = T::TABLE;
            let mut rows = Vec::new();
            {
                let mut statement = self.prepare(&format!(
                    "SELECT rowid AS _agent_id, * FROM {} WHERE rowid = ?1",
                    meta.name
                ))?;
                for id in ids {
                    if let Some(mut data) = statement
                        .query_row([id], |row| decode(row, T::RESOURCE, &meta))
                        .optional()?
                    {
                        if let Some((property, child, foreign_key)) = meta.child {
                            let child_table = child.table();
                            let foreign_column =
                                child_table.column_for(foreign_key).with_context(|| {
                                    format!(
                                        "No column mapping for {}.{foreign_key}",
                                        child_table.name
                                    )
                                })?;
                            let mut child_statement = self.prepare(&format!(
                                "SELECT rowid AS _agent_id, * FROM {} WHERE {} = ?1 ORDER BY rowid",
                                child_table.name, foreign_column
                            ))?;
                            let children = child_statement
                                .query_map([id], |row| decode(row, child, &child_table))?
                                .collect::<rusqlite::Result<Vec<_>>>()?;
                            data.insert(property.into(), json!(children));
                        }
                        rows.push(data);
                    }
                }
            }
            rows.into_iter()
                .map(|row| Ok(serde_json::from_value(json!(row))?))
                .collect()
        })
    }
}

fn insert_record(tx: &Connection, resource: Resource, data: &Record) -> Result<i64> {
    robot_common_lib::logged_result!("insert_record", {
        let table = resource.table();
        let fields = values(&table, data, false)?;
        let (columns, params): (Vec<_>, Vec<_>) = fields.into_iter().unzip();
        ensure!(!columns.is_empty(), "No fields to insert");
        let placeholders = vec!["?"; columns.len()].join(", ");
        tx.execute(
            &format!(
                "INSERT INTO {} ({}) VALUES ({placeholders})",
                table.name,
                columns.join(", ")
            ),
            params_from_iter(params),
        )?;
        Ok(tx.last_insert_rowid())
    })
}

fn identity(resource: Resource, data: &Record) -> Result<(String, Vec<Sql>)> {
    robot_common_lib::logged_result!("identity", {
        if let Some(id) = data.get("id").and_then(Value::as_i64).filter(|id| *id > 0) {
            return Ok(("rowid = ?".into(), vec![Sql::Integer(id)]));
        }
        let table = resource.table();
        ensure!(table.keys != ["id"], "Record ID is required");
        let mut conditions = Vec::new();
        let mut parameters = Vec::new();
        for key in table.keys {
            let value = match data.get(*key) {
                Some(Value::Number(number)) => {
                    Sql::Integer(number.as_i64().context("Invalid key")?)
                }
                Some(Value::String(value)) => Sql::Text(value.clone()),
                _ => anyhow::bail!("Missing or invalid record key {key}"),
            };
            let column = table
                .column_for(key)
                .with_context(|| format!("No column mapping for {}.{key}", table.name))?;
            conditions.push(format!("{column} = ?"));
            parameters.push(value);
        }
        Ok((conditions.join(" AND "), parameters))
    })
}

fn values(table: &DbTableAttribute, data: &Record, updating: bool) -> Result<Vec<(String, Sql)>> {
    robot_common_lib::logged_result!("values", {
        table
            .fields
            .iter()
            .filter(|(field, _)| !updating || *field != "jointName")
            .filter_map(|&(field, column)| data.get(field).map(|value| (field, column, value)))
            .map(|(field, column, value)| {
                let value = match value {
                    Value::Null => Sql::Null,
                    Value::Bool(value) => Sql::Integer(i64::from(*value)),
                    Value::String(value) => Sql::Text(value.clone()),
                    Value::Number(value) => {
                        if let Some(number) = value.as_i64() {
                            Sql::Integer(number)
                        } else {
                            Sql::Real(value.as_f64().context("Invalid number")?)
                        }
                    }
                    _ => anyhow::bail!("Expected scalar value for {field}"),
                };
                Ok((column.to_owned(), value))
            })
            .collect()
    })
}

fn decode(row: &Row<'_>, resource: Resource, table: &DbTableAttribute) -> rusqlite::Result<Record> {
    let mut record = Record::new();
    record.insert("id".into(), json!(row.get::<_, i64>("_agent_id")?));
    for (field, column) in table
        .fields
        .iter()
        .copied()
        .chain([("createdAt", "created_at"), ("updatedAt", "updated_at")])
    {
        let value = match row.get_ref(column) {
            Ok(ValueRef::Integer(value)) if field.starts_with("is") => json!(value != 0),
            Ok(ValueRef::Integer(value)) => json!(value),
            Ok(ValueRef::Real(value)) => json!(value),
            Ok(ValueRef::Text(value)) => json!(String::from_utf8_lossy(value)),
            Ok(ValueRef::Null) | Err(rusqlite::Error::InvalidColumnName(_)) => {
                if matches!(field, "createdAt" | "updatedAt") {
                    json!("0001-01-01T00:00:00")
                } else {
                    Value::Null
                }
            }
            Ok(ValueRef::Blob(_)) => Value::Null,
            Err(error) => return Err(error),
        };
        record.insert(field.into(), value);
    }
    if let Some((property, _, _)) = table.child {
        record.insert(property.into(), Value::Null);
    }
    if resource == Resource::RobotConfiguration {
        record.insert("jointGroups".into(), Value::Null);
    }
    Ok(record)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_column_with_name_unrelated_to_api_field() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        let table = DbTableAttribute {
            fields: &[("description", "legacy_caption")],
            ..Resource::RobotProfile.table()
        };
        let record = connection.query_row(
            "SELECT 1 AS _agent_id, 'Mapped description' AS legacy_caption",
            [],
            |row| decode(row, Resource::RobotProfile, &table),
        )?;
        let profile: robot_db_objects::RobotProfile = serde_json::from_value(json!(record))?;
        assert_eq!(profile.descriptions.as_deref(), Some("Mapped description"));
        assert_eq!(
            serde_json::to_value(profile)?["description"],
            "Mapped description"
        );
        Ok(())
    }
}
