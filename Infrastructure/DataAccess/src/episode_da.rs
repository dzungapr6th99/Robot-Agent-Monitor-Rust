use crate::base_da::BaseDA;
use robot_db_objects::Episode;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type EpisodeDA = BaseDA<Episode>;
