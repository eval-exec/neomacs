#!/usr/bin/env python3
"""Observe the real startup UI with cold, throttled downloads."""
import argparse

from selenium import webdriver
from browser_test_support import BrowserEditorHarness, chrome_options


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", default="http://127.0.0.1:4173/")
    parser.add_argument("--chrome")
    parser.add_argument("--artifacts-dir", required=True)
    args = parser.parse_args()
    driver = webdriver.Chrome(options=chrome_options(args.chrome, True))
    editor = BrowserEditorHarness(driver, 120)
    try:
        driver.execute_cdp_cmd("Network.enable", {})
        driver.execute_cdp_cmd("Network.setCacheDisabled", {"cacheDisabled": True})
        driver.execute_cdp_cmd("Network.emulateNetworkConditions", {
            "offline": False, "latency": 10,
            "downloadThroughput": 4 * 1024 * 1024,
            "uploadThroughput": 4 * 1024 * 1024,
        })
        driver.execute_cdp_cmd("Page.addScriptToEvaluateOnNewDocument", {"source": r"""
          globalThis.downloadStates = [];
          new MutationObserver(() => {
            const status = document.querySelector('#browser-status');
            const progress = document.querySelector('#browser-progress');
            if (!status || !progress) return;
            const state = {text: status.textContent, hidden: progress.hidden,
              label: document.querySelector('#browser-progress-label').textContent};
            const previous = globalThis.downloadStates.at(-1);
            if (JSON.stringify(previous) !== JSON.stringify(state)) {
              globalThis.downloadStates.push(state);
            }
          }).observe(document, {subtree: true, childList: true,
            attributes: true, characterData: true});
        """})
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        states = driver.execute_script("return globalThis.downloadStates")
        for phase in ("Downloading editor frontend…", "Downloading editor and runtime assets…"):
            assert any(s["text"] == phase and not s["hidden"] and "MiB" in s["label"] for s in states), states
        starting = [s for s in states if s["text"] == "Starting NEO Emacs…"]
        assert starting and all(s["hidden"] for s in starting), states
        assert states[-1]["hidden"], states
        print("PASS: cold downloads show byte progress; initialization and ready hide the bar")
    except Exception:
        editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
