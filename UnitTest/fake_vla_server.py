"""Contract fixture for VLA only. Does not emulate ROS or model inference."""
import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json


class Handler(BaseHTTPRequestHandler):
    def do_POST(self):
        if self.path != "/infer":
            self.send_error(404)
            return
        try:
            size = int(self.headers.get("Content-Length", "0"))
            if not 0 < size <= 65536:
                raise ValueError("Invalid request length")
            request = json.loads(self.rfile.read(size))
            assert set(request) == {"profileId", "task", "observation"}
            assert isinstance(request["profileId"], str) and request["profileId"]
            assert isinstance(request["task"], str) and request["task"].strip()
            observation = request["observation"]
            assert set(observation) == {"name", "position"}
            assert observation["name"] and len(observation["name"]) == len(observation["position"])
            body = json.dumps({"actions": [observation for _ in range(self.server.steps)], "done": True}).encode()
        except (ValueError, KeyError, TypeError, AssertionError):
            self.send_error(400)
            return
        self.send_response(200)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def log_message(self, *_):
        pass  # Never log task payloads.


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=8091)
    parser.add_argument("--steps", type=int, default=12)
    args = parser.parse_args()
    if not 0 < args.steps <= 1000:
        parser.error("steps must be between 1 and 1000")
    server = ThreadingHTTPServer(("127.0.0.1", args.port), Handler)
    server.steps = args.steps
    print(f"VLA contract fixture listening on 127.0.0.1:{args.port}", flush=True)
    server.serve_forever()


if __name__ == "__main__":
    main()
