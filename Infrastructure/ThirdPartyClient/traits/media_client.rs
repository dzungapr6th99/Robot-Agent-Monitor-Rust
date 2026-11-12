use anyhow::Result;
use serde_json::Value;
use std::future::Future;

pub trait MediaClientTrait: Send + Sync {
    fn call(
        &self,
        operation: &'static str,
        payload: Value,
    ) -> impl Future<Output = Result<Value>> + Send;
}
