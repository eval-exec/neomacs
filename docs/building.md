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
algorithm is ported from `src/xwidget.c`.

**This support is partial, not complete GNU Emacs xwidget compatibility.**
`xwidget-webkit-browse-url` works end to end on the primary frame; the gaps are
tracked in
[issue #300](https://github.com/eval-exec/neomacs/issues/300):

- **Primary frame only.** An xwidget displayed in a second top-level frame gets
  no view at all.
- **Load progress is approximate.** `xwidget-webkit-estimated-load-progress`
  reports 0.0 before a navigation and 1.0 once one is dispatched — it is not
  measured, because there are no `WKNavigationDelegate` or KVO events yet. A
  script run immediately after opening a page can therefore run before the page
  exists.
- **No script result callbacks.** `xwidget-webkit-execute-script` signals if you
  pass its optional `FUN`, rather than dropping it silently and leaving the
  caller waiting for a call that never comes. `xwidget-webkit-get-selection` and
  `xwidget-webkit-insert-string` use `FUN` and so do not work.
- **Keyboard focus is not handed off** in either direction. Mouse input works.

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
