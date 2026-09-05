#!/usr/bin/env python3
"""Verify packaged Neomacs geometry across browser HiDPI changes."""

from __future__ import annotations

import argparse
import time
from pathlib import Path

import cbor2
from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--chrome", help="path to Chrome or Chromium")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir")
    parser.add_argument("--timeout", type=float, default=60.0)
    return parser.parse_args()


def set_device_metrics(
    driver: webdriver.Chrome,
    *,
    width: int,
    height: int,
    scale: float,
) -> None:
    driver.execute_cdp_cmd(
        "Emulation.setDeviceMetricsOverride",
        {
            "width": width,
            "height": height,
            "deviceScaleFactor": scale,
            "mobile": False,
        },
    )


def latest_frame(driver: webdriver.Chrome) -> dict[str, object]:
    values = driver.execute_script(
        "return Array.from(new Uint8Array("
        "globalThis.__neomacsLastFrame || new ArrayBuffer()))"
    )
    return cbor2.loads(bytes(values)) if values else {}


def wait_for_logical_geometry(
    driver: webdriver.Chrome,
    *,
    width: int,
    height: int,
    timeout: float,
    after_presentation: int | None = None,
) -> dict[str, object]:
    deadline = time.monotonic() + timeout
    observed: dict[str, object] = {}
    while time.monotonic() < deadline:
        observed = latest_frame(driver)
        if (
            observed.get("frame_pixel_width") == float(width)
            and observed.get("frame_pixel_height") == float(height)
            and observed.get("font_pixel_size") == 16.0
            and (
                after_presentation is None
                or observed.get("presentation_id") != after_presentation
            )
        ):
            return observed
        time.sleep(0.1)
    raise RuntimeError(
        f"editor did not adopt logical viewport {width}x{height}; "
        f"last frame geometry={frame_geometry(observed)!r}"
    )


def assert_editor_frame_scale(
    editor: BrowserEditorHarness,
    expected: float,
) -> None:
    marker_prefix = f"HIDPI-SCALE-{time.time_ns()}-"
    editor.eval_expression(
        f'''(if (= (frame-scale-factor) {expected})
               (message (concat "{marker_prefix}" "OK"))
             (message (concat "{marker_prefix}" "FAIL:"
                              (number-to-string (frame-scale-factor)))))''',
        f"{marker_prefix}OK",
        failure_marker=f"{marker_prefix}FAIL:",
    )


def assert_hidpi_keyboard_cursor(editor: BrowserEditorHarness) -> None:
    editor.click_editor_canvas()
    editor.type_native_meta_prefix("x")
    editor.wait_for_frame_text("the HiDPI native M-x prompt", contains="M-x")
    editor.type_native_text("a")
    editor.wait_for_frame_text("HiDPI native text in M-x", contains="M-x a")
    editor.assert_active_cursor("HiDPI native text in the M-x minibuffer")
    editor.type_native_control_key("g")
    editor.wait_for_frame_text(
        "the cancelled HiDPI native M-x prompt",
        contains="*scratch*",
        excludes="M-x a",
    )


def frame_geometry(frame: dict[str, object]) -> dict[str, object]:
    return {
        key: frame.get(key)
        for key in (
            "presentation_id",
            "frame_pixel_width",
            "frame_pixel_height",
            "font_pixel_size",
            "char_width",
            "char_height",
        )
    }


def assert_browser_viewport(
    driver: webdriver.Chrome,
    *,
    width: int,
    height: int,
    scale: float,
) -> None:
    observed = driver.execute_script(
        r"""
        const canvas = document.querySelector("canvas");
        const bounds = canvas.getBoundingClientRect();
        return {
          width: globalThis.innerWidth,
          height: globalThis.innerHeight,
          scale: globalThis.devicePixelRatio,
          canvasWidth: bounds.width,
          canvasHeight: bounds.height,
        };
        """
    )
    expected = {
        "width": width,
        "height": height,
        "scale": scale,
        "canvasWidth": width - 18,
        "canvasHeight": height - 52,
    }
    if observed != expected:
        raise RuntimeError(
            f"browser did not expose the requested logical viewport: "
            f"expected={expected!r}, observed={observed!r}"
        )


def main() -> None:
    args = parse_args()
    driver = webdriver.Chrome(options=chrome_options(args.chrome, args.headless))
    editor = BrowserEditorHarness(driver, args.timeout)
    try:
        editor.install_frame_observer()
        set_device_metrics(driver, width=1975, height=1100, scale=1.75)
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        assert_browser_viewport(driver, width=1975, height=1100, scale=1.75)
        wait_for_logical_geometry(
            driver,
            width=1957,
            height=1048,
            timeout=args.timeout,
        )
        assert_editor_frame_scale(editor, 1.75)
        assert_hidpi_keyboard_cursor(editor)

        before_scale_change = latest_frame(driver).get("presentation_id")
        set_device_metrics(driver, width=1975, height=1100, scale=2.0)
        assert_browser_viewport(driver, width=1975, height=1100, scale=2.0)
        wait_for_logical_geometry(
            driver,
            width=1957,
            height=1048,
            timeout=args.timeout,
            after_presentation=before_scale_change,
        )
        assert_editor_frame_scale(editor, 2.0)

        before_resize = latest_frame(driver).get("presentation_id")
        set_device_metrics(driver, width=1440, height=900, scale=2.0)
        assert_browser_viewport(driver, width=1440, height=900, scale=2.0)
        wait_for_logical_geometry(
            driver,
            width=1422,
            height=848,
            timeout=args.timeout,
            after_presentation=before_resize,
        )

        print(
            "PASS: browser editor preserved logical geometry across DPR-only "
            "and viewport changes"
        )
    except Exception:
        if args.artifacts_dir:
            editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
