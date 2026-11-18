use crate::base_bl::BaseBL;
use robot_db_objects::PosePresetJoint;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type PosePresetJointBL = BaseBL<PosePresetJoint>;
