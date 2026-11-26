"""Socket protocol rows; these do not claim macOS input or permission grants."""
import json
from pathlib import Path
import socket
import tempfile
import threading
import unittest

from driver_socket import DriverBlocked, PhysicalDriver, preflight


class Server:
    def __init__(self, *, trusted=True, fail_q=False, q_error="injected q input rejection"):
        self.directory = tempfile.TemporaryDirectory(prefix="neo-driver-", dir="/tmp")
        self.path = Path(self.directory.name) / "s"
        self.listener = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
        self.listener.bind(str(self.path))
        self.listener.listen(1)
        self.listener.settimeout(3)
        self.requests = []
        self.error = None
        self.trusted, self.fail_q, self.q_error = trusted, fail_q, q_error
        self.thread = threading.Thread(target=self.serve)

    def serve(self):
        try:
            connection, _ = self.listener.accept()
            with connection, connection.makefile("rwb") as stream:
                for line in stream:
                    message = json.loads(line)
                    self.requests.append(message)
                    if message["op"] == "preflight":
                        response = {"result": {"accessibility": self.trusted, "post_events": self.trusted,
                                               "desktop": True, "screen_capture": False}}
                    elif self.fail_q and message.get("action") == {"kind": "key", "key": "q", "down": True}:
                        response = {"error": self.q_error}
                    else:
                        response = {"result": True}
                    stream.write(json.dumps(response).encode() + b"\n")
                    stream.flush()
        except BaseException as error:
            self.error = error

    def __enter__(self):
        self.thread.start()
        return self

    def __exit__(self, *_):
        self.thread.join(4)
        self.listener.close()
        self.directory.cleanup()
        if self.thread.is_alive():
            raise AssertionError("fake driver did not observe client disconnect")
        if self.error:
            raise self.error


class DriverSocketContract(unittest.TestCase):
    def test_missing_socket_reports_exact_blocker(self):
        with tempfile.TemporaryDirectory(prefix="neo-driver-", dir="/tmp") as directory:
            path = Path(directory) / "absent"
            self.assertEqual(preflight(path), {"status": "BLOCKED",
                "reason": f"persistent GUI driver socket absent: {path}"})

    def test_ungranted_driver_sends_no_attach_or_input(self):
        with Server(trusted=False) as server:
            self.assertEqual(preflight(server.path), {"status": "BLOCKED",
                "reason": "GUI driver preflight: accessibility=false, post_events=false"})
        self.assertEqual(server.requests, [{"op": "preflight"}])

    def test_command_q_uses_persistent_driver_protocol_with_owned_executable(self):
        with Server() as server:
            executable = Path(server.directory.name).resolve() / "neomacs"
            with PhysicalDriver(server.path) as driver:
                driver.command_q(12345, executable)
        self.assertEqual(server.requests, [
            {"op": "preflight"},
            {"op": "attach", "pid": 12345, "expected_executable": str(executable), "input_only": True},
            {"op": "input", "action": {"kind": "key", "key": "command", "down": True}},
            {"op": "input", "action": {"kind": "key", "key": "q", "down": True}},
            {"op": "input", "action": {"kind": "key", "key": "q", "down": False}},
            {"op": "input", "action": {"kind": "key", "key": "command", "down": False}},
        ])

    def test_input_failure_releases_command_and_disconnects(self):
        with Server(fail_q=True) as server:
            with PhysicalDriver(server.path) as driver:
                with self.assertRaisesRegex(DriverBlocked, "GUI driver input: injected q input rejection"):
                    driver.command_q(12345, Path("/tmp/owned-neomacs"))
        self.assertEqual(server.requests[-1],
            {"op": "input", "action": {"kind": "key", "key": "command", "down": False}})

    def test_focus_loss_is_blocked_without_reporting_a_successful_chord(self):
        with Server(fail_q=True, q_error="target not frontmost") as server:
            with PhysicalDriver(server.path) as driver:
                with self.assertRaisesRegex(DriverBlocked, "GUI driver input: target not frontmost"):
                    driver.command_q(12345, Path("/tmp/owned-neomacs"))
        self.assertEqual([r["action"] for r in server.requests if r["op"] == "input"], [
            {"kind": "key", "key": "command", "down": True},
            {"kind": "key", "key": "q", "down": True},
            {"kind": "key", "key": "command", "down": False},
        ])


if __name__ == "__main__":
    unittest.main()
