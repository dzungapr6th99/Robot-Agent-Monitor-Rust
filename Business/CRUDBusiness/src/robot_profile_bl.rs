use crate::base_bl::BaseBL;
use robot_db_objects::RobotProfile;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type RobotProfileBL = BaseBL<RobotProfile>;
