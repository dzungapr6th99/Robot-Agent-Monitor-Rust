use crate::base_da::BaseDA;
use robot_db_objects::RobotProfile;
// Corresponds to the C# class that specializes BaseDA<T>.
pub type RobotProfileDA = BaseDA<RobotProfile>;
