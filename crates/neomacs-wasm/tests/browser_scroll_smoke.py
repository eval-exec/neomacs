#!/usr/bin/env python3
"""A real browser wheel scrolls the hovered Org pane, preserving selection."""

import argparse
from pathlib import Path

from selenium import webdriver
from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.actions.wheel_input import ScrollOrigin

from browser_test_support import BrowserEditorHarness, chrome_options


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--binary")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir", required=True)
    args = parser.parse_args()
    driver = webdriver.Chrome(options=chrome_options(args.binary, args.headless))
    driver.set_window_size(1800, 950)
    editor = BrowserEditorHarness(driver, 90)
    artifacts = Path(args.artifacts_dir)
    artifacts.mkdir(parents=True, exist_ok=True)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        editor.wait_for_frame_text("welcome", contains="Welcome to the browser editor.")
        editor.eval_expression('''(progn
          (setq eval-expression-debug-on-error nil)
          (setq neo-test-scroll-window (get-buffer-window "*NEO Emacs*"))
          (setq neo-test-scroll-start (window-start neo-test-scroll-window))
          (setq neo-test-scroll-selected (selected-window))
          (setq neo-test-playground-start (window-start neo-test-scroll-selected))
          (unless (equal (buffer-name) "*NEO Emacs Playground*")
            (error "Playground must be selected before hovering the Org pane"))
          (message (concat "SCROLL-" "READY")))''', "SCROLL-READY")
        origin = ScrollOrigin.from_viewport(500, 300)
        for _ in range(6):
            ActionChains(driver).scroll_from_origin(origin, 0, 600).perform()
        editor.eval_expression('''(progn
          (unless (and (> (window-start neo-test-scroll-window) neo-test-scroll-start)
                       (eq (selected-window) neo-test-scroll-selected)
                       (= (window-start neo-test-scroll-selected) neo-test-playground-start))
            (error (concat "Wheel did not " "scroll only the hovered Org pane")))
          (setq neo-test-scroll-down (window-start neo-test-scroll-window))
          (message (concat "SCROLL-DOWN-" "PASSED")))''',
          "SCROLL-DOWN-PASSED", "Wheel did not scroll only the hovered Org pane")
        driver.save_screenshot(str(artifacts / "scroll-down.png"))
        for _ in range(6):
            ActionChains(driver).scroll_from_origin(origin, 0, -600).perform()
        editor.eval_expression('''(progn
          (unless (< (window-start neo-test-scroll-window) neo-test-scroll-down)
            (error (concat "Reverse wheel did not " "scroll upward")))
          (message (concat "SCROLL-UP-" "PASSED")))''',
          "SCROLL-UP-PASSED", "Reverse wheel did not scroll upward")
        driver.save_screenshot(str(artifacts / "scroll.png"))
        print("PASS: real wheel scrolls Org down/up without selecting it or scrolling the playground")
    except Exception:
        editor.capture_failure_artifacts(str(artifacts))
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
