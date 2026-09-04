#!/usr/bin/env python3
"""Prove that an ordinary reload selects one coherent browser release.

The test serves the stable shell from a real ``cargo xtask build-wasm``
distribution with deliberately long-lived cache headers.  A manifest update
then switches from a synthetic release A to release B without changing the
origin, URL, Chrome session, or browser cache.
"""

from __future__ import annotations

import argparse
import json
import threading
import time
from collections import Counter
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlsplit

from selenium import webdriver

from browser_test_support import chrome_options


RELEASE_ASSET_FILENAMES = ("main.js", "editor-worker.js", "editor.wasm")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--distribution",
        type=Path,
        required=True,
        help="browser distribution produced by cargo xtask build-wasm",
    )
    parser.add_argument("--chrome", help="path to Chrome or Chromium")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir", type=Path)
    parser.add_argument("--timeout", type=float, default=30.0)
    return parser.parse_args()


class ReleaseServer(ThreadingHTTPServer):
    def __init__(self, shell: bytes) -> None:
        super().__init__(("127.0.0.1", 0), ReleaseRequestHandler)
        self.shell = shell
        self.current_release = "a"
        self.requests: Counter[str] = Counter()


class ReleaseRequestHandler(BaseHTTPRequestHandler):
    server: ReleaseServer

    def do_GET(self) -> None:  # noqa: N802 - BaseHTTPRequestHandler API
        path = urlsplit(self.path).path
        self.server.requests[path] += 1

        if path in {"/", "/index.html"}:
            self.send_payload(
                self.server.shell,
                "text/html; charset=utf-8",
                "public, max-age=31536000, immutable",
            )
            return
        if path == "/manifest.json":
            release = self.server.current_release
            manifest = {
                "schema": 1,
                "bundle_id": release,
                "entry": f"./builds/{release}/main.js",
            }
            self.send_payload(
                (json.dumps(manifest) + "\n").encode(),
                "application/json",
                "public, max-age=31536000, immutable",
            )
            return
        release_asset = release_asset_path(path)
        if release_asset is not None:
            release, filename = release_asset
        else:
            self.send_error(404)
            return

        if filename == "main.js":
            script = (
                "const worker = new Worker(\n"
                "  new URL('./editor-worker.js', import.meta.url),\n"
                "  { type: 'module' },\n"
                ");\n"
                "worker.addEventListener('message', ({ data }) => {\n"
                "  if (data.error) {\n"
                "    document.documentElement.dataset.neomacsFailure = data.error;\n"
                "  } else {\n"
                "    document.documentElement.dataset.neomacsRelease = data.release;\n"
                "  }\n"
                "  worker.terminate();\n"
                "});\n"
            )
            self.send_payload(
                script.encode(),
                "text/javascript; charset=utf-8",
                "public, max-age=31536000, immutable",
            )
            return
        if filename == "editor-worker.js":
            script = f"""
            const release = {json.dumps(release)};
            try {{
              const response = await fetch(new URL("./editor.wasm", import.meta.url));
              const importName = `fs_create_directory_${{release}}`;
              const imports = {{ neomacs_host: {{ [importName]: () => {{}} }} }};
              const {{ instance }} = await WebAssembly.instantiateStreaming(response, imports);
              instance.exports.run();
              self.postMessage({{ release }});
            }} catch (error) {{
              self.postMessage({{ error: String(error) }});
            }}
            """
            self.send_payload(
                script.encode(),
                "text/javascript; charset=utf-8",
                "public, max-age=31536000, immutable",
            )
            return
        if filename == "editor.wasm":
            self.send_payload(
                incompatible_worker_wasm(release),
                "application/wasm",
                "public, max-age=31536000, immutable",
            )
            return

        self.send_error(404)

    def log_message(self, format: str, *args: object) -> None:
        return

    def send_payload(self, body: bytes, content_type: str, cache_control: str) -> None:
        self.send_response(200)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", cache_control)
        self.end_headers()
        self.wfile.write(body)


def release_asset_path(path: str) -> tuple[str, str] | None:
    parts = path.strip("/").split("/")
    if len(parts) != 3 or parts[0] != "builds" or parts[1] not in {"a", "b"}:
        return None
    if parts[2] not in RELEASE_ASSET_FILENAMES:
        return None
    return parts[1], parts[2]


