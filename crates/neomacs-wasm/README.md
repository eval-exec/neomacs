# Browser preview

Build and package the editor from the repository root (use fresh output paths):

```sh
cargo xtask fresh-build --release --portable-seed --portable-runtime-image tmp/editor.portable
cargo xtask package-portable-assets --portable-runtime-image tmp/editor.portable --output-dir tmp/editor-assets
cargo xtask build-wasm --portable-assets tmp/editor-assets --output-dir tmp/editor-browser
python crates/neomacs-wasm/tools/preview.py --directory tmp/editor-browser
```

Open <http://127.0.0.1:4173/>. The preview server binds only to loopback.
Startup shows the full phase checklist immediately: pending, in progress,
done, or failed. Checkboxes are read-only. Download and compilation phases
can be active simultaneously. Phases form one top-to-bottom list. Each
download bar appears in its phase's heading after the text (wrapping within
that phase on narrow screens). Indented sublists record start/completion times,
durations, and available byte counts, mount paths, and frame settings. When a
response has no usable size (including compressed transfers), the bar is
indeterminate and displays received bytes without inventing a percentage.
The checklist, status, and bar share one overlay. The first visible editor frame removes
that entire overlay from layout and painting immediately; late download
messages cannot restore it. Startup failures retain the checklist for diagnosis;
later runtime failures show a text-only error without reviving the checklist.
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

## Personal initialization and shipped defaults

Edit `~/.emacs.d/init.el` inside the editor (`C-x C-f`), save it, and reload the
page. Its browser filesystem path is `/neomacs-fake/.emacs.d/init.el`; it lives
in persistent origin-private storage, not in the source repository. We never
seed or overwrite it. GNU startup's ordinary init discovery also retains its
`early-init.el`, legacy `~/.emacs`, and error-reporting behavior.

The source-controlled defaults live separately in `lisp/neomacs-wasm/`:

- `neomacs-wasm-startup.el`: browser policy and startup-profile selection.
- `neomacs-wasm-packages.el`: available package defaults (currently bundled
  which-key; personal init can disable it).
- `neomacs-wasm-landing.el`: optional welcome/playground window layout.

The worker mounts runtime resources and OPFS first, loads shipped defaults,
then enters shared Emacs startup. Shared startup loads personal initialization
and runs the window-setup hook after frame settings. No UI-thread evaluator or
second init-file loader is involved. The landing hook removes itself after
running and does not take ownership of subsequent resizes or window changes.
An init-selected buffer (`initial-buffer-choice`) or existing custom window
layout takes precedence over automatic landing setup.

Normal editor startup remains the default. To opt into the landing profile:

```elisp
(setq neomacs-wasm-startup-profile 'landing)
(setq neomacs-wasm-landing-personal-info "Your own introduction here.\n")
;; Optional: override the bundled default.
;; (which-key-mode -1)
```

Wide layouts show welcome and an empty `emacs-lisp-mode` playground side by
side, with an optional personal sidebar on sufficiently wide frames. Narrow
layouts show welcome; the other buffers remain accessible with `C-x b`.
`M-x neomacs-wasm-landing-open` reopens the layout without erasing playground
edits. Set the profile to `editor` to retain ordinary scratch-buffer startup.

This is the startup/configuration foundation: the separate pinned Treemacs and
doom-themes download/cache bundle is **not implemented yet**. Neither package
is vendored or fetched by these Lisp modules. `M-x load-theme` lists themes
already present in the runtime; no default theme is changed here.

## Basic acceptance checks

Install `tests/requirements.txt` in a virtual environment under `tmp/`, then run:

```sh
python crates/neomacs-wasm/tests/browser_basic_smoke.py --browser chrome --headless --artifacts-dir tmp/chrome-basic
python crates/neomacs-wasm/tests/browser_basic_smoke.py --browser firefox --headless --artifacts-dir tmp/firefox-basic
python crates/neomacs-wasm/tests/browser_init_smoke.py --headless --artifacts-dir tmp/browser-init
node --test crates/neomacs-wasm/web/*.test.mjs
cargo test -p xtask wasm
python crates/neomacs-wasm/tests/browser_download_smoke.py --artifacts-dir tmp/download-smoke
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
