use crate::base_da::BaseDA;
use robot_db_objects::Sensor;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type SensorDA = BaseDA<Sensor>;
