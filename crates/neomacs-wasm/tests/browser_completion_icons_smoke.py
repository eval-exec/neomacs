#!/usr/bin/env python3
"""Check Nerd Icons in actual browser minibuffer completion candidates."""

import argparse
from pathlib import Path
import time

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def assert_candidate_icons(editor, prompt, candidates):
    """Ignore icons in tabs/tree: inspect only the prompted minibuffer rows."""
    deadline = time.monotonic() + 10
    seen = set()
    while time.monotonic() < deadline:
        seen = set()
        frame = editor.frame_payload()
        for entry in frame.get("window_matrices", []):
            if prompt not in editor.matrix_text(entry):
                continue
            for row in entry["matrix"]["rows"]:
                glyphs = [glyph for area in row["glyphs"] for glyph in area]
                chars = [
                    (glyph, glyph["glyph_type"]["Char"]["ch"])
                    for glyph in glyphs
                    if isinstance(glyph["glyph_type"], dict)
                    and "Char" in glyph["glyph_type"]
                ]
                text = "".join(char for _, char in chars)
                for candidate in candidates:
                    start = text.find(candidate)
                    if start < 0:
                        continue
                    for glyph, char in chars[:start]:
                        codepoint = ord(char)
                        if not (0xE000 <= codepoint <= 0xF8FF
                                or 0xF0000 <= codepoint <= 0xFFFFD):
                            continue
                        face = frame["faces"][glyph["face_id"]]
                        font = frame["fonts"][face["default_resolved_font_id"]]
                        if font["family"] == "Symbols Nerd Font Mono":
                            seen.add(candidate)
        if seen == set(candidates):
            return
        time.sleep(0.1)
    raise AssertionError(
        f"Missing minibuffer icons for {set(candidates) - seen!r}; "
        f"frame={editor.frame_text()!r}"
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--binary")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir", required=True)
    args = parser.parse_args()
    artifacts = Path(args.artifacts_dir)
    artifacts.mkdir(parents=True, exist_ok=True)
    driver = webdriver.Chrome(options=chrome_options(args.binary, args.headless))
    driver.set_window_size(1800, 1100)
    editor = BrowserEditorHarness(driver, 90)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_frame_text("landing", contains="Welcome to the browser editor.")
        editor.finish_startup_frame_capture()
        editor.eval_expression(
            r'''(progn
                  (make-directory "/neomacs-fake/completion-icons/icon-folder" t)
                  (with-temp-file "/neomacs-fake/completion-icons/icon-code.el"
                    (insert ";; Completion icon file\n(+ 1 2)\n"))
                  (with-temp-file "/neomacs-fake/completion-icons/icon-notes.org"
                    (insert "* Completion icon notes\n"))
                  (setq default-directory "/neomacs-fake/completion-icons/")
                  (with-current-buffer (get-buffer-create "wasm-icons-code.el")
                    (emacs-lisp-mode))
                  (with-current-buffer (get-buffer-create "wasm-icons-notes.org")
                    (org-mode))
                  (get-buffer-create "wasm-icons-special")
                  (message (concat "ICON-FIXTURES-" "READY")))''',
            "ICON-FIXTURES-READY",
        )
        editor.type_native_control_key("x")
        editor.type_native_control_key("f")
        editor.wait_for_frame_text("file prompt", contains="Find file:")
        editor.commit_text("icon-")
        assert_candidate_icons(editor, "Find file:", [
            "icon-code.el", "icon-notes.org", "icon-folder/",
        ])
        driver.save_screenshot(str(artifacts / "file-icons.png"))
        editor.commit_text("code.el")
        editor.dispatch_key("Enter")
        editor.wait_for_frame_text("selected file", contains="Completion icon file")
        editor.eval_expression(
            r'''(message (concat "FILE-ICON-" "%s")
                         (if (equal buffer-file-name
                                    "/neomacs-fake/completion-icons/icon-code.el")
                             "PASS" "FAIL"))''',
            "FILE-ICON-PASS", failure_marker="FILE-ICON-FAIL",
        )
        print("PASS: C-x C-f renders file/directory icons and opens the unmodified file name")
        editor.type_native_control_prefix("x", "b")
        editor.wait_for_frame_text("Consult buffer prompt", contains="Switch to:")
        editor.commit_text("wasm-icons-")
        assert_candidate_icons(editor, "Switch to:", [
            "wasm-icons-code.el", "wasm-icons-notes.org", "wasm-icons-special",
        ])
        driver.save_screenshot(str(artifacts / "buffer-icons.png"))
        editor.commit_text("code.el")
        editor.dispatch_key("Enter")
        editor.eval_expression(
            r'''(message (concat "BUFFER-ICON-" "%s")
                         (if (equal (buffer-name) "wasm-icons-code.el")
                             "PASS" "FAIL"))''',
            "BUFFER-ICON-PASS", failure_marker="BUFFER-ICON-FAIL",
        )
        print("PASS: C-x b renders buffer icons and selects the unmodified buffer name")
    except Exception:
        editor.capture_failure_artifacts(str(artifacts))
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
