#!/usr/bin/env python3
"""Verify replacement-save failure isolation with real Worker OPFS streams.

No editor build is required. Faults are injected at the browser storage API;
the production adapter, staging/abort implementation, and reads are real.
"""

import argparse
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from threading import Thread

from selenium import webdriver
from browser_test_support import chrome_options


class FixtureHandler(BaseHTTPRequestHandler):
    def log_message(self, *_args):
        pass

    def do_GET(self):
        if self.path == "/opfs-storage.mjs":
            body = (Path(__file__).resolve().parents[1] / "web/opfs-storage.mjs").read_bytes()
            kind = "text/javascript"
        else:
            body, kind = b"<!doctype html><title>OPFS write contract</title>", "text/html"
        self.send_response(200)
        self.send_header("Content-Type", kind)
        self.end_headers()
        self.wfile.write(body)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--chrome")
    args = parser.parse_args()
    server = ThreadingHTTPServer(("127.0.0.1", 0), FixtureHandler)
    thread = Thread(target=server.serve_forever, daemon=True)
    thread.start()
    try:
        with webdriver.Chrome(options=chrome_options(args.chrome, True)) as driver:
            driver.set_script_timeout(30)
            driver.get(f"http://127.0.0.1:{server.server_port}/")
            result = driver.execute_async_script(r"""
              const done = arguments[arguments.length - 1];
              const moduleUrl = new URL('/opfs-storage.mjs', location.href).href;
              const script = `
                import { OriginPrivateFileSystem, WRITE_MODE } from ${JSON.stringify(moduleUrl)};
                (async () => {
                  const root = await navigator.storage.getDirectory();
                  const name = 'save-contract-' + crypto.randomUUID();
                  const directory = await root.getDirectoryHandle(name, {create: true});
                  const fs = await OriginPrivateFileSystem.open({getDirectory: async () => directory});
                  const encode = s => new TextEncoder().encode(s);
                  const read = async () => new TextDecoder().decode(await fs.read('/notes'));
                  const request = {mode: WRITE_MODE.TRUNCATE, offset: 0, sync: true};
                  const original = FileSystemFileHandle.prototype.createWritable;
                  try {
                    await fs.write('/notes', encode('previous notes'), request);
                    for (const failAt of ['write', 'close']) {
                      FileSystemFileHandle.prototype.createWritable = async function(options) {
                        const stream = await original.call(this, options);
                        return {
                          write: async data => {
                            if (failAt === 'write') throw new DOMException('fixture quota', 'QuotaExceededError');
                            await stream.write(data);
                          },
                          close: async () => { throw new DOMException('fixture quota', 'QuotaExceededError'); },
                          abort: () => stream.abort(),
                        };
                      };
                      let rejected = false;
                      try { await fs.write('/notes', encode('replacement'), request); }
                      catch (error) { rejected = error.name === 'QuotaExceededError'; }
                      if (!rejected || await read() !== 'previous notes') throw Error(failAt + ' destroyed previous contents');
                    }
                    FileSystemFileHandle.prototype.createWritable = original;
                    await fs.write('/notes', encode('new'), request);
                    if (await read() !== 'new') throw Error('replacement retained old tail');
                    await fs.write('/notes', encode('!'), {mode: WRITE_MODE.APPEND, offset: 0, sync: true});
                    if (await read() !== 'new!') throw Error('append failed');
                  } finally {
                    FileSystemFileHandle.prototype.createWritable = original;
                    await root.removeEntry(name, {recursive: true});
                  }
                  postMessage({ok: true});
                })().catch(error => postMessage({error: String(error)}));
              `;
              const url = URL.createObjectURL(new Blob([script], {type: 'text/javascript'}));
              const worker = new Worker(url, {type: 'module'});
              worker.onmessage = event => { worker.terminate(); URL.revokeObjectURL(url); done(event.data); };
              worker.onerror = event => { worker.terminate(); URL.revokeObjectURL(url); done({error: event.message}); };
            """)
            if result != {"ok": True}:
                raise RuntimeError(result)
            print("PASS: real Worker OPFS preserves old contents after write/close failure; replacement and append work")
    finally:
        server.shutdown()
        server.server_close()
        thread.join()


if __name__ == "__main__":
    main()
