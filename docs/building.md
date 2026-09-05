# Building NEO Emacs from Source

Prebuilt binaries for Linux, macOS, and Windows are available on the
[releases page](https://github.com/eval-exec/neomacs/releases) — building from source
is only needed for development or unsupported platforms.

## Prerequisites

- **Rust** (stable, pinned via `rust-toolchain.toml` — rustup installs it automatically)
- **GStreamer** (optional, for video playback)
- **WPE WebKit** (optional, for the inline browser on Linux)
- **WebKit.framework** (macOS: the inline browser uses the system
  `WKWebView`; nothing to install)
- **VA-API** (optional, for hardware video decode on Linux)
- **GNU Emacs** (optional, for pre-compiling .el files — speeds up bootstrap ~17x)
- **SQLite** is built into Neomacs from the version bundled by `rusqlite`; no
  system SQLite package or runtime library is required

Build commands in this document are run from the repository root.

## Quick Start

```bash
# Optional (recommended): use the repo dev shell (handles all dependencies)
nix develop --accept-flake-config

# Build NEO Emacs (compiles Rust, bootstraps Elisp, generates pdump)
cargo xtask fresh-build --release

# On memory-constrained machines (including 8 GiB WSL guests), serialize the
# Cargo compilation stages. This trades build time for a lower peak RSS.
cargo xtask fresh-build --release --low-memory

# Run
./target/release/neomacs
```

## Testing

After a release fresh build, run the main parity suites with:

```bash
cargo nextest run -p neovm-core --no-fail-fast
cargo nextest run -p neovm-oracle-tests --no-fail-fast
cargo nextest run -p neomacs-tui-tests --release --no-fail-fast
```

The TUI harness uses `target/release/neomacs` by default, regardless of the
Cargo test profile. Set `NEOMACS_TUI_NEOMACS_BIN` to use a different binary.

Set `NEOMACS_TUI_RECORD=on` to write an asciicast v3 recording for every
`TuiSession`. Recording is disabled by default. Core parity tests are grouped
by Rust test name and package parity tests by package scenario:

```text
target/tui-recordings/
├── neomacs-tui-tests/<test-name>/{gnu,neomacs}.cast
└── neomacs-melpa-tests/<scenario>/{gnu,neomacs}.cast
```

Replay a recording with `asciinema play <path>`. Set
`NEOMACS_TUI_RECORD_DIR=<path>` to select another artifact root. A relative
artifact root is resolved from the Cargo workspace. CI explicitly enables
recording for TUI jobs and uploads the resulting casts.

## Browser WebAssembly (Experimental)

The browser build uses the same editor session as desktop Neomacs, compiled for
`wasm32-unknown-unknown`. Build its portable runtime image and static
distribution through `xtask`:

```bash
cargo xtask fresh-build --release --portable-seed \
  --portable-runtime-image ./tmp/neomacs.portable
cargo xtask package-portable-assets \
  --portable-runtime-image ./tmp/neomacs.portable \
  --output-dir ./tmp/neomacs-portable-assets
cargo xtask build-wasm \
  --portable-assets ./tmp/neomacs-portable-assets \
  --output-dir ./tmp/neomacs-wasm-dist
python3 -m http.server 4174 --directory ./tmp/neomacs-wasm-dist
```

Open <http://127.0.0.1:4174/> in current Chrome. No experimental Chrome flags
are required. Browser Neomacs exposes `/neomacs-fake` as its home directory;
that namespace is backed by origin-private file system storage and persists for
the page origin. `/tmp` is session-local memory and is discarded with the
editor worker.

With the server running, the real-browser smoke tests verify logical editor
geometry across non-integer device scale factors and live viewport changes, as
well as a visible editor, native Chrome text input, `C-x 2`, `C-x 3`,
`M-x switch-to-buffer`, buffer contents across switches, file mutations,
temporary files, and OPFS persistence across a page reload:

```bash
nix-shell \
  -p google-chrome chromedriver python3Packages.selenium python3Packages.cbor2 \
  --run 'python3 crates/neomacs-wasm/tests/browser_hidpi_smoke.py \
    --headless --url http://127.0.0.1:4174/'
nix-shell \
  -p google-chrome chromedriver python3Packages.selenium python3Packages.cbor2 \
  --run 'python3 crates/neomacs-wasm/tests/browser_opfs_smoke.py \
    --headless --url http://127.0.0.1:4174/'
```

Pass `--chrome PATH` when Chrome is not discoverable on `PATH`, or omit
`--headless` for an interactive run.

The browser build has four complementary test layers:

```bash
# DOM input, worker bootstrap, asset loading, and OPFS adapter behavior.
node --test crates/neomacs-wasm/web/*.test.mjs

# Host-profile, typed input-protocol, and editor-worker behavior.
cargo nextest run \
  -p neomacs-wasm -p neomacs-wasm-protocol -p neomacs-wasm-worker

# Compile the real editor worker for the browser target.
cargo check --target wasm32-unknown-unknown -p neomacs-wasm-worker

# Verify HTTP status/body handling, CORS, and filtered headers in Chrome.
# Starts its own controlled endpoints; no editor build or preview is needed.
python3 crates/neomacs-wasm/tests/browser_http_smoke.py --headless

# Exercise Lisp URL retrieval and EWW through Fetch in the assembled editor.
python3 crates/neomacs-wasm/tests/browser_http_editor_smoke.py \
  --headless --url http://127.0.0.1:4174/

# Exercise HiDPI startup and a live viewport/scale change in default Chrome.
python3 crates/neomacs-wasm/tests/browser_hidpi_smoke.py \
  --headless --url http://127.0.0.1:4174/

# Exercise editing and persistent storage in the assembled product.
python3 crates/neomacs-wasm/tests/browser_opfs_smoke.py \
  --headless --url http://127.0.0.1:4174/
```

Add user-visible workflows such as buffer, window, and file commands to the
real-browser smoke test. Keep protocol translation and storage edge cases in
the faster JavaScript or Rust layers so failures identify the broken boundary.

Browser HTTP uses Fetch through an asynchronous `url-retrieve` loader, not
emulated TCP processes. Native desktop URL loaders are unchanged. Browser
CORS, mixed-content, redirect, and forbidden-header rules still apply:
arbitrary websites that do not allow the editor's origin cannot be read by
EWW. There is no implicit proxy or security bypass, and browser login cookies
are not sent. Requests currently allow eight outstanding operations, a 1 MiB
request body, a 16 MiB decoded response body, and a 30-second host timeout.
Killing a retrieval buffer cancels its request. Fetch completion transfers
owned bytes; Lisp callbacks run only on the editor VM thread.

## Linux (Arch Linux)

```bash
# Install dependencies
sudo pacman -S --needed \
  base-devel autoconf automake texinfo clang git pkg-config \
  gtk4 glib2 cairo \
  gstreamer gst-plugins-base gst-plugins-good gst-plugins-bad \
  wpewebkit wpebackend-fdo \
  wayland wayland-protocols \
  mesa libva \
  libjpeg-turbo libtiff giflib libpng librsvg libwebp \
  ncurses gnutls libxml2 jansson tree-sitter \
  gmp acl libxpm \
  libgccjit

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build NEO Emacs (compiles Rust, bootstraps Elisp, generates pdump)
cargo xtask fresh-build --release

# Run
./target/release/neomacs
```

Other distributions should follow similar dependency installation with their
package manager.

## macOS (Experimental)

macOS support is experimental — see
[issue #22](https://github.com/eval-exec/neomacs/issues/22) for status.

### The inline browser (xwidgets)

`xwidget-internal` is provided on macOS, through the system `WKWebView` rather
than WPE: `WKWebView` has no offscreen render path, so the view is a native
`NSView` placed over the GPU surface rather than a texture composited into it.
This is what GNU Emacs does on macOS (`src/nsxwidget.m`), and the placement
algorithm is ported from `src/xwidget.c`.  The adapter lives in
`crates/neomacs-webview/src/platform/macos` and is compiled by the `webview`
Cargo feature, which the darwin production capability row in `Cargo.toml`
requests, so `cargo xtask fresh-build --release` and the Nix package ship it.
`xwidget-internal` is advertised only when that backend is compiled in.

What works, as of the follow-ups to
[issue #300](https://github.com/eval-exec/neomacs/issues/300):

- `xwidget-webkit-browse-url` in any top-level frame: creation, navigation,
  placement, clipping, hiding, resize, and a view moving between frames.
- Measured load progress: `xwidget-webkit-estimated-load-progress` follows
  `WKWebView`'s `estimatedProgress` through key-value observing, and the
  `WKNavigationDelegate` reports the GNU load phases, delivered to Lisp as
  `(xwidget-event load-changed XWIDGET "load-started" | "load-redirected" |
  "load-committed" | "load-finished")`, exactly what `xwidget-webkit-callback`
  in `lisp/xwidget.el` keys its progress timer and buffer title on.  One load
  state per view folds the delegate's milestones and the observed readings
  together, so each progress value reaches Lisp once and nothing follows a
  finished load's `1.0` until the next load starts; a title or URL the page
  loses is reported as empty rather than kept.
- Script results: `xwidget-webkit-execute-script` delivers its optional `FUN`
  the JSON-converted result asynchronously, so `xwidget-webkit-get-selection`
  and `xwidget-webkit-insert-string` work.  As in GNU, `FUN` is not called when
  the script throws.
- Keyboard focus, following `src/nsxwidget.m`: a key that reaches the web view
  stays with Emacs unless the page reports an input element (INPUT or
  TEXTAREA) focused, and `C-g` typed into such an element hands focus back to
  Emacs without being relayed.  The page's answers are checked at the native
  boundary (a redefined `xwHasFocus` returning a non-boolean, or a non-string
  message, is treated as "Emacs keeps the key"), and a probe that completes
  after the view moved to another frame, or after that frame closed, is
  dropped rather than delivered to the old one.  Mouse input goes through the responder chain.

Known gaps:

- GNU's `isearch-mode` special case (keys always go to Emacs while searching)
  reads a buffer-local Lisp variable from the AppKit thread; the render thread
  cannot ask Lisp synchronously, so while isearch is active a page input keeps
  the keys.
- GNU's script-blocked branch (a response whose `Content-Security-Policy:
  sandbox` lacks `allow-scripts` sends every key to Emacs without asking the
  page) is not ported; in such a document the focus probe fails and the key
  goes to Emacs anyway.
- Persistent browser profiles are isolated by the OS only on macOS 14 and
  later; older systems fall back to WebKit's process-wide store.

Two more limits come from the overlay technique itself, and GNU Emacs has
shipped with all of them for years: Neomacs UI cannot paint over the web view
(mode line, minibuffer, popups, cursor), and scrolling it lags the
GPU-composited content by about a frame. One further limit GNU shares: a single
web view can be shown in only one window at a time.

Maintainers should use the reproducible signing, notarization, and artifact
verification flow in [releasing-macos.md](releasing-macos.md) rather than
uploading a locally assembled app bundle.

```bash
# Install dependencies (Homebrew)
brew install pkgconf \
  glib cairo \
  gstreamer gst-plugins-base gst-plugins-good \
  jpeg-turbo libtiff giflib libpng librsvg webp \
  gnutls libxml2 jansson tree-sitter gmp

# gmp-mpfr-sys is built with system GMP support. Its build script probes
# GMP with the C compiler directly, so Homebrew's keg must be visible to
# both the C compiler and linker.
export CPATH="$(brew --prefix gmp)/include${CPATH:+:$CPATH}"
export LIBRARY_PATH="$(brew --prefix gmp)/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build NEO Emacs
cargo xtask fresh-build --release

# Run
./target/release/neomacs
```

## NixOS / Nix

The development shell provides WPE WebKit through
[nix-wpe-webkit](https://github.com/eval-exec/nix-wpe-webkit), so opt-in
`webview` builds do not compile WebKit locally. The default production package
currently enables Linux video but not `webview`; this policy lives in
`Cargo.toml` and is consumed by both `xtask` and `flake.nix`.

The `flake.nix` includes `nixConfig` for the Cachix cache. Pass
`--accept-flake-config` to use it automatically, or configure it system-wide:

**NixOS** — add to your configuration (e.g., `/etc/nixos/configuration.nix`):

```nix
{
  nix.settings.substituters = [ "https://nix-wpe-webkit.cachix.org" ];
  nix.settings.trusted-public-keys = [ "nix-wpe-webkit.cachix.org-1:ItCjHkz1Y5QcwqI9cTGNWHzcox4EqcXqKvOygxpwYHE=" ];
}
```

**Non-NixOS** — add to `~/.config/nix/nix.conf`:

```
extra-substituters = https://nix-wpe-webkit.cachix.org
extra-trusted-public-keys = nix-wpe-webkit.cachix.org-1:ItCjHkz1Y5QcwqI9cTGNWHzcox4EqcXqKvOygxpwYHE=
```

### Build with Nix

**Option 1** — Trust the `nixConfig` in `flake.nix` (simplest):

```bash
nix build --accept-flake-config

# Or enter development shell
nix develop --accept-flake-config
```

Validate all advertised package/app/dev-shell outputs without building the
large package:

```bash
nix flake check --all-systems --no-build --accept-flake-config \
  --option allow-import-from-derivation false
```

Run the installed-package and Home Manager startup contracts on native Linux:

```bash
nix build --accept-flake-config \
  .#checks.x86_64-linux.installed-package-contract \
  .#checks.x86_64-linux.home-manager-contract
```

Both startup checks use a clean temporary home and deliberately do not pass
`--quick`, `-Q`, or `--no-site-file`.

### Home Manager

Select the package from the Neomacs flake explicitly instead of relying on an
unrelated ambient `pkgs.neomacs` attribute:

```nix
{
  programs.emacs = {
    enable = true;
    package = inputs.neomacs.packages.${pkgs.system}.default;
  };
}
```

The package provides `emacs`/`emacsclient` compatibility names alongside
`neomacs`/`neomacsclient`, so Home Manager's Emacs wrapper can use the same
installed runtime and portable dump.

**Option 2** — Pass Cachix flags directly:

```bash
nix build \
  --extra-substituters "https://nix-wpe-webkit.cachix.org" \
  --extra-trusted-public-keys "nix-wpe-webkit.cachix.org-1:ItCjHkz1Y5QcwqI9cTGNWHzcox4EqcXqKvOygxpwYHE="
```

> **Note:** Both options require your user to be in `trusted-users` in
> `/etc/nix/nix.conf` (e.g., `trusted-users = root @wheel your-username`), or
> configure the cache system-wide as shown above.

### Manual build (inside dev shell)

```bash
cargo xtask fresh-build --release

# Lower-memory alternative; equivalent to passing --jobs 1 to every Cargo
# compilation owned by the fresh-build pipeline.
cargo xtask fresh-build --release --low-memory
```
