"""Compare the same HTTP CRUD workflow before and after the n-layer refactor.

Run with the Python standard library. Each process uses its own temporary SQLite
database and URDF directory. No deployed database is opened.
"""
import argparse
from contextlib import closing
import json
import os
from pathlib import Path
import re
import socket
import sqlite3
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
# Fixture schema for the Monitor SQLite tables; columns follow DbObjects metadata.
TABLES = {
    "robot_profiles": "name TEXT NOT NULL UNIQUE, description TEXT",
    "robot_configurations": "robot_profile_id INTEGER NOT NULL REFERENCES robot_profiles(id) ON DELETE RESTRICT, "
                            "name TEXT NOT NULL, urdf_path TEXT NOT NULL, UNIQUE(robot_profile_id, name)",
    "joint_interfaces": "robot_configuration_id INTEGER NOT NULL REFERENCES robot_configurations(id) ON DELETE CASCADE, "
                        "name TEXT NOT NULL, operation TEXT, ros_endpoint TEXT, data_type TEXT, is_enabled INTEGER, "
                        "is_default_command INTEGER",
    "joint_groups": "robot_configuration_id INTEGER NOT NULL REFERENCES robot_configurations(id) ON DELETE CASCADE, "
                    "name TEXT NOT NULL, display_name TEXT NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0",
    "joint_group_members": "joint_group_id INTEGER NOT NULL REFERENCES joint_groups(id) ON DELETE CASCADE, "
                           "joint_name TEXT NOT NULL, sort_order INTEGER NOT NULL DEFAULT 0, "
                           "UNIQUE(joint_group_id, joint_name)",
    "sensors": "robot_configuration_id INTEGER NOT NULL REFERENCES robot_configurations(id) ON DELETE CASCADE, "
               "code TEXT, display_name TEXT, sensor_type TEXT, is_enabled INTEGER, UNIQUE(robot_configuration_id, code)",
    "sensor_streams": "sensor_id INTEGER NOT NULL REFERENCES sensors(id) ON DELETE CASCADE, "
                      "role TEXT, topic TEXT, data_type TEXT, is_enabled INTEGER",
    "pose_presets": "robot_configuration_id INTEGER REFERENCES robot_configurations(id) ON DELETE CASCADE, name TEXT",
    "pose_preset_joints": "pose_preset_id INTEGER REFERENCES pose_presets(id) ON DELETE CASCADE, "
                          "joint_name TEXT, position REAL",
    "episodes": "robot_configuration_id INTEGER NOT NULL REFERENCES robot_configurations(id) ON DELETE RESTRICT, "
                "name TEXT NOT NULL, task TEXT, status TEXT, result TEXT, recording_path TEXT, operator_note TEXT, "
                "started_at TEXT, ended_at TEXT, duration_ns INTEGER",
    "episode_annotations": "episode_id INTEGER REFERENCES episodes(id) ON DELETE CASCADE, "
                           "start_time_ns INTEGER, end_time_ns INTEGER, label TEXT, note TEXT",
}


def schema():
    statements = []
    for table, fields in TABLES.items():
        columns, _, unique = fields.partition(", UNIQUE(")
        constraint = f", UNIQUE({unique}" if unique else ""
        statements.append(
            f"CREATE TABLE {table} (id INTEGER PRIMARY KEY, {columns}, "
            "created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP, "
            f"updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP{constraint});")
    return "\n".join(statements)


