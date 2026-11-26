#!/usr/bin/env python3
"""Own a real GNOME GSettings -> XSETTINGS/Xresources bridge, never a display.

Run --report-display FILE once from Sway's `exec` to discover its DISPLAY.
The parent then directly spawns --serve with that environment and retains the
Child. ready.json contains the private environment for both editors. Atomically
replace control.json with {"sequence": 1, "antialias": "grayscale",
"hinting": "full", "rgba": "rgb"}; reply-1.json is published only after both
native properties agree. {"sequence": 2, "stop": true} stops and reaps children.
No font pixels, GNU behavior, or timing claims are established by this helper.
"""

import argparse
import ast
import ctypes as c
import ctypes.util
import json
import os
from pathlib import Path
import selectors
import shutil
import signal
import struct
import subprocess
import time
import uuid
from xml.sax.saxutils import escape


def publish(path, value):
    temporary = path.with_name(path.name + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def decode_xsettings(data):
    if len(data) < 12 or data[0] not in (0, 1):
        raise ValueError("invalid XSETTINGS header")
    endian = "<" if data[0] == 0 else ">"
    serial, count = struct.unpack_from(endian + "II", data, 4)
    offset = 12
    values = {}
    for _ in range(count):
        kind, _, length = struct.unpack_from(endian + "BBH", data, offset)
        offset += 4
        end = offset + length
        if end > len(data):
            raise ValueError("truncated XSETTINGS name")
        name = data[offset:end].decode("utf-8")
        offset = (end + 3) & ~3
        changed, = struct.unpack_from(endian + "I", data, offset)
        offset += 4
        if kind == 0:
            value, = struct.unpack_from(endian + "i", data, offset)
            offset += 4
        elif kind == 1:
            length, = struct.unpack_from(endian + "I", data, offset)
            offset += 4
            end = offset + length
            if end > len(data):
                raise ValueError("truncated XSETTINGS string")
            value = data[offset:end].decode("utf-8")
            offset = (end + 3) & ~3
        elif kind == 2:
            value = struct.unpack_from(endian + "HHHH", data, offset)
            offset += 8
        else:
            raise ValueError(f"unknown XSETTINGS type {kind}")
        values[name] = {"type": kind, "serial": changed, "value": value}
    return serial, values


def expected_settings(policy):
    # GNOME publishes the order only for subpixel AA; a gray/mono order edit
    # may legitimately leave all Xft values and the XSETTINGS serial unchanged.
    return {"Xft/Antialias": int(policy["antialias"] != "none"),
        "Xft/Hinting": int(policy["hinting"] != "none"),
        "Xft/HintStyle": "hint" + policy["hinting"],
        "Xft/RGBA": policy["rgba"] if policy["antialias"] == "rgba" else "none"}


class XSettingsReader:
    def __init__(self, display):
        self.lib = c.CDLL(ctypes.util.find_library("X11") or "libX11.so.6")
        signatures = {
            "XOpenDisplay": ([c.c_char_p], c.c_void_p),
            "XDefaultRootWindow": ([c.c_void_p], c.c_ulong),
            "XInternAtom": ([c.c_void_p, c.c_char_p, c.c_int], c.c_ulong),
            "XGetSelectionOwner": ([c.c_void_p, c.c_ulong], c.c_ulong),
            "XGetWindowProperty": ([c.c_void_p, c.c_ulong, c.c_ulong, c.c_long,
                c.c_long, c.c_int, c.c_ulong, c.POINTER(c.c_ulong),
                c.POINTER(c.c_int), c.POINTER(c.c_ulong), c.POINTER(c.c_ulong),
                c.POINTER(c.c_void_p)], c.c_int),
            "XQueryExtension": ([c.c_void_p, c.c_char_p, c.POINTER(c.c_int),
                c.POINTER(c.c_int), c.POINTER(c.c_int)], c.c_int),
            "XFree": ([c.c_void_p], c.c_int),
            "XCloseDisplay": ([c.c_void_p], c.c_int),
        }
        for name, (arguments, result) in signatures.items():
            function = getattr(self.lib, name)
            function.argtypes, function.restype = arguments, result
        self.display = self.lib.XOpenDisplay(display.encode())
        if not self.display:
            raise RuntimeError(f"cannot connect to supplied XWayland DISPLAY {display!r}")
        opcode, event, error = c.c_int(), c.c_int(), c.c_int()
        if not self.lib.XQueryExtension(self.display, b"XWAYLAND",
                c.byref(opcode), c.byref(event), c.byref(error)):
            self.close()
            raise RuntimeError("supplied display does not advertise XWAYLAND")
        self.root = self.lib.XDefaultRootWindow(self.display)
        self.selection = self.atom("_XSETTINGS_S0")

    def atom(self, name):
        return self.lib.XInternAtom(self.display, name.encode(), 0)

    def property(self, window, name):
        actual, format_, count, remaining = c.c_ulong(), c.c_int(), c.c_ulong(), c.c_ulong()
        pointer = c.c_void_p()
        result = self.lib.XGetWindowProperty(self.display, window, self.atom(name),
            0, 262144, 0, 0, c.byref(actual), c.byref(format_), c.byref(count),
            c.byref(remaining), c.byref(pointer))
        try:
            if result != 0 or remaining.value or format_.value not in (0, 8):
                raise RuntimeError(f"invalid or oversized {name} property")
            return c.string_at(pointer, count.value) if pointer.value else b""
        finally:
            if pointer.value:
                self.lib.XFree(pointer)

    def snapshot(self):
        owner = self.lib.XGetSelectionOwner(self.display, self.selection)
        serial, values = decode_xsettings(self.property(owner, "_XSETTINGS_SETTINGS")) if owner else (None, {})
        resources = {}
        for line in self.property(self.root, "RESOURCE_MANAGER").decode("utf-8").splitlines():
            key, separator, value = line.partition(":")
            if separator and key.startswith("Xft."):
                resources[key] = value.strip()
        return {"owner": owner, "serial": serial, "settings": values, "resources": resources,
                "xwayland_extension": True}

    def close(self):
        if self.display:
            self.lib.XCloseDisplay(self.display)
            self.display = None


class DesktopBridge:
    def __init__(self, arguments):
        self.root = arguments.artifacts.resolve()
        self.root.mkdir(parents=True, exist_ok=False)
        self.children = []
        self.logs = []
        self.reader = None
        self.executable = arguments.gsd or shutil.which("gsd-xsettings")
        if not self.executable:
            raise RuntimeError("supply --gsd with the real gsd-xsettings executable")
        self.env = dict(os.environ)
        for variable in ("WAYLAND_SOCKET", "GSETTINGS_SCHEMA_DIR", "DBUS_SESSION_BUS_ADDRESS",
                         "DBUS_STARTER_ADDRESS", "DBUS_STARTER_BUS_TYPE", "GNOME_SETUP_DISPLAY"):
            self.env.pop(variable, None)
        for variable, directory in (("XDG_CONFIG_HOME", "settings"), ("XDG_CONFIG_DIRS", "empty-config"),
                                    ("XDG_CACHE_HOME", "cache"), ("XDG_DATA_HOME", "data")):
            location = self.root / directory
            location.mkdir()
            self.env[variable] = str(location)
        # Use complete enum-bearing schemas; gsd calls g_settings_get_enum and
        # also reads desktop/plugin keys outside the three raster preferences.
        schemas = [Path(directory).resolve() for directory in arguments.schema_dir]
        if not schemas or any(not (directory / "gschemas.compiled").is_file() for directory in schemas):
            raise RuntimeError("--schema-dir requires complete compiled desktop and gsd schemas")
        self.env.update(GSETTINGS_BACKEND="keyfile", GSETTINGS_SCHEMA_DIR=str(schemas[0]),
            XDG_DATA_DIRS=":".join(str(directory.parent.parent) for directory in schemas),
            NO_AT_BRIDGE="1")
        self.schema_dirs = [str(directory) for directory in schemas]

    def spawn(self, command, name, stdout=None):
        log = (self.root / (name + ".log")).open("wb")
        self.logs.append(log)
        child = subprocess.Popen(command, env=self.env, stdin=subprocess.DEVNULL,
            stdout=stdout if stdout is not None else log, stderr=log)
        self.children.append((name, child))
        return child

    def set_policy(self, antialias, hinting, rgba):
        if antialias not in ("none", "grayscale", "rgba") or hinting not in ("none", "slight", "medium", "full") or rgba not in ("rgb", "bgr", "vrgb", "vbgr"):
            raise ValueError("invalid GNOME raster preference")
        for key, value in (("font-antialiasing", antialias), ("font-hinting", hinting), ("font-rgba-order", rgba)):
            self.gsettings("set", key, value)
        return {"antialias": antialias, "hinting": hinting, "rgba": rgba}

    def gsettings(self, operation, key, value=None):
        command = ["gsettings", operation, "org.gnome.desktop.interface", key]
        if value is not None:
            command.append(value)
        return subprocess.run(command, env=self.env, check=True, capture_output=True,
            text=True, timeout=5).stdout.strip()

    def read_settings(self):
        printed = {key: self.gsettings("get", key) for key in (
            "font-antialiasing", "font-hinting", "font-rgba-order", "text-scaling-factor")}
        # These known schema keys print string/double GVariants, whose literal
        # syntax is also accepted by literal_eval. Never evaluate command text.
        values = {key: ast.literal_eval(value) for key, value in printed.items()}
        return values, printed

    def start(self, initial):
        self.reader = XSettingsReader(self.env["DISPLAY"])
        if self.reader.snapshot()["owner"]:
            raise RuntimeError("private display already has an XSETTINGS manager; refusing to replace it")
        # No service directories: this private bus cannot activate host session
        # helpers while gsd checks for GNOME SessionManager/Mutter/Shell names.
        config = self.root / "dbus.conf"
        address = "unix:abstract=neomacs-font-bridge-" + uuid.uuid4().hex
        config.write_text("<busconfig><type>session</type><listen>" + escape(address) +
            '</listen><policy context="default"><allow send_destination="*"/>' +
            '<allow receive_sender="*"/><allow own="*"/></policy></busconfig>')
        bus = self.spawn(["dbus-daemon", "--nofork", "--print-address=1", "--config-file=" + str(config)], "dbus", subprocess.PIPE)
        with selectors.DefaultSelector() as selector:
            selector.register(bus.stdout, selectors.EVENT_READ)
            if not selector.select(5):
                raise RuntimeError("private D-Bus address was not published")
        self.env["DBUS_SESSION_BUS_ADDRESS"] = bus.stdout.readline().decode().strip()
        if not self.env["DBUS_SESSION_BUS_ADDRESS"]:
            raise RuntimeError("private D-Bus exited before publishing its address")
        # Pin the private logical font DPI instead of trusting desktop schema
        # defaults or the host's text scale. Mutter is absent on this private
        # bus, so gsd's window-scale fallback is one; verify its wire output.
        self.gsettings("set", "text-scaling-factor", "1.0")
        self.set_policy(**initial)
        self.gsd = self.spawn([self.executable, "--verbose"], "gsd-xsettings")
        snapshot = self.wait_policy(initial)
        publish(self.root / "ready.json", {"pid": os.getpid(), "gsd_pid": self.gsd.pid,
            "dbus_pid": bus.pid, "gsd_executable": self.executable,
            "schema_dirs": self.schema_dirs, "environment": self.export_environment(),
            "policy": initial, "snapshot": snapshot})
        return snapshot

    def export_environment(self):
        keys = ("DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "SWAYSOCK", "XAUTHORITY",
            "XDG_CONFIG_HOME", "XDG_CONFIG_DIRS", "XDG_CACHE_HOME", "XDG_DATA_HOME", "XDG_DATA_DIRS",
            "GSETTINGS_BACKEND", "WAYLAND_SOCKET", "GSETTINGS_SCHEMA_DIR", "DBUS_SESSION_BUS_ADDRESS", "NO_AT_BRIDGE")
        return {key: self.env[key] for key in keys if key in self.env}

    def wait_policy(self, policy, after=None, require_advance=False):
        expected = expected_settings(policy)
        expected_wire = expected | {"Xft/DPI": 98304, "Gdk/UnscaledDPI": 98304,
                                    "Gdk/WindowScalingFactor": 1}
        # GNOME's XSETTINGS names are case-sensitive MixedCase; its X resource
        # suffixes are lowercase (gsd-xsettings-manager.c:682-694,727-750).
        expected_resources = {"Xft." + key.split("/")[1].lower(): str(value)
                              for key, value in expected.items()} | {"Xft.dpi": "96"}
        expected_raw = {"font-antialiasing": policy["antialias"],
            "font-hinting": policy["hinting"], "font-rgba-order": policy["rgba"],
            "text-scaling-factor": 1.0}
        last = None
        deadline = time.monotonic() + 10
        while time.monotonic() < deadline:
            if any(child.poll() is not None for _, child in self.children):
                raise RuntimeError("owned bridge child exited; see dbus/gsd logs")
            last = self.reader.snapshot()
            correct = all(last["settings"].get(key, {}).get("value") == value
                          for key, value in expected_wire.items()) and all(
                last["resources"].get(key) == value for key, value in expected_resources.items())
            same_owner = after is None or last["owner"] == after["owner"]
            advanced = not require_advance or (after is not None and last["serial"] != after["serial"])
            if correct and same_owner and advanced:
                raw, printed = self.read_settings()
                last.update(raw_settings=raw, raw_settings_printed=printed)
                if raw == expected_raw and type(raw["text-scaling-factor"]) is float:
                    return last
            time.sleep(0.03)  # Readiness bound, never a performance measurement.
        raise RuntimeError(f"live XSETTINGS/resources did not reach {policy}: {last}")

    def serve(self, initial):
        snapshot = self.start(initial)
        current = initial
        sequence = None
        while True:
            if any(child.poll() is not None for _, child in self.children):
                raise RuntimeError("owned bridge child exited")
            control = self.root / "control.json"
            if not control.exists():
                time.sleep(0.03)
                continue
            request = json.loads(control.read_text())
            if request["sequence"] == sequence:
                time.sleep(0.03)
                continue
            sequence = request["sequence"]
            if not isinstance(sequence, int) or sequence < 0:
                raise ValueError("control sequence must be a nonnegative integer")
            if request.get("stop"):
                return
            next_policy = self.set_policy(request["antialias"], request["hinting"], request["rgba"])
            effective_change = expected_settings(next_policy) != expected_settings(current)
            next_snapshot = self.wait_policy(next_policy, snapshot, require_advance=effective_change)
            publish(self.root / f"reply-{sequence}.json", {"sequence": sequence,
                "policy": next_policy, "previous_serial": snapshot["serial"], "snapshot": next_snapshot,
                "effective_change": effective_change,
                "gsd_pid": self.gsd.pid, "environment": self.export_environment()})
            current, snapshot = next_policy, next_snapshot

    def close(self):
        cleanup = []
        for name, child in reversed(self.children):
            if child.poll() is None:
                child.terminate()
                try:
                    child.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    child.kill()
                    child.wait(timeout=5)
            cleanup.append({"name": name, "pid": child.pid, "returncode": child.returncode})
            if child.stdout:
                child.stdout.close()
        if self.reader:
            self.reader.close()
        for log in self.logs:
            log.close()
        publish(self.root / "cleanup.json", cleanup)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--report-display", type=Path)
    parser.add_argument("--artifacts", type=Path)
    parser.add_argument("--gsd")
    parser.add_argument("--schema-dir", action="append", default=[])
    parser.add_argument("--serve", action="store_true")
    parser.add_argument("--antialias", default="grayscale")
    parser.add_argument("--hinting", default="slight")
    parser.add_argument("--rgba", default="rgb")
    arguments = parser.parse_args()
    if arguments.report_display:
        environment = {key: os.environ[key] for key in
            ("DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR", "SWAYSOCK", "XAUTHORITY") if key in os.environ}
        if not environment.get("DISPLAY"):
            raise RuntimeError("Sway reporter did not inherit DISPLAY")
        publish(arguments.report_display, environment)
        return
    if not arguments.serve or arguments.artifacts is None:
        parser.error("use --report-display FILE or --serve --artifacts NEW_DIRECTORY")
    bridge = DesktopBridge(arguments)
    def interrupted(_signal, _frame):
        raise KeyboardInterrupt
    signal.signal(signal.SIGTERM, interrupted)
    try:
        bridge.serve({"antialias": arguments.antialias, "hinting": arguments.hinting, "rgba": arguments.rgba})
    except BaseException as error:
        publish(bridge.root / "error.json", {"type": type(error).__name__, "error": str(error)})
        raise
    finally:
        bridge.close()


if __name__ == "__main__":
    main()
