use crate::base_bl::BaseBL;
use robot_db_objects::JointInteface;
// Corresponds to the C# class that specializes BaseBL<T>.
pub type JointInterfaceBL = BaseBL<JointInteface>;
