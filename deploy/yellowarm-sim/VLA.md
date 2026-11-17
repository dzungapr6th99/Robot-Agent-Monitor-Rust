# Chat VLA -> ROS

The backend calls an external inference server. It does not run SmolVLA or GR00T
itself. Both profile families use the same HTTP adapter; an existing model server
with another protocol needs an adapter implementing this contract.
Only named joint positions are sent as observations. No camera is used.

| Variable | Default | Meaning |
| --- | --- | --- |
| VLA_URL | http://127.0.0.1:8091/ | Base URL ending in `/`; HTTPS or loopback HTTP |
| VLA_MODELS_FILE | unset | JSON array of profiles; unset returns `[]` |
| VLA_TIMEOUT_MS | 600000 | Whole-command deadline; positive and below 750000 |
| VLA_COMMAND_TOPIC | /yellowarm_sim/joint_command | Configured JointState publisher |
| VLA_STATE_TOPIC | /yellowarm_sim/joint_states | Configured JointState subscription |
| VLA_MAX_STEPS | 1000 | Positive maximum published actions per command |
| VLA_STEP_PERIOD_MS | 100 | Positive delay between published actions |

Use `vla-models.json` as a template; replace its checkpoint version with an actual
trained checkpoint accepted by the inference server. Profile ids must be unique,
model must be `smolvla` or `groot`, and unknown fields are rejected.
The backend validates both configured topics at startup when profiles are enabled.
The adapter also supports optional server authentication via deployment configuration.

```bash
export VLA_URL=http://127.0.0.1:8091/
export VLA_MODELS_FILE="$PWD/deploy/yellowarm-sim/vla-models.json"
# Source ROS, build with --features ros2 and configure the existing database first.
bash deploy/yellowarm-sim/run.sh
```

`POST {VLA_URL}infer` request:

```json
{"profileId":"yellowarm-smolvla","task":"hold position","observation":{"name":["joint1"],"position":[0.0]}}
```

Response:

```json
{"actions":[{"name":["joint1"],"position":[0.0]}],"done":true}
```

Each action must contain finite positions for exactly the observed joint names.
The backend validates the whole batch before publishing it, then publishes each
action as `sensor_msgs/msg/JointState`. With `done:false`, it gets a fresh joint
observation and calls inference again. It waits up to 5 seconds for observations.
An empty batch with `done:false` is rejected. `max_steps` truncates a longer batch.
Stop cancels observation, inference, publish waiting and step delay; the result is
the number of steps completed before cancellation. One command runs at a time.
Joint limits are not clamped: use a checkpoint and robot-specific limits in the
inference adapter and verify in Isaac Sim before commanding hardware.

Frontend API:

- `GET /api/vla/models` returns profiles.
- `POST /api/vla/commands` accepts `{profileId,task}` and returns `{publishedSteps}`.
- `POST /api/vla/stop` returns `{stopping}` (false when idle).

Errors contain `{message}`: 400 invalid request, 404 unknown profile, 409 busy,
503 ROS/inference unavailable, 504 timeout. POST routes check Origin.
Keep the frontend proxy deadline above the backend whole-command deadline.

## Verification

```bash
python UnitTest/fake_vla_server.py --steps 12
python UnitTest/vla_http.py --executable target/debug/robot-agent
```

The fixture only replaces inference, returning the observed positions unchanged
(a hold command). It never emulates ROS or proves model-controlled movement.
The HTTP check launches the actual backend with ROS disabled and verifies 503.

For the real acceptance run: start Isaac Sim with Play, start a real trained VLA
server, then run the backend built with `ros2`. Confirm `/api/ros/status` reports
`running:true`; run `ros2 topic echo /yellowarm_sim/joint_command` while submitting
a chat instruction. Compare received command count with `publishedSteps`, observe
movement, and verify Stop prevents subsequent steps within one configured period.
Save stdout/stderr under `logs/yellowarm_rust_vla_*`. This acceptance run requires
the actual ROS/Isaac environment and has not been established by the HTTP tests.
