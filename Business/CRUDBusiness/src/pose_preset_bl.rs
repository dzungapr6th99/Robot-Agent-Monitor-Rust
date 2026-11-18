use crate::base_bl::BaseBL;
use robot_db_objects::PosePreset;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type PosePresetBL = BaseBL<PosePreset>;
