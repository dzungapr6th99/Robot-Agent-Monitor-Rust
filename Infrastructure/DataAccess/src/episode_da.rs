use crate::base_da::BaseDA;
use robot_db_objects::Episode;
// Corresponds to the C# class that specializes BaseDA<T>.
pub type EpisodeDA = BaseDA<Episode>;
