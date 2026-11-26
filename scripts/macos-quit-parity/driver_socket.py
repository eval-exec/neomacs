"""Input adapter for the persistent, user-granted org.neomacs.GuiDriver app.

This client never launches the app or asks for permissions. Ordinary clients
close their connection to release held input, leaving the persistent app alive.
The explicit --shutdown command is for the user's setup/restart procedure.
"""
import argparse
import json
import os
from pathlib import Path
import socket
import stat
import sys


class DriverBlocked(Exception):
    pass


class PhysicalDriver:
    def __init__(self, path=None):
        path = path or os.environ.get("NEOMACS_GUI_DRIVER_SOCKET")
        if not path:
            raise DriverBlocked("NEOMACS_GUI_DRIVER_SOCKET is not set")
        self.path = str(path)
        try:
            info = Path(path).lstat()
        except FileNotFoundError:
            raise DriverBlocked(f"persistent GUI driver socket absent: {path}") from None
        if not stat.S_ISSOCK(info.st_mode) or info.st_uid != os.getuid():
            raise DriverBlocked(f"GUI driver path is not a socket owned by this user: {path}")
        self.connection = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.connection.settimeout(15)
        try:
            self.connection.connect(self.path)
        except OSError as error:
            self.connection.close()
            raise DriverBlocked(f"GUI driver socket unavailable: {path}: {error}") from error
        self.stream = self.connection.makefile("rb")

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()

    def close(self):
        self.stream.close()
        try:
            self.connection.shutdown(socket.SHUT_RDWR)
        except OSError:
            pass
        self.connection.close()

    def request(self, operation, **fields):
        self.connection.sendall(json.dumps({"op": operation, **fields}).encode() + b"\n")
        line = self.stream.readline(1_048_577)
        if len(line) > 1_048_576 or not line.endswith(b"\n"):
            raise RuntimeError("GUI driver response is truncated or oversized")
        response = json.loads(line)
        if "error" in response:
            raise DriverBlocked(f"GUI driver {operation}: {response['error']}")
        if "result" not in response:
            raise RuntimeError("GUI driver response has no result")
        return response["result"]

    def preflight(self):
        state = self.request("preflight")
        required = ("accessibility", "post_events", "desktop")
        missing = [f"{key}={str(state.get(key)).lower()}" for key in required if state.get(key) is not True]
        if missing:
            raise DriverBlocked("GUI driver preflight: " + ", ".join(missing))
        return state

    def command_q(self, pid, executable):
        state = self.preflight()
        self.request("attach", pid=pid, expected_executable=str(Path(executable).resolve()), input_only=True)
        try:
            for key, down in (("command", True), ("q", True), ("q", False)):
                self.request("input", action={"kind": "key", "key": key, "down": down})
        finally:
            # Disconnect is a second release guarantee if this request fails.
            self.request("input", action={"kind": "key", "key": "command", "down": False})
        return state


def preflight(path=None):
    try:
        with PhysicalDriver(path) as driver:
            return {"status": "ready", "driver": driver.preflight()}
    except DriverBlocked as error:
        return {"status": "BLOCKED", "reason": str(error)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--socket", default=os.environ.get("NEOMACS_GUI_DRIVER_SOCKET"))
    parser.add_argument("--shutdown", action="store_true", help="user-requested clean app restart; never used by tests")
    args = parser.parse_args()
    if args.shutdown:
        try:
            with PhysicalDriver(args.socket) as driver:
                result = {"status": "stopped", "result": driver.request("shutdown")}
        except DriverBlocked as error:
            result = {"status": "BLOCKED", "reason": str(error)}
    else:
        result = preflight(args.socket)
    print(json.dumps(result, sort_keys=True))
    return 42 if result["status"] == "BLOCKED" else 0


if __name__ == "__main__":
    sys.exit(main())
