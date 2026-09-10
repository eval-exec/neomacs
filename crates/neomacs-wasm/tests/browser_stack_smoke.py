#!/usr/bin/env python3
"""Require Lisp recursion limits to signal without killing the WASM Worker."""

import argparse

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
        for limit in (200, 400, 800, 1600):
            editor.eval_expression(
                f"""(progn
                      (defalias 'neomacs-stack-probe
                        (lambda () (neomacs-stack-probe)))
                      (unwind-protect
                          (condition-case err
                              (let ((max-lisp-eval-depth {limit}))
                                (neomacs-stack-probe)
                                (message (concat "STACK-" "FAIL: returned")))
                            (excessive-lisp-nesting
                              (message (concat "STACK-" "{limit}-PASS")))
                            (error (message (concat "STACK-" "FAIL: %S") err)))
                        (fmakunbound 'neomacs-stack-probe)))""",
                f"STACK-{limit}-PASS",
                failure_marker="STACK-FAIL:",
            )
            print(f"PASS: Lisp recursion limit {limit} signals without trapping", flush=True)
        editor.eval_expression(
            '(message (concat "STACK-" "ALIVE-%d") (+ 20 22))',
            "STACK-ALIVE-42",
        )
    except Exception:
        if args.artifacts_dir:
            editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
