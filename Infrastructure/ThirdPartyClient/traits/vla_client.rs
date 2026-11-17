use anyhow::Result;
use robot_common_lib::vla::{VlaInference, VlaJoints};
use std::future::Future;

pub trait VlaClientTrait: Send + Sync {
    fn infer(
        &self,
        profile_id: &str,
        task: &str,
        observation: &VlaJoints,
    ) -> impl Future<Output = Result<VlaInference>> + Send;
}
