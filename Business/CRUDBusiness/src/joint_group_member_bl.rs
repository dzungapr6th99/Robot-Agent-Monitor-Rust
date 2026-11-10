use crate::base_bl::BaseBL;
use robot_db_objects::JointGroupMembers;
// Corresponds to the C# class that specializes BaseBL<T>.
pub type JointGroupMemberBL = BaseBL<JointGroupMembers>;
