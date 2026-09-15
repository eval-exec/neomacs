#!/usr/bin/env python3
"""Observe the real startup UI with cold, throttled downloads."""
import argparse
from pathlib import Path

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
          const OriginalWorker = globalThis.Worker;
          globalThis.Worker = class extends OriginalWorker {
            constructor(url, options) {
              super(url, options);
              if (String(url).endsWith('/editor-worker.js')) globalThis.startupWorker = this;
            }
          };
          new MutationObserver(() => {
            const status = document.querySelector('#browser-status');
            const progress = document.querySelector('#browser-progress');
            if (!status || !progress) return;
            const state = {text: status.textContent, hidden: progress.hidden,
              displayed: progress.getClientRects().length > 0,
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
        progress = driver.find_element("id", "browser-progress")
        assert not progress.is_displayed(), driver.execute_script(
            "const e = arguments[0]; return {hidden: e.hidden, display: getComputedStyle(e).display, bounds: e.getBoundingClientRect().toJSON()};",
            progress,
        )
        for element_id in ("browser-startup", "browser-status", "browser-progress-label"):
            element = driver.find_element("id", element_id)
            assert not element.is_displayed(), f"{element_id} remains visible after startup"
            assert driver.execute_script("return arguments[0].getClientRects().length", element) == 0
        states = driver.execute_script("return globalThis.downloadStates")
        for phase in ("Downloading editor frontend…", "Downloading editor and runtime assets…"):
            assert any(s["text"] == phase and s["displayed"] and "MiB" in s["label"] for s in states), states
        starting = [s for s in states if s["text"] == "Starting NEO Emacs…"]
        assert starting and all(not s["displayed"] for s in starting), states
        assert states[-1]["hidden"], states
        artifact_dir = Path(args.artifacts_dir)
        artifact_dir.mkdir(parents=True, exist_ok=True)
        driver.save_screenshot(str(artifact_dir / "ready.png"))
        # Delayed messages from the worker must not revive the completed UI.
        driver.execute_script(r"""
          for (const data of [
            {type: 'status', phase: 'download'},
            {type: 'progress', received: 1, total: 10, complete: false},
            {type: 'progress', received: 10, total: 10, complete: true},
          ]) globalThis.startupWorker.dispatchEvent(new MessageEvent('message', {data}));
        """)
        assert not driver.find_element("id", "browser-startup").is_displayed()
        assert driver.find_element("id", "browser-status").get_attribute("data-state") == "ready"
        # A genuine runtime failure still needs a visible, text-only status.
        driver.execute_script(r"""
          globalThis.startupWorker.dispatchEvent(new MessageEvent('message', {
            data: {type: 'failed', message: 'startup-lifetime-test-failure'},
          }));
        """)
        error_status = driver.find_element("id", "browser-status")
        assert error_status.is_displayed()
        assert "startup-lifetime-test-failure" in error_status.text
        assert not driver.find_element("id", "browser-progress").is_displayed()
        print("PASS: cold downloads show byte progress; initialization and ready hide the bar")
    except Exception:
        editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
