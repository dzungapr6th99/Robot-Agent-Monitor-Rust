use anyhow::{Context, Result, ensure};
use robot_agent::HttpServer;
use robot_common_lib::urdf_storage::UrdfStorage;
use robot_crud_business::traits::{RobotRosBLTrait, RobotRosConfigurationTrait, VlaModelBLTrait};
use robot_crud_business::{
    OnnxVlaRuntime, VlaCameraFeed, VlaModelBL,
    robot_ros_bl::RobotRosBL,
    robot_ros_configuration::RobotRosConfiguration,
    *,
};
use robot_data_access::{
    VlaModelDA, db_management::DbManagement, robot_ros_configuration_da::RobotRosConfigurationDA,
    *,
};
use robot_ros_bridge::RobotRos;
use std::{
    env,
    net::SocketAddr,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::sync::watch;

pub struct StartupApp {
    pub address: SocketAddr,
    pub http_server: HttpServer,
    pub shutdown: watch::Sender<bool>,
}

impl StartupApp {
    pub async fn init_services() -> Result<Self> {
        let address: SocketAddr = setting("AGENT_LISTEN", "127.0.0.1:8080").parse()?;
        ensure!(
            address.ip().is_loopback(),
            "Run the agent behind a TLS reverse proxy on loopback"
        );
        let database = setting("ROBOT_AGENT_DATABASE", "data/robot-agent.db");
        let db_management = Arc::new(DbManagement(database.clone().into()));
        let urdf_data_root = setting("URDF_DATA_ROOT", "data");
        let vla_data_root = PathBuf::from(setting("VLA_DATA_ROOT", &urdf_data_root));
        let vla_model_da = Arc::new(VlaModelDA::new(Arc::clone(&db_management)));
        let sensors = Arc::new(SensorBL::new(Arc::new(SensorDA::new(Arc::clone(
            &db_management,
        )))));
        let robot_configurations = Arc::new(RobotConfigurationBL {
            data_access: Arc::new(RobotConfigurationDA::new(Arc::clone(&db_management))),
            vla_models: Arc::clone(&vla_model_da),
            max_urdf_bytes: setting("URDF_MAX_BYTES", "10485760").parse()?,
            urdf_storage: UrdfStorage(urdf_data_root.into()),
        });

        let crud = Arc::new(CrudServices {
            robot_profiles: Arc::new(RobotProfileBL::new(Arc::new(RobotProfileDA::new(
                Arc::clone(&db_management),
            )))),
            joint_interfaces: Arc::new(JointInterfaceBL::new(Arc::new(JointInterfaceDA::new(
                Arc::clone(&db_management),
            )))),
            sensors: Arc::clone(&sensors),
            sensor_streams: Arc::new(SensorStreamBL::new(Arc::new(SensorStreamDA::new(
                Arc::clone(&db_management),
            )))),
            joint_groups: Arc::new(JointGroupBL::new(Arc::new(JointGroupDA::new(Arc::clone(
                &db_management,
            ))))),
            joint_group_members: Arc::new(JointGroupMemberBL::new(Arc::new(
                JointGroupMemberDA::new(Arc::clone(&db_management)),
            ))),
            pose_presets: Arc::new(PosePresetBL::new(Arc::new(PosePresetDA::new(Arc::clone(
                &db_management,
            ))))),
            pose_preset_joints: Arc::new(PosePresetJointBL::new(Arc::new(PosePresetJointDA::new(
                Arc::clone(&db_management),
            )))),
            episodes: Arc::new(EpisodeBL::new(Arc::new(EpisodeDA::new(Arc::clone(
                &db_management,
            ))))),
            episode_annotations: Arc::new(EpisodeAnnotationBL::new(Arc::new(
                EpisodeAnnotationDA::new(Arc::clone(&db_management)),
            ))),
            robot_configurations: Arc::clone(&robot_configurations),
            vla_models: Arc::clone(&vla_model_da),
        });

        let repository =
            RobotRosConfiguration(Arc::new(RobotRosConfigurationDA(Arc::clone(&db_management))));
        let configuration_id: Option<i64> = match env::var("ROBOT_CONFIGURATION_ID") {
            Ok(raw) if !raw.trim().is_empty() => Some(
                raw.trim()
                    .parse::<i64>()
                    .context("Invalid ROBOT_CONFIGURATION_ID")?,
            ),
            _ => None,
        };
        let topics_file = env::var("ROS_TOPICS_FILE").ok();
        let mut topics = match configuration_id {
            Some(id) => repository.get_topics(id)?,
            None if topics_file.is_none() => robot_common_lib::ros::default_topics(),
            None => Vec::new(),
        };
        if let Some(path) = topics_file {
            let contents = std::fs::read_to_string(&path)
                .with_context(|| format!("Cannot read ROS_TOPICS_FILE: {path}"))?;
            let deployment: robot_common_lib::ros::RosTopicConfiguration =
                serde_json::from_str(&contents).context("Invalid ROS topic configuration")?;
            topics.extend(deployment.into_topics()?);
        }
        let topics = RobotRosConfiguration::merge_topics(topics)?;
        let ros = Arc::new(if setting("ROS_ENABLED", "false").parse::<bool>()? {
            RobotRos::start(
                setting("ROS_DOMAIN_ID", "0").parse()?,
                &setting("ROS_NAMESPACE", "/"),
            )?
        } else {
            RobotRos::default()
        });
        let mut ros_monitor = RobotRosBL::new(Arc::clone(&ros), topics);
        ros_monitor
            .register::<robot_ros_bridge::messages::DiagnosticArray>()
            .await?;
        ros_monitor
            .register::<robot_ros_bridge::messages::DiagnosticStatus>()
            .await?;
        ros_monitor
            .register::<robot_ros_bridge::messages::JointState>()
            .await?;
        ros_monitor
            .register::<robot_ros_bridge::messages::Odometry>()
            .await?;
        ros_monitor
            .register::<robot_ros_bridge::messages::TFMessage>()
            .await?;
        ros_monitor
            .register::<robot_ros_bridge::messages::Clock>()
            .await?;
        ros_monitor.validate_monitor_topics()?;
        let ros_monitor = Arc::new(ros_monitor);

        let vla_max_package_bytes: usize =
            usize::try_from(setting("VLA_MAX_PACKAGE_BYTES", "4294967296").parse::<u64>()?)
                .unwrap_or(usize::MAX);
        let vla_max_unpacked_bytes: u64 =
            setting("VLA_MAX_UNPACKED_BYTES", "8589934592").parse()?;
        let providers = setting("VLA_ONNX_PROVIDERS", "tensorrt,cuda,cpu");
        let runtime = Arc::new(OnnxVlaRuntime::from_env_providers(&providers));
        let camera_feed = Arc::new(VlaCameraFeed::new(Arc::clone(&ros)));
        let active_model_id: Arc<Mutex<Option<i64>>> = Arc::default();

        let vla_models = Arc::new(VlaModelBL {
            data_access: Arc::clone(&vla_model_da),
            robot_configurations: Arc::clone(&robot_configurations),
            sensors: Arc::clone(&sensors),
            runtime: Arc::clone(&runtime),
            data_root: vla_data_root.clone(),
            max_unpacked_bytes: vla_max_unpacked_bytes,
            active_model_id: Arc::clone(&active_model_id),
        });
        vla_models.ensure_table()?;
        let _ = vla_models.cleanup_staging()?;

        let options = robot_common_lib::vla::VlaOptions {
            timeout_ms: setting("VLA_TIMEOUT_MS", "600000").parse()?,
            command_topic: setting("VLA_COMMAND_TOPIC", "/yellowarm_sim/joint_command"),
            state_topic: setting("VLA_STATE_TOPIC", "/yellowarm_sim/joint_states"),
            max_steps: setting("VLA_MAX_STEPS", "1000").parse()?,
            step_period_ms: setting("VLA_STEP_PERIOD_MS", "100").parse()?,
            image_max_age_ms: setting("VLA_IMAGE_MAX_AGE_MS", "500").parse()?,
            configuration_id,
        };
        let vla = Arc::new(vla_bl::VlaBL::new(
            runtime,
            Arc::clone(&ros_monitor),
            camera_feed,
            vla_model_da,
            sensors,
            vla_data_root.clone(),
            options,
            active_model_id,
        )?);
        for topic in &ros_monitor.status().subscriptions {
            eprintln!(
                "ROS subscribe configured: {} [{}]",
                topic.name, topic.message_type
            );
        }

        let origins = setting("REALTIME_ALLOWED_ORIGINS", "")
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();

        let (shutdown, shutdown_rx) = watch::channel(false);
        let http_server = HttpServer::new(
            crud,
            ros_monitor,
            vla,
            vla_models,
            vla_max_package_bytes,
            vla_data_root,
            origins,
            shutdown_rx,
        );
        Ok(Self {
            address,
            http_server,
            shutdown,
        })
    }
}

fn setting(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}
