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

## Landing content

The landing site consists of ordinary Org files in `etc/neomacs-landing/`,
included by the existing portable runtime packager. `index.org` is visited as
a file; relative Org links and Treemacs open the other documents. The packaged
site is read-only. Treemacs shows it as **NEO Emacs**, separately from **Your
files**, the persistent browser home at `/neomacs-fake/`.

On first use, `playground.el` is copied into browser home without overwriting
an existing file. Its editable buffer shows line numbers; `C-x C-s` saves it.
The `neo:copy` action copies a site document to a user-chosen destination and
refuses to overwrite existing files. Named `neo:` actions are explicitly
dispatched, not evaluated as arbitrary Lisp. Styling and layout remain in
`lisp/neomacs-wasm/neomacs-wasm-landing.el`; content changes need no Rust edits.
Fido vertical completion is enabled before personal init, which may override it.
Org's in-memory parser cache remains enabled, but its persistent disk cache is
disabled by default: Org's temporary-file rename crosses the browser's `/tmp`
and persistent-home mounts. This does not disable saving Org documents.

## Font resources

Browser presentation protocol v6 transfers immutable font bytes as binary
resources, then references them from frame-local font bindings. Resources are
scoped by catalog generation and asset identity; changed bytes get fresh IDs.
Every packet is decoded in order before render-frame coalescing. The transport
cache keeps only the latest frame's resources, while older presentations retain
their own shared font ownership. A transport or post-decode validation failure
terminates the stream; it cannot silently skip a resource-bearing packet.

## Nerd Icons

The landing profile uses `nerd-icons`, `treemacs-nerd-icons`,
`nerd-icons-dired`, and `nerd-icons-completion` for the sidebar, tab labels,
Dired, and minibuffer candidates. Their pinned Git
sources and `Symbols Nerd Font Mono` font are downloaded as part of the package
bundle, not vendored in this repository or installed on the user's system.
The worker validates and registers the font before editor startup; glyphs use
the same font selection, shaping, and shared replay transport as ordinary text.
`lisp/neomacs-wasm/neomacs-wasm-icons.el` owns the Lisp integration, which runs
before personal init so users can override it.

`C-x b` keeps `consult-buffer` and shows icons for its buffer candidates;
`C-x C-f` keeps ordinary `find-file` and shows file and directory icons.
Both use the existing Fido vertical completion interface and bundled font.
No Marginalia, replacement completion frontend, or system font installation
is required. Personal init can disable completion icons with
`(nerd-icons-completion-mode -1)`.

## Inline images

Inline `:data` images use the shared native decoder in the editor worker and
the shared wgpu image cache in the frontend. Redisplay queues missing images;
decoding happens at the worker's wait boundary (or during an explicit Lisp
`image-size` query). Pixels transfer once per realization, not on each redraw.
The landing banner reads `assets/banner.svg`, packaged at
`etc/images/neomacs-banner.svg`, through the editor filesystem. Its display
property fits the visible Org pane and scrolls with the buffer.

Packaged runtime images also support direct `:file` specs. The worker shares the evaluator's immutable resource
store; it does not copy the entire runtime bundle. User-file images still need
to be read through Lisp and passed as `:data`; external SVG resources are not
supported. It uses a 32 MiB resident-image admission limit; `image-flush` and
`clear-image-cache` release entries. It does not implement automatic eviction
or animation-cache invalidation yet. Decoding is CPU work in the worker;
cached drawing is GPU work, not a zero-copy pipeline.

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
- `neomacs-wasm-packages.el`: Treemacs, doom-themes, and bundled which-key
  defaults; personal init can override them.
- `neomacs-wasm-landing.el`: the default welcome/playground window layout.

The worker mounts runtime resources and OPFS first, loads shipped defaults,
then enters shared Emacs startup. Shared startup loads personal initialization
and runs the window-setup hook after frame settings. No UI-thread evaluator or
second init-file loader is involved. The landing hook removes itself after
running and does not take ownership of subsequent resizes or window changes.
An init-selected buffer (`initial-buffer-choice`) or existing custom window
layout takes precedence over automatic landing setup.

