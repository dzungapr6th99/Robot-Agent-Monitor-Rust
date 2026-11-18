use anyhow::Result;
use robot_common_lib::vla::VlaProfile;
use std::future::Future;

pub trait VlaBLTrait: Send + Sync {
    fn models(&self) -> Vec<VlaProfile>;
    fn run(&self, profile_id: &str, task: &str) -> impl Future<Output = Result<u32>> + Send;
    fn stop(&self) -> bool;
}

pub(crate) trait VlaRunGuardTrait {
    fn finish(&mut self) -> bool;
}
