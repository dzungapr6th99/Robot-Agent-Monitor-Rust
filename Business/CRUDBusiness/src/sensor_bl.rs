use crate::base_bl::BaseBL;
use robot_db_objects::Sensor;
// Corresponds to the C# class that specializes BaseBL<T>.
pub type SensorBL = BaseBL<Sensor>;
