#!/usr/bin/env python3
"""Exercise the packaged Neomacs editor and its persistent browser filesystem.

Serve an output produced by ``cargo xtask build-wasm`` before running this test.
The test deliberately drives ordinary browser input instead of calling a
test-only Wasm API, so editor and filesystem assertions cross the same Lisp,
worker, Rust host-import, and OPFS boundaries as an interactive browser session.
"""

from __future__ import annotations

import argparse
import json
import os
import shutil
import time
from pathlib import Path

import cbor2
from selenium import webdriver
from selenium.webdriver.chrome.options import Options


STARTUP_WARNING = "Unable to create `user-emacs-directory' (~/.emacs.d/)"


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:4174/")
    parser.add_argument("--chrome", help="path to Chrome or Chromium")
    parser.add_argument(
        "--artifacts-dir",
        help="write a screenshot and browser state here when the smoke test fails",
    )
    parser.add_argument("--headless", action="store_true")
    parser.add_argument(
        "--persistence-only",
        action="store_true",
        help="skip the pre-reload mutation coverage for a tighter persistence loop",
    )
    parser.add_argument("--timeout", type=float, default=60.0)
    return parser.parse_args()


def chrome_binary(explicit: str | None) -> str | None:
    if explicit:
        return explicit
    configured = os.environ.get("NEOMACS_WASM_CHROME")
    if configured:
        return configured
    for candidate in (
        "google-chrome-stable",
        "google-chrome",
        "chromium",
        "chromium-browser",
    ):
        if resolved := shutil.which(candidate):
            return resolved
    return None


def wait_ready(driver: webdriver.Chrome, timeout: float) -> None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        status = driver.find_element("id", "browser-status")
        state = status.get_attribute("data-state") or ""
        if state == "ready":
            return
        if state in {"failed", "stopped"}:
            raise RuntimeError(f"browser state {state}: {status.text}")
        time.sleep(0.25)
    raise RuntimeError("browser did not become ready")


def frame_text(driver: webdriver.Chrome) -> str:
    values = driver.execute_script(
        "return Array.from(new Uint8Array("
        "globalThis.__neomacsLastFrame || new ArrayBuffer()))"
    )
    if not values:
        return ""
    frame = cbor2.loads(bytes(values))
    characters: list[str] = []
    for entry in frame.get("window_matrices", []):
        for row in entry["matrix"]["rows"]:
            for area in row["glyphs"]:
                for glyph in area:
                    kind = glyph["glyph_type"]
                    if isinstance(kind, dict) and "Char" in kind:
                        characters.append(kind["Char"].get("ch", ""))
    return "".join(characters)


def wait_for_presentation(driver: webdriver.Chrome, timeout: float) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        text = frame_text(driver)
        visible_canvas = driver.execute_script(
            "const canvas = document.querySelector('canvas');"
            "if (!canvas) return false;"
            "const rect = canvas.getBoundingClientRect();"
            "return rect.width > 0 && rect.height > 0;"
        )
        if text and visible_canvas:
            return text
        time.sleep(0.2)
    raise RuntimeError("editor did not produce a visible, non-empty presentation")


def accepted_input_count(driver: webdriver.Chrome) -> int:
    return driver.execute_script(
        "return globalThis.__neomacsMessages.filter("
        "message => message.type === 'input-accepted').length"
    )


def wait_for_input_acceptance(
    driver: webdriver.Chrome,
    previous_count: int,
    additional_batches: int,
    timeout: float,
) -> None:
    expected = previous_count + additional_batches
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if accepted_input_count(driver) >= expected:
            return
        time.sleep(0.05)
    raise RuntimeError(
        f"editor did not accept {additional_batches} browser input batches"
    )


def dispatch_key(
    driver: webdriver.Chrome,
    key: str,
    timeout: float,
    *,
    alt: bool = False,
) -> None:
    accepted = accepted_input_count(driver)
    driver.execute_script(
        """
        const options = {
          key: arguments[0],
          altKey: arguments[1],
          bubbles: true,
          cancelable: true,
        };
        dispatchEvent(new KeyboardEvent("keydown", options));
        dispatchEvent(new KeyboardEvent("keyup", options));
        """,
        key,
        alt,
    )
    wait_for_input_acceptance(driver, accepted, 2, timeout)


