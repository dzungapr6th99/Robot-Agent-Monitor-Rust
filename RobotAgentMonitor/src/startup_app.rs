use anyhow::{Context, Result, ensure};
use robot_agent::{HttpServer, TokenVerifier};
use robot_common_lib::{integrations::MediaClientOptions, urdf_storage::UrdfStorage};
use robot_crud_business::traits::{RobotRosBLTrait, RobotRosConfigurationTrait};
use robot_crud_business::{
    camera_bl::CameraBL, robot_ros_bl::RobotRosBL, robot_ros_configuration::RobotRosConfiguration,
    *,
};
use robot_data_access::{
    db_management::DbManagement, robot_ros_configuration_da::RobotRosConfigurationDA, *,
};
use robot_ros_bridge::{CameraRos, RobotRos};
use std::{collections::HashMap, env, net::SocketAddr, sync::Arc};
use tokio::sync::watch;

pub struct StartupApp {
    pub address: SocketAddr,
    pub http_server: HttpServer,
    pub camera: Arc<CameraBL>,
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
        let crud = Arc::new(CrudServices {
            robot_profiles: Arc::new(RobotProfileBL::new(Arc::new(RobotProfileDA::new(
                Arc::clone(&db_management),
            )))),
            joint_interfaces: Arc::new(JointInterfaceBL::new(Arc::new(JointInterfaceDA::new(
                Arc::clone(&db_management),
            )))),
            sensors: Arc::new(SensorBL::new(Arc::new(SensorDA::new(Arc::clone(
                &db_management,
            ))))),
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
            robot_configurations: Arc::new(RobotConfigurationBL {
                data_access: Arc::new(RobotConfigurationDA::new(Arc::clone(&db_management))),
                max_urdf_bytes: setting("URDF_MAX_BYTES", "10485760").parse()?,
                urdf_storage: UrdfStorage(setting("URDF_DATA_ROOT", "data").into()),
            }),
        });
        let repository = RobotRosConfiguration(Arc::new(RobotRosConfigurationDA(db_management)));
        let configuration_id = env::var("ROBOT_CONFIGURATION_ID").ok();
        let topics_file = env::var("ROS_TOPICS_FILE").ok();
        let mut topics = match &configuration_id {
            Some(id) => repository.get_topics(id.parse()?)?,
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
        let encoders: HashMap<String, String> = ["head", "front", "back"]
            .into_iter()
            .map(|camera| {
                (
                    camera.into(),
                    setting(
                        &format!("CAMERA_{}_ENCODER_SERVICE", camera.to_uppercase()),
                        &format!("/{camera}_camera/encoder_control"),
                    ),
                )
            })
            .collect();
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
        for topic in &ros_monitor.status().subscriptions {
            eprintln!(
                "ROS subscribe configured: {} [{}]",
                topic.name, topic.message_type
            );
        }

        let media = Arc::new(MediaClient::new(MediaClientOptions {
            base_url: setting("MEDIA_URL", "http://127.0.0.1:8090/"),
            access_token: setting("MEDIA_ACCESS_TOKEN", ""),
            ..MediaClientOptions::default()
        })?);
        let camera_ros = Arc::new(CameraRos::new(Arc::clone(&ros), &encoders).await?);
        let camera = Arc::new(CameraBL::new(camera_ros, media, encoders.into_keys()));
        let auth = match env::var("REALTIME_AUTHORITY") {
            Ok(issuer) if !issuer.is_empty() => Some(
                TokenVerifier::discover(issuer, setting("REALTIME_AUDIENCE", "robot-hand-monitor"))
                    .await?,
            ),
            _ => None,
        };
        let origins = setting("REALTIME_ALLOWED_ORIGINS", "")
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();

        let (shutdown, shutdown_rx) = watch::channel(false);
        let http_server = HttpServer::new(
            crud,
            Arc::new(ros_monitor),
            Arc::clone(&camera),
            auth,
            origins,
            shutdown_rx,
        );
        Ok(Self {
            address,
            http_server,
            camera,
            shutdown,
        })
    }
}

fn setting(name: &str, default: &str) -> String {
    env::var(name).unwrap_or_else(|_| default.to_owned())
}
