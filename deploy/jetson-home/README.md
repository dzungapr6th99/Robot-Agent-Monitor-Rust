# Robot Agent Monitor on Jetson

Deployment layout in the user's home:

```text
robot-agent-monitor/
  backend/                 Rust n-layer source
    bin/robot-agent         Built Linux aarch64 binary with ros2
  frontend/                Next.js source and native Linux dependencies/build
    design_figma/          Original Claude Design HTML
  .tools/node/             User-local Node 22; no system Node changes
  logs/                    Build logs
  start-backend.sh
  start-frontend.sh
  backend.env.example
```

The executable is built against the Jetson's existing ROS Humble/generated Rust interfaces. Windows target/node_modules/.next and browser token settings were excluded from copying. Both lockfiles are retained. Rust build cache/toolchain from the isolated test directory is reused only for building; the backend executable is copied into backend/bin.

## Run

Copy backend.env.example to backend.env and set ROBOT_AGENT_DATABASE to the existing Monitor SQLite database and URDF_DATA_ROOT to its asset directory. No production database is created or replaced by this deployment.

```bash
cd ~/robot-agent-monitor
cp backend.env.example backend.env
# Edit backend.env with the real database/storage paths first.
bash start-backend.sh
# In another terminal:
bash start-frontend.sh
```

Frontend: http://127.0.0.1:3000. Backend: http://127.0.0.1:8080. Frontend uses the same-origin /backend proxy. To access it from a workstation without changing the loopback bind, use an SSH tunnel: ssh -L 3000:127.0.0.1:3000 user@jetson. Alternatively set FRONTEND_HOST and FRONTEND_PORT explicitly when launching the frontend.

The YellowArm launcher subscribes /yellowarm_sim/joint_states and /yellowarm_sim/clock. Isaac Sim and the backend must use the same ROS_DOMAIN_ID (default 0), then Play the scene. See backend/deploy/yellowarm-sim/README.md for details. Realtime access on the LAN deployment opens directly without login.

The copy/build operation does not create a system service or start persistent application processes. Build logs are under logs/.
