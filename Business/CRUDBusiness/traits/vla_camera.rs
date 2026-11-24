use anyhow::Result;
use robot_common_lib::vla::VlaImage;
use std::time::Duration;

pub trait VlaCameraFeedTrait: Send + Sync {
    fn start(&self, topics: &[(String, String)]) -> Result<bool>;
    fn latest(&self, camera: &str, max_age: Duration) -> Result<VlaImage>;
    fn stop(&self) -> bool;
}
