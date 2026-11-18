use crate::base_da::BaseDA;
use robot_db_objects::JointGroup;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type JointGroupDA = BaseDA<JointGroup>;
