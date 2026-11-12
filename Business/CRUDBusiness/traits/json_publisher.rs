use anyhow::Result;
use serde_json::Value;
use std::{future::Future, pin::Pin};

pub(crate) type PublishFuture<'a> = Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

pub(crate) trait JsonPublisher: Send + Sync {
    fn message_type(&self) -> &'static str;
    fn publish(&self, data: Value) -> PublishFuture<'_>;
}
