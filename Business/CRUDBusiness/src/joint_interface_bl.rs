use crate::base_bl::BaseBL;
use robot_db_objects::JointInteface;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type JointInterfaceBL = BaseBL<JointInteface>;
