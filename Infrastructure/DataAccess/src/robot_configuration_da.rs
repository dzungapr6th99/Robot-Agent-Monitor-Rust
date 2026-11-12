use crate::base_da::BaseDA;
use crate::traits::RobotConfigurationDATrait;
use robot_db_objects::RobotConfiguration;
// Corresponds to the C# class that specializes BaseDA<T>.
pub type RobotConfigurationDA = BaseDA<RobotConfiguration>;

impl RobotConfigurationDATrait for BaseDA<RobotConfiguration> {
    fn profile_exists(
        &self,
        connection: &rusqlite::Connection,
        profile: i64,
    ) -> anyhow::Result<bool> {
        robot_common_lib::logged_result!("profile_exists", {
            Ok(connection.query_row(
                "SELECT EXISTS (SELECT 1 FROM robot_profiles WHERE id = ?1)",
                [profile],
                |row| row.get(0),
            )?)
        })
    }

    fn update_urdf_path(
        &self,
        connection: &rusqlite::Connection,
        id: i64,
        path: &str,
    ) -> anyhow::Result<usize> {
        robot_common_lib::logged_result!("update_urdf_path", {
            Ok(connection.execute(
                "UPDATE robot_configurations SET urdf_path = ?1 WHERE id = ?2",
                (path, id),
            )?)
        })
    }
}
