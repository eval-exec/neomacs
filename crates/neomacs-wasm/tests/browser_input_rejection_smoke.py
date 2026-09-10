#!/usr/bin/env python3
"""Reject one malformed input batch and keep the real editor Worker usable."""

import argparse

from selenium import webdriver
from selenium.webdriver.support.ui import WebDriverWait

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
        driver.execute_cdp_cmd("Page.addScriptToEvaluateOnNewDocument", {
            "source": r"""
            const ObservedWorker = globalThis.Worker;
            globalThis.Worker = class extends ObservedWorker {
              postMessage(message, ...rest) {
                if (globalThis.__rejectNextText && message?.type === "input"
                    && message.batch.events.some(event => event.type === "text-committed")) {
                  globalThis.__rejectNextText = false;
                  message = { ...message, batch: { ...message.batch,
                    events: [{ type: "invalid-regression-probe" }] } };
                }
                return super.postMessage(message, ...rest);
              }
            };
            """,
        })
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        driver.execute_script("globalThis.__rejectNextText = true")
        editor.commit_text("discard this invalid batch")
        WebDriverWait(driver, 20).until(lambda driver: driver.execute_script(
            "return globalThis.__neomacsMessages.some(message => message.type === 'input-rejected')"
        ))
        editor.eval_expression(
            '(message (concat "INPUT-" "RECOVERED-%d") (+ 20 22))',
            "INPUT-RECOVERED-42",
        )
        receipts = driver.execute_script(
            "return globalThis.__neomacsMessages.filter(message => message.type === 'input-rejected')"
        )
        assert len(receipts) == 1, receipts
        assert "invalid browser input batch" in receipts[0]["message"], receipts
        print("PASS: malformed input rejected once; subsequent editor command succeeds")
    except Exception:
        if args.artifacts_dir:
            editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()


if __name__ == "__main__":
    main()
