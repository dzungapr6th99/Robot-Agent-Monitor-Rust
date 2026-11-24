use crate::SensorBL;
use crate::traits::{BaseBLTrait, VlaCameraFeedTrait};
use anyhow::Result;
use robot_common_lib::vla::{VlaError, VlaImage, vla_err};
use robot_ros_bridge::RobotRos;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tokio::sync::watch;

#[cfg(feature = "ros2")]
use robot_ros_bridge::{Subscription, messages::Image, traits::RobotRosTrait};
#[cfg(feature = "ros2")]
use std::sync::atomic::{AtomicBool, Ordering};

pub const ROS_IMAGE_DATA_TYPE: &str = "sensor_msgs/msg/Image";

type LatestFrame = Option<(Instant, VlaImage)>;

pub struct VlaCameraFeed {
    ros: Arc<RobotRos>,
    channels: Mutex<HashMap<String, watch::Receiver<LatestFrame>>>,
    #[cfg(feature = "ros2")]
    subscriptions: Mutex<Vec<Subscription<Image>>>,
}

impl VlaCameraFeed {
    pub fn new(ros: Arc<RobotRos>) -> Self {
        Self {
            ros,
            channels: Mutex::new(HashMap::new()),
            #[cfg(feature = "ros2")]
            subscriptions: Mutex::new(Vec::new()),
        }
    }
}

impl VlaCameraFeedTrait for VlaCameraFeed {
    fn start(&self, topics: &[(String, String)]) -> Result<bool> {
        robot_common_lib::logged_result!("vla_camera_start", {
            self.stop();
            if topics.is_empty() {
                return Ok(true);
            }
            #[cfg(feature = "ros2")]
            {
                if !self.ros.running() {
                    return Err(vla_err(
                        VlaError::Unavailable,
                        "ROS is disabled or unavailable for camera feed",
                    ));
                }
                let mut new_channels = HashMap::with_capacity(topics.len());
                let mut new_subs = Vec::with_capacity(topics.len());
                for (camera, topic) in topics {
                    let qos = RobotRos::qos(true, 1, false);
                    let (tx, rx) = watch::channel::<LatestFrame>(None);
                    let seen_first = Arc::new(AtomicBool::new(false));
                    let camera_name = camera.clone();
                    let topic_name = topic.clone();
                    let sub = self.ros.subscriber::<Image>(topic, qos, move |msg| {
                        let image = VlaImage {
                            width: msg.width,
                            height: msg.height,
                            encoding: msg.encoding,
                            data: msg.data,
                        };
                        if !seen_first.swap(true, Ordering::AcqRel) {
                            robot_common_lib::operation_log::info(
                                module_path!(),
                                format_args!(
                                    "VLA camera '{}' received first frame on {} ({}x{}, {})",
                                    camera_name, topic_name, image.width, image.height, image.encoding
                                ),
                            );
                        }
                        tx.send_replace(Some((Instant::now(), image)));
                    })?;
                    new_channels.insert(camera.clone(), rx);
                    new_subs.push(sub);
                }
                *self.channels.lock().unwrap_or_else(|e| e.into_inner()) = new_channels;
                *self.subscriptions.lock().unwrap_or_else(|e| e.into_inner()) = new_subs;
                Ok(true)
            }
            #[cfg(not(feature = "ros2"))]
            {
                let _ = &self.ros;
                Err(vla_err(
                    VlaError::Unavailable,
                    "ROS 2 camera subscription requires building with --features ros2",
                ))
            }
        })
    }

    fn latest(&self, camera: &str, max_age: Duration) -> Result<VlaImage> {
        let channels = self.channels.lock().unwrap_or_else(|e| e.into_inner());
        let rx = channels.get(camera).ok_or_else(|| {
            vla_err(
                VlaError::Unavailable,
                format!("Camera '{camera}' is not subscribed"),
            )
        })?;
        let borrowed = rx.borrow();
        let Some((captured_at, image)) = borrowed.as_ref() else {
            return Err(vla_err(
                VlaError::Unavailable,
                format!("Camera '{camera}' has not received a frame yet"),
            ));
        };
        if captured_at.elapsed() > max_age {
            return Err(vla_err(
                VlaError::Unavailable,
                format!(
                    "Camera '{camera}' frame is stale ({} ms > {} ms)",
                    captured_at.elapsed().as_millis(),
                    max_age.as_millis()
                ),
            ));
        }
        Ok(image.clone())
    }

    fn stop(&self) -> bool {
        let had_channels = {
            let mut channels = self.channels.lock().unwrap_or_else(|e| e.into_inner());
            let had = !channels.is_empty();
            channels.clear();
            had
        };
        #[cfg(feature = "ros2")]
        {
            let mut subs = self.subscriptions.lock().unwrap_or_else(|e| e.into_inner());
            let had_subs = !subs.is_empty();
            subs.clear();
            had_channels || had_subs
        }
        #[cfg(not(feature = "ros2"))]
        {
            had_channels
        }
    }
}

/// Maps enabled `sensors.code` -> enabled `sensor_msgs/msg/Image` stream topic for a URDF configuration.
pub fn list_configuration_cameras(
    sensors: &SensorBL,
    configuration_id: i64,
) -> Result<HashMap<String, String>> {
    let mut map = HashMap::new();
    let all_sensors = sensors.list(500, 0)?;
    for sensor in all_sensors {
        if sensor.robot_configuration_id != Some(configuration_id) {
            continue;
        }
        if !sensor.is_enabled.unwrap_or(false) {
            continue;
        }
        let Some(code) = sensor
            .code
            .as_deref()
            .map(str::trim)
            .filter(|c| !c.is_empty())
        else {
            continue;
        };
        if let Some(streams) = sensor.sensor_streams.as_ref() {
            for stream in streams {
                if !stream.is_enabled.unwrap_or(false) {
                    continue;
                }
                if stream.data_type.as_deref() != Some(ROS_IMAGE_DATA_TYPE) {
                    continue;
                }
                if let Some(topic) = stream
                    .topic
                    .as_deref()
                    .map(str::trim)
                    .filter(|t| !t.is_empty())
                {
                    map.insert(code.to_owned(), topic.to_owned());
                    break;
                }
            }
        }
    }
    Ok(map)
}

pub fn resolve_camera_topics(
    sensors: &SensorBL,
    configuration_id: i64,
    cameras: &[String],
) -> Result<Vec<(String, String)>> {
    let available = list_configuration_cameras(sensors, configuration_id)?;
    let mut resolved = Vec::with_capacity(cameras.len());
    for camera in cameras {
        let Some(topic) = available.get(camera) else {
            return Err(vla_err(
                VlaError::InvalidPackage,
                format!(
                    "Camera '{camera}' is not configured with an enabled {ROS_IMAGE_DATA_TYPE} stream for URDF #{configuration_id}"
                ),
            ));
        };
        resolved.push((camera.clone(), topic.clone()));
    }
    Ok(resolved)
}
