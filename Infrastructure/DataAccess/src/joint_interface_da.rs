use crate::base_da::BaseDA;
use robot_db_objects::JointInteface;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type JointInterfaceDA = BaseDA<JointInteface>;
