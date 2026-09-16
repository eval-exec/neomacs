#!/usr/bin/env python3
"""Check the published landing page in Chrome with ordinary browser settings."""

import argparse
import json
from pathlib import Path
from tempfile import TemporaryDirectory
import time
from urllib.error import URLError
from urllib.request import urlopen

from selenium import webdriver

from browser_basic_smoke import exercise_editing_and_persistence
from browser_test_support import BrowserEditorHarness, chrome_options


def wait_for_release(url, expected, timeout):
    """Allow Pages' CDN to finish publishing, without testing an older release."""
    deadline = time.monotonic() + timeout
    observed = "not fetched"
    while time.monotonic() < deadline:
        try:
            with urlopen(url + "manifest.json", timeout=15) as response:
                observed = json.load(response)["bundle_id"]
            if observed == expected:
                return
        except (URLError, TimeoutError, ValueError, KeyError) as error:
            observed = str(error)
        time.sleep(2)
    raise AssertionError(f"Pages did not publish {expected}; last observed: {observed}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", required=True)
    parser.add_argument("--expected-bundle", required=True)
    parser.add_argument("--artifacts-dir", required=True)
    parser.add_argument("--timeout", type=float, default=180)
    args = parser.parse_args()
    url = args.url.rstrip("/") + "/"
    artifacts = Path(args.artifacts_dir)
    artifacts.mkdir(parents=True, exist_ok=True)
    wait_for_release(url, args.expected_bundle, args.timeout)
    scratch = Path(__file__).resolve().parents[3] / "tmp"
    scratch.mkdir(exist_ok=True)
    with TemporaryDirectory(prefix="pages-chrome-", dir=scratch) as profile:
        options = chrome_options(None, True)
        options.add_argument(f"--user-data-dir={profile}")
        options.add_experimental_option("excludeSwitches", ["disable-popup-blocking"])

        def make_driver():
            driver = webdriver.Chrome(options=options)
            driver.set_window_size(1800, 1100)
            return driver

        editor = BrowserEditorHarness(make_driver(), args.timeout)
        try:
            editor.install_frame_observer()
            editor.driver.get(url)
            title = editor.driver.find_element("id", "browser-window-title")
            assert "NEO Emacs (WebAssembly build)" in title.text, title.text
            warning = editor.driver.find_element("id", "browser-build-warning")
            assert warning.is_displayed()
            assert warning.text == "EXPERIMENTAL · INCOMPLETE · WORK IN PROGRESS", warning.text
            assert "landing page still needs substantial polish" in title.text, title.text
            link = title.find_element("tag name", "a")
            assert link.get_attribute("href") == "https://github.com/eval-exec/neomacs"
            assert link.get_attribute("target") == "_blank"
            editor.wait_ready()
            editor.wait_for_frame_text("landing", contains="Welcome to the browser editor.")
            editor.wait_for_window_matrices("About", contains="@eval-exec", count=1)
            assert not editor.driver.find_element("id", "browser-startup").is_displayed()
            assert warning.is_displayed(), "The warning disappeared with the startup overlay"
            rotation = editor.driver.execute_script(r"""
              const icon = document.querySelector('#browser-window-icon');
              const animation = icon.getAnimations()[0];
              if (!animation) return null;
              animation.pause();
              animation.currentTime = animation.effect.getTiming().duration / 4;
              const matrix = new DOMMatrix(getComputedStyle(icon).transform);
              animation.play();
              return {b: matrix.b, c: matrix.c};
            """)
            assert rotation and rotation["b"] > 0.99 and rotation["c"] < -0.99, rotation
            editor.driver.execute_cdp_cmd("Emulation.setEmulatedMedia", {
                "features": [{"name": "prefers-reduced-motion", "value": "reduce"}],
            })
            assert editor.driver.execute_script(r"""
              return document.querySelector('#browser-window-icon').getAnimations().length === 0;
            """), "The icon ignores reduced-motion preferences"
            editor.driver.execute_cdp_cmd("Emulation.setEmulatedMedia", {"features": []})
            loaded = editor.driver.execute_script(r"""
              return [...document.scripts].some(script => script.src.includes(arguments[0]))
                || performance.getEntriesByType('resource').some(entry =>
                  entry.name.includes('/builds/' + arguments[0] + '/main.js'));
            """, args.expected_bundle)
            assert loaded, "The page loaded a different release than the manifest"
            editor.driver.save_screenshot(str(artifacts / "landing.png"))
            editor.eval_expression('''(progn
              (unless (and (memq 'doom-one custom-enabled-themes)
                           tab-bar-mode
                           (get-buffer "*Playgorund*")
                           (= 33 (window-total-width (get-buffer-window "*About*"))))
                (error "Landing layout is incomplete"))
              (message (concat "PAGES-LAYOUT-" "PASSED")))''', "PAGES-LAYOUT-PASSED")
            errors = editor.driver.execute_script("return globalThis.__neomacsConsoleErrors")
            assert not errors, errors
            exercise_editing_and_persistence(editor, make_driver)
            errors = editor.driver.execute_script("return globalThis.__neomacsConsoleErrors")
            assert not errors, errors
            print(f"PASS: {url} renders release {args.expected_bundle}; title, layout, "
                  "editing, save and browser-restart persistence work", flush=True)
        except Exception:
            editor.capture_failure_artifacts(str(artifacts))
            raise
        finally:
            editor.driver.quit()


if __name__ == "__main__":
    main()
