#!/usr/bin/env python3
"""User init survives browser restart and overrides shipped WASM defaults."""

import argparse
import json
from pathlib import Path
from tempfile import TemporaryDirectory

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--binary")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--artifacts-dir", required=True)
    args = parser.parse_args()
    with TemporaryDirectory(prefix="browser-init-", dir=Path(__file__).resolve().parents[3] / "tmp") as profile:
        options = chrome_options(args.binary, args.headless)
        options.add_argument(f"--user-data-dir={profile}")
        editor = BrowserEditorHarness(webdriver.Chrome(options=options), args.timeout)
        try:
            editor.install_frame_observer()
            editor.driver.get(args.url)
            editor.wait_ready()
            editor.wait_for_presentation()
            init = '''(setq neomacs-wasm-startup-profile 'editor)
(setq neomacs-wasm-test-init-count (1+ (or (bound-and-true-p neomacs-wasm-test-init-count) 0)))
(which-key-mode -1)
'''
            editor.eval_expression(
                '(progn (make-directory user-emacs-directory t) '
                '(with-temp-file (expand-file-name "init.el" user-emacs-directory) '
                f'(insert {json.dumps(init)})) (message "INIT-SAVED"))',
                "INIT-SAVED",
            )
            editor.driver.quit()
            editor.driver = webdriver.Chrome(options=options)
            editor.install_frame_observer()
            editor.driver.get(args.url)
            editor.wait_ready()
            editor.wait_for_presentation()
            editor.eval_expression(
                '''(progn
                  (unless (and (equal user-init-file "/neomacs-fake/.emacs.d/init.el")
                               (equal (bound-and-true-p neomacs-wasm-test-init-count) 1)
                               (eq neomacs-wasm-startup-profile 'editor)
                               (not which-key-mode)
                               (get-buffer-window "*scratch*"))
                    (error "user init not honored: %S" user-init-file))
                  (message "USER-INIT-HONORED"))''',
                "USER-INIT-HONORED",
            )
            print("PASS: persistent user init loaded once; user defaults win; editor profile opens scratch")
        except Exception:
            editor.capture_failure_artifacts(args.artifacts_dir)
            raise
        finally:
            editor.driver.quit()


if __name__ == "__main__":
    main()
