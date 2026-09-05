#!/usr/bin/env python3
"""Exercise GNU Dired against the packaged browser filesystem."""

import argparse
import time

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def check(editor, expression, marker):
    editor.eval_expression(
        f'''(condition-case err
              (progn {expression} (message (concat "DIRED-" "{marker}")))
            (error (message (concat "DIRED-" "FAIL: %S") err)))''',
        f"DIRED-{marker}", failure_marker="DIRED-FAIL:",
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--chrome")
    parser.add_argument("--headless", action="store_true")
    args = parser.parse_args()
    driver = webdriver.Chrome(options=chrome_options(args.chrome, args.headless))
    editor = BrowserEditorHarness(driver, 60)
    fixture = f"/neomacs-fake/dired-smoke-{time.time_ns()}"
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        check(editor, '''(dired "~")
          (unless (derived-mode-p 'dired-mode) (error "Not in Dired"))
          (unless (dired-goto-file (expand-file-name "~/.emacs.d"))
            (error "Home listing lacks .emacs.d"))''', "HOME-PASS")
        print("PASS: browser Dired lists the virtual home directory")
        check(editor, f'''(dired "~")
          (setq wasm-dired-fixture "{fixture}")
          (dired-create-directory wasm-dired-fixture)
          (unless (dired-goto-file wasm-dired-fixture) (error "New directory not listed"))
          (dired-find-file)
          (unless (equal (expand-file-name default-directory) (file-name-as-directory wasm-dired-fixture))
            (error "Did not enter created directory: %S expected %S"
                   default-directory (file-name-as-directory wasm-dired-fixture)))
          (dired-create-directory (concat wasm-dired-fixture "/child"))
          (unless (dired-goto-file (concat wasm-dired-fixture "/child"))
            (error "Child directory not listed"))
          (dired-find-file)''', "CREATE-PASS")
        check(editor, '''(setq wasm-dired-source (expand-file-name "source.txt")
                              wasm-dired-moved (expand-file-name "renamed.txt"))
          (with-temp-file wasm-dired-source (insert "Dired browser contents"))
          (revert-buffer nil t)
          (unless (dired-goto-file wasm-dired-source) (error "Refresh missed new file"))
          (unless (equal (dired-get-filename) wasm-dired-source) (error "Wrong filename at point"))
          (dired-find-file)
          (unless (equal (buffer-string) "Dired browser contents") (error "Wrong file contents"))
          (dired (concat wasm-dired-fixture "/child"))
          (dired-rename-file wasm-dired-source wasm-dired-moved nil)
          (revert-buffer nil t)
          (when (dired-goto-file wasm-dired-source) (error "Old name remains listed"))
          (unless (dired-goto-file wasm-dired-moved) (error "Renamed file missing"))
          (dired-delete-file wasm-dired-moved nil nil)
          (revert-buffer nil t)
          (when (dired-goto-file wasm-dired-moved) (error "Deleted file remains listed"))''',
          "FILES-PASS")
        # Only remove this test's uniquely named fixture, never a shared root.
        check(editor, '''(dired "~")
          (dired-delete-file wasm-dired-fixture 'always nil)
          (revert-buffer nil t)
          (when (file-exists-p wasm-dired-fixture) (error "Fixture still exists"))''',
          "CLEANUP-PASS")
        print("PASS: browser Dired creates directories, opens, renames, and deletes files")
        check(editor, '''(dired "/")
          (unless (dired-goto-file "/neomacs") (error "Root lacks runtime mount"))
          (dired-find-file)
          (unless (equal default-directory "/neomacs/") (error "Did not enter runtime mount"))
          (unless (dired-goto-file "/neomacs/lisp") (error "Runtime lacks Lisp directory"))
          (dired-find-file)
          (unless (dired-goto-file "/neomacs/lisp/dired.el") (error "Missing bundled dired.el"))
          (revert-buffer nil t)
          (unless (dired-goto-file "/neomacs/lisp/dired.el") (error "Refresh lost dired.el"))''',
          "RUNTIME-PASS")
        print("PASS: browser Dired navigates and refreshes immutable runtime directories")
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
