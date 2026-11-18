use crate::base_da::BaseDA;
use robot_db_objects::EpisodeAnnotation;
// Entity data access uses the generic BaseDA<T> CRUD implementation.
pub type EpisodeAnnotationDA = BaseDA<EpisodeAnnotation>;
