use crate::base_bl::BaseBL;
use robot_db_objects::EpisodeAnnotation;
// Entity business logic uses the generic BaseBL<T> CRUD implementation.
pub type EpisodeAnnotationBL = BaseBL<EpisodeAnnotation>;