def incompatible_worker_wasm(release: str) -> bytes:
    """Return a module whose host import is intentionally release-specific."""

    def name(value: str) -> bytes:
        encoded = value.encode()
        if len(encoded) >= 128:
            raise ValueError("test Wasm names must fit in one unsigned LEB128 byte")
        return bytes([len(encoded)]) + encoded

    def section(section_id: int, payload: bytes) -> bytes:
        if len(payload) >= 128:
            raise ValueError("test Wasm sections must fit in one unsigned LEB128 byte")
        return bytes([section_id, len(payload)]) + payload

    function_type = section(1, b"\x01\x60\x00\x00")
    imported_function = section(
        2,
        b"\x01"
        + name("neomacs_host")
        + name(f"fs_create_directory_{release}")
        + b"\x00\x00",
    )
    declared_function = section(3, b"\x01\x00")
    exported_function = section(7, b"\x01" + name("run") + b"\x00\x01")
    function_body = section(10, b"\x01\x04\x00\x10\x00\x0b")
    return (
        b"\x00asm\x01\x00\x00\x00"
        + function_type
        + imported_function
        + declared_function
        + exported_function
        + function_body
    )


def wait_for_release(driver: webdriver.Chrome, release: str, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        selected = driver.execute_script(
            "return document.documentElement.dataset.neomacsRelease || ''"
        )
        if selected == release:
            return
        failure = driver.execute_script(
            "return document.documentElement.dataset.neomacsFailure || ''"
        )
        if failure:
            raise RuntimeError(f"release {release!r} failed: {failure}")
        time.sleep(0.05)
    raise RuntimeError(f"browser did not select release {release!r}")


def validate_distribution(distribution: Path) -> bytes:
    shell = distribution.joinpath("index.html").read_bytes()
    manifest = json.loads(distribution.joinpath("manifest.json").read_text())
    if manifest.get("schema") != 1:
        raise RuntimeError("browser distribution does not use manifest schema 1")
    entry = manifest.get("entry")
    if not isinstance(entry, str) or not entry.startswith("./builds/"):
        raise RuntimeError("browser distribution entry is not content-addressed")
    if not distribution.joinpath(entry.removeprefix("./")).is_file():
        raise RuntimeError(f"browser distribution entry is missing: {entry}")
    return shell


def capture_failure(driver: webdriver.Chrome, directory: Path) -> None:
    errors: list[str] = []
    try:
        directory.mkdir(parents=True, exist_ok=True)
    except Exception:  # noqa: BLE001 - preserve the original failure
        return
    try:
        driver.save_screenshot(str(directory / "browser-release-upgrade.png"))
    except Exception as error:  # noqa: BLE001 - preserve the original failure
        errors.append(f"screenshot: {error}")
    try:
        directory.joinpath("browser-release-upgrade.html").write_text(
            driver.page_source,
            encoding="utf-8",
        )
    except Exception as error:  # noqa: BLE001 - preserve the original failure
        errors.append(f"page source: {error}")
    if errors:
        try:
            directory.joinpath("browser-release-upgrade-artifact-errors.txt").write_text(
                "\n".join(errors) + "\n",
                encoding="utf-8",
            )
        except Exception:  # noqa: BLE001 - preserve the original failure
            pass


def main() -> None:
    args = parse_args()
    shell = validate_distribution(args.distribution)
    server = ReleaseServer(shell)
    server_thread = threading.Thread(target=server.serve_forever, daemon=True)
    server_thread.start()

    options = chrome_options(args.chrome, args.headless)

    driver = webdriver.Chrome(options=options)
    try:
        url = f"http://127.0.0.1:{server.server_port}/"
        driver.get(url)
        wait_for_release(driver, "a", args.timeout)

        server.current_release = "b"
        driver.refresh()
        wait_for_release(driver, "b", args.timeout)

        if server.requests["/manifest.json"] < 2:
            raise RuntimeError("ordinary reload reused the cached release manifest")
        for release in ("a", "b"):
            for filename in RELEASE_ASSET_FILENAMES:
                entry = f"/builds/{release}/{filename}"
                if server.requests[entry] != 1:
                    raise RuntimeError(
                        f"expected one request for {entry}, got {server.requests[entry]}"
                    )
        print("PASS: ordinary reload upgraded one coherent browser release")
    except Exception:
        if args.artifacts_dir:
            capture_failure(driver, args.artifacts_dir)
        raise
    finally:
        driver.quit()
        server.shutdown()
        server.server_close()
        server_thread.join()


if __name__ == "__main__":
    main()
