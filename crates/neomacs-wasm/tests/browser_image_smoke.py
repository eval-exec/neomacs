#!/usr/bin/env python3
"""Exercise Lisp image sizing and actual SVG pixels in the browser canvas."""

import argparse
from pathlib import Path
import time

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def green_pixels(driver):
    return driver.execute_async_script(r"""
      const [png, done] = arguments;
      const image = new Image();
      image.onload = () => {
        const canvas = document.createElement('canvas');
        canvas.width = image.naturalWidth;
        canvas.height = image.naturalHeight;
        const ctx = canvas.getContext('2d');
        ctx.drawImage(image, 0, 0);
        const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
        let green = 0;
        for (let i = 0; i < pixels.length; i += 4)
          if (pixels[i] < 15 && pixels[i + 1] > 240 && pixels[i + 2] < 15) green++;
        done(green);
      };
      image.onerror = () => done(-1);
      image.src = 'data:image/png;base64,' + png;
    """, driver.find_element("css selector", "canvas").screenshot_as_base64)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir", required=True)
    args = parser.parse_args()
    artifacts = Path(args.artifacts_dir)
    artifacts.mkdir(parents=True, exist_ok=True)
    driver = webdriver.Chrome(options=chrome_options(None, args.headless))
    driver.set_window_size(1600, 950)
    editor = BrowserEditorHarness(driver, 90)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        editor.eval_expression('''(with-current-buffer "*NEO Emacs*"
          (font-lock-flush)
          (font-lock-ensure)
          (let* ((image (get-char-property (point-min) 'display))
                 (size (and image (image-size image t)))
                 (window (get-buffer-window (current-buffer))))
            (unless (and size (> (car size) 100)
                         (<= (car size) (window-body-width window t))
                         (< (abs (- (/ (float (cdr size)) (car size)) 0.2375)) 0.01))
              (error (concat "Landing banner " "missing or incorrectly sized")))
            (message (concat "BANNER-" "PASSED"))))''',
          "BANNER-PASSED", "Landing banner missing or incorrectly sized")
        driver.save_screenshot(str(artifacts / "banner.png"))
        initial_width = editor.frame_payload()["frame_pixel_width"]
        driver.set_window_size(1200, 950)
        deadline = time.monotonic() + 20
        while editor.frame_payload()["frame_pixel_width"] >= initial_width:
            if time.monotonic() >= deadline:
                raise AssertionError("editor did not follow browser resize")
            time.sleep(0.1)
        editor.eval_expression('''(with-current-buffer "*NEO Emacs*"
          (let* ((image (get-char-property (point-min) 'display))
                 (width (car (image-size image t)))
                 (window (get-buffer-window (current-buffer))))
            (unless (<= width (window-body-width window t))
              (error (concat "Banner overflowed " "resized pane")))
            (message (concat "BANNER-RESIZE-" "PASSED"))))''',
          "BANNER-RESIZE-PASSED", "Banner overflowed resized pane")
        driver.save_screenshot(str(artifacts / "banner-resized.png"))
        editor.eval_expression('''(progn
          (setq eval-expression-debug-on-error nil)
          (setq neo-test-image (create-image
            "<svg xmlns='http://www.w3.org/2000/svg' width='160' height='80'><rect width='160' height='80' fill='#00ff00'/></svg>"
            'svg t :width 160 :scale 1))
          (unless (equal (image-size neo-test-image t) '(160 . 80))
            (error (concat "Image dimensions " "incorrect")))
          (switch-to-buffer (get-buffer-create "*Image Test*"))
          (fundamental-mode)
          (erase-buffer)
          (insert-image neo-test-image)
          (insert "\\nInline SVG pixels.\\n")
          (dotimes (i 100) (insert (format "Scrolling line %d\\n" i)))
          (goto-char (point-min))
          (message (concat "IMAGE-SIZE-" "PASSED")))''',
          "IMAGE-SIZE-PASSED", "Image dimensions incorrect")
        screenshot = artifacts / "image.png"
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            driver.save_screenshot(str(screenshot))
            green = green_pixels(driver)
            if green > 10000:
                break
            time.sleep(0.2)
        else:
            raise AssertionError("SVG rectangle was not painted into the canvas")
        editor.eval_expression('''(progn
          (goto-char (point-min))
          (forward-line 10)
          (set-window-start (selected-window) (point))
          (unless (> (window-start) 1)
            (error (concat "Image buffer did not " "scroll")))
          (message (concat "IMAGE-SCROLL-" "PASSED")))''',
          "IMAGE-SCROLL-PASSED", "Image buffer did not scroll")
        deadline = time.monotonic() + 20
        while time.monotonic() < deadline:
            green = green_pixels(driver)
            if green == 0:
                break
            time.sleep(0.2)
        else:
            raise AssertionError("Image pixels remained after scrolling out of view")
        driver.save_screenshot(str(artifacts / "image-scrolled.png"))
        print("PASS: banner fits/resizes; inline SVG paints and scrolls out of view")
    except Exception:
        editor.capture_failure_artifacts(str(artifacts))
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
