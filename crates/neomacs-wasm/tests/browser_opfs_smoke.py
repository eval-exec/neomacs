#!/usr/bin/env python3
"""Exercise the packaged Neomacs editor and its persistent browser filesystem.

Serve an output produced by ``cargo xtask build-wasm`` before running this test.
The test deliberately drives ordinary browser input instead of calling a
test-only Wasm API, so editor and filesystem assertions cross the same Lisp,
worker, Rust host-import, and OPFS boundaries as an interactive browser session.
"""

from __future__ import annotations

import argparse
import time

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


STARTUP_WARNING_FRAGMENTS = (
    "Unable to create `user-emacs-directory'",
    "Any data that would normally be written there may be lost!",
    "customize the variable `user-emacs-directory-warning'",
)


def startup_warning_rendered(frame_texts: list[str]) -> bool:
    return any(
        fragment in text
        for text in frame_texts
        for fragment in STARTUP_WARNING_FRAGMENTS
    )


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--url", default="http://127.0.0.1:4174/")
    parser.add_argument("--chrome", help="path to Chrome or Chromium")
    parser.add_argument(
        "--artifacts-dir",
        help="write a screenshot and browser state here when the smoke test fails",
    )
    parser.add_argument("--headless", action="store_true")
    parser.add_argument(
        "--persistence-only",
        action="store_true",
        help="skip the pre-reload mutation coverage for a tighter persistence loop",
    )
    parser.add_argument("--timeout", type=float, default=60.0)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    options = chrome_options(args.chrome, args.headless)

    token = f"neomacs-opfs-{time.time_ns()}"
    driver = webdriver.Chrome(options=options)
    editor = BrowserEditorHarness(driver, args.timeout)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        if startup_warning_rendered(editor.finish_startup_frame_capture()):
            raise RuntimeError("startup rendered the user-emacs-directory warning")

        if not args.persistence_only:
            editor.exercise_buffer_switching()
            print("PASS: browser editor switched buffers and preserved text")

            operations_marker = f"OPFS-OPERATIONS:{token}"
            editor.eval_expression(
                f'''(let* ((dir "/neomacs-fake/browser-smoke")
                       (source (concat dir "/source"))
                       (copy (concat dir "/copy"))
                       (moved (concat dir "/moved"))
                       (temporary (make-temp-file "/neomacs-fake/browser-temp-" nil ".txt" "temporary")))
                  (make-directory dir t)
                  (with-temp-file source (insert "{token}"))
                  (copy-file source copy t)
                  (rename-file copy moved t)
                  (unless (and (file-directory-p dir)
                               (file-exists-p moved)
                               (string= "{token}"
                                        (with-temp-buffer
                                          (insert-file-contents moved)
                                          (buffer-string)))
                               (string= "temporary"
                                        (with-temp-buffer
                                          (insert-file-contents temporary)
                                          (buffer-string))))
                    (error "browser filesystem operation mismatch"))
                  (delete-file temporary)
                  (delete-directory dir t)
                  (message "{operations_marker}"))''',
                operations_marker,
            )

        wrote_marker = f"OPFS-WROTE:{token}"
        editor.eval_expression(
            f'''(progn
                  (with-temp-file "/neomacs-fake/persistence-probe"
                    (insert "{token}"))
                  (message "{wrote_marker}"))''',
            wrote_marker,
        )

        driver.refresh()
        editor.wait_ready()
        editor.wait_for_presentation()
        if startup_warning_rendered(editor.finish_startup_frame_capture()):
            raise RuntimeError("reload rendered the user-emacs-directory warning")

        persisted_marker = f"OPFS-PERSISTED:{token}"
        editor.eval_expression(
            """(condition-case error-data
                  (message "OPFS-PERSISTED:%s"
                           (with-temp-buffer
                             (insert-file-contents "/neomacs-fake/persistence-probe")
                             (buffer-string)))
                (error (message "OPFS-READ-ERROR:%S" error-data)))""",
            persisted_marker,
            failure_marker="OPFS-READ-ERROR:",
        )
        editor.eval_expression(
            f"""(progn
                  (delete-file "/neomacs-fake/persistence-probe")
                  (message "OPFS-CLEANED:{token}"))""",
            f"OPFS-CLEANED:{token}",
        )
        print(f"PASS: browser filesystem persisted {token} across reload")
    except Exception:
        if args.artifacts_dir:
            editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
