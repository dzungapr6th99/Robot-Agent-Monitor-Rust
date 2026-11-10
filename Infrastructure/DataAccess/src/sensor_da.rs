use crate::base_da::BaseDA;
use robot_db_objects::Sensor;
// Corresponds to the C# class that specializes BaseDA<T>.
pub type SensorDA = BaseDA<Sensor>;
