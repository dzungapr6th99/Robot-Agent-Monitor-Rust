use super::RobotRos;
use anyhow::{Context as _, Result, ensure};
use rclrs::rosidl_runtime_rs::{Message, Service as ServiceType};
use rclrs::{
    Client, Context, CreateBasicExecutor, ExecutorCommands, InitOptions, IntoNodeOptions,
    IntoPrimitiveOptions, Node, Publisher, QoSDurabilityPolicy, QoSHistoryPolicy, QoSProfile,
    QoSReliabilityPolicy, Service, SpinOptions, Subscription,
};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

pub(super) struct RosRuntime {
    context: Context,
    node: Node,
    commands: Arc<ExecutorCommands>,
    running: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl RosRuntime {
    pub(super) fn start(domain_id: u16, namespace: &str) -> Result<Self> {
        ensure!(domain_id <= 232, "ROS_DOMAIN_ID must be in 0..=232");
        let context = Context::new(
            std::iter::empty::<String>(),
            InitOptions::default().with_domain_id(Some(usize::from(domain_id))),
        )?;
        let mut executor = context.create_basic_executor();
        let node = executor.create_node("robot_hand_monitor_rust".namespace(namespace))?;
        let commands = Arc::clone(executor.commands());
        let running = Arc::new(AtomicBool::new(true));
        let thread_running = Arc::clone(&running);
        let thread = thread::Builder::new()
            .name("ros2-rclrs".into())
            .spawn(move || {
                let errors = executor.spin(SpinOptions::default());
                thread_running.store(false, Ordering::Release);
                for error in errors {
                    eprintln!("rclrs executor stopped: {error}");
                }
            })?;
        Ok(Self {
            context,
            node,
            commands,
            running,
            thread: Some(thread),
        })
    }

    pub(super) fn running(&self) -> bool {
        self.context.ok() && self.running.load(Ordering::Acquire)
    }
}
impl Drop for RosRuntime {
    fn drop(&mut self) {
        self.commands.halt_spinning();
        if let Some(thread) = self.thread.take()
            && thread.thread().id() != thread::current().id()
        {
            let _ = thread.join();
        }
    }
}

impl RobotRos {
    /// The actual shared rclrs node. Services can call other rclrs factories on it directly.
    pub fn node(&self) -> Result<Node> {
        ensure!(self.running(), "ROS is disabled or unavailable");
        Ok(Arc::clone(
            &self.runtime.as_ref().context("ROS is disabled")?.node,
        ))
    }

    pub fn publisher<T: Message>(&self, topic: &str, qos: QoSProfile) -> Result<Publisher<T>> {
        Ok(self.node()?.create_publisher::<T>(topic.qos(qos))?)
    }

    pub fn subscriber<T: Message>(
        &self,
        topic: &str,
        qos: QoSProfile,
        callback: impl Fn(T) + Send + Sync + 'static,
    ) -> Result<Subscription<T>> {
        Ok(self
            .node()?
            .create_subscription::<T, _>(topic.qos(qos), callback)?)
    }

    pub fn service_client<S: ServiceType>(
        &self,
        service: &str,
        qos: QoSProfile,
    ) -> Result<Client<S>> {
        Ok(self.node()?.create_client::<S>(service.qos(qos))?)
    }

    pub fn service_server<S: ServiceType>(
        &self,
        service: &str,
        qos: QoSProfile,
        callback: impl Fn(S::Request) -> S::Response + Send + Sync + 'static,
    ) -> Result<Service<S>> {
        Ok(self
            .node()?
            .create_service::<S, _>(service.qos(qos), callback)?)
    }

    pub fn qos(best_effort: bool, depth: u32, transient_local: bool) -> QoSProfile {
        QoSProfile {
            history: QoSHistoryPolicy::KeepLast { depth },
            reliability: if best_effort {
                QoSReliabilityPolicy::BestEffort
            } else {
                QoSReliabilityPolicy::Reliable
            },
            durability: if transient_local {
                QoSDurabilityPolicy::TransientLocal
            } else {
                QoSDurabilityPolicy::Volatile
            },
            ..QoSProfile::topics_default()
        }
    }

    pub async fn wait_for_service<S: ServiceType>(&self, client: &Client<S>) -> Result<()> {
        client.notify_on_service_ready().await?;
        Ok(())
    }
}
