pub mod resource;
pub use resource::Resource;
pub mod db_base_object;
pub use db_base_object::DbBaseObject;

pub type Record = serde_json::Map<String, serde_json::Value>;

pub trait DbObject: serde::Serialize + serde::de::DeserializeOwned + Send + Sync + 'static {
    const RESOURCE: Resource;
    const TABLE: DbTableAttribute;
}
pub mod robot_profile;
pub use robot_profile::RobotProfile;
pub mod robot_configuration;
pub use robot_configuration::RobotConfiguration;
pub mod joint_inteface;
pub use joint_inteface::JointInteface;
pub mod sensor;
pub use sensor::Sensor;
pub mod sensor_stream;
pub use sensor_stream::SensorStream;
pub mod joint_group;
pub use joint_group::JointGroup;
pub mod joint_group_members;
pub use joint_group_members::JointGroupMembers;
pub mod pose_preset;
pub use pose_preset::PosePreset;
pub mod pose_preset_joint;
pub use pose_preset_joint::PosePresetJoint;
pub mod episode;
pub use episode::Episode;
pub mod episode_annotation;
pub use episode_annotation::EpisodeAnnotation;

pub mod attributes;
pub use attributes::DbTableAttribute;
