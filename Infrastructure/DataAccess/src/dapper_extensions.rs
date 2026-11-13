use anyhow::{Context, Result, ensure};
use robot_db_objects::{DbObject, DbTableAttribute, Record, Resource};
use rusqlite::{
    Connection, OptionalExtension, Row, ToSql, named_params,
    types::{Value as Sql, ValueRef},
};
use serde_json::{Value, json};

pub use crate::traits::DapperExtensions;

impl DapperExtensions for Connection {
    fn count<T: DbObject>(&self, filter: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("count", {
            let (sql_text, execute_params) = build_count_sql_text(&T::TABLE, filter)?;
            log_sql(
                &sql_text,
                execute_params.iter().map(|(name, _)| name.as_str()),
            );
            Ok(self.query_row(
                &sql_text,
                named_bindings(&execute_params).as_slice(),
                |row| row.get(0),
            )?)
        })
    }

    fn get_first_or_default<T: DbObject>(&self, filter: &Record) -> Result<Option<T>> {
        robot_common_lib::logged_result!("get_first_or_default", {
            Ok(get_records::<T>(self, filter, true)?.into_iter().next())
        })
    }

    fn get<T: DbObject>(&self, filter: &Record) -> Result<Vec<T>> {
        robot_common_lib::logged_result!("get", { get_records::<T>(self, filter, false) })
    }

    fn insert<T: DbObject>(&self, data: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("insert", {
            let (sql_text, execute_params) = build_insert_sql_text(&T::TABLE, data)?;
            log_sql(
                &sql_text,
                execute_params.iter().map(|(name, _)| name.as_str()),
            );
            self.execute(&sql_text, named_bindings(&execute_params).as_slice())?;
            Ok(self.last_insert_rowid())
        })
    }

    fn insert_many<T: DbObject>(&self, records: &[Record]) -> Result<usize> {
        robot_common_lib::logged_result!("insert_many", {
            let Some((first, rest)) = records.split_first() else {
                return Ok(0);
            };
            let (sql_text, rows) = build_insert_many_sql_text(&T::TABLE, first, rest)?;
            log_sql(&sql_text, [":rows"]);
            Ok(self.execute(&sql_text, named_params! {":rows": rows})?)
        })
    }

    fn update<T: DbObject>(&self, data: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("update", {
            let Some((sql_text, execute_params)) = build_update_sql_text(&T::TABLE, data)? else {
                return Ok(0);
            };
            log_sql(
                &sql_text,
                execute_params.iter().map(|(name, _)| name.as_str()),
            );
            Ok(self.execute(&sql_text, named_bindings(&execute_params).as_slice())? as i64)
        })
    }

