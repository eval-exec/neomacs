#!/usr/bin/env python3
"""Basic editor acceptance checks in stock Chrome or Firefox.

Uses isolated WebDriver profiles and ordinary editor input. No experimental
WebGPU, JSPI, or filesystem browser preferences are enabled.
"""

import argparse
from pathlib import Path
from tempfile import TemporaryDirectory
import time

from selenium import webdriver
from selenium.webdriver.common.action_chains import ActionChains
from selenium.webdriver.common.keys import Keys
from selenium.webdriver.firefox.options import Options as FirefoxOptions

from browser_opfs_smoke import exercise_native_keyboard, startup_warning_rendered
from browser_test_support import BrowserEditorHarness, chrome_options


def exercise_font_queries_and_vertical_motion(editor):
    # Test the fresh landing page before changing faces or disabling packages.
    for key in (Keys.ARROW_UP, Keys.ARROW_DOWN):
        ActionChains(editor.driver).send_keys(key).perform()
    editor.eval_expression(
        '''(progn
             (unless (and (stringp (face-font 'default))
                          (> (aref (font-info (face-font 'default)) 3) 0))
               (error "default font cannot be reopened"))
             (with-current-buffer (get-buffer-create "*wasm-motion-test*")
               (erase-buffer) (insert "one\\ntwo\\nthree") (goto-char 1))
             (switch-to-buffer "*wasm-motion-test*")
             (message (concat "MOTION-" "READY")))''',
        "MOTION-READY",
    )
    ActionChains(editor.driver).send_keys(Keys.ARROW_DOWN).perform()
    editor.eval_expression(
        '(if (= (point) 5) (message (concat "MOVED-" "DOWN")) (error "down stopped at %s" (point)))',
        "MOVED-DOWN",
    )
    ActionChains(editor.driver).send_keys(Keys.ARROW_UP).perform()
    editor.eval_expression(
        '''(progn
             (unless (= (point) 1) (error "up stopped at %s" (point)))
             (unless (fontp (font-at (point))) (error "font-at did not resolve text"))
             (let ((height (default-font-height)) (name (face-font 'default)))
               (text-scale-set 2)
               (unless (> (default-font-height) height)
                 (error "scaled font metrics did not grow: %s %s -> %s %s; %S"
                        height name (default-font-height) (face-font 'default)
                        face-remapping-alist))
               (text-scale-set 0))
             (when (get-buffer "*Messages*")
               (with-current-buffer "*Messages*"
                 (when (string-match-p "Wrong type argument: stringp, nil" (buffer-string))
                   (error "font query failed during arrow motion"))))
             (kill-buffer "*wasm-motion-test*")
             (message (concat "FONT-MOTION-" "PASSED")))''',
        "FONT-MOTION-PASSED",
    )


def exercise_editing_and_persistence(editor, restart):
    token = f"BASIC-{time.time_ns():x}"
    path = f"/neomacs-fake/{token}.txt"
    editor.eval_expression(
        f'(progn (find-file "{path}") (buffer-enable-undo) (message "FILE-READY"))',
        "FILE-READY",
    )
    editor.type_native_text(token)
    editor.wait_for_frame_text("typed file contents", contains=token)
    editor.type_native_control_key("a")
    editor.type_native_control_key(" ")
    editor.type_native_control_key("e")
    editor.type_native_control_key("w")
    editor.eval_expression(
        '(if (= (buffer-size) 0) (message "REGION-KILLED") (error "region was not deleted"))',
        "REGION-KILLED",
    )
    editor.type_native_control_key("y")
    editor.invoke_mx("undo")
    editor.eval_expression(
        '(if (= (buffer-size) 0) (message "YANK-UNDONE") (error "undo did not restore empty buffer"))',
        "YANK-UNDONE",
    )
    editor.type_native_text(token)
    editor.type_native_control_key("x")
    editor.type_native_control_key("s")
    editor.eval_expression(
        f'''(progn
              (unless (and (not (buffer-modified-p))
                           (string= "{token}\\n" (buffer-string)))
                (error "interactive save failed: %S modified=%S"
                       (buffer-string) (buffer-modified-p)))
              (with-temp-file "{path}.large" (insert (make-string 65537 ?z)))
              (message "FILE-SAVED"))''',
        "FILE-SAVED",
    )
    url = editor.driver.current_url
    editor.driver.quit()
    editor.driver = restart()
    editor.install_frame_observer()
    editor.driver.get(url)
    editor.wait_ready()
    editor.wait_for_presentation()
    editor.eval_expression(
        f'''(progn
              (unless (and
                (string= "{token}\\n" (with-temp-buffer
                  (insert-file-contents "{path}") (buffer-string)))
                (string= (make-string 65537 ?z) (with-temp-buffer
                  (insert-file-contents "{path}.large") (buffer-string))))
                (error "persistent file contents changed"))
              (delete-file "{path}")
              (delete-file "{path}.large")
              (message "FILES-PERSISTED"))''',
        "FILES-PERSISTED",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--browser", choices=("chrome", "firefox"), default="chrome")
    parser.add_argument("--binary")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--timeout", type=float, default=60)
    parser.add_argument("--artifacts-dir", required=True)
    parser.add_argument("--persistence-only", action="store_true",
                        help="only run the editing, save, and browser-restart checks")
    args = parser.parse_args()
    with TemporaryDirectory(prefix="browser-basic-", dir=Path(__file__).resolve().parents[3] / "tmp") as profile:
        run(args, profile)


def run(args, profile):
    if args.browser == "firefox":
        options = FirefoxOptions()
        if args.binary:
            options.binary_location = args.binary
        if args.headless:
            options.add_argument("-headless")
        options.set_capability("webSocketUrl", True)
        options.add_argument("-profile")
        options.add_argument(profile)
        make_driver = lambda: webdriver.Firefox(options=options)
    else:
        options = chrome_options(args.binary, args.headless)
        options.add_argument(f"--user-data-dir={profile}")
        make_driver = lambda: webdriver.Chrome(options=options)
    driver = make_driver()
    editor = BrowserEditorHarness(driver, args.timeout)

    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        if startup_warning_rendered(editor.finish_startup_frame_capture()):
            raise AssertionError("startup displayed the user-emacs-directory warning")
        if not args.persistence_only:
            exercise_font_queries_and_vertical_motion(editor)
            exercise_native_keyboard(editor)
            editor.exercise_buffer_switching()
            print(f"PASS: {args.browser} startup, typing, M-x, Org, splits, buffers", flush=True)
        exercise_editing_and_persistence(editor, make_driver)
        print(f"PASS: {args.browser} region, kill/yank, undo, save, large read, browser-restart persistence", flush=True)
    except Exception:
        editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        editor.driver.quit()


if __name__ == "__main__":
    main()
