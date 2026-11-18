use crate::base_bl::BaseBL;
use robot_db_objects::SensorStream;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type SensorStreamBL = BaseBL<SensorStream>;
