"""Play an existing USD stage read-only for a bounded ROS receive smoke test.
Run with Isaac Sim python.bat after setup_ros_env.bat; never regenerates/saves USD.
"""
import argparse
import json
from pathlib import Path
import time

parser = argparse.ArgumentParser()
parser.add_argument("--stage", type=Path, required=True)
parser.add_argument("--seconds", type=float, default=180)
args = parser.parse_args()
if not args.stage.is_file():
    raise FileNotFoundError(args.stage)
from isaacsim import SimulationApp
app = SimulationApp({"headless": True, "width": 1280, "height": 720, "renderer": "RayTracedLighting"})
try:
    import omni.kit.app
    import omni.timeline
    import omni.usd
    from pxr import UsdPhysics
    extensions = omni.kit.app.get_app().get_extension_manager()
    extensions.set_extension_enabled_immediate("isaacsim.ros2.bridge", True)
    assert extensions.is_extension_enabled("isaacsim.ros2.bridge")
    assert omni.usd.get_context().open_stage(str(args.stage.resolve()))
    stage = omni.usd.get_context().get_stage()
    graph = []
    for prim in stage.Traverse():
        kind = prim.GetAttribute("node:type").Get()
        if kind and ("ROS2Publish" in str(kind) or "ROS2Subscribe" in str(kind)):
            graph.append({"prim": str(prim.GetPath()), "nodeType": str(kind), "topic": prim.GetAttribute("inputs:topicName").Get()})
    roots = [str(p.GetPath()) for p in stage.Traverse() if p.HasAPI(UsdPhysics.ArticulationRootAPI)]
    print("ISAAC_ROS_SCENE " + json.dumps({"stage":str(args.stage), "roots":roots, "nodes":graph}), flush=True)
    assert roots == ["/World/robot/Geometry"], roots
    timeline = omni.timeline.get_timeline_interface()
    timeline.play()
    start = time.monotonic()
    heartbeat = start
    while app.is_running() and time.monotonic() - start < args.seconds:
        app.update()
        if time.monotonic() - heartbeat > 5:
            print("ISAAC_ROS_PLAYING " + json.dumps({"wallSeconds":round(time.monotonic()-start,1),"simulationTime":timeline.get_current_time()}),flush=True)
            heartbeat = time.monotonic()
    timeline.stop()
    print("ISAAC_ROS_PUBLISHER_FINISHED",flush=True)
finally:
    app.close()
