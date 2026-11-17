"""Exercise the actual agent HTTP VLA routes with ROS disabled; stdlib only."""
import argparse
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

ROOT = Path(__file__).resolve().parents[1]


def check(executable, configured):
    with tempfile.TemporaryDirectory(prefix="vla-http-", dir=ROOT / "target") as directory:
        root = Path(directory)
        database = root / "fixture.db"
        sqlite3.connect(database).close()
        profiles = [{"id": "arm", "model": "smolvla", "version": "v1"}]
        models = root / "profiles.json"
        models.write_text(json.dumps(profiles), encoding="utf-8")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        env = dict(os.environ, ROS_ENABLED="false", ROBOT_AGENT_DATABASE=str(database),
                   URDF_DATA_ROOT=str(root), AGENT_LISTEN=f"127.0.0.1:{port}",
                   ROS_TOPICS_FILE=str(ROOT / "deploy/yellowarm-sim/ros-topics.json"),
                   VLA_URL="http://127.0.0.1:8091/", VLA_TIMEOUT_MS="600000",
                   VLA_COMMAND_TOPIC="/yellowarm_sim/joint_command",
                   VLA_STATE_TOPIC="/yellowarm_sim/joint_states", REALTIME_ALLOWED_ORIGINS="")
        env.pop("ROBOT_CONFIGURATION_ID", None)
        env.pop("VLA_MODELS_FILE", None)
        if configured:
            env["VLA_MODELS_FILE"] = str(models)
        with (root / "agent.log").open("wb") as log:
            process = subprocess.Popen([str(executable)], cwd=ROOT, env=env, stdout=log, stderr=log,
                                       creationflags=getattr(subprocess, "CREATE_NO_WINDOW", 0))
            try:
                def request(path, data=None, origin=None):
                    headers = {"Content-Type": "application/json"}
                    if origin:
                        headers["Origin"] = origin
                    req = urllib.request.Request(f"http://127.0.0.1:{port}/api/vla/{path}",
                        data=None if data is None else json.dumps(data).encode(), headers=headers)
                    try:
                        response = urllib.request.urlopen(req, timeout=5)
                    except urllib.error.HTTPError as error:
                        response = error
                    with response:
                        return response.status, json.load(response)
                for _ in range(100):
                    try:
                        result = request("models")
                        break
                    except urllib.error.URLError:
                        if process.poll() is not None:
                            raise AssertionError("Agent exited before accepting HTTP")
                        time.sleep(0.1)
                else:
                    raise AssertionError("Agent did not start")
                assert result == (200, profiles if configured else []), result
                assert request("stop", {}) == (200, {"stopping": False})
                status, body = request("commands", {"profileId": "arm", "task": "move"})
                assert status == (503 if configured else 404) and body.get("message"), (status, body)
                assert request("commands", {"profileId": "missing", "task": "move"})[0] == 404
                for body in [{"profileId": "arm", "task": " "}, {"profileId": "arm", "task": "x" * 2049}, {}, {"profileId": "arm", "task": "move", "extra": True}]:
                    status, result = request("commands", body)
                    assert status == 400 and result.get("message"), (status, result)
                assert request("commands", {"profileId": "arm", "task": "move"}, "https://untrusted.invalid")[0] == 403
                assert request("stop", {}, "https://untrusted.invalid")[0] == 403
                print(f"PASS: {'configured' if configured else 'empty'} profiles; 10 HTTP assertions")
            finally:
                process.terminate()
                process.wait(timeout=10)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--executable", type=Path, required=True)
    args = parser.parse_args()
    for configured in (False, True):
        check(args.executable.resolve(), configured)
