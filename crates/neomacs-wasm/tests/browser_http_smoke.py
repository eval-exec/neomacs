#!/usr/bin/env python3
"""Test browser HTTP policy using two controlled origins and ordinary Chrome.

No editor build is required: this exercises the production HTTP adapter itself.
Uses the same Selenium requirements and Chrome options as editor smoke tests.
"""

from __future__ import annotations

import argparse
from contextlib import contextmanager
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

from selenium import webdriver

from browser_test_support import chrome_options


HTTP_MODULE = Path(__file__).resolve().parents[1] / "web/network/http.mjs"


class FixtureHandler(BaseHTTPRequestHandler):
    def log_message(self, _format, *_args):
        pass

    def do_GET(self):
        if self.path == "/":
            body = b"<!doctype html><title>Neomacs HTTP test</title>"
            content_type, status = "text/html", 200
        elif self.path == "/http.mjs":
            body = HTTP_MODULE.read_bytes()
            content_type, status = "text/javascript", 200
        else:
            body = bytes([0, 128, 255, 10])
            content_type, status = "application/octet-stream", 404
        self.send_response(status)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("X-Visible", "visible")
        self.send_header("X-Hidden", "hidden")
        if self.path == "/allowed":
            self.send_header("Access-Control-Allow-Origin", "*")
            self.send_header("Access-Control-Expose-Headers", "X-Visible")
        self.end_headers()
        self.wfile.write(body)


@contextmanager
def origin():
    server = ThreadingHTTPServer(("127.0.0.1", 0), FixtureHandler)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        yield f"http://127.0.0.1:{server.server_port}"
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


def exercise(driver, target):
    result = driver.execute_async_script(
        r"""
        const target = arguments[0];
        const done = arguments[arguments.length - 1];
        (async () => {
          const { fetchHttp } = await import('/http.mjs');
          const summarize = response => ({
            status: response.status,
            body: Array.from(response.body),
            headers: Object.fromEntries(response.headers),
          });
          const sameOrigin = summarize(await fetchHttp({url: location.origin + '/denied'}));
          const allowed = summarize(await fetchHttp({url: target + '/allowed'}));
          let blocked;
          try {
            await fetchHttp({url: target + '/denied'});
            blocked = {unexpectedSuccess: true};
          } catch (error) {
            blocked = {name: error.name};
          }
          return {sameOrigin, allowed, blocked};
        })().then(done, error => done({error: String(error)}));
        """,
        target,
    )
    assert "error" not in result, result
    for response in (result["sameOrigin"], result["allowed"]):
        assert response["status"] == 404, result
        assert response["body"] == [0, 128, 255, 10], result
        assert response["headers"]["x-visible"] == "visible", result
    assert result["sameOrigin"]["headers"]["x-hidden"] == "hidden", result
    assert "x-hidden" not in result["allowed"]["headers"], result
    assert result["blocked"] == {"name": "TypeError"}, result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--chrome")
    parser.add_argument("--headless", action="store_true")
    args = parser.parse_args()
    with origin() as page, origin() as target:
        with webdriver.Chrome(options=chrome_options(args.chrome, args.headless)) as driver:
            driver.set_script_timeout(20)
            driver.get(page)
            exercise(driver, target)
            print(f"PASS: Chrome {driver.capabilities['browserVersion']}: HTTP errors, binary bodies, CORS and filtered headers")


if __name__ == "__main__":
    main()
