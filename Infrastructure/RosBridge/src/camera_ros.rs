use crate::RobotRos;
use crate::traits::CameraRosTrait;
use crate::traits::RobotRosTrait;
use anyhow::Result;
use std::{collections::HashMap, sync::Arc};

#[cfg(feature = "camera-ros")]
use crate::{
    Client,
    messages::{StreamEncoder, StreamEncoder_Request, StreamEncoder_Response},
};
#[cfg(feature = "camera-ros")]
use anyhow::{Context, ensure};
#[cfg(feature = "camera-ros")]
use std::time::Duration;

/// Camera ROS clients use the rosidl-generated service type from the sourced robot workspace.
pub struct CameraRos {
    ros: Arc<RobotRos>,
    #[cfg(feature = "camera-ros")]
    encoders: HashMap<String, Client<StreamEncoder>>,
}
impl CameraRos {
    pub async fn new(ros: Arc<RobotRos>, services: &HashMap<String, String>) -> Result<Self> {
        robot_common_lib::logged_result!(async "new", {
            #[cfg(feature = "camera-ros")]
            {
                let mut encoders = HashMap::new();
                if ros.running() {
                    for (camera, service) in services {
                        encoders.insert(
                            camera.clone(),
                            ros.service_client::<StreamEncoder>(
                                service,
                                rclrs::QoSProfile::services_default(),
                            )?,
                        );
                    }
                }
                Ok(Self { ros, encoders })
            }
            #[cfg(not(feature = "camera-ros"))]
            {
                let _ = services;
                Ok(Self { ros })
            }
        })
    }
}

impl CameraRosTrait for CameraRos {
    fn running(&self) -> bool {
        self.ros.running()
    }

    async fn encoder(&self, camera: &str, action: &str, width: u32, height: u32) -> Result<String> {
        robot_common_lib::logged_result!(async "encoder", {
            #[cfg(feature = "camera-ros")]
            {
                ensure!(self.running(), "ROS is disabled or unavailable");
                let client = self
                    .encoders
                    .get(camera)
                    .context("Encoder is not configured")?;
                let response: StreamEncoder_Response =
                    tokio::time::timeout(Duration::from_secs(10), async {
                        self.ros.wait_for_service(client).await?;
                        let request = StreamEncoder_Request {
                            camera: camera.into(),
                            action: action.into(),
                            width,
                            height,
                        };
                        Ok::<_, anyhow::Error>(
                            client.call::<_, StreamEncoder_Response>(&request)?.await?,
                        )
                    })
                    .await??;
                ensure!(response.success, "Encoder rejected {action}");
                Ok(response.message)
            }
            #[cfg(not(feature = "camera-ros"))]
            {
                let _ = (camera, action, width, height);
                anyhow::bail!(
                    "Camera encoder requires --features camera-ros and generated vmo_camera_interface Rust bindings"
                );
            }
        })
    }
}
