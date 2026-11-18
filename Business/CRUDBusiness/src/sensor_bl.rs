use crate::base_bl::BaseBL;
use robot_db_objects::Sensor;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type SensorBL = BaseBL<Sensor>;