def commit_text(driver: webdriver.Chrome, text: str, timeout: float) -> None:
    accepted = accepted_input_count(driver)
    driver.execute_script(
        """
        const input = document.querySelector("#browser-text-input");
        input.value = arguments[0];
        input.dispatchEvent(new InputEvent("input", {
          bubbles: true,
          inputType: "insertText",
          data: arguments[0],
        }));
        """,
        text,
    )
    wait_for_input_acceptance(driver, accepted, 1, timeout)


def wait_for_frame_text(
    driver: webdriver.Chrome,
    description: str,
    timeout: float,
    *,
    contains: str,
    excludes: str | None = None,
) -> str:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        text = frame_text(driver)
        if contains in text and (excludes is None or excludes not in text):
            return text
        time.sleep(0.2)
    raise RuntimeError(
        f"editor did not render {description}; "
        f"worker messages={driver.execute_script('return globalThis.__neomacsMessages')!r}; "
        f"frame={frame_text(driver)!r}"
    )


def invoke_mx(driver: webdriver.Chrome, command: str, timeout: float) -> None:
    dispatch_key(driver, "x", timeout, alt=True)
    wait_for_frame_text(driver, "the M-x prompt", timeout, contains="M-x")
    commit_text(driver, command, timeout)
    wait_for_frame_text(driver, command, timeout, contains=command)
    dispatch_key(driver, "Enter", timeout)


def switch_to_buffer(
    driver: webdriver.Chrome,
    buffer_name: str,
    timeout: float,
) -> None:
    invoke_mx(driver, "switch-to-buffer", timeout)
    wait_for_frame_text(
        driver,
        "the switch-to-buffer prompt",
        timeout,
        contains="Switch to buffer",
    )
    commit_text(driver, buffer_name, timeout)
    dispatch_key(driver, "Enter", timeout)
    wait_for_frame_text(driver, f"buffer {buffer_name!r}", timeout, contains=buffer_name)


def exercise_buffer_switching(driver: webdriver.Chrome, timeout: float) -> None:
    suffix = f"{time.time_ns():x}"[-8:]
    buffer_name = f"wasm-{suffix}"
    buffer_text = f"WASM-BUFFER:{suffix}"

    switch_to_buffer(driver, buffer_name, timeout)
    commit_text(driver, buffer_text, timeout)
    wait_for_frame_text(driver, "inserted buffer text", timeout, contains=buffer_text)

    switch_to_buffer(driver, "*scratch*", timeout)
    wait_for_frame_text(
        driver,
        "the scratch buffer without the test buffer contents",
        timeout,
        contains="*scratch*",
        excludes=buffer_text,
    )

    switch_to_buffer(driver, buffer_name, timeout)
    wait_for_frame_text(
        driver,
        "the preserved test buffer contents",
        timeout,
        contains=buffer_text,
    )


def eval_expression(
    driver: webdriver.Chrome,
    expression: str,
    marker: str,
    timeout: float,
    failure_marker: str | None = None,
) -> str:
    dispatch_key(driver, ":", timeout, alt=True)
    expression = " ".join(line.strip() for line in expression.splitlines())
    commit_text(driver, expression, timeout)
    dispatch_key(driver, "Enter", timeout)

    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        text = frame_text(driver)
        if marker in text:
            return text
        if failure_marker and failure_marker in text:
            raise RuntimeError(
                f"editor rendered failure marker {failure_marker!r}; frame={text!r}"
            )
        time.sleep(0.2)
    raise RuntimeError(
        f"editor did not render marker {marker!r}; "
        f"worker messages={driver.execute_script('return globalThis.__neomacsMessages')!r}; "
        f"frame={frame_text(driver)!r}"
    )


def install_frame_observer(driver: webdriver.Chrome) -> None:
    driver.execute_cdp_cmd(
        "Page.addScriptToEvaluateOnNewDocument",
        {
            "source": """
            globalThis.__neomacsLastFrame = null;
            globalThis.__neomacsMessages = [];
            const NativeWorker = globalThis.Worker;
            globalThis.Worker = class extends NativeWorker {
              constructor(...args) {
                super(...args);
                this.addEventListener("message", (event) => {
                  globalThis.__neomacsMessages.push({
                    type: event.data?.type,
                    sequence: event.data?.sequence,
                    message: event.data?.message,
                  });
                  if (event.data?.type === "frame") {
                    globalThis.__neomacsLastFrame = event.data.payload.slice(0);
                  }
                });
              }
            };
            """,
        },
    )


