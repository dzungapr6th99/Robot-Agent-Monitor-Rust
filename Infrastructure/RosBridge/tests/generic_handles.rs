#[cfg(not(feature = "ros2"))]
#[test]
fn enabling_ros_requires_an_explicit_ros_build() {
    assert!(!robot_ros_bridge::RobotRos::default().running());
    let error = robot_ros_bridge::RobotRos::start(0, "/")
        .err()
        .expect("ROS build feature is required");
    assert!(error.to_string().contains("--features ros2"));
}
