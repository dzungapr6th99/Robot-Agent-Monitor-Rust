use crate::base_da::BaseDA;
use robot_db_objects::PosePreset;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type PosePresetDA = BaseDA<PosePreset>;
