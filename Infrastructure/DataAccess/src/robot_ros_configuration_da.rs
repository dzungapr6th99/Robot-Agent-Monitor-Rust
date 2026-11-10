use std::sync::Arc;

use crate::db_management::DbManagement;
use anyhow::{Result, bail, ensure};
use robot_common_lib::ros::{Direction, Topic};

pub struct RobotRosConfigurationDA(pub Arc<DbManagement>);
impl RobotRosConfigurationDA {
    pub fn topics(&self, id: i64) -> Result<Vec<Topic>> {
        let mut db = self.0.get_connection(false)?;
        let tx = db.transaction()?;
        ensure!(
            tx.query_row(
                "SELECT EXISTS (SELECT 1 FROM robot_configurations WHERE id = ?1)",
                [id],
                |row| row.get::<_, bool>(0)
            )?,
            "Robot configuration {id} does not exist"
        );

        let mut topics = Vec::new();
        {
            let mut statement = tx.prepare(
                "SELECT operation, ros_endpoint, data_type FROM joint_interfaces
                 WHERE robot_configuration_id = ?1 AND is_enabled = 1",
            )?;
            let mut rows = statement.query([id])?;
            while let Some(row) = rows.next()? {
                let direction = match row.get::<_, String>(0)?.as_str() {
                    "subscribe" => Direction::Subscribe,
                    "publish" => Direction::Publish,
                    operation => bail!("Unsupported joint operation {operation}"),
                };
                topics.push(Topic {
                    name: row.get(1)?,
                    message_type: row.get(2)?,
                    direction,
                    best_effort: false,
                    transient_local: false,
                });
            }
        }
        {
            let mut statement = tx.prepare(
                "SELECT stream.topic, stream.data_type FROM sensor_streams AS stream
                 JOIN sensors AS sensor ON sensor.id = stream.sensor_id
                 WHERE sensor.robot_configuration_id = ?1 AND sensor.is_enabled = 1 AND stream.is_enabled = 1")?;
            for row in statement.query_map([id], |row| {
                Ok(Topic {
                    name: row.get(0)?,
                    message_type: row.get(1)?,
                    direction: Direction::Subscribe,
                    best_effort: true,
                    transient_local: false,
                })
            })? {
                topics.push(row?);
            }
        }
        tx.commit()?;
        Ok(topics)
    }
}
