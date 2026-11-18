use crate::base_da::BaseDA;
use robot_db_objects::JointGroupMembers;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type JointGroupMemberDA = BaseDA<JointGroupMembers>;
