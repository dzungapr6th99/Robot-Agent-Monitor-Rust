use crate::base_bl::BaseBL;
use robot_db_objects::JointGroup;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type JointGroupBL = BaseBL<JointGroup>;
