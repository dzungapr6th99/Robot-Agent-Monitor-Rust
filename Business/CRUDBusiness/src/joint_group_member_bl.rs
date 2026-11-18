use crate::base_bl::BaseBL;
use robot_db_objects::JointGroupMembers;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type JointGroupMemberBL = BaseBL<JointGroupMembers>;
