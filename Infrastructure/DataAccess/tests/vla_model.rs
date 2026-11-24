use anyhow::Result;
use robot_data_access::{
    dapper_extensions::DapperExtensions,
    vla_model_da::ensure_vla_models_table,
};
use robot_db_objects::{Record, VlaModel};
use rusqlite::Connection;
use serde_json::json;

fn setup_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.execute_batch(
        "CREATE TABLE robot_configurations (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT NOT NULL
        );
        INSERT INTO robot_configurations (id, name) VALUES (1, 'YellowArm');",
    )?;
    Ok(conn)
}

#[test]
fn ensure_table_is_idempotent_and_decodes_vla_models() -> Result<()> {
    let conn = setup_in_memory()?;
    assert!(ensure_vla_models_table(&conn)?);
    assert!(ensure_vla_models_table(&conn)?);

    let record = Record::from_iter([
        ("robotConfigurationId".into(), json!(1)),
        ("code".into(), json!("yellowarm-smolvla")),
        ("displayName".into(), json!("YellowArm Pick Cube")),
        ("modelType".into(), json!("smolvla")),
        ("version".into(), json!("2026-11-20")),
        ("description".into(), json!("Test checkpoint")),
        ("formatVersion".into(), json!(1)),
        ("packageSha256".into(), json!("a".repeat(64))),
        ("packageBytes".into(), json!(4096)),
        ("storagePath".into(), json!("vla/1/1")),
        ("manifestJson".into(), json!("{}")),
    ]);

    let id = conn.insert::<VlaModel>(&record)?;
    assert_eq!(id, 1);

    let rows = conn.query::<VlaModel>(&[id])?;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].robot_configuration_id, Some(1));
    assert_eq!(rows[0].code.as_deref(), Some("yellowarm-smolvla"));
    assert_eq!(rows[0].display_name.as_deref(), Some("YellowArm Pick Cube"));
    assert_eq!(rows[0].model_type.as_deref(), Some("smolvla"));
    assert_eq!(rows[0].version.as_deref(), Some("2026-11-20"));
    assert_eq!(rows[0].format_version, Some(1));
    assert_eq!(rows[0].package_bytes, Some(4096));

    // Duplicate (robot_configuration_id, code, version) must fail.
    assert!(conn.insert::<VlaModel>(&record).is_err());
    Ok(())
}