def run_workflow(executable):
    responses = []
    with tempfile.TemporaryDirectory(prefix="robot-http-") as directory:
        storage = Path(directory)
        database = storage / "fixture.db"
        with closing(sqlite3.connect(database)) as connection, connection:
            connection.executescript(schema())
        with socket.socket() as listener:
            listener.bind(("127.0.0.1", 0))
            port = listener.getsockname()[1]
        environment = dict(os.environ, ROBOT_AGENT_DATABASE=str(database),
                           URDF_DATA_ROOT=directory, AGENT_LISTEN=f"127.0.0.1:{port}",
                           ROS_ENABLED="false")
        environment.pop("ROBOT_CONFIGURATION_ID", None)
        flags = getattr(subprocess, "CREATE_NO_WINDOW", 0)
        with (storage / "agent.log").open("wb") as log:
            process = subprocess.Popen([str(executable)], cwd=ROOT, env=environment,
                                       stdout=log, stderr=log, creationflags=flags)
            try:
                base = f"http://127.0.0.1:{port}"

                def request(path, method="GET", data=None, content_type="application/json", record=True):
                    if isinstance(data, dict):
                        data = json.dumps(data).encode()
                    query = urllib.request.Request(base + path, data=data, method=method,
                                                   headers={"Content-Type": content_type})
                    try:
                        response = urllib.request.urlopen(query, timeout=10)
                    except urllib.error.HTTPError as error:
                        response = error
                    with response:
                        raw = response.read()
                        try:
                            body = json.loads(raw) if raw else None
                        except json.JSONDecodeError:
                            body = raw.decode()
                    result = (response.status, body)
                    if record:
                        responses.append((method, path, result))
                    return result

                for _ in range(80):
                    try:
                        if request("/api/ros/status", record=False)[0] == 200:
                            break
                    except urllib.error.URLError:
                        if process.poll() is not None:
                            raise AssertionError((storage / "agent.log").read_text())
                        time.sleep(0.1)
                else:
                    raise AssertionError("Agent did not start")

                def freeze_timestamps():
                    with closing(sqlite3.connect(database)) as connection, connection:
                        for table, in connection.execute("SELECT name FROM sqlite_master WHERE type='table'"):
                            if not re.fullmatch(r"[a-z_]+", table):
                                continue
                            columns = {row[1] for row in connection.execute(f"PRAGMA table_info({table})")}
                            for column in ("created_at", "updated_at"):
                                if column in columns:
                                    connection.execute(f"UPDATE {table} SET {column}='2026-11-05T01:00:00Z'")

                def crud(resource, action, method="GET", data=None, query="", content_type="application/json"):
                    freeze_timestamps()
                    return request(f"/api/v1/{resource}/{action}{query}", method, data, content_type)

                def create(resource, data):
                    status, body = crud(resource, "create", "POST", {"requestId": "create", "data": data})
                    assert status == 200 and body["code"] == [1], (resource, status, body)
                    return body["recordId"]

                def configuration(name, filename, xml, profile, expected=200):
                    boundary = "monitor-test-boundary"
                    parts = [
                        'Content-Disposition: form-data; name="RequestId"\r\n\r\nupload',
                        f'Content-Disposition: form-data; name="Data.RobotProfileId"\r\n\r\n{profile}',
                        f'Content-Disposition: form-data; name="Data.Name"\r\n\r\n{name}',
                        f'Content-Disposition: form-data; name="AttachFiles"; filename="{filename}"\r\nContent-Type: application/xml\r\n\r\n{xml}',
                    ]
                    body = "".join(f"--{boundary}\r\n{part}\r\n" for part in parts) + f"--{boundary}--\r\n"
                    status, response = crud("robot-configurations", "create", "POST", body.encode(),
                                            content_type=f"multipart/form-data; boundary={boundary}")
                    assert status == expected, (status, response)
                    return response.get("recordId")

                ids = {}
                profile = ids["robot-profiles"] = create("robot-profiles", {"name": "Robot", "description": "Description"})
                urdf = '<robot name="robot"><link name="base"/></robot>'
                config = ids["robot-configurations"] = configuration("Default", "robot.urdf", urdf, profile)
                assert (storage / f"robots/{profile}/{config}/robot.urdf").read_text() == urdf
                ids["joint-interfaces"] = create("joint-interfaces", {
                    "robotConfigurationId": config, "name": "joints", "operation": "subscribe",
                    "rosEndpoint": "/joint_states", "dataType": "sensor_msgs/msg/JointState"})
                sensor = ids["sensors"] = create("sensors", {
                    "robotConfigurationId": config, "code": "rgb", "displayName": "RGB", "sensorType": "camera_rgb"})
                ids["sensor-streams"] = create("sensor-streams", {
                    "sensorId": sensor, "role": "color", "topic": "/image", "dataType": "sensor_msgs/msg/Image"})
                group = ids["joint-groups"] = create("joint-groups", {
                    "robotConfigurationId": config, "name": "arm", "displayName": "Arm"})
                ids["joint-group-members"] = create("joint-group-members", {"jointGroupId": group, "jointName": "joint_a"})
                preset = ids["pose-presets"] = create("pose-presets", {"robotConfigurationId": config, "name": "home"})
                ids["pose-preset-joints"] = create("pose-preset-joints", {
                    "posePresetId": preset, "jointName": "joint_a", "position": 0.125})
                episode = ids["episodes"] = create("episodes", {
                    "robotConfigurationId": config, "name": "ep", "status": "completed", "result": "success",
                    "recordingPath": "ep", "startedAt": "2026-11-05T01:00:00Z"})
                ids["episode-annotations"] = create("episode-annotations", {
                    "episodeId": episode, "startTimeNs": 0, "endTimeNs": 100, "label": "begin"})

                updates = {
                    "robot-profiles": {"name": "Renamed", "description": None},
                    "robot-configurations": {"name": "Configuration"},
                    "joint-interfaces": {"isEnabled": False},
                    "sensors": {"displayName": "Camera"},
                    "sensor-streams": {"isEnabled": False},
                    "joint-groups": {"sortOrder": 2},
                    "joint-group-members": {"sortOrder": 3},
                    "pose-presets": {"name": "Pose"},
                    "pose-preset-joints": {"position": 0.25},
                    "episodes": {"operatorNote": "Reviewed"},
                    "episode-annotations": {"note": "Annotation"},
                }
                for resource, record_id in ids.items():
                    status, rows = crud(resource, "query", query=f"?ids={record_id}&ids=99999")
                    assert status == 200 and rows[0]["id"] == record_id, (resource, status, rows)
                    if resource == "robot-profiles":
                        assert rows[0]["description"] == "Description"
                        assert rows[0]["createdAt"] == "2026-11-05T01:00:00Z"
                        child = rows[0]["robotConfigurations"][0]
                        assert child["robotProfileId"] == profile
                        assert child["urdfPath"] == f"robots/{profile}/{config}/robot.urdf"
                    if resource == "joint-interfaces":
                        assert rows[0]["rosEndpoint"] == "/joint_states"
                        assert rows[0]["dataType"] == "sensor_msgs/msg/JointState"
                    data = dict(updates[resource], id=record_id)
                    assert crud(resource, "update", "PUT", {"requestId": "update", "data": data})[0] == 200
                    status, updated = crud(resource, "query", query=f"?ids={record_id}")
                    assert status == 200
                    for field, expected in updates[resource].items():
                        assert updated[0][field] == expected, (resource, field, updated)

                assert crud("sensors", "query", query=f"?ids={sensor}")[1][0]["sensorStreams"][0]["id"] == ids["sensor-streams"]
                configuration("Bad", "bad.txt", urdf, profile, 400)
                configuration("Bad", "bad.urdf", "<robot>", profile, 400)
                configuration("Bad", "bad.urdf", urdf, 99999, 400)
                assert request("/api/ros/status", record=False)[0] == 200
                assert crud("robot-profiles", "query")[0] == 400
                assert crud("robot-profiles", "query", query="?ids=bad")[0] == 400
                for resource, record_id in reversed(list(ids.items())):
                    assert crud(resource, "delete", "DELETE",
                                {"requestId": "delete", "data": {"id": record_id}})[0] == 200
                assert crud("robot-profiles", "query", query=f"?ids={profile}")[0] == 400
            except BaseException:
                log.flush()
                print((storage / "agent.log").read_text(encoding="utf-8", errors="replace"))
                raise
            finally:
                process.terminate()
                process.wait(timeout=10)
    return responses


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--executable", type=Path, required=True)
    parser.add_argument("--baseline", type=Path)
    options = parser.parse_args()
    actual = run_workflow(options.executable.resolve())
    if options.baseline:
        expected = run_workflow(options.baseline.resolve())
        assert len(expected) == len(actual)
        for before, after in zip(expected, actual):
            assert before == after, f"HTTP compatibility mismatch\nBefore: {before}\nAfter: {after}"
        print(f"Matched {len(actual)} HTTP responses against the pre-refactor executable")
    else:
        print(f"Passed {len(actual)} HTTP checks across all 11 CRUD resources")


if __name__ == "__main__":
    main()
