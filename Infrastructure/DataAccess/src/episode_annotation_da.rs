use crate::base_da::BaseDA;
use robot_db_objects::EpisodeAnnotation;
// Corresponds to the C# class that specializes BaseDA<T>.
pub type EpisodeAnnotationDA = BaseDA<EpisodeAnnotation>;
