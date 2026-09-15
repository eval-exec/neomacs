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
        options.add_argument("--window-size=1800,1100")
        editor = BrowserEditorHarness(webdriver.Chrome(options=options), args.timeout)
        try:
            editor.install_frame_observer()
            editor.driver.get(args.url)
            editor.wait_ready()
            editor.wait_for_presentation()
            editor.wait_for_frame_text("default landing page", contains="Welcome to the browser editor.")
            editor.eval_expression(
                '''(progn
                  (unless (and (eq neomacs-wasm-startup-profile 'landing)
                               (featurep 'treemacs) (featurep 'doom-themes)
                               (not neomacs-wasm-package-error)
                               (treemacs-get-local-window)
                               (get-buffer-window "*NEO Emacs About*")
                               (memq 'doom-one (custom-available-themes)))
                    (error "landing packages missing: %S" neomacs-wasm-package-error))
                  (message "DEFAULT-LANDING-READY"))''',
                "DEFAULT-LANDING-READY",
            )
            artifacts = Path(args.artifacts_dir)
            artifacts.mkdir(parents=True, exist_ok=True)
            editor.driver.save_screenshot(str(artifacts / "default-landing.png"))
            editor.eval_expression(
                '''(progn (neomacs-wasm-landing-theme 'doom-one)
                  (unless (and (equal custom-enabled-themes '(doom-one))
                               (equal (face-background 'default) "#282c34"))
                    (error "doom-one theme not applied"))
                  (message "DOOM-THEME-APPLIED"))''',
                "DOOM-THEME-APPLIED",
            )
            editor.driver.save_screenshot(str(artifacts / "dark-landing.png"))
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
            editor.invoke_mx("neomacs-wasm-landing-open")
            editor.wait_for_frame_text("landing command in editor profile", contains="Welcome to the browser editor.")
            landing_init = '''(setq neomacs-wasm-startup-profile 'landing)
(setq neomacs-wasm-landing-personal-info "A test user's introduction.")
'''
            editor.eval_expression(
                '(progn (with-temp-file user-init-file '
                f'(insert {json.dumps(landing_init)})) (message "LANDING-INIT-SAVED"))',
                "LANDING-INIT-SAVED",
            )
            editor.driver.quit()
            editor.driver = webdriver.Chrome(options=options)
            editor.install_frame_observer()
            editor.driver.get(args.url)
            editor.wait_ready()
            editor.wait_for_presentation()
            editor.wait_for_frame_text("landing page", contains="Welcome to the browser editor.")
            editor.eval_expression(
                '''(progn
                  (unless (and which-key-mode
                               (get-buffer-window "*NEO Emacs*")
                               (get-buffer-window "*NEO Emacs Playground*")
                               (eq major-mode 'emacs-lisp-mode)
                               (= (buffer-size) 0)
                               (get-buffer-window "*NEO Emacs About*"))
                    (error "landing windows not ready"))
                  (message "LANDING-READY"))''',
                "LANDING-READY",
            )
            editor.type_native_text("(setq neomacs-wasm-test-evaluation (+ 1 2))")
            editor.wait_for_frame_text("playground editing", contains="(+ 1 2)")
            editor.type_native_control_key("x")
            editor.type_native_control_key("e")
            editor.eval_expression(
                '''(progn
                  (unless (equal (bound-and-true-p neomacs-wasm-test-evaluation) 3)
                    (error "C-x C-e did not evaluate the playground expression"))
                  (message "PLAYGROUND-EVALUATED"))''',
                "PLAYGROUND-EVALUATED",
            )
            editor.eval_expression(
                '''(progn (neomacs-wasm-landing-open)
                  (unless (equal (buffer-string) "(setq neomacs-wasm-test-evaluation (+ 1 2))")
                    (error "playground edits were lost"))
                  (unless (and (treemacs-get-local-window)
                               (get-buffer-window "*NEO Emacs About*"))
                    (error "reopening lost a landing sidebar"))
                  (message "PLAYGROUND-PRESERVED"))''',
                "PLAYGROUND-PRESERVED",
            )
            artifacts = Path(args.artifacts_dir)
            artifacts.mkdir(parents=True, exist_ok=True)
            editor.driver.save_screenshot(str(artifacts / "landing.png"))
            print("PASS: landing command/profile, personal sidebar, which-key, Lisp evaluation; edits survive reopening")
        except Exception:
            editor.capture_failure_artifacts(args.artifacts_dir)
            raise
        finally:
            editor.driver.quit()


if __name__ == "__main__":
    main()
