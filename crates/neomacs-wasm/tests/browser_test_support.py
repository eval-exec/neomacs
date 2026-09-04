"""Shared Chrome configuration for Neomacs browser integration tests."""

from __future__ import annotations

import json
import os
import shutil
import time
from pathlib import Path

import cbor2
from selenium import webdriver
from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.keys import Keys
from selenium.webdriver.chrome.options import Options


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


def chrome_options(explicit_binary: str | None, headless: bool) -> Options:
    options = Options()
    if binary := chrome_binary(explicit_binary):
        options.binary_location = binary
    if headless or os.environ.get("NEOMACS_WASM_HEADLESS", "0") != "0":
        options.add_argument("--headless=new")
    options.add_argument("--no-sandbox")
    options.add_argument("--disable-dev-shm-usage")
    return options


class BrowserEditorHarness:
    """Drive and observe one Neomacs browser editor session."""

    def __init__(self, driver: webdriver.Chrome, timeout: float) -> None:
        self.driver = driver
        self.timeout = timeout
        self.completed_startup_frame_texts: list[str] = []

    def install_frame_observer(self) -> None:
        self.driver.execute_cdp_cmd(
            "Page.addScriptToEvaluateOnNewDocument",
            {
                "source": """
                globalThis.__neomacsLastFrame = null;
                globalThis.__neomacsStartupFrames = [];
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
                        const frame = event.data.payload.slice(0);
                        globalThis.__neomacsLastFrame = frame;
                        if (globalThis.__neomacsStartupFrames !== null) {
                          globalThis.__neomacsStartupFrames.push(frame);
                        }
                      }
                    });
                  }
                };
                """,
            },
        )

    def wait_ready(self) -> None:
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            status = self.driver.find_element("id", "browser-status")
            state = status.get_attribute("data-state") or ""
            if state == "ready":
                return
            if state in {"failed", "stopped"}:
                raise RuntimeError(f"browser state {state}: {status.text}")
            time.sleep(0.25)
        raise RuntimeError("browser did not become ready")

    @staticmethod
    def decode_frame_text(values: list[int]) -> str:
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

    def frame_text(self) -> str:
        values = self.driver.execute_script(
            "return Array.from(new Uint8Array("
            "globalThis.__neomacsLastFrame || new ArrayBuffer()))"
        )
        return self.decode_frame_text(values)

    def frame_payload(self) -> dict[str, object]:
        values = self.driver.execute_script(
            "return Array.from(new Uint8Array("
            "globalThis.__neomacsLastFrame || new ArrayBuffer()))"
        )
        return cbor2.loads(bytes(values)) if values else {}

    @staticmethod
    def matrix_text(entry: dict[str, object]) -> str:
        characters: list[str] = []
        for row in entry["matrix"]["rows"]:
            for area in row["glyphs"]:
                for glyph in area:
                    kind = glyph["glyph_type"]
                    if isinstance(kind, dict) and "Char" in kind:
                        characters.append(kind["Char"].get("ch", ""))
        return "".join(characters)

    def wait_for_window_matrices(
        self,
        description: str,
        *,
        contains: str,
        count: int,
    ) -> list[dict[str, object]]:
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            matching = [
                entry
                for entry in self.frame_payload().get("window_matrices", [])
                if contains in self.matrix_text(entry)
            ]
            if len(matching) == count:
                return matching
            time.sleep(0.1)
        raise RuntimeError(
            f"editor did not render {description}; frame={self.frame_text()!r}"
        )

    def finish_startup_frame_capture(self) -> list[str]:
        frames = self.driver.execute_script(
            "const frames = globalThis.__neomacsStartupFrames || [];"
            "globalThis.__neomacsStartupFrames = null;"
            "return frames.map(frame => Array.from(new Uint8Array(frame)))"
        )
        self.completed_startup_frame_texts = [
            self.decode_frame_text(values) for values in frames
        ]
        return self.completed_startup_frame_texts

    def observed_startup_frame_texts(self) -> list[str]:
        active = self.driver.execute_script(
            "return (globalThis.__neomacsStartupFrames || []).map("
            "frame => Array.from(new Uint8Array(frame)))"
        )
        return self.completed_startup_frame_texts + [
            self.decode_frame_text(values) for values in active
        ]

    def wait_for_presentation(self) -> str:
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            text = self.frame_text()
            visible_canvas = self.driver.execute_script(
                "const canvas = document.querySelector('canvas');"
                "if (!canvas) return false;"
                "const rect = canvas.getBoundingClientRect();"
                "return rect.width > 0 && rect.height > 0;"
            )
            if text and visible_canvas:
                return text
            time.sleep(0.2)
        raise RuntimeError("editor did not produce a visible, non-empty presentation")

    def accepted_input_count(self) -> int:
        return self.driver.execute_script(
            "return globalThis.__neomacsMessages.filter("
            "message => message.type === 'input-accepted').length"
        )

    def wait_for_input_acceptance(
        self,
        previous_count: int,
        additional_batches: int,
    ) -> None:
        expected = previous_count + additional_batches
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            if self.accepted_input_count() >= expected:
                return
            time.sleep(0.05)
        raise RuntimeError(
            f"editor did not accept {additional_batches} browser input batches"
        )

    def dispatch_key(self, key: str, *, alt: bool = False) -> None:
        accepted = self.accepted_input_count()
        self.driver.execute_script(
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
        self.wait_for_input_acceptance(accepted, 2)

    def commit_text(self, text: str) -> None:
        accepted = self.accepted_input_count()
        self.driver.execute_script(
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
        self.wait_for_input_acceptance(accepted, 1)

    def type_native_text(self, text: str) -> None:
        ActionChains(self.driver).send_keys(text).perform()

    def type_native_control_prefix(self, key: str, suffix: str) -> None:
        ActionChains(self.driver).key_down(Keys.CONTROL).send_keys(key).key_up(
            Keys.CONTROL
        ).send_keys(suffix).perform()

    def wait_for_frame_text(
        self,
        description: str,
        *,
        contains: str,
        excludes: str | None = None,
    ) -> str:
        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            text = self.frame_text()
            if contains in text and (excludes is None or excludes not in text):
                return text
            time.sleep(0.2)
        messages = self.driver.execute_script("return globalThis.__neomacsMessages")
        raise RuntimeError(
            f"editor did not render {description}; "
            f"worker messages={messages!r}; frame={self.frame_text()!r}"
        )

    def invoke_mx(self, command: str) -> None:
        self.dispatch_key("x", alt=True)
        self.wait_for_frame_text("the M-x prompt", contains="M-x")
        self.commit_text(command)
        self.wait_for_frame_text(command, contains=command)
        self.dispatch_key("Enter")

    def switch_to_buffer(self, buffer_name: str) -> None:
        self.invoke_mx("switch-to-buffer")
        self.wait_for_frame_text(
            "the switch-to-buffer prompt",
            contains="Switch to buffer",
        )
        self.commit_text(buffer_name)
        self.dispatch_key("Enter")
        self.wait_for_frame_text(f"buffer {buffer_name!r}", contains=buffer_name)

    def exercise_buffer_switching(self) -> None:
        suffix = f"{time.time_ns():x}"[-8:]
        buffer_name = f"wasm-{suffix}"
        buffer_text = f"WASM-BUFFER:{suffix}"

        self.switch_to_buffer(buffer_name)
        self.commit_text(buffer_text)
        self.wait_for_frame_text("inserted buffer text", contains=buffer_text)

        self.switch_to_buffer("*scratch*")
        self.wait_for_frame_text(
            "the scratch buffer without the test buffer contents",
            contains="*scratch*",
            excludes=buffer_text,
        )

        self.switch_to_buffer(buffer_name)
        self.wait_for_frame_text(
            "the preserved test buffer contents",
            contains=buffer_text,
        )

    def eval_expression(
        self,
        expression: str,
        marker: str,
        failure_marker: str | None = None,
    ) -> str:
        self.dispatch_key(":", alt=True)
        expression = " ".join(line.strip() for line in expression.splitlines())
        self.commit_text(expression)
        self.dispatch_key("Enter")

        deadline = time.monotonic() + self.timeout
        while time.monotonic() < deadline:
            text = self.frame_text()
            if marker in text:
                return text
            if failure_marker and failure_marker in text:
                raise RuntimeError(
                    f"editor rendered failure marker {failure_marker!r}; frame={text!r}"
                )
            time.sleep(0.2)
        messages = self.driver.execute_script("return globalThis.__neomacsMessages")
        raise RuntimeError(
            f"editor did not render marker {marker!r}; "
            f"worker messages={messages!r}; frame={self.frame_text()!r}"
        )

    def capture_failure_artifacts(self, directory: str) -> None:
        output = Path(directory)
        output.mkdir(parents=True, exist_ok=True)

        errors: list[str] = []
        try:
            self.driver.save_screenshot(str(output / "browser.png"))
        except Exception as error:  # noqa: BLE001 - preserve the original failure
            errors.append(f"screenshot: {error}")
        try:
            (output / "page.html").write_text(self.driver.page_source, encoding="utf-8")
        except Exception as error:  # noqa: BLE001 - preserve the original failure
            errors.append(f"page source: {error}")
        try:
            state = {
                "frame_text": self.frame_text(),
                "startup_frame_texts": self.observed_startup_frame_texts(),
                "worker_messages": self.driver.execute_script(
                    "return globalThis.__neomacsMessages || []"
                ),
            }
            (output / "browser-state.json").write_text(
                json.dumps(state, indent=2, sort_keys=True),
                encoding="utf-8",
            )
        except Exception as error:  # noqa: BLE001 - preserve the original failure
            errors.append(f"browser state: {error}")
        if errors:
            (output / "artifact-errors.txt").write_text(
                "\n".join(errors) + "\n",
                encoding="utf-8",
            )
