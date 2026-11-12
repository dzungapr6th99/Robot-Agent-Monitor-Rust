use crate::traits::CameraBLTrait;
use robot_api_object::camera::{CameraResolution, CameraStreamState};
use robot_ros_bridge::traits::CameraRosTrait;
use robot_third_party_client::traits::MediaClientTrait;
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    sync::Arc,
};

use anyhow::{Context, Result, ensure};
use serde_json::{Value, json};
use tokio::sync::{Mutex, broadcast};

use robot_ros_bridge::CameraRos;
use robot_third_party_client::MediaClient;

pub const RESOLUTIONS: [CameraResolution; 3] = [
    CameraResolution {
        preset: "SD",
        width: 854,
        height: 480,
    },
    CameraResolution {
        preset: "HD",
        width: 1280,
        height: 720,
    },
    CameraResolution {
        preset: "FHD",
        width: 1920,
        height: 1080,
    },
];

pub enum CameraQuery {
    Resolutions,
    Streams,
    Health,
    RtpCapabilities,
}

pub enum CameraCommand {
    Start { camera: String, resolution: String },
    Stop(String),
    CreateTransport,
    ConnectTransport(Value),
    Consume { camera: String, capabilities: Value },
    Resume(String),
    Leave(String),
}

impl CameraCommand {
    pub fn requires_manage(&self) -> bool {
        matches!(self, Self::Start { .. } | Self::Stop(_))
    }
}

struct CameraState {
    streams: BTreeMap<String, CameraStreamState>,
    peers: HashMap<String, HashSet<String>>,
    pending_peers: HashSet<String>,
    pending_cameras: HashSet<String>,
}

pub struct CameraBL {
    ros: Arc<CameraRos>,
    media: Arc<MediaClient>,
    state: Mutex<CameraState>,
    events: broadcast::Sender<CameraStreamState>,
}

impl CameraBL {
    pub fn new(
        ros: Arc<CameraRos>,
        media: Arc<MediaClient>,
        cameras: impl IntoIterator<Item = String>,
    ) -> Self {
        let streams = cameras
            .into_iter()
            .map(|camera| (camera.clone(), Self::stopped(&camera)))
            .collect();
        Self {
            ros,
            media,
            state: Mutex::new(CameraState {
                streams,
                peers: HashMap::new(),
                pending_peers: HashSet::new(),
                pending_cameras: HashSet::new(),
            }),
            events: broadcast::channel(64).0,
        }
    }

    fn stopped(camera: &str) -> CameraStreamState {
        CameraStreamState {
            camera: camera.into(),
            active: Some(false),
            resolution: None,
            producer_id: None,
            last_error: None,
            updated_at: chrono::Utc::now(),
        }
    }

    fn set_state(&self, state: &mut CameraState, mut stream: CameraStreamState) -> usize {
        stream.updated_at = chrono::Utc::now();
        state.streams.insert(stream.camera.clone(), stream.clone());
        self.events.send(stream).unwrap_or(0)
    }

    async fn start(
        &self,
        state: &mut CameraState,
        peer: &str,
        camera: &str,
        preset: &str,
    ) -> Result<Value> {
        robot_common_lib::logged_result!(async "start", {
            let resolution = RESOLUTIONS
                .iter()
                .copied()
                .find(|r| r.preset == preset)
                .context("Invalid resolution")?;
            let previous = state
                .streams
                .get(camera)
                .context("Camera is not configured")?;
            ensure!(
                previous.active.is_some() && !state.pending_cameras.contains(camera),
                "Stop the camera before retrying"
            );
            if previous.active == Some(true) {
                ensure!(
                    previous.resolution == Some(resolution),
                    "Stop the camera before changing resolution"
                );
                state.peers.get_mut(peer).context("Camera peer has disconnected")?.insert(camera.into());
                return Ok(json!(previous));
            }
            state.peers.get_mut(peer).context("Camera peer has disconnected")?.insert(camera.into());
            self.set_state(
                state,
                CameraStreamState {
                    camera: camera.into(),
                    active: None,
                    resolution: Some(resolution),
                    producer_id: None,
                    last_error: Some("START_FAILED_REQUIRES_STOP"),
                    updated_at: chrono::Utc::now(),
                },
            );
            let raw = self.ros.encoder(camera, "configs", 0, 0).await?;
            let entries: Vec<Value> = serde_json::from_str(&raw)?;
            ensure!(entries.len() == 1, "Expected one encoder configuration");
            let port = parse_number::<u16>(&entries[0]["port"])?;
            let ssrc = parse_number::<u32>(&entries[0]["ssrc"])?;
            ensure!(port > 0 && ssrc > 0, "Invalid encoder RTP configuration");
            let producer = self
                .media
                .call(
                    "prepareCamera",
                    json!({"camera":camera,"port":port,"ssrc":ssrc}),
                )
                .await?;
            let producer_id = producer["producerId"]
                .as_str()
                .context("Media server returned no producer ID")?;
            self.ros
                .encoder(camera, "start", resolution.width, resolution.height)
                .await?;
            let stream = CameraStreamState {
                camera: camera.into(),
                active: Some(true),
                resolution: Some(resolution),
                producer_id: Some(producer_id.into()),
                last_error: None,
                updated_at: chrono::Utc::now(),
            };
            self.set_state(state, stream);
            Ok(json!(state.streams.get(camera).context("Camera state is unavailable")?))
        })
    }

