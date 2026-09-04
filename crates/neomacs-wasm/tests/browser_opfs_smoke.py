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


def assert_split_orientation(
    matrices: list[dict[str, object]],
    *,
    side_by_side: bool,
) -> None:
    first, second = (matrix["pixel_bounds"] for matrix in matrices)
    if side_by_side:
        correct = first["x"] != second["x"] and first["y"] == second["y"]
        expected = "side-by-side"
    else:
        correct = first["x"] == second["x"] and first["y"] != second["y"]
        expected = "top/bottom"
    if not correct:
        raise RuntimeError(
            f"browser editor did not render a {expected} split: "
            f"{first!r}, {second!r}"
        )


def exercise_native_keyboard(editor: BrowserEditorHarness) -> None:
    editor.switch_to_buffer("*scratch*")
    editor.click_editor_canvas()
    editor.type_native_meta_prefix("x")
    editor.wait_for_frame_text("the native M-x prompt", contains="M-x")
    editor.type_native_text("a")
    editor.wait_for_frame_text("native text in the M-x prompt", contains="M-x a")
    editor.type_native_control_key("g")
    editor.wait_for_frame_text(
        "the cancelled native M-x prompt",
        contains="*scratch*",
        excludes="M-x a",
    )

    marker = f"NATIVE-{time.time_ns():x}"[-16:]
    editor.type_native_text(marker)
    editor.wait_for_frame_text("native Chrome text input", contains=marker)

    for digit, side_by_side in (("2", False), ("3", True)):
        editor.type_native_control_prefix("x", digit)
        matrices = editor.wait_for_window_matrices(
            f"C-x {digit} split",
            contains=marker,
            count=2,
        )
        assert_split_orientation(matrices, side_by_side=side_by_side)
        editor.type_native_control_prefix("x", "1")
        editor.wait_for_window_matrices(
            "C-x 1 split cleanup",
            contains=marker,
            count=1,
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
            exercise_native_keyboard(editor)
            print("PASS: native Chrome typing, C-x 2, and C-x 3")

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
