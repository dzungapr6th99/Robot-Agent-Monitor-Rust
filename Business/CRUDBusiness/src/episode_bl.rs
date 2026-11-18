use crate::base_bl::BaseBL;
use robot_db_objects::Episode;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type EpisodeBL = BaseBL<Episode>;
