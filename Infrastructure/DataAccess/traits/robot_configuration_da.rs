use super::BaseDATrait;
use robot_db_objects::RobotConfiguration;

pub trait RobotConfigurationDATrait: BaseDATrait<RobotConfiguration> {
    fn profile_exists(
        &self,
        connection: &rusqlite::Connection,
        profile: i64,
    ) -> anyhow::Result<bool>;
    fn update_urdf_path(
        &self,
        connection: &rusqlite::Connection,
        id: i64,
        path: &str,
    ) -> anyhow::Result<usize>;
}
