#!/usr/bin/env bash
set -euo pipefail
workspace_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$workspace_root"
: "${ROS_DISTRO:?Source the ROS 2 setup.bash first}"
: "${ROBOT_AGENT_DATABASE:?Set ROBOT_AGENT_DATABASE to the existing Monitor SQLite database}"
export ROS_ENABLED=true
export ROS_TOPICS_FILE="${ROS_TOPICS_FILE:-$workspace_root/deploy/yellowarm-sim/ros-topics.json}"
export ROS_DOMAIN_ID="${ROS_DOMAIN_ID:-0}"
export ROS_NAMESPACE="${ROS_NAMESPACE:-/}"
export VLA_COMMAND_TOPIC="${VLA_COMMAND_TOPIC:-/yellowarm_sim/joint_command}"
export VLA_STATE_TOPIC="${VLA_STATE_TOPIC:-/yellowarm_sim/joint_states}"
export ROS_LOCALHOST_ONLY="${ROS_LOCALHOST_ONLY:-0}"
export RMW_IMPLEMENTATION="${RMW_IMPLEMENTATION:-rmw_fastrtps_cpp}"
binary="${ROBOT_AGENT_BINARY:-$workspace_root/target/debug/robot-agent}"
if [[ ! -x "$binary" ]]; then
    echo 'Build first: cargo build -p robot-agent --features ros2 --locked' >&2
    exit 1
fi
exec "$binary"
