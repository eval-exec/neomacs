#!/usr/bin/env python3
"""Verify the landing page's realized fonts, not just its Lisp face settings."""

import argparse
from pathlib import Path

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


def row_font(frame, text):
    for entry in frame["window_matrices"]:
        for row in entry["matrix"]["rows"]:
            glyphs = [glyph for area in row["glyphs"] for glyph in area]
            characters = [(glyph, glyph["glyph_type"].get("Char", {}).get("ch", ""))
                          for glyph in glyphs if isinstance(glyph["glyph_type"], dict)]
            if text not in "".join(char for _, char in characters):
                continue
            glyph = next(glyph for glyph, char in characters if char.isalpha())
            face = frame["faces"][glyph["face_id"]]
            return frame["fonts"][face["default_resolved_font_id"]]
    raise AssertionError(f"No rendered row containing {text!r}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--binary")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--artifacts-dir", required=True)
    args = parser.parse_args()
    driver = webdriver.Chrome(options=chrome_options(args.binary, args.headless))
    driver.set_window_size(1800, 1100)
    editor = BrowserEditorHarness(driver, 90)
    artifacts = Path(args.artifacts_dir)
    artifacts.mkdir(parents=True, exist_ok=True)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        editor.wait_for_frame_text("landing typography", contains="Make it your own")
        frame = editor.frame_payload()
        title = row_font(frame, "* NEO Emacs")
        section = row_font(frame, "Your first little spark")
        subsection = row_font(frame, "Make it your own")
        body = row_font(frame, "Welcome to the browser editor.")
        code = row_font(frame, '(message "Hello from Lisp')
        assert title["family"] == section["family"] == subsection["family"] == "Noto Serif"
        assert title["pixel_size"] > section["pixel_size"] > subsection["pixel_size"] > body["pixel_size"]
        assert body["family"] == "Ubuntu", body
        assert code["family"] == "Hack", code
        driver.save_screenshot(str(artifacts / "typography.png"))
        print("PASS: serif H1 > H2 > H3; proportional Ubuntu prose; monospace Hack code")
        print({name: (font["family"], font["pixel_size"])
               for name, font in [("H1", title), ("H2", section), ("H3", subsection),
                                  ("body", body), ("code", code)]})
    except Exception:
        editor.capture_failure_artifacts(str(artifacts))
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
