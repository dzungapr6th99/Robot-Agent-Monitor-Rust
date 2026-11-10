use crate::base_bl::BaseBL;
use robot_db_objects::EpisodeAnnotation;
// Corresponds to the C# class that specializes BaseBL<T>.
pub type EpisodeAnnotationBL = BaseBL<EpisodeAnnotation>;