    async fn stop(&self, state: &mut CameraState, camera: &str) -> Result<CameraStreamState> {
        robot_common_lib::logged_result!(async "stop", {
            let mut stream = state
                .streams
                .get(camera)
                .context("Camera is not configured")?
                .clone();
            stream.active = None;
            stream.last_error = Some("STOP_FAILED_REQUIRES_RECONCILIATION");
            self.set_state(state, stream);
            self.ros.encoder(camera, "stop", 0, 0).await?;
            self.media
                .call("closeCamera", json!({"camera":camera}))
                .await?;
            let stopped = Self::stopped(camera);
            self.set_state(state, stopped.clone());
            state.pending_cameras.remove(camera);
            for owned in state.peers.values_mut() {
                owned.remove(camera);
            }
            Ok(stopped)
        })
    }
}

impl CameraBLTrait for CameraBL {
    fn subscribe(&self) -> broadcast::Receiver<CameraStreamState> {
        self.events.subscribe()
    }

    async fn connect(&self, peer: String) -> Result<bool> {
        robot_common_lib::operation_log::info(
            module_path!(),
            format_args!("Receiving camera connection"),
        );
        robot_common_lib::logged_result!(async "connect", {
            let mut state = self.state.lock().await;
            ensure!(
                state.peers.len() + state.pending_peers.len() < 16,
                "Camera peer limit reached"
            );
            Ok(state.peers.insert(peer, HashSet::new()).is_none())
        })
    }

    async fn disconnect(&self, peer: &str) -> bool {
        let mut state = self.state.lock().await;
        let Some(cameras) = state.peers.remove(peer) else {
            return false;
        };
        state.pending_peers.insert(peer.into());
        for camera in cameras {
            if !state.peers.values().any(|other| other.contains(&camera)) {
                state.pending_cameras.insert(camera);
            }
        }
        true
    }

    async fn retry_cleanup(&self) -> bool {
        let mut state = self.state.lock().await;
        for peer in state.pending_peers.clone() {
            if self
                .media
                .call("removePeer", json!({"peerId":peer}))
                .await
                .is_ok()
            {
                state.pending_peers.remove(&peer);
            }
        }
        for camera in state.pending_cameras.clone() {
            let _ = self.stop(&mut state, &camera).await;
        }
        let active = state
            .streams
            .values()
            .any(|stream| stream.active == Some(true));
        let health = if active {
            self.media.call("health", json!({})).await.ok()
        } else {
            None
        };
        let producers = health
            .as_ref()
            .filter(|health| health["ready"] == true)
            .and_then(|health| health["streams"].as_array());
        if let Some(producers) = producers {
            let missing = state
                .streams
                .values()
                .filter(|stream| {
                    stream.active == Some(true)
                        && !producers.iter().any(|producer| {
                            producer["camera"].as_str() == Some(&stream.camera)
                                && producer["producerId"].as_str() == stream.producer_id.as_deref()
                        })
                })
                .cloned()
                .collect::<Vec<_>>();
            for mut stream in missing {
                stream.active = None;
                stream.last_error = Some("MEDIA_SESSION_LOST");
                self.set_state(&mut state, stream);
            }
        }
        state.pending_peers.is_empty()
            && state.pending_cameras.is_empty()
            && (!active || producers.is_some())
    }

