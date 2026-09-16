#!/usr/bin/env python3
"""Exercise compiled-file help, standard mode lines, and browser completion."""

import argparse

from selenium import webdriver
from selenium.webdriver import ActionChains, Keys

from browser_test_support import BrowserEditorHarness, chrome_options


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--chrome")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir")
    args = parser.parse_args()
    driver = webdriver.Chrome(options=chrome_options(args.chrome, args.headless))
    driver.set_window_size(1800, 1100)
    editor = BrowserEditorHarness(driver, 90)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_frame_text("landing", contains="Welcome to the browser editor.")
        ActionChains(driver).key_down(Keys.CONTROL).send_keys("h").key_up(
            Keys.CONTROL
        ).send_keys("f").perform()
        editor.wait_for_frame_text("describe-function prompt", contains="Describe function")
        editor.commit_text("insert-file")
        editor.dispatch_key("Enter")
        editor.wait_for_frame_text("compiled function help", contains="Insert contents of file")
        editor.eval_expression(
            r'''(condition-case err
                    (progn
                      (unless (and (null neomacs-wasm-package-error)
                                   (not (bound-and-true-p doom-modeline-mode))
                                   fido-mode
                                   (featurep 'consult))
                        (error "Package startup: %S" neomacs-wasm-package-error))
                      (message (concat "PACKAGES-" "PASS")))
                  (error (message (concat "PACKAGES-" "FAIL: %S") err)))''',
            "PACKAGES-PASS",
            failure_marker="PACKAGES-FAIL:",
        )
        ActionChains(driver).key_down(Keys.CONTROL).send_keys("x").key_up(
            Keys.CONTROL
        ).send_keys("b").perform()
        editor.wait_for_frame_text("Consult buffer prompt", contains="Switch to:")
        editor.commit_text("*scratch*")
        editor.dispatch_key("Enter")
        editor.eval_expression(
            r'''(message (concat "CONSULT-" "%s")
                         (if (equal (buffer-name) "*scratch*") "PASS" "FAIL"))''',
            "CONSULT-PASS",
            failure_marker="CONSULT-FAIL",
        )
        print("PASS: compiled-file help, standard mode line, Fido and Consult buffer selection")
    except Exception:
        if args.artifacts_dir:
            editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
