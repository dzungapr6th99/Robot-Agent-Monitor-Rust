use crate::traits::CrudServicesTrait;
use crate::traits::{BaseBLTrait, RobotConfigurationBLTrait};
use crate::*;
use anyhow::Result;
use robot_common_lib::Upload;
use robot_db_objects::{Record, Resource};
use std::sync::Arc;

pub struct CrudServices {
    pub robot_profiles: Arc<RobotProfileBL>,
    pub robot_configurations: Arc<RobotConfigurationBL>,
    pub joint_interfaces: Arc<JointInterfaceBL>,
    pub sensors: Arc<SensorBL>,
    pub sensor_streams: Arc<SensorStreamBL>,
    pub joint_groups: Arc<JointGroupBL>,
    pub joint_group_members: Arc<JointGroupMemberBL>,
    pub pose_presets: Arc<PosePresetBL>,
    pub pose_preset_joints: Arc<PosePresetJointBL>,
    pub episodes: Arc<EpisodeBL>,
    pub episode_annotations: Arc<EpisodeAnnotationBL>,
}
impl CrudServicesTrait for CrudServices {
    fn max_urdf_bytes(&self) -> usize {
        self.robot_configurations.max_urdf_bytes
    }
    fn insert(&self, resource: Resource, data: Record, upload: Option<Upload>) -> Result<i64> {
        robot_common_lib::logged_result!("insert", {
            match resource {
                Resource::RobotProfile => self.robot_profiles.insert(data),
                Resource::RobotConfiguration => self.robot_configurations.insert(data, upload),
                Resource::JointInteface => self.joint_interfaces.insert(data),
                Resource::Sensor => self.sensors.insert(data),
                Resource::SensorStream => self.sensor_streams.insert(data),
                Resource::JointGroup => self.joint_groups.insert(data),
                Resource::JointGroupMembers => self.joint_group_members.insert(data),
                Resource::PosePreset => self.pose_presets.insert(data),
                Resource::PosePresetJoint => self.pose_preset_joints.insert(data),
                Resource::Episode => self.episodes.insert(data),
                Resource::EpisodeAnnotation => self.episode_annotations.insert(data),
            }
        })
    }
    fn update(&self, resource: Resource, data: Record) -> Result<i64> {
        robot_common_lib::logged_result!("update", {
            match resource {
                Resource::RobotProfile => self.robot_profiles.update(data),
                Resource::RobotConfiguration => self.robot_configurations.update(data),
                Resource::JointInteface => self.joint_interfaces.update(data),
                Resource::Sensor => self.sensors.update(data),
                Resource::SensorStream => self.sensor_streams.update(data),
                Resource::JointGroup => self.joint_groups.update(data),
                Resource::JointGroupMembers => self.joint_group_members.update(data),
                Resource::PosePreset => self.pose_presets.update(data),
                Resource::PosePresetJoint => self.pose_preset_joints.update(data),
                Resource::Episode => self.episodes.update(data),
                Resource::EpisodeAnnotation => self.episode_annotations.update(data),
            }
        })
    }
    fn delete(&self, resource: Resource, data: Record) -> Result<i64> {
        robot_common_lib::logged_result!("delete", {
            match resource {
                Resource::RobotProfile => self.robot_profiles.delete(data),
                Resource::RobotConfiguration => self.robot_configurations.delete(data),
                Resource::JointInteface => self.joint_interfaces.delete(data),
                Resource::Sensor => self.sensors.delete(data),
                Resource::SensorStream => self.sensor_streams.delete(data),
                Resource::JointGroup => self.joint_groups.delete(data),
                Resource::JointGroupMembers => self.joint_group_members.delete(data),
                Resource::PosePreset => self.pose_presets.delete(data),
                Resource::PosePresetJoint => self.pose_preset_joints.delete(data),
                Resource::Episode => self.episodes.delete(data),
                Resource::EpisodeAnnotation => self.episode_annotations.delete(data),
            }
        })
    }
    fn list(&self, resource: Resource, limit: u32, offset: u32) -> Result<Vec<serde_json::Value>> {
        robot_common_lib::logged_result!("list", {
            match resource {
                Resource::RobotProfile => Ok(self
                    .robot_profiles
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::RobotConfiguration => Ok(self
                    .robot_configurations
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::JointInteface => Ok(self
                    .joint_interfaces
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::Sensor => Ok(self
                    .sensors
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::SensorStream => Ok(self
                    .sensor_streams
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::JointGroup => Ok(self
                    .joint_groups
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::JointGroupMembers => Ok(self
                    .joint_group_members
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::PosePreset => Ok(self
                    .pose_presets
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::PosePresetJoint => Ok(self
                    .pose_preset_joints
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::Episode => Ok(self
                    .episodes
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::EpisodeAnnotation => Ok(self
                    .episode_annotations
                    .list(limit, offset)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
            }
        })
    }
    fn query(&self, resource: Resource, ids: &[i64]) -> Result<Vec<serde_json::Value>> {
        robot_common_lib::logged_result!("query", {
            match resource {
                Resource::RobotProfile => Ok(self
                    .robot_profiles
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::RobotConfiguration => Ok(self
                    .robot_configurations
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::JointInteface => Ok(self
                    .joint_interfaces
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::Sensor => Ok(self
                    .sensors
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::SensorStream => Ok(self
                    .sensor_streams
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::JointGroup => Ok(self
                    .joint_groups
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::JointGroupMembers => Ok(self
                    .joint_group_members
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::PosePreset => Ok(self
                    .pose_presets
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::PosePresetJoint => Ok(self
                    .pose_preset_joints
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::Episode => Ok(self
                    .episodes
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
                Resource::EpisodeAnnotation => Ok(self
                    .episode_annotations
                    .query(ids)?
                    .into_iter()
                    .map(serde_json::to_value)
                    .collect::<serde_json::Result<Vec<_>>>()?),
            }
        })
    }
}
