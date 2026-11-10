use crate::base_bl::BaseBL;
use robot_db_objects::RobotProfile;
// Corresponds to the C# class that specializes BaseBL<T>.
pub type RobotProfileBL = BaseBL<RobotProfile>;
