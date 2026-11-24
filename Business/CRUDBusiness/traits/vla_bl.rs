use anyhow::Result;
use robot_common_lib::vla::VlaRuntimeStatus;
use std::future::Future;

pub trait VlaBLTrait: Send + Sync {
    fn topics_ready(&self) -> Result<()>;
    fn status(&self) -> VlaRuntimeStatus;
    fn run(
        &self,
        configuration_id: i64,
        model_id: i64,
        task: &str,
    ) -> impl Future<Output = Result<u32>> + Send;
    fn stop(&self) -> bool;
}

pub(crate) trait VlaRunGuardTrait {
    fn finish(&mut self) -> bool;
}
