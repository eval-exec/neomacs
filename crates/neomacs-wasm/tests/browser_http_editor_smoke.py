#!/usr/bin/env python3
"""Exercise Lisp URL retrieval and EWW in the packaged browser editor.

The fixture is a separate, CORS-enabled origin; no public Internet dependency
or browser security override is needed.
"""

from __future__ import annotations

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import threading

from selenium import webdriver

from browser_test_support import BrowserEditorHarness, chrome_options


class Fixture(BaseHTTPRequestHandler):
    def do_GET(self) -> None:
        if self.path == "/redirect":
            self.send_response(302)
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Location", "/page")
            self.end_headers()
            return
        body = (
            b"<!doctype html><html><head><title>HTTP fixture</title></head>"
            b"<body><p>NEOMACS-EWW-CONTENT</p></body></html>"
            if self.path == "/page" else b"neomacs-http-body"
        )
        if self.path == "/archive-contents":
            body = b'(1 (http-fixture . [(1 0) nil "HTTP fixture package" single]))'
        self.send_response(200)
        self.send_header("Access-Control-Allow-Origin", "*")
        self.send_header("Content-Type", "text/html; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        try:
            self.wfile.write(body)
        except (BrokenPipeError, ConnectionResetError):
            # The cancellation check may abort while this response is sent.
            pass

    def log_message(self, format: str, *args: object) -> None:
        pass


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--url", required=True)
    parser.add_argument("--chrome")
    parser.add_argument("--headless", action="store_true")
    parser.add_argument("--timeout", type=float, default=90)
    parser.add_argument("--artifacts-dir")
    args = parser.parse_args()
    fixture = ThreadingHTTPServer(("127.0.0.1", 0), Fixture)
    thread = threading.Thread(target=fixture.serve_forever, daemon=True)
    thread.start()
    origin = f"http://127.0.0.1:{fixture.server_port}"
    driver = webdriver.Chrome(options=chrome_options(args.chrome, args.headless))
    editor = BrowserEditorHarness(driver, args.timeout)
    try:
        editor.install_frame_observer()
        driver.get(args.url)
        editor.wait_ready()
        editor.wait_for_presentation()
        editor.eval_expression(
            f'''(url-retrieve "{origin}/body"
                 (lambda (status)
                   (let ((ok (and (not (plist-get status :error))
                                  (search-forward "neomacs-http-body" nil t))))
                     (kill-buffer (current-buffer))
                     (message (concat "HTTP-ASYNC-" (if ok "PASS" "FAIL"))))))''',
            "HTTP-ASYNC-PASS", "HTTP-ASYNC-FAIL",
        )
        print("PASS: asynchronous url-retrieve through browser Fetch", flush=True)
        editor.eval_expression(
            f'''(condition-case err
                   (let ((buffer (url-retrieve-synchronously "{origin}/body" t t 10)))
                     (unless buffer (error "no response"))
                     (with-current-buffer buffer
                       (goto-char (point-min))
                       (unless (search-forward "neomacs-http-body" nil t)
                         (error "wrong response")))
                     (kill-buffer buffer)
                     (message (concat "HTTP-SYNC-" "PASS")))
                 (error (message (concat "HTTP-SYNC-" "FAIL: %S") err)))''',
            "HTTP-SYNC-PASS", "HTTP-SYNC-FAIL",
        )
        print("PASS: synchronous URL retrieval yields to browser Fetch", flush=True)
        editor.eval_expression(
            f'''(progn
                 (setq http-smoke-cancelled-callback nil)
                 (let ((buffer (url-retrieve "{origin}/body"
                                 (lambda (_status)
                                   (setq http-smoke-cancelled-callback t)))))
                   (kill-buffer buffer))
                 (sleep-for 0.05)
                 (message (concat "HTTP-CANCEL-"
                            (if http-smoke-cancelled-callback "FAIL" "PASS"))))''',
            "HTTP-CANCEL-PASS", "HTTP-CANCEL-FAIL",
        )
        print("PASS: killing a URL buffer suppresses its callback", flush=True)
        # Use the worker's volatile temporary filesystem. Closing this browser
        # session discards it; recursive filesystem removal is a separate test.
        editor.eval_expression(
            f'''(condition-case err
                   (progn
                     (require 'package)
                     (let ((package-archives '(("fixture" . "{origin}/")))
                           (package-archive-contents nil)
                           (package-check-signature nil)
                           (package-user-dir (make-temp-file "http-packages-" t)))
                       (package-refresh-contents)
                       (unless (assq 'http-fixture package-archive-contents)
                         (error "archive not loaded")))
                     (message (concat "HTTP-PACKAGE-" "PASS")))
                 (error (message (concat "HTTP-PACKAGE-" "FAIL: %S") err)))''',
            "HTTP-PACKAGE-PASS", "HTTP-PACKAGE-FAIL",
        )
        print("PASS: package-refresh-contents reads a browser HTTP archive", flush=True)
        editor.eval_expression(
            f'''(progn (require 'eww) (eww "{origin}/redirect"))''',
            "NEOMACS-EWW-CONTENT",
        )
        print("PASS: EWW renders browser HTTP after redirect", flush=True)
    except Exception:
        if args.artifacts_dir:
            editor.capture_failure_artifacts(args.artifacts_dir)
        raise
    finally:
        driver.quit()
        fixture.shutdown()
        fixture.server_close()
        thread.join()


if __name__ == "__main__":
    main()
