"""Exercise real agent HTTP VLA management & execution routes; stdlib only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import socket
import sqlite3
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
import uuid

from http_compatibility import schema as crud_schema
from make_vla_package import create_package

ROOT = Path(__file__).resolve().parents[1]

SAMPLE_URDF = b"""<?xml version="1.0"?>
<robot name="yellowarm">
  <link name="base_link"/>
  <link name="link1"/>
  <link name="link2"/>
  <joint name="joint1" type="revolute">
    <parent link="base_link"/>
    <child link="link1"/>
  </joint>
  <joint name="joint2" type="continuous">
    <parent link="link1"/>
    <child link="link2"/>
  </joint>
</robot>
"""


def encode_multipart(fields: list[tuple[str, str]], files: list[tuple[str, str, bytes, str]]):
    boundary = f"----vla-{uuid.uuid4().hex}"
    chunks: list[bytes] = []
    for name, value in fields:
        chunks.append(
            f"--{boundary}\r\n"
            f'Content-Disposition: form-data; name="{name}"\r\n\r\n'
            f"{value}\r\n".encode("utf-8")
        )
    for name, filename, data, content_type in files:
        chunks.append(
            f"--{boundary}\r\n"
            f'Content-Disposition: form-data; name="{name}"; filename="{filename}"\r\n'
            f"Content-Type: {content_type}\r\n\r\n".encode("utf-8")
        )
        chunks.append(data)
        chunks.append(b"\r\n")
    chunks.append(f"--{boundary}--\r\n".encode("utf-8"))
    return b"".join(chunks), f"multipart/form-data; boundary={boundary}"


def check(executable: Path) -> None:
    with tempfile.TemporaryDirectory(prefix="vla-http-", dir=ROOT / "target") as directory:
        root = Path(directory)
        database = root / "fixture.db"
        conn = sqlite3.connect(database)
        conn.executescript(crud_schema())
        conn.close()

        pkg_path = create_package(
            root / "yellowarm-smolvla-1.0.0.zip",
            code="yellowarm-smolvla",
            name="YellowArm Pick Cube",
            version="1.0.0",
            joints=["joint1", "joint2"],
            camera="front",
            dummy_onnx=True,
        )
        pkg_bytes = pkg_path.read_bytes()
        pkg_sha = hashlib.sha256(pkg_bytes).hexdigest()

        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]

        env = dict(
            os.environ,
            ROS_ENABLED="false",
            ROBOT_AGENT_DATABASE=str(database),
            URDF_DATA_ROOT=str(root / "storage"),
            VLA_DATA_ROOT=str(root / "storage"),
            VLA_MAX_PACKAGE_BYTES="1048576",
            AGENT_LISTEN=f"127.0.0.1:{port}",
            ROS_TOPICS_FILE=str(ROOT / "deploy/yellowarm-sim/ros-topics.json"),
            VLA_TIMEOUT_MS="600000",
            VLA_COMMAND_TOPIC="/yellowarm_sim/joint_command",
            VLA_STATE_TOPIC="/yellowarm_sim/joint_states",
            REALTIME_ALLOWED_ORIGINS="",
        )
        env.pop("ROBOT_CONFIGURATION_ID", None)

        with (root / "agent.log").open("wb") as log:
            process = subprocess.Popen(
                [str(executable)],
                cwd=ROOT,
                env=env,
                stdout=log,
                stderr=log,
                creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0),
            )
            try:
                base = f"http://127.0.0.1:{port}"

                def request(
                    path: str,
                    method: str = "GET",
                    data=None,
                    content_type: str = "application/json",
                    origin: str | None = None,
                ):
                    headers = {"Content-Type": content_type}
                    if origin:
                        headers["Origin"] = origin
                    payload = json.dumps(data).encode("utf-8") if isinstance(data, dict) else data
                    req = urllib.request.Request(base + path, data=payload, method=method, headers=headers)
                    try:
                        resp = urllib.request.urlopen(req, timeout=10)
                    except urllib.error.HTTPError as err:
                        resp = err
                    with resp:
                        raw = resp.read()
                        try:
                            parsed = json.loads(raw) if raw else None
                        except json.JSONDecodeError:
                            parsed = raw.decode("utf-8", errors="replace")
                        return resp.status, parsed

                for _ in range(100):
                    try:
                        status, _ = request("/api/vla/status")
                        if status == 200:
                            break
                    except urllib.error.URLError:
                        if process.poll() is not None:
                            raise AssertionError((root / "agent.log").read_text(encoding="utf-8", errors="replace"))
                        time.sleep(0.1)
                else:
                    raise AssertionError("Agent did not start")

                # Setup profile + URDF configuration + camera sensor/stream.
                st, prof = request(
                    "/api/v1/robot-profiles/create",
                    "POST",
                    {"requestId": "r1", "data": {"name": "YellowArm"}},
                )
                assert st == 200, (st, prof)
                profile_id = prof["recordId"]

                urdf_body, urdf_ct = encode_multipart(
                    [("RequestId", "r2"), ("Data.RobotProfileId", str(profile_id)), ("Data.Name", "YellowArm v1")],
                    [("AttachFiles", "yellowarm.urdf", SAMPLE_URDF, "application/xml")],
                )
                st, cfg = request("/api/v1/robot-configurations/create", "POST", urdf_body, urdf_ct)
                assert st == 200, (st, cfg)
                config_id = cfg["recordId"]

                st, sens = request(
                    "/api/v1/sensors/create",
                    "POST",
                    {
                        "requestId": "r3",
                        "data": {
                            "robotConfigurationId": config_id,
                            "code": "front",
                            "displayName": "Front Cam",
                            "sensorType": "camera",
                            "isEnabled": True,
                        },
                    },
                )
                assert st == 200, (st, sens)
                sensor_id = sens["recordId"]

                st, stream = request(
                    "/api/v1/sensor-streams/create",
                    "POST",
                    {
                        "requestId": "r4",
                        "data": {
                            "sensorId": sensor_id,
                            "role": "rgb",
                            "topic": "/cam/front/image_raw",
                            "dataType": "sensor_msgs/msg/Image",
                            "isEnabled": True,
                        },
                    },
                )
                assert st == 200, (st, stream)

                # 1. Upload VLA zip package.
                vla_body, vla_ct = encode_multipart(
                    [("robotConfigurationId", str(config_id))],
                    [("package", pkg_path.name, pkg_bytes, "application/zip")],
                )
                st, created = request("/api/vla/models", "POST", vla_body, vla_ct)
                assert st == 201 and created["id"] > 0, (st, created)
                model_id = created["id"]

                # 2. GET detail & verify manifest + sha256 match.
                st, detail = request(f"/api/vla/models/{model_id}")
                assert st == 200, (st, detail)
                assert detail["packageSha256"] == pkg_sha, detail
                assert detail["manifest"]["code"] == "yellowarm-smolvla", detail

                # Search by version.
                st, found = request(f"/api/vla/models?robotConfigurationId={config_id}&q=1.0.0")
                assert st == 200 and len(found) == 1 and found[0]["id"] == model_id, (st, found)

                # Update displayName & description.
                st, updated = request(
                    f"/api/vla/models/{model_id}",
                    "PUT",
                    {"displayName": "YellowArm Updated", "description": "Test desc"},
                )
                assert st == 200 and updated["displayName"] == "YellowArm Updated", (st, updated)

                # 3. Command with wrong robotConfigurationId -> 409.
                st, err = request(
                    "/api/vla/commands",
                    "POST",
                    {"robotConfigurationId": config_id + 99, "modelId": model_id, "task": "move"},
                )
                assert st == 409 and err.get("message"), (st, err)

                # 4. Command with ROS disabled -> 503.
                st, err = request(
                    "/api/vla/commands",
                    "POST",
                    {"robotConfigurationId": config_id, "modelId": model_id, "task": "move"},
                )
                assert st == 503 and err.get("message"), (st, err)

                # Stop when idle.
                st, stop_res = request("/api/vla/stop", "POST", {})
                assert st == 200 and stop_res == {"stopping": False}, (st, stop_res)

                # Untrusted origin -> 403.
                st, _ = request(
                    "/api/vla/commands",
                    "POST",
                    {"robotConfigurationId": config_id, "modelId": model_id, "task": "move"},
                    origin="https://untrusted.invalid",
                )
                assert st == 403, st

                # Oversized package -> 413.
                big_body, big_ct = encode_multipart(
                    [("robotConfigurationId", str(config_id))],
                    [("package", "huge.zip", b"0" * (1024 * 1024 + 4096), "application/zip")],
                )
                st, _ = request("/api/vla/models", "POST", big_body, big_ct)
                assert st == 413, st

                # 5. Delete URDF while VLA model exists -> blocked (400 validation error).
                st, del_urdf = request(
                    "/api/v1/robot-configurations/delete",
                    "DELETE",
                    {"requestId": "r5", "data": {"id": config_id}},
                )
                assert st == 400 and "Xóa hết 1 model VLA" in del_urdf["message"][0], (st, del_urdf)

                # 6. Delete VLA model, then delete URDF -> succeeds.
                st, del_model = request(f"/api/vla/models/{model_id}", "DELETE")
                assert st == 200 and del_model["deleted"] is True, (st, del_model)

                st, del_urdf_ok = request(
                    "/api/v1/robot-configurations/delete",
                    "DELETE",
                    {"requestId": "r6", "data": {"id": config_id}},
                )
                assert st == 200, (st, del_urdf_ok)

                print("PASS: full VLA HTTP management & validation lifecycle")
            finally:
                process.terminate()
                process.wait(timeout=10)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    args = parser.parse_args()
    check(args.executable.resolve())
