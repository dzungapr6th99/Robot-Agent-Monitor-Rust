use crate::base_da::BaseDA;
use robot_db_objects::SensorStream;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type SensorStreamDA = BaseDA<SensorStream>;
