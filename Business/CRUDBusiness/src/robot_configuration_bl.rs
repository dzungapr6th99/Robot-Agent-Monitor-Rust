use anyhow::{Context, Result};
use robot_common_lib::{Upload, ValidationError, urdf_storage::UrdfStorage};
use robot_data_access::{RobotConfigurationDA, dapper_extensions::DapperExtensions};
use robot_db_objects::{Record, RobotConfiguration};
use serde_json::{Value, json};
use std::sync::Arc;

pub struct RobotConfigurationBL {
    pub data_access: Arc<RobotConfigurationDA>,
    pub max_urdf_bytes: usize,
    pub urdf_storage: UrdfStorage,
}

impl RobotConfigurationBL {
    pub fn insert(&self, mut data: Record, upload: Option<Upload>) -> Result<i64> {
        validate_configuration(&data)?;
        let upload = upload.ok_or(ValidationError(-100100005, "URDF file is required"))?;
        self.validate_urdf(&upload)?;
        let profile = data["robotProfileId"]
            .as_i64()
            .context("Missing profile ID")?;
        let mut db = self.data_access.db_management.get_connection(true)?;
        let transaction = db.transaction()?;
        if !self.data_access.profile_exists(&transaction, profile)? {
            return Err(ValidationError(-100100003, "Robot profile does not exist").into());
        }
        let database_error =
            || ValidationError(-100100019, "Cannot save robot configuration to database");
        data.insert("urdfPath".into(), json!("pending.urdf"));
        let id = transaction
            .insert::<RobotConfiguration>(&data)
            .map_err(|_| database_error())?;
        let relative = self.urdf_storage.store(&upload, profile, id)?;
        let result = (|| -> Result<()> {
            self.data_access
                .update_urdf_path(&transaction, id, &relative)
                .map_err(|_| database_error())?;
            transaction.commit().map_err(|_| database_error())?;
            Ok(())
        })();
        if result.is_err() {
            self.urdf_storage.remove_stored(&relative);
        }
        result?;
        Ok(id)
    }

    pub fn asset_manifest(&self, id: i64) -> Result<Vec<String>> {
        let config = self
            .query(&[id])?
            .into_iter()
            .next()
            .context("Configuration not found")?;
        self.urdf_storage
            .manifest(&config.urdf_path.context("URDF not found")?)
    }
    pub fn read_asset(&self, id: i64, path: &str) -> Result<(Vec<u8>, &'static str)> {
        let configuration = self
            .query(&[id])?
            .into_iter()
            .next()
            .context("Configuration not found")?;
        let urdf = configuration.urdf_path.context("URDF not found")?;
        self.urdf_storage.read_asset(&urdf, path)
    }
    pub fn update(&self, data: Record) -> Result<i64> {
        self.data_access.update(&data)
    }
    pub fn delete(&self, data: Record) -> Result<i64> {
        self.data_access.delete(&data)
    }
    pub fn list(&self, limit: u32, offset: u32) -> Result<Vec<RobotConfiguration>> {
        self.data_access.list(limit, offset)
    }
    pub fn query(&self, ids: &[i64]) -> Result<Vec<RobotConfiguration>> {
        self.data_access.query(ids)
    }
    fn validate_urdf(&self, file: &Upload) -> Result<()> {
        if file.bytes.is_empty() {
            return Err(ValidationError(-100100006, "URDF file is empty").into());
        }
        if file.bytes.len() > self.max_urdf_bytes {
            return Err(ValidationError(-100100008, "URDF file exceeds the allowed size").into());
        }
        let extension = std::path::Path::new(&file.filename)
            .extension()
            .and_then(|ext| ext.to_str());
        if !extension
            .is_some_and(|ext| ext.eq_ignore_ascii_case("urdf") || ext.eq_ignore_ascii_case("xml"))
        {
            return Err(
                ValidationError(-100100007, "URDF file extension must be .urdf or .xml").into(),
            );
        }
        let invalid = || ValidationError(-100100009, "URDF XML is invalid");
        let document =
            roxmltree::Document::parse(std::str::from_utf8(&file.bytes).map_err(|_| invalid())?)
                .map_err(|_| invalid())?;
        let robot = document.root_element();
        if robot.tag_name().name() != "robot" {
            return Err(ValidationError(-100100010, "URDF root element must be 'robot'").into());
        }
        if name(robot).is_none() {
            return Err(ValidationError(-100100011, "URDF robot name is required").into());
        }
        let mut links = std::collections::HashSet::new();
        let bad_link = || ValidationError(-100100012, "URDF contains an invalid link");
        for link in robot.children().filter(|node| node.has_tag_name("link")) {
            if !links.insert(name(link).ok_or_else(bad_link)?) {
                return Err(
                    ValidationError(-100100013, "URDF contains duplicate link names").into(),
                );
            }
        }
        if links.is_empty() {
            return Err(bad_link().into());
        }
        let mut joints = std::collections::HashSet::new();
        for joint in robot.children().filter(|node| node.has_tag_name("joint")) {
            let invalid = || ValidationError(-100100014, "URDF contains an invalid joint");
            let joint_name = name(joint).ok_or_else(invalid)?;
            let endpoint = |tag| {
                joint
                    .children()
                    .find(|node| node.has_tag_name(tag))
                    .and_then(|node| node.attribute("link"))
                    .map(str::trim)
                    .filter(|link| !link.is_empty())
            };
            let parent = endpoint("parent").ok_or_else(invalid)?;
            let child = endpoint("child").ok_or_else(invalid)?;
            if parent == child
                || !matches!(
                    joint.attribute("type").map(str::trim),
                    Some("revolute" | "continuous" | "prismatic" | "fixed" | "floating" | "planar")
                )
            {
                return Err(invalid().into());
            }
            if !joints.insert(joint_name) {
                return Err(
                    ValidationError(-100100015, "URDF contains duplicate joint names").into(),
                );
            }
            if !links.contains(parent) || !links.contains(child) {
                return Err(ValidationError(
                    -100100016,
                    "URDF joint references an unknown parent or child link",
                )
                .into());
            }
        }
        Ok(())
    }
}

fn name<'a>(node: roxmltree::Node<'a, '_>) -> Option<&'a str> {
    node.attribute("name")
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn validate_configuration(data: &Record) -> Result<()> {
    if !data
        .get("robotProfileId")
        .and_then(Value::as_i64)
        .is_some_and(|id| id > 0)
    {
        return Err(ValidationError(-100100002, "Robot profile id is invalid").into());
    }
    if !data
        .get("name")
        .and_then(Value::as_str)
        .is_some_and(|name| !name.trim().is_empty())
    {
        return Err(ValidationError(
            -100100004,
            "Robot configuration name must not be null or empty",
        )
        .into());
    }
    Ok(())
}
