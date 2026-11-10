use robot_ros2_interface_message::*;

#[test]
fn joint_state_rejects_inconsistent_arrays() {
    let message = JointState {
        name: vec!["a".into(), "b".into()],
        position: vec![1.0],
        ..Default::default()
    };
    assert!(message.validate().is_err());
    let message = JointState {
        name: vec!["a".into()],
        position: vec![1.0],
        ..Default::default()
    };
    assert!(message.validate().is_ok());
}
#[test]
fn odometry_covariance_and_rotation_defaults_are_valid() {
    let message = Odometry::default();
    assert_eq!(message.pose.pose.orientation.w, 1.0);
    assert_eq!(message.pose.covariance, [0.0; 36]);
    let mut value = serde_json::to_value(message).unwrap();
    value["pose"]["covariance"] = serde_json::json!([0.0]);
    assert!(serde_json::from_value::<Odometry>(value).is_err());
}
#[test]
fn diagnostic_event_uses_the_existing_json_contract() {
    let message = DiagnosticArray {
        status: vec![DiagnosticStatus {
            level: DiagnosticStatus::WARN,
            name: "motor".into(),
            values: vec![KeyValue {
                key: "temperature".into(),
                value: "80".into(),
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let event = RosMessageEvent::from_message("/diagnostics".into(), message).unwrap();
    let value = serde_json::to_value(event).unwrap();
    assert_eq!(value["messageType"], "diagnostic_msgs/msg/DiagnosticArray");
    assert_eq!(
        value["data"]["status"][0]["values"][0]["key"],
        "temperature"
    );
}

#[test]
fn clock_uses_ros_simulation_time_json_and_rejects_invalid_nanoseconds() {
    let mut message: Clock =
        serde_json::from_value(serde_json::json!({"clock":{"sec":42,"nanosec":123}})).unwrap();
    assert!(message.validate().is_ok());
    let event =
        RosMessageEvent::from_message("/yellowarm_sim/clock".into(), message.clone()).unwrap();
    assert_eq!(event.message_type, "rosgraph_msgs/msg/Clock");
    assert_eq!(event.data["clock"]["sec"], 42);
    message.clock.nanosec = 1_000_000_000;
    assert!(message.validate().is_err());
}