def capture_failure_artifacts(driver: webdriver.Chrome, directory: str) -> None:
    output = Path(directory)
    output.mkdir(parents=True, exist_ok=True)

    errors: list[str] = []
    try:
        driver.save_screenshot(str(output / "browser.png"))
    except Exception as error:  # noqa: BLE001 - diagnostics must not mask the failure
        errors.append(f"screenshot: {error}")
    try:
        (output / "page.html").write_text(driver.page_source, encoding="utf-8")
    except Exception as error:  # noqa: BLE001 - diagnostics must not mask the failure
        errors.append(f"page source: {error}")
    try:
        state = {
            "frame_text": frame_text(driver),
            "worker_messages": driver.execute_script(
                "return globalThis.__neomacsMessages || []"
            ),
        }
        (output / "browser-state.json").write_text(
            json.dumps(state, indent=2, sort_keys=True),
            encoding="utf-8",
        )
    except Exception as error:  # noqa: BLE001 - diagnostics must not mask the failure
        errors.append(f"browser state: {error}")
    if errors:
        (output / "artifact-errors.txt").write_text(
            "\n".join(errors) + "\n",
            encoding="utf-8",
        )


def main() -> None:
    args = parse_args()
    options = Options()
    if binary := chrome_binary(args.chrome):
        options.binary_location = binary
    if args.headless or os.environ.get("NEOMACS_WASM_HEADLESS", "0") != "0":
        options.add_argument("--headless=new")
    options.add_argument("--no-sandbox")
    options.add_argument("--disable-dev-shm-usage")

    token = f"neomacs-opfs-{time.time_ns()}"
    driver = webdriver.Chrome(options=options)
    try:
        install_frame_observer(driver)
        driver.get(args.url)
        wait_ready(driver, args.timeout)
        initial = wait_for_presentation(driver, args.timeout)
        if STARTUP_WARNING in initial:
            raise RuntimeError("startup rendered the user-emacs-directory warning")

        if not args.persistence_only:
            exercise_buffer_switching(driver, args.timeout)
            print("PASS: browser editor switched buffers and preserved text")

            operations_marker = f"OPFS-OPERATIONS:{token}"
            eval_expression(
                driver,
                f'''(let* ((dir "/neomacs-fake/browser-smoke")
                       (source (concat dir "/source"))
                       (copy (concat dir "/copy"))
                       (moved (concat dir "/moved"))
                       (temporary (make-temp-file "/neomacs-fake/browser-temp-" nil ".txt" "temporary")))
                  (make-directory dir t)
                  (with-temp-file source (insert "{token}"))
                  (copy-file source copy t)
                  (rename-file copy moved t)
                  (unless (and (file-directory-p dir)
                               (file-exists-p moved)
                               (string= "{token}"
                                        (with-temp-buffer
                                          (insert-file-contents moved)
                                          (buffer-string)))
                               (string= "temporary"
                                        (with-temp-buffer
                                          (insert-file-contents temporary)
                                          (buffer-string))))
                    (error "browser filesystem operation mismatch"))
                  (delete-file temporary)
                  (delete-directory dir t)
                  (message "{operations_marker}"))''',
                operations_marker,
                args.timeout,
            )

        wrote_marker = f"OPFS-WROTE:{token}"
        eval_expression(
            driver,
            f'''(progn
                  (with-temp-file "/neomacs-fake/persistence-probe"
                    (insert "{token}"))
                  (message "{wrote_marker}"))''',
            wrote_marker,
            args.timeout,
        )

        driver.refresh()
        wait_ready(driver, args.timeout)
        refreshed = wait_for_presentation(driver, args.timeout)
        if STARTUP_WARNING in refreshed:
            raise RuntimeError("reload rendered the user-emacs-directory warning")

        persisted_marker = f"OPFS-PERSISTED:{token}"
        eval_expression(
            driver,
            """(condition-case error-data
                  (message "OPFS-PERSISTED:%s"
                           (with-temp-buffer
                             (insert-file-contents "/neomacs-fake/persistence-probe")
                             (buffer-string)))
                (error (message "OPFS-READ-ERROR:%S" error-data)))""",
            persisted_marker,
            args.timeout,
            failure_marker="OPFS-READ-ERROR:",
        )
        eval_expression(
            driver,
            f"""(progn
                  (delete-file "/neomacs-fake/persistence-probe")
                  (message "OPFS-CLEANED:{token}"))""",
            f"OPFS-CLEANED:{token}",
            args.timeout,
        )
        print(f"PASS: browser filesystem persisted {token} across reload")
    except Exception:
        if args.artifacts_dir:
            capture_failure_artifacts(driver, args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
