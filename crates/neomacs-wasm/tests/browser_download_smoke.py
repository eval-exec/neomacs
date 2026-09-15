#!/usr/bin/env python3
"""Observe the real startup UI with cold, throttled downloads."""
import argparse
from pathlib import Path

from selenium import webdriver
from selenium.webdriver.support.ui import WebDriverWait
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
              phases: Array.from(document.querySelectorAll('#browser-startup-log > li'), e => ({
                id: e.dataset.phase, state: e.dataset.state,
                checked: e.querySelector('input').checked,
                active: e.querySelector('input').indeterminate,
                details: e.querySelectorAll('.phase-details > li').length,
              })),
              owner: progress.closest('[data-phase]')?.dataset.phase,
              vertical: (() => {
                const rows = Array.from(document.querySelectorAll('#browser-startup-log > li'));
                return rows.every((row, i) => i === 0 || row.getBoundingClientRect().top >= rows[i-1].getBoundingClientRect().bottom);
              })(),
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
        WebDriverWait(driver, 120, poll_frequency=0.05).until(lambda browser: browser.execute_script(
            "return document.querySelector('#browser-progress')?.getClientRects().length > 0"))
        artifact_dir = Path(args.artifacts_dir)
        artifact_dir.mkdir(parents=True, exist_ok=True)
        driver.save_screenshot(str(artifact_dir / "downloading.png"))
        editor.wait_ready()
        editor.wait_for_presentation()
        progress = driver.find_element("id", "browser-progress")
        assert not progress.is_displayed(), driver.execute_script(
            "const e = arguments[0]; return {hidden: e.hidden, display: getComputedStyle(e).display, bounds: e.getBoundingClientRect().toJSON()};",
            progress,
        )
        for element_id in ("browser-startup", "browser-startup-log", "browser-status", "browser-progress-label"):
            element = driver.find_element("id", element_id)
            assert not element.is_displayed(), f"{element_id} remains visible after startup"
            assert driver.execute_script("return arguments[0].getClientRects().length", element) == 0
        states = driver.execute_script("return globalThis.downloadStates")
        expected = ["page", "release", "frontend-modules", "frontend-download", "frontend-init",
                    "worker-start", "worker-download", "storage", "worker-compile", "worker-probe", "packages",
                    "verify-image", "verify-resources", "unpack", "restore", "mounts", "configure",
                    "lisp", "first-frame"]
        initial = next(s for s in states if s["phases"])
        assert [p["id"] for p in initial["phases"]] == expected
        assert any(p["state"] == "pending" for p in initial["phases"])
        assert all(p["state"] == "done" and p["checked"] and p["details"] > 0 for p in states[-1]["phases"]), states[-1]
        assert all(s["vertical"] for s in states), "phases must form one vertical list"
        download_phases = ("frontend-download", "worker-download", "packages")
        for phase in download_phases:
            assert any(s["displayed"] and "MiB" in s["label"] and any(
                p["id"] == phase and p["active"] for p in s["phases"]) and s["owner"] == phase for s in states), states
        assert any(sum(p["active"] for p in s["phases"]) > 1 for s in states), "overlapping phases should remain active"
        for state in states:
            if state["phases"] and all(p["state"] == "done" for p in state["phases"] if p["id"] in download_phases):
                assert not state["displayed"], state
        assert states[-1]["hidden"], states
        driver.save_screenshot(str(artifact_dir / "ready.png"))
        # Delayed messages from the worker must not revive the completed UI.
        driver.execute_script(r"""
          for (const data of [
            {type: 'status', phase: 'download'},
            {type: 'progress', received: 1, total: 10, complete: false},
            {type: 'progress', received: 10, total: 10, complete: true},
            {type: 'startup-phase', phase: 'worker-download', state: 'active'},
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
        assert not driver.find_element("id", "browser-startup-log").is_displayed()
        print("PASS: complete startup checklist, overlapping phases, text before bar, and terminal UI lifetime")
    except Exception:
        editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