    async fn shutdown(&self) -> bool {
        let mut state = self.state.lock().await;
        let peers = std::mem::take(&mut state.peers);
        for (peer, cameras) in peers {
            state.pending_peers.insert(peer);
            state.pending_cameras.extend(cameras);
        }
        let open = state
            .streams
            .values()
            .filter(|stream| stream.active != Some(false))
            .map(|stream| stream.camera.clone())
            .collect::<Vec<_>>();
        state.pending_cameras.extend(open);
        drop(state);
        self.retry_cleanup().await
    }

    async fn query(&self, query: CameraQuery) -> Result<Value> {
        robot_common_lib::operation_log::info(
            module_path!(),
            format_args!("Receiving camera query"),
        );
        robot_common_lib::logged_result!(async "query", {
            match query {
                CameraQuery::Resolutions => Ok(json!(RESOLUTIONS)),
                CameraQuery::Streams => Ok(json!(
                    self.state.lock().await.streams.values().collect::<Vec<_>>()
                )),
                CameraQuery::RtpCapabilities => self.media.call("capabilities", json!({})).await,
                CameraQuery::Health => {
                    let media = self.media.call("health", json!({})).await.ok();
                    let available = media
                        .as_ref()
                        .and_then(|health| health["ready"].as_bool())
                        .unwrap_or(false);
                    Ok(
                        json!({"rosRunning":self.ros.running(), "mediaAvailable":available,
                        "streams":self.state.lock().await.streams.values().collect::<Vec<_>>(), "media":media}),
                    )
                }
            }
        })
    }

    async fn execute(&self, peer: &str, command: CameraCommand) -> Result<Value> {
        robot_common_lib::operation_log::info(
            module_path!(),
            format_args!("Receiving camera command"),
        );
        robot_common_lib::logged_result!(async "execute", {
            let mut state = self.state.lock().await;
            ensure!(
                state.peers.contains_key(peer),
                "Camera peer has disconnected"
            );
            match command {
                CameraCommand::Start { camera, resolution } => {
                    self.start(&mut state, peer, &camera, &resolution).await
                }
                CameraCommand::Stop(camera) => Ok(json!(self.stop(&mut state, &camera).await?)),
                CameraCommand::CreateTransport => {
                    self.media
                        .call("createTransport", json!({"peerId":peer}))
                        .await
                }
                CameraCommand::ConnectTransport(dtls) => {
                    ensure!(dtls.is_object(), "Invalid DTLS parameters");
                    self.media
                        .call(
                            "connectTransport",
                            json!({"peerId":peer, "dtlsParameters":dtls}),
                        )
                        .await?;
                    Ok(json!(true))
                }
                CameraCommand::Consume {
                    camera,
                    capabilities,
                } => {
                    ensure!(capabilities.is_object(), "Invalid RTP capabilities");
                    let stream = state
                        .streams
                        .get(&camera)
                        .context("Camera is not configured")?;
                    ensure!(stream.active == Some(true), "Camera stream is unavailable");
                    // A lost RPC reply still leaves an owned consumer for disconnect cleanup.
                    state.peers.get_mut(peer).context("Camera peer has disconnected")?.insert(camera.clone());
                    self.media
                        .call(
                            "consume",
                            json!({"peerId":peer, "camera":camera, "rtpCapabilities":capabilities}),
                        )
                        .await
                }
                CameraCommand::Resume(camera) => {
                    ensure!(
                        state.peers.get(peer).is_some_and(|owned| owned.contains(&camera)),
                        "Camera consumer is unavailable"
                    );
                    self.media
                        .call("resume", json!({"peerId":peer, "camera":camera}))
                        .await?;
                    Ok(json!(true))
                }
                CameraCommand::Leave(camera) => {
                    ensure!(
                        state.streams.contains_key(&camera),
                        "Camera is not configured"
                    );
                    self.media
                        .call("closeConsumer", json!({"peerId":peer, "camera":camera}))
                        .await?;
                    let owned = state.peers.get_mut(peer).context("Camera peer has disconnected")?.remove(&camera);
                    if owned && !state.peers.values().any(|other| other.contains(&camera)) {
                        state.pending_cameras.insert(camera.clone());
                        self.stop(&mut state, &camera).await?;
                    }
                    Ok(json!(true))
                }
            }
        })
    }
}

fn parse_number<T: std::str::FromStr>(value: &Value) -> Result<T> {
    robot_common_lib::logged_result!("parse_number", {
        let text = value
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| value.to_string());
        text.parse()
            .map_err(|_| anyhow::anyhow!("Invalid encoder RTP configuration"))
    })
}
