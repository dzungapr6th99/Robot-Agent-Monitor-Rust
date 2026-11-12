use super::RobotRos;
use anyhow::{Context as _, Result, ensure};
use rclrs::rosidl_runtime_rs::{Message, Service as ServiceType};
use rclrs::{
    Client, Context, CreateBasicExecutor, ExecutorCommands, InitOptions, IntoNodeOptions,
    IntoPrimitiveOptions, Node, Publisher, QoSDurabilityPolicy, QoSHistoryPolicy, QoSProfile,
    QoSReliabilityPolicy, Service, SpinOptions, Subscription,
};
use robot_common_lib::operation_log;
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
        robot_common_lib::logged_result!("start", {
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
                    operation_log::info(
                        module_path!(),
                        format_args!("Starting ROS 2 executor thread (domain {domain_id})"),
                    );
                    let errors = executor.spin(SpinOptions::default());
                    thread_running.store(false, Ordering::Release);
                    for error in errors {
                        operation_log::error(
                            module_path!(),
                            format_args!("rclrs executor stopped: {error}"),
                        );
                    }
                })?;
            Ok(Self {
                context,
                node,
                commands,
                running,
                thread: Some(thread),
            })
        })
    }
}

impl crate::traits::RosRuntimeTrait for RosRuntime {
    fn running(&self) -> bool {
        self.context.ok() && self.running.load(Ordering::Acquire)
    }
}
impl Drop for RosRuntime {
    fn drop(&mut self) {
        self.commands.halt_spinning();
        if let Some(thread) = self.thread.take()
            && thread.thread().id() != thread::current().id()
            && thread.join().is_err()
        {
            operation_log::error(
                module_path!(),
                format_args!("ROS 2 executor thread panicked"),
            );
        }
    }
}

impl crate::traits::RobotRosTrait for RobotRos {
    fn running(&self) -> bool {
        self.runtime
            .as_ref()
            .is_some_and(crate::traits::RosRuntimeTrait::running)
    }
    /// The actual shared rclrs node. Services can call other rclrs factories on it directly.
    fn node(&self) -> Result<Node> {
        robot_common_lib::logged_result!("node", {
            ensure!(self.running(), "ROS is disabled or unavailable");
            Ok(Arc::clone(
                &self.runtime.as_ref().context("ROS is disabled")?.node,
            ))
        })
    }

    fn publisher<T: Message>(&self, topic: &str, qos: QoSProfile) -> Result<Publisher<T>> {
        robot_common_lib::logged_result!("publisher", {
            Ok(self.node()?.create_publisher::<T>(topic.qos(qos))?)
        })
    }

    fn subscriber<T: Message>(
        &self,
        topic: &str,
        qos: QoSProfile,
        callback: impl Fn(T) + Send + Sync + 'static,
    ) -> Result<Subscription<T>> {
        robot_common_lib::logged_result!("subscriber", {
            Ok(self
                .node()?
                .create_subscription::<T, _>(topic.qos(qos), callback)?)
        })
    }

    fn service_client<S: ServiceType>(&self, service: &str, qos: QoSProfile) -> Result<Client<S>> {
        robot_common_lib::logged_result!("service_client", {
            Ok(self.node()?.create_client::<S>(service.qos(qos))?)
        })
    }

    fn service_server<S: ServiceType>(
        &self,
        service: &str,
        qos: QoSProfile,
        callback: impl Fn(S::Request) -> S::Response + Send + Sync + 'static,
    ) -> Result<Service<S>> {
        robot_common_lib::logged_result!("service_server", {
            let service_name = service.to_owned();
            Ok(self.node()?.create_service::<S, _>(
                service.qos(qos),
                move |request: S::Request| {
                    operation_log::info(
                        module_path!(),
                        format_args!("Receiving ROS service request for {service_name}"),
                    );
                    callback(request)
                },
            )?)
        })
    }

    async fn wait_for_service<S: ServiceType>(&self, client: &Client<S>) -> Result<()> {
        robot_common_lib::logged_result!(async "wait_for_service", {
            client.notify_on_service_ready().await?;
            Ok(())
        })
    }
}

impl RobotRos {
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
}