The WASM build opens the landing page automatically, without generating an
init file. Personal configuration is optional:

```elisp
(setq neomacs-wasm-landing-personal-info "Your own introduction here.\n")
;; To opt out of the landing page:
;; (setq neomacs-wasm-startup-profile 'editor)
;; Optional: override the bundled default.
;; (which-key-mode -1)
```

Wide layouts show Treemacs on the left, an Org-mode introduction and an
`emacs-lisp-mode` playground with runnable examples in the center, and a personal
sidebar on the right. The playground cursor starts after `(+ 1 2)`: press
`C-x C-e` to evaluate it. Examples are inserted only once. Medium layouts
omit the personal sidebar and then Treemacs as space decreases. Narrow
layouts show welcome; the other buffers remain accessible with `C-x b`.
`M-x neomacs-wasm-landing-open` reopens the layout without erasing playground
edits. Set the profile to `editor` to retain ordinary scratch-buffer startup.

The welcome buffer uses local face remapping: proportional sans-serif prose,
contrasting serif Org headings, and heading scales of 1.8×, 1.4×, and 1.15×
for levels one through three. The playground keeps its monospace font.
Customize `neomacs-wasm-landing-body` and `neomacs-wasm-welcome-heading` to
choose other families. The portable font catalog supplies Hack, Ubuntu Light,
and Noto Serif Regular from pinned font dependencies; it does not discover
the browser host's installed fonts.

The About sidebar also uses the proportional body face. Browser wheel input
uses ordinary Emacs wheel commands, including `mouse-wheel-follow-mouse` and
`mouse-wheel-scroll-amount`. Small pixel deltas accumulate into wheel gestures;
this is discrete scrolling, not pixel-precision scrolling. Horizontal-only
wheel gestures are not translated into vertical movement.

`cargo xtask build-wasm` fetches exact Git objects from `packages.lock.toml`
into ignored `target/wasm-package-sources/`. Treemacs uses the upstream 3.2
release; dependencies and doom-themes are pinned too. Sources, icons, and
available license files form a separate deterministic `packages.bundle`.
No third-party source is tracked by git or written into the user's home.

The browser downloads that optional bundle during the package startup phase,
verifies SHA-256, and caches it by digest using Cache Storage. Each reuse is
verified again; Rust validates the archive and mounts it read-only alongside
the core runtime. Package files cannot replace core files. A download failure
starts the basic landing page; reload retries. Clearing browser site data also
clears this cache. Old package versions may remain until site data is cleared.

Treemacs browses `/neomacs-fake`; Git, Python collapsing, and file watchers are
disabled because the browser has no native subprocesses. which-key is enabled.
Its initial idle delay is 0.5 seconds; subsequent popups in the same key
sequence use 0.1 seconds. Personal init may override both settings.
The default design uses `doom-one`, a frame tab bar, and buffer tab
lines, with landing-pane header lines disabled. Keycast (`keycast-tab-bar-mode`) shows
command feedback in the tab bar. It and its dependencies are pinned in the
same optional package bundle. The welcome page's theme action selects from
installed themes. Personal init runs after these defaults and can change the
theme or disable any of these modes.
The standard Emacs mode line is retained; Doom Modeline is not bundled or enabled.

## Basic acceptance checks

Install `tests/requirements.txt` in a virtual environment under `tmp/`, then run:

```sh
python crates/neomacs-wasm/tests/browser_basic_smoke.py --browser chrome --headless --artifacts-dir tmp/chrome-basic
python crates/neomacs-wasm/tests/browser_basic_smoke.py --browser firefox --headless --artifacts-dir tmp/firefox-basic
python crates/neomacs-wasm/tests/browser_init_smoke.py --headless --artifacts-dir tmp/browser-init
python crates/neomacs-wasm/tests/browser_treemacs_smoke.py --headless --artifacts-dir tmp/browser-treemacs
python crates/neomacs-wasm/tests/browser_completion_icons_smoke.py --headless --artifacts-dir tmp/browser-completion-icons
python crates/neomacs-wasm/tests/browser_init_smoke.py --headless --block-packages tmp/wasm-dist --artifacts-dir tmp/browser-fallback
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
