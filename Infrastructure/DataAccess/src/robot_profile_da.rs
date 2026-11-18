use crate::base_da::BaseDA;
use robot_db_objects::RobotProfile;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type RobotProfileDA = BaseDA<RobotProfile>;
