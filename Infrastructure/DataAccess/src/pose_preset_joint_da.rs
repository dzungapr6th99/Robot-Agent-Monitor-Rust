use crate::base_da::BaseDA;
use robot_db_objects::PosePresetJoint;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type PosePresetJointDA = BaseDA<PosePresetJoint>;
