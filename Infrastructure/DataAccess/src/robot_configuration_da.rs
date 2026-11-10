use crate::base_da::BaseDA;
use robot_db_objects::RobotConfiguration;
// Corresponds to the C# class that specializes BaseDA<T>.
pub type RobotConfigurationDA = BaseDA<RobotConfiguration>;

impl BaseDA<RobotConfiguration> {
    pub fn profile_exists(
        &self,
        connection: &rusqlite::Connection,
        profile: i64,
    ) -> anyhow::Result<bool> {
        Ok(connection.query_row(
            "SELECT EXISTS (SELECT 1 FROM robot_profiles WHERE id = ?1)",
            [profile],
            |row| row.get(0),
        )?)
    }

    pub fn update_urdf_path(
        &self,
        connection: &rusqlite::Connection,
        id: i64,
        path: &str,
    ) -> anyhow::Result<()> {
        connection.execute(
            "UPDATE robot_configurations SET urdf_path = ?1 WHERE id = ?2",
            (path, id),
        )?;
        Ok(())
    }
}
