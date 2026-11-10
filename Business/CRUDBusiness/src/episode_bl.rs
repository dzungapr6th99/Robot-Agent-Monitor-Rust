use crate::base_bl::BaseBL;
use robot_db_objects::Episode;
// Corresponds to the C# class that specializes BaseBL<T>.
pub type EpisodeBL = BaseBL<Episode>;
