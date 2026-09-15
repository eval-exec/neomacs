# Browser preview

Build and package the editor from the repository root (use fresh output paths):

```sh
cargo xtask fresh-build --release --portable-seed --portable-runtime-image tmp/editor.portable
cargo xtask package-portable-assets --portable-runtime-image tmp/editor.portable --output-dir tmp/editor-assets
cargo xtask build-wasm --portable-assets tmp/editor-assets --output-dir tmp/editor-browser
python crates/neomacs-wasm/tools/preview.py --directory tmp/editor-browser
```

Open <http://127.0.0.1:4173/>. The preview server binds only to loopback.
Production hosting must use HTTPS, serve `.wasm` as `application/wasm`, and
send these headers on the page and worker resources:

```text
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

These headers enable shared-memory worker waits in browsers without JSPI.
Experimental browser flags are not required by the application. An available
WebGL/WebGPU graphics backend is still necessary.

## Storage and evaluator ownership

`Context` and Lisp evaluation live only in the editor worker. The filesystem
import ABI and the `OriginPrivateFileSystem` implementation are shared:

- With JSPI, filesystem imports suspend while the editor worker awaits OPFS.
- Otherwise, `web/storage/blocking.mjs` waits on a shared-memory reply while
  `web/storage/worker.js` performs the asynchronous OPFS operation. The storage
  worker never receives Lisp state or the evaluator's memory.
- Large replies negotiate a larger buffer without repeating the operation.
  A transport timeout stops that transport; mutations are never retried
  automatically when their completion is uncertain.

The virtual home is `/neomacs-fake`. Data belongs to the page's origin, not to
the desktop home directory. Changing host or port selects different storage;
clearing site data deletes it. Browser quota and eviction rules still apply.
File rename is reported as unsupported when the browser cannot provide the
required move operation; it is not silently replaced with copy-and-delete.

## Basic acceptance checks

Install `tests/requirements.txt` in a virtual environment under `tmp/`, then run:

```sh
python crates/neomacs-wasm/tests/browser_basic_smoke.py --browser chrome --headless --artifacts-dir tmp/chrome-basic
python crates/neomacs-wasm/tests/browser_basic_smoke.py --browser firefox --headless --artifacts-dir tmp/firefox-basic
node --test crates/neomacs-wasm/web/*.test.mjs
cargo test -p xtask wasm
```

Use `--binary PATH` to select an installed browser. Tests use isolated profiles
and no experimental browser preferences. The basic check covers startup,
trusted keyboard input, M-x, Org, splits, buffers, region deletion, kill/yank,
undo, interactive save, large reads, and persistence across browser restart
using the same temporary profile. `browser_opfs_smoke.py` separately checks
page reload persistence.
Separate `browser_theme_smoke.py`, `browser_hidpi_smoke.py`,
`browser_cursor_smoke.py`, and `browser_dired_smoke.py` exercise rendering and
directory operations. Passing kill/yank does not establish system-clipboard
integration.

Subprocesses and Magit are outside the current basic-editor milestone.
