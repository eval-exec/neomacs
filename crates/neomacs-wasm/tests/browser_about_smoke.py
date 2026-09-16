#!/usr/bin/env python3
"""The About avatar paints and the author name opens a real browser tab."""

import argparse
from pathlib import Path
import time

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def click_name(driver, editor):
    about = editor.wait_for_window_matrices("About", contains="@eval-exec", count=1)[0]
    row = next(row for row in about["matrix"]["rows"]
               if "Eval Exec" in editor.matrix_text({"matrix": {"rows": [row]}}))
    canvas = driver.find_element("css selector", "canvas").rect
    x = canvas["x"] + about["text_pixel_bounds"]["x"] + 20
    y = canvas["y"] + about["text_pixel_bounds"]["y"] + row["pixel_y"] + row["height_px"] / 2
    for kind in ("mousePressed", "mouseReleased"):
        driver.execute_cdp_cmd("Input.dispatchMouseEvent", {
            "type": kind, "x": x, "y": y, "button": "left",
            "buttons": 1 if kind == "mousePressed" else 0, "clickCount": 1,
        })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir", required=True)
    args = parser.parse_args()
    artifacts = Path(args.artifacts_dir)
    artifacts.mkdir(parents=True, exist_ok=True)
    options = chrome_options(None, args.headless)
    # ChromeDriver normally disables popup blocking. Test ordinary Chrome policy.
    options.add_experimental_option("excludeSwitches", ["disable-popup-blocking"])
    driver = webdriver.Chrome(options=options)
    driver.set_window_size(1800, 1100)
    editor = BrowserEditorHarness(driver, 90)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_frame_text("landing", contains="Welcome to the browser editor.")
        editor.eval_expression('''(with-current-buffer "*About*"
          (let* ((image-position (if (get-text-property (point-min) 'display)
                                     (point-min)
                                   (next-single-property-change (point-min) 'display)))
                 (image (and image-position (get-text-property image-position 'display))))
            (unless (and image (equal (image-size image t) '(112 . 112)))
              (error (concat "Author image " "missing or wrong size"))))
          (let ((button (next-button (point-min))))
            (unless (and button (equal (button-label button) "Eval Exec"))
              (error (concat "Author name " "is not a button"))))
          (message (concat "ABOUT-" "READY")))''', "ABOUT-READY",
            "Author image missing or wrong size")
        about = editor.wait_for_window_matrices("About", contains="@eval-exec", count=1)[0]
        # Check actual avatar pixels in this pane, not just an image Lisp spec.
        pink = driver.execute_async_script(r"""
          const [png, bounds, done] = arguments;
          const image = new Image();
          image.onload = () => {
            const canvas = document.createElement('canvas');
            canvas.width = image.naturalWidth; canvas.height = image.naturalHeight;
            const ctx = canvas.getContext('2d'); ctx.drawImage(image, 0, 0);
            const scale = image.naturalWidth / document.querySelector('canvas').getBoundingClientRect().width;
            const pixels = ctx.getImageData(Math.floor(bounds.x * scale), Math.floor(bounds.y * scale),
              Math.floor(bounds.width * scale), Math.floor(bounds.height * scale)).data;
            let pink = 0;
            for (let i = 0; i < pixels.length; i += 4)
              if (pixels[i] > 170 && pixels[i] > pixels[i+1] + 25
                  && pixels[i+1] > 110 && pixels[i+2] > 110) pink++;
            done(pink / (scale * scale));
          };
          image.onerror = () => done(-1);
          image.src = 'data:image/png;base64,' + png;
        """, driver.find_element("css selector", "canvas").screenshot_as_base64,
            about["text_pixel_bounds"])
        assert pink > 1000, f"Avatar was not painted: {pink} pink pixels"
        driver.save_screenshot(str(artifacts / "about.png"))

        original = driver.current_window_handle
        click_name(driver, editor)
        deadline = time.monotonic() + 15
        while len(driver.window_handles) < 2:
            if time.monotonic() >= deadline:
                raise AssertionError("Clicking the author did not open a tab")
            time.sleep(0.1)
        driver.switch_to.window(next(handle for handle in driver.window_handles if handle != original))
        deadline = time.monotonic() + 15
        while driver.current_url == "about:blank" and time.monotonic() < deadline:
            time.sleep(0.1)
        assert driver.current_url.rstrip("/") == "https://github.com/eval-exec", driver.current_url
        assert driver.execute_script("return window.opener === null")
        driver.close()
        driver.switch_to.window(original)

        # A denied popup must leave an explicit ordinary link, not lose the action.
        driver.execute_script("window.open = () => null")
        click_name(driver, editor)
        deadline = time.monotonic() + 10
        while not driver.find_element("id", "browser-pending-link").is_displayed():
            if time.monotonic() >= deadline:
                raise AssertionError("Blocked tab did not offer a fallback link")
            time.sleep(0.1)
        fallback = driver.find_element("css selector", "#browser-pending-link a")
        assert fallback.get_attribute("href") == "https://github.com/eval-exec"
        assert fallback.get_attribute("target") == "_blank"
        fallback.click()
        deadline = time.monotonic() + 10
        while len(driver.window_handles) < 2:
            if time.monotonic() >= deadline:
                raise AssertionError("Fallback link did not open a tab")
            time.sleep(0.1)
        assert not driver.find_element("id", "browser-pending-link").is_displayed()
        print("PASS: About avatar paints; name opens profile; blocked popup offers a link")
    except Exception:
        editor.capture_failure_artifacts(str(artifacts))
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
