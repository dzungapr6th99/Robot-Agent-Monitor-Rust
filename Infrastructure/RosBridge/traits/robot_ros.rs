#[cfg(feature = "ros2")]
use crate::{Client, Message, Node, Publisher, QoSProfile, Service, ServiceType, Subscription};
#[cfg(feature = "ros2")]
use anyhow::Result;
#[cfg(feature = "ros2")]
use std::future::Future;

pub trait RobotRosTrait: Send + Sync {
    fn running(&self) -> bool;
    #[cfg(feature = "ros2")]
    fn node(&self) -> Result<Node>;
    #[cfg(feature = "ros2")]
    fn publisher<T: Message>(&self, topic: &str, qos: QoSProfile) -> Result<Publisher<T>>;
    #[cfg(feature = "ros2")]
    fn subscriber<T: Message>(
        &self,
        topic: &str,
        qos: QoSProfile,
        callback: impl Fn(T) + Send + Sync + 'static,
    ) -> Result<Subscription<T>>;
    #[cfg(feature = "ros2")]
    fn service_client<S: ServiceType>(&self, service: &str, qos: QoSProfile) -> Result<Client<S>>;
    #[cfg(feature = "ros2")]
    fn service_server<S: ServiceType>(
        &self,
        service: &str,
        qos: QoSProfile,
        callback: impl Fn(S::Request) -> S::Response + Send + Sync + 'static,
    ) -> Result<Service<S>>;
    #[cfg(feature = "ros2")]
    fn wait_for_service<S: ServiceType>(
        &self,
        client: &Client<S>,
    ) -> impl Future<Output = Result<()>> + Send;
}
