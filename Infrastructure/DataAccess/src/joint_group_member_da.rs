use crate::base_da::BaseDA;
use robot_db_objects::JointGroupMembers;
// Corresponds to the C# class that specializes BaseDA<T>.
pub type JointGroupMemberDA = BaseDA<JointGroupMembers>;
