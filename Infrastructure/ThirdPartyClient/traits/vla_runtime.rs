use anyhow::Result;
use robot_common_lib::{
    vla::{VlaInference, VlaObservation},
    vla_package::VlaManifest,
};
use std::{future::Future, path::Path};

pub trait VlaRuntimeTrait: Send + Sync {
    fn is_available(&self) -> bool {
        true
    }
    fn load(&self, dir: &Path, manifest: &VlaManifest, package_sha256: &str) -> Result<bool>;
    fn unload(&self) -> bool;
    fn loaded(&self) -> Option<String>;
    fn providers(&self) -> Vec<String>;
    fn infer(
        &self,
        task: &str,
        observation: &VlaObservation,
    ) -> impl Future<Output = Result<VlaInference>> + Send;
}