    fn delete<T: DbObject>(&self, data: &Record) -> Result<i64> {
        robot_common_lib::logged_result!("delete", {
            let (sql_text, execute_params) = build_delete_sql_text(&T::TABLE, data)?;
            log_sql(
                &sql_text,
                execute_params.iter().map(|(name, _)| name.as_str()),
            );
            Ok(self.execute(&sql_text, named_bindings(&execute_params).as_slice())? as i64)
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
                let sql_text = build_get_by_id_sql_text(&meta, None);
                log_sql(&sql_text, [":id"]);
                let mut statement = self.prepare(&sql_text)?;
                for id in ids {
                    if let Some(mut data) = statement
                        .query_row(named_params! {":id": id}, |row| {
                            decode(row, T::RESOURCE, &meta)
                        })
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
                            let sql_text =
                                build_get_by_id_sql_text(&child_table, Some(foreign_column));
                            log_sql(&sql_text, [":id"]);
                            let mut child_statement = self.prepare(&sql_text)?;
                            let children = child_statement
                                .query_map(named_params! {":id": id}, |row| {
                                    decode(row, child, &child_table)
                                })?
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

// Identifiers come from DbTableAttribute. Record values are bound separately.
pub(crate) fn quote_identifier(identifier: &'static str) -> String {
    format!("\"{}\"", identifier.replace('"', "\"\""))
}

type ExecuteParams = Vec<(String, Sql)>;
type FieldValue = (&'static str, &'static str, Sql);

fn get_records<T: DbObject>(
    connection: &Connection,
    filter: &Record,
    first: bool,
) -> Result<Vec<T>> {
    robot_common_lib::logged_result!("get_records", {
        let (sql_text, execute_params) = build_get_sql_text(&T::TABLE, filter, first)?;
        log_sql(
            &sql_text,
            execute_params.iter().map(|(name, _)| name.as_str()),
        );
        let mut statement = connection.prepare(&sql_text)?;
        let ids = statement
            .query_map(named_bindings(&execute_params).as_slice(), |row| row.get(0))?
            .collect::<rusqlite::Result<Vec<i64>>>()?;
        connection.query::<T>(&ids)
    })
}

fn build_get_sql_text(
    table: &DbTableAttribute,
    filter: &Record,
    first: bool,
) -> Result<(String, ExecuteParams)> {
    robot_common_lib::logged_result!("build_get_sql_text", {
        let (where_clause, execute_params) = build_filter(table, filter)?;
        let limit = if first { " LIMIT 1" } else { "" };
        let sql_text = format!(
            "SELECT rowid FROM {}{where_clause} ORDER BY rowid{limit}",
            quote_identifier(table.name)
        );
        Ok((sql_text, execute_params))
    })
}

fn named_bindings(params: &ExecuteParams) -> Vec<(&str, &dyn ToSql)> {
    params
        .iter()
        .map(|(name, value)| (name.as_str(), value as &dyn ToSql))
        .collect()
}

fn build_count_sql_text(
    table: &DbTableAttribute,
    filter: &Record,
) -> Result<(String, ExecuteParams)> {
    robot_common_lib::logged_result!("build_count_sql_text", {
        let (where_clause, execute_params) = build_filter(table, filter)?;
        let sql_text = format!(
            "SELECT COUNT(*) FROM {}{where_clause}",
            quote_identifier(table.name)
        );
        Ok((sql_text, execute_params))
    })
}

fn build_filter(table: &DbTableAttribute, filter: &Record) -> Result<(String, ExecuteParams)> {
    robot_common_lib::logged_result!("build_filter", {
        let mut conditions = Vec::new();
        let mut execute_params = Vec::new();
        for (field, value) in filter {
            let column = if field == "id" {
                "rowid"
            } else {
                table
                    .column_for(field)
                    .context("Unknown count filter field")?
            };
            let column = quote_identifier(column);
            let values = value
                .as_array()
                .map(Vec::as_slice)
                .unwrap_or_else(|| std::slice::from_ref(value));
            let mut names = Vec::with_capacity(values.len());
            for value in values {
                let name = format!(":p_{}", execute_params.len());
                names.push(name.clone());
                execute_params.push((name, scalar_value(field, value)?));
            }
            if names.is_empty() {
                conditions.push("0 = 1".into());
            } else if value.is_array() {
                conditions.push(format!("{column} IN ({})", names.join(", ")));
            } else {
                conditions.push(format!("{column} IS {}", names[0]));
            }
        }
        let where_clause = if conditions.is_empty() {
            String::new()
        } else {
            format!(" WHERE {}", conditions.join(" AND "))
        };
        Ok((where_clause, execute_params))
    })
}

fn build_insert_sql_text(
    table: &DbTableAttribute,
    data: &Record,
) -> Result<(String, ExecuteParams)> {
    robot_common_lib::logged_result!("build_insert_sql_text", {
        let fields = values(table, data, false)?;
        let (columns, execute_params): (Vec<_>, ExecuteParams) = fields
            .into_iter()
            .map(|(field, column, value)| (quote_identifier(column), (format!(":{field}"), value)))
            .unzip();
        ensure!(!columns.is_empty(), "No fields to insert");
        let placeholders = execute_params
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        let sql_text = format!(
            "INSERT INTO {} ({}) VALUES ({placeholders})",
            quote_identifier(table.name),
            columns.join(", ")
        );
        Ok((sql_text, execute_params))
    })
}

fn build_insert_many_sql_text(
    table: &DbTableAttribute,
    first: &Record,
    rest: &[Record],
) -> Result<(String, String)> {
    robot_common_lib::logged_result!("build_insert_many_sql_text", {
        let fields: Vec<_> = table
            .fields
            .iter()
            .copied()
            .filter(|(field, _)| first.contains_key(*field))
            .collect();
        ensure!(!fields.is_empty(), "No fields to insert");
        let columns = fields
            .iter()
            .map(|(_, column)| quote_identifier(column))
            .collect::<Vec<_>>()
            .join(", ");
        let expressions = (0..fields.len())
            .map(|index| {
                let value = format!("json_extract(batch.value, '$[{index}]')");
                // Encode text losslessly, including embedded NUL bytes.
                format!("CASE json_type(batch.value, '$[{index}]') WHEN 'text' THEN CAST(unhex({value}) AS TEXT) ELSE {value} END")
            })
            .collect::<Vec<_>>()
            .join(", ");
        let mut rows = Vec::with_capacity(rest.len() + 1);
        for (index, record) in std::iter::once(first).chain(rest).enumerate() {
            ensure!(
                table
                    .fields
                    .iter()
                    .all(|(field, _)| first.contains_key(*field) == record.contains_key(*field)),
                "Batch row {index} has different insert columns"
            );
            let row = fields
                .iter()
                .map(|(field, _)| {
                    let value = scalar_value(field, &record[*field])?;
                    Ok(match value {
                        Sql::Null => Value::Null,
                        Sql::Integer(number) => json!(number),
                        Sql::Real(number) => json!(number),
                        Sql::Text(text) => Value::String(
                            text.as_bytes()
                                .iter()
                                .map(|byte| format!("{byte:02x}"))
                                .collect(),
                        ),
                        Sql::Blob(_) => anyhow::bail!("Unsupported batch field {field}"),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            rows.push(row);
        }
        let sql_text = format!(
            "INSERT OR ABORT INTO {} ({columns}) SELECT {expressions} FROM json_each(:rows) AS batch ORDER BY batch.key",
            quote_identifier(table.name)
        );
        Ok((sql_text, serde_json::to_string(&rows)?))
    })
}

fn build_update_sql_text(
    table: &DbTableAttribute,
    data: &Record,
) -> Result<Option<(String, ExecuteParams)>> {
    robot_common_lib::logged_result!("build_update_sql_text", {
        let (where_clause, mut execute_params) = build_identity(table, data)?;
        let fields = values(table, data, true)?;
        if fields.is_empty() {
            return Ok(None);
        }
        let mut assignments = Vec::with_capacity(fields.len());
        for (field, column, value) in fields {
            let name = format!(":{field}");
            assignments.push(format!("{} = {name}", quote_identifier(column)));
            execute_params.push((name, value));
        }
        let sql_text = format!(
            "UPDATE {} SET {} WHERE {where_clause}",
            quote_identifier(table.name),
            assignments.join(", ")
        );
        Ok(Some((sql_text, execute_params)))
    })
}

fn build_delete_sql_text(
    table: &DbTableAttribute,
    data: &Record,
) -> Result<(String, ExecuteParams)> {
    robot_common_lib::logged_result!("build_delete_sql_text", {
        let (where_clause, execute_params) = build_identity(table, data)?;
        let sql_text = format!(
            "DELETE FROM {} WHERE {where_clause}",
            quote_identifier(table.name)
        );
        Ok((sql_text, execute_params))
    })
}

fn build_get_by_id_sql_text(
    table: &DbTableAttribute,
    foreign_column: Option<&'static str>,
) -> String {
    let table_name = quote_identifier(table.name);
    match foreign_column {
        Some(column) => format!(
            "SELECT rowid AS _agent_id, * FROM {table_name} WHERE {} = :id ORDER BY rowid",
            quote_identifier(column)
        ),
        None => format!("SELECT rowid AS _agent_id, * FROM {table_name} WHERE rowid = :id"),
    }
}

pub(crate) fn log_sql<'a>(sql_text: &str, parameter_names: impl IntoIterator<Item = &'a str>) {
    let names = parameter_names.into_iter().collect::<Vec<_>>().join(", ");
    robot_common_lib::operation_log::info(
        module_path!(),
        format_args!("SQL Text: {sql_text} | Parameters: [{names}]"),
    );
}

fn build_identity(table: &DbTableAttribute, data: &Record) -> Result<(String, ExecuteParams)> {
    robot_common_lib::logged_result!("build_identity", {
        if let Some(id) = data.get("id").and_then(Value::as_i64).filter(|id| *id > 0) {
            return Ok((
                "rowid = :p_id".into(),
                vec![(":p_id".into(), Sql::Integer(id))],
            ));
        }
        ensure!(
            !table.keys.is_empty() && table.keys != ["id"],
            "Record ID is required"
        );
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
            let name = format!(":p_{key}");
            conditions.push(format!("{} = {name}", quote_identifier(column)));
            parameters.push((name, value));
        }
        Ok((conditions.join(" AND "), parameters))
    })
}

fn values(table: &DbTableAttribute, data: &Record, updating: bool) -> Result<Vec<FieldValue>> {
    robot_common_lib::logged_result!("values", {
        table
            .fields
            .iter()
            .filter(|(field, _)| !updating || *field != "jointName")
            .filter_map(|&(field, column)| data.get(field).map(|value| (field, column, value)))
            .map(|(field, column, value)| Ok((field, column, scalar_value(field, value)?)))
            .collect()
    })
}

fn scalar_value(field: &str, value: &Value) -> Result<Sql> {
    robot_common_lib::logged_result!("scalar_value", {
        Ok(match value {
            Value::Null => Sql::Null,
            Value::Bool(value) => Sql::Integer(i64::from(*value)),
            Value::String(value) => Sql::Text(value.clone()),
            Value::Number(value) => match value.as_i64() {
                Some(number) => Sql::Integer(number),
                None => Sql::Real(value.as_f64().context("Invalid number")?),
            },
            _ => anyhow::bail!("Expected scalar value for {field}"),
        })
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
    use robot_db_objects::{JointGroupMembers, RobotProfile};

    #[test]
    fn insert_many_binds_values_and_rolls_back_the_batch_on_error() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(
            "CREATE TABLE robot_profiles (
                name TEXT NOT NULL UNIQUE,
                description TEXT DEFAULT 'default description'
            );
            CREATE TABLE robot_configurations (robot_profile_id INTEGER NOT NULL);",
        )?;
        let first = Record::from_iter([("name".into(), json!("O'Brien\u{0}tail"))]);
        let second = Record::from_iter([
            ("name".into(), json!("second")),
            ("description".into(), Value::Null),
        ]);
        assert_eq!(connection.insert_many::<RobotProfile>(&[])?, 0);
        assert!(
            connection
                .insert_many::<RobotProfile>(&[first.clone(), second.clone()])
                .is_err()
        );
        assert_eq!(connection.count::<RobotProfile>(&Record::new())?, 0);
        let third = Record::from_iter([("name".into(), json!("third"))]);
        assert_eq!(
            connection.insert_many::<RobotProfile>(&[first.clone(), third])?,
            2
        );
        assert_eq!(connection.insert_many::<RobotProfile>(&[second])?, 1);
        let saved = connection.get::<RobotProfile>(&Record::new())?;
        assert_eq!(saved.len(), 3);
        assert_eq!(saved[0].name.as_deref(), Some("O'Brien\u{0}tail"));
        assert_eq!(
            saved[0].descriptions.as_deref(),
            Some("default description")
        );
        assert_eq!(saved[2].descriptions, None);

        let new_record = Record::from_iter([("name".into(), json!("rolled back"))]);
        assert!(
            connection
                .insert_many::<RobotProfile>(&[new_record, first])
                .is_err()
        );
        assert_eq!(connection.count::<RobotProfile>(&Record::new())?, 3);
        Ok(())
    }

    #[test]
    fn binds_sql_like_text_in_values_and_composite_keys() -> Result<()> {
        let connection = Connection::open_in_memory()?;
        connection.execute_batch(
            "CREATE TABLE joint_group_members (
                joint_group_id INTEGER NOT NULL,
                joint_name TEXT NOT NULL,
                sort_order INTEGER NOT NULL,
                UNIQUE(joint_group_id, joint_name)
            )",
        )?;
        let sql_like_name = "wrist' OR 1=1 --";
        let mut member = Record::from_iter([
            ("jointGroupId".into(), json!(1)),
            ("jointName".into(), json!(sql_like_name)),
            ("sortOrder".into(), json!(1)),
        ]);
        let id = connection.insert::<JointGroupMembers>(&member)?;
        let other = Record::from_iter([
            ("jointGroupId".into(), json!(1)),
            ("jointName".into(), json!("other")),
            ("sortOrder".into(), json!(2)),
        ]);
        let other_id = connection.insert::<JointGroupMembers>(&other)?;
        let filter = Record::from_iter([("jointName".into(), json!([sql_like_name]))]);
        assert_eq!(connection.count::<JointGroupMembers>(&filter)?, 1);
        assert_eq!(connection.get::<JointGroupMembers>(&filter)?.len(), 1);
        let saved = connection
            .get_first_or_default::<JointGroupMembers>(&filter)?
            .context("Expected the inserted joint")?;
        assert_eq!(saved.joint_name.as_deref(), Some(sql_like_name));
        member.insert("sortOrder".into(), json!(10));

        assert_eq!(connection.update::<JointGroupMembers>(&member)?, 1);
        let rows = connection.query::<JointGroupMembers>(&[id, other_id])?;
        assert_eq!(rows[0].joint_name.as_deref(), Some(sql_like_name));
        assert_eq!(rows[0].sort_order, 10);
        assert_eq!(rows[1].sort_order, 2);

        assert_eq!(connection.delete::<JointGroupMembers>(&member)?, 1);
        let remaining = connection.query::<JointGroupMembers>(&[id, other_id])?;
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].joint_name.as_deref(), Some("other"));
        Ok(())
    }

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
