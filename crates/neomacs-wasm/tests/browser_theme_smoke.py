#!/usr/bin/env python3
"""Load bundled light/dark themes and verify the rendered editor background."""

import argparse
import time

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--chrome")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir")
    args = parser.parse_args()
    driver = webdriver.Chrome(options=chrome_options(args.chrome, args.headless))
    editor = BrowserEditorHarness(driver, 60)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        for theme, background in (("modus-vivendi", 0), ("modus-operandi", 255)):
            editor.eval_expression(
                f'''(condition-case err
                      (progn (mapc #'disable-theme custom-enabled-themes)
                             (load-theme '{theme} t)
                             (message (concat "THEME-" "{theme}-PASS")))
                      (error (message (concat "THEME-" "FAIL: %S") err)))''',
                f"THEME-{theme}-PASS",
                failure_marker="THEME-FAIL:",
            )
            # Sample blank scratch-buffer space, not the DOM page background.
            bounds = driver.execute_script(
                'return document.querySelector("canvas").getBoundingClientRect().toJSON()'
            )
            scale = driver.execute_script("return devicePixelRatio")
            point = (round((bounds["x"] + bounds["width"] / 2) * scale),
                     round((bounds["y"] + bounds["height"] / 2) * scale))
            deadline = time.monotonic() + 10
            while True:
                color = driver.execute_async_script(
                    r"""
                    const [png, x, y, done] = arguments;
                    const image = new Image();
                    image.onload = () => {
                      const canvas = document.createElement("canvas");
                      canvas.width = image.width;
                      canvas.height = image.height;
                      const context = canvas.getContext("2d");
                      context.drawImage(image, 0, 0);
                      done(Array.from(context.getImageData(x, y, 1, 1).data).slice(0, 3));
                    };
                    image.onerror = () => done(null);
                    image.src = `data:image/png;base64,${png}`;
                    """,
                    driver.get_screenshot_as_base64(), *point,
                )
                if color == [background] * 3:
                    break
                if time.monotonic() >= deadline:
                    raise AssertionError(f"{theme}: rendered background {color}, expected {background}")
                time.sleep(0.1)
        print("PASS: bundled Modus dark and light themes load and render")
    except Exception:
        if args.artifacts_dir:
            editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
