use crate::base_da::BaseDA;
use robot_db_objects::SensorStream;
// Corresponds to the C# class that specializes BaseDA<T>.
pub type SensorStreamDA = BaseDA<SensorStream>;
