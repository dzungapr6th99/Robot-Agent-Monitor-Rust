use crate::base_da::BaseDA;
use robot_db_objects::JointInteface;
// Corresponds to the C# class that specializes BaseDA<T>.
pub type JointInterfaceDA = BaseDA<JointInteface>;
