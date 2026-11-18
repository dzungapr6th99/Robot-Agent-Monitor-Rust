#!/usr/bin/env bash
set -euo pipefail
app_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
if [[ -f "$app_root/backend.env" ]]; then
    set -a
    source "$app_root/backend.env"
    set +a
fi
# ROS setup scripts reference optional unset variables.
set +u
source "/opt/ros/${ROS_DISTRO:-humble}/setup.bash"
set -u
export ROBOT_AGENT_BINARY="$app_root/backend/bin/robot-agent"
exec bash "$app_root/backend/deploy/yellowarm-sim/run.sh"
