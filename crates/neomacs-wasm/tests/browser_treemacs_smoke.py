#!/usr/bin/env python3
"""Real Treemacs mouse selection and double-click file navigation."""

import argparse
from pathlib import Path
import time

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def click(driver, count):
    # features.org in the shipped tree at this fixed logical viewport.
    for kind in ("mousePressed", "mouseReleased"):
        driver.execute_cdp_cmd("Input.dispatchMouseEvent", {
            "type": kind, "x": 100, "y": 150, "button": "left",
            "buttons": 1 if kind == "mousePressed" else 0, "clickCount": count,
        })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir", required=True)
    args = parser.parse_args()
    artifacts = Path(args.artifacts_dir)
    artifacts.mkdir(parents=True, exist_ok=True)
    driver = webdriver.Chrome(options=chrome_options(None, args.headless))
    driver.set_window_size(1800, 1100)
    editor = BrowserEditorHarness(driver, 90)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_frame_text("landing", contains="Welcome to the browser editor.")
        editor.timeout = 10
        click(driver, 1)
        time.sleep(0.5)
        editor.eval_expression('''(progn
          (unless (and (eq (selected-window) (treemacs-get-local-window))
                       (string-match-p "features.org"
                         (buffer-substring-no-properties
                           (line-beginning-position) (line-end-position))))
            (error (concat "Mouse did not " "select the tree file")))
          (message (concat "TREE-CLICK-" "SELECTED")))''', "TREE-CLICK-SELECTED")
        click(driver, 1)
        time.sleep(0.06)
        click(driver, 2)
        editor.wait_for_frame_text("double-clicked features file",
                                  contains="An Emacs from the future",
                                  excludes="Wrong type argument: commandp")
        editor.eval_expression('''(progn
          (unless (get-buffer-window
                   (get-file-buffer (expand-file-name "neomacs-landing/features.org" data-directory)))
            (error (concat "Double click did not " "display the file")))
          (message (concat "TREE-DOUBLE-CLICK-" "PASSED")))''', "TREE-DOUBLE-CLICK-PASSED")
        driver.save_screenshot(str(artifacts / "treemacs-file.png"))
        print("PASS: single click selects the tree file; double click displays it")
    except Exception:
        editor.capture_failure_artifacts(str(artifacts))
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
