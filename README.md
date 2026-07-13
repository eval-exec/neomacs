"I started Neomacs because I love Emacs, I respect Emacs, and I want to evolve the legendary Emacs into the ultimate modern powerhouse." — *Eval Exec*

> ✨ *"While other editors can save your files, only Emacs can save your soul."* ✨

<p align="center">
  <img src="assets/banner.svg" alt="NEOMACS banner"/>
</p>

<p align="center">
  <a href="https://github.com/eval-exec/neomacs/actions/workflows/ci.yml"><img src="https://github.com/eval-exec/neomacs/actions/workflows/ci.yml/badge.svg" alt="CI"/></a>
  <a href="#features"><img src="https://img.shields.io/badge/status-alpha-blueviolet?style=for-the-badge" alt="Status: Alpha"/></a>
  <a href="#building"><img src="https://img.shields.io/badge/rust-1.95.0-orange?style=for-the-badge&logo=rust" alt="Rust 1.95.0"/></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-GPL--3.0-blue?style=for-the-badge" alt="License: GPL-3.0"/></a>
</p>

# **[❤️ Sponsor Neomacs](https://github.com/sponsors/eval-exec)**
Neomacs is a long-term project that takes significant ongoing work to build, test, and maintain. If you want to support its development, please consider sponsoring it on [❤️ GitHub Sponsors](https://github.com/sponsors/eval-exec).

> **Note:** Neomacs is in active alpha development. Expect rough edges, breaking changes, and missing features. Contributions and bug reports are very welcome!

> **Fork notice:** Neomacs is a hard fork of GNU Emacs, forked from commit [`705c0e3729`](https://git.savannah.gnu.org/cgit/emacs.git/commit/?id=705c0e3729bf53db9e84ae7c8b932ebc3b2da934). The initial Neomacs source tree was synced up to GNU Emacs commit [`0ee48ac4df2`](https://git.savannah.gnu.org/cgit/emacs.git/commit/?id=0ee48ac4df2), tagged `emacs-31.0.90`. The changes are too invasive to ever be accepted upstream, so we did not preserve the original git history to keep the repository lightweight. If you need the full Emacs git history for reference, open an issue, and we can re-add it.

> **Why a fork, not from scratch?** Neomacs aims for 100% compatibility with official GNU Emacs — every config, package, and workflow should just work. By forking, we keep the original Emacs C code as a reference and test oracle: we can verifying that each Rust rewrite produces identical behavior, ensuring nothing breaks as subsystems are replaced one by one.

---

## The Problem

Emacs is a 40-year-old C codebase that hasn't kept up with modern hardware or software engineering:

- **Display engine** — ~50,000 lines of C in `xdisp.c`, designed for text terminals in the 1980s. CPU-only rendering, no GPU acceleration, no native video/animations, no smooth visual effects
  - **Large images** — rendering slows down significantly
  - **Video playback** — not natively supported
  - **Modern animations** — no smooth cursor movement, buffer transitions, or visual effects
  - **Web content** — limited browser integration
  - **GPU utilization** — everything runs on CPU while your GPU sits idle
- **Elisp performance** — no inline caching, stop-the-world GC, dynamic dispatch overhead. Even with native-comp (AOT), Elisp lacks runtime JIT optimization, speculative inlining, and concurrent GC — leaving significant performance on the table
- **Unsafe C codebase** — ~300,000 lines of unsafe C with manual memory management, monolithic architecture (runtime and editor entangled), single-threaded design that prevents real concurrency

## The Solution

Throw it all away and start fresh.

**Neomacs** is rewriting Emacs from the ground up in **Rust** — starting with the display engine and expanding to the core:

- **GPU display engine** *(done)* — ~4,000 lines of Rust replacing ~50,000 lines of legacy C, powered by wgpu (Vulkan/Metal/DX12/OpenGL)
- **Rust layout engine** *(done)* — bypasses `xdisp.c` entirely, reads buffer text via FFI and computes layout in Rust
- **Inline video/images/WebKit** *(done)* — 4K video, GPU-decoded images, and WPE WebKit browser views embedded directly in buffers
- **21 scroll effects, 8 cursor modes, 10 buffer transitions** *(done)* — GPU-accelerated animations running on the render thread at display refresh rate
- **Zero-copy DMA-BUF** *(done)* — efficient GPU texture sharing (Linux)
- **Rewrite entire Emacs core in Rust** *(in progress)* — replacing all ~300,000 lines of C with safe, modern Rust: Elisp runtime, evaluator, bytecode VM, GC, buffer/window/frame subsystems, and all editor internals
- **True multi-threaded Elisp** *(planned)* — real concurrency for the Lisp machine, not just cooperative threading
- **10x performance, and 100% Emacs compatibility.** *(planned)* — Rust-optimized Lisp machine with JIT compilation and inline caching

---

## Showcase

🎬 Neomacs youtube video

[![Neomacs Showcase Video](https://img.youtube.com/vi/WZRWWuuNZX0/maxresdefault.jpg)](https://youtu.be/WZRWWuuNZX0?si=yHUy1lUDTznUbTMx)

### Animations (Cursor, Buffer Switch, Scroll)

https://github.com/user-attachments/assets/85b7ee7b-3f4a-4cd2-a84f-86a91d052f11

### GPU Text with Rounded Box Faces

<img width="1868" alt="Round corner box face attribute" src="https://github.com/user-attachments/assets/65db32f0-8852-4091-bd99-d61f839e0c95" />

### Inline 4K Images

GPU-decoded directly — no CPU cost, won't block Emacs main thread.

<img width="1447" alt="Inline 4K images in Emacs buffer" src="https://github.com/user-attachments/assets/325719dc-dac4-4bd8-8fd9-e638450a489f" />

### Inline Web Browser (WPE WebKit)

GPU backend, DMA-BUF zero-copy.

<img width="1851" alt="Inline WPE WebKit browser in Emacs buffer" src="https://github.com/user-attachments/assets/10e833ca-34b2-4200-b368-09f7510f50d0" />

### Inline Terminal (Alacritty)

GPU-backed terminal emulator embedded in Emacs buffer.

<img width="1448" alt="Inline Alacritty terminal in Emacs buffer" src="https://github.com/user-attachments/assets/175ffd75-78b5-46c9-9562-61cfd705e358" />

### Inline 4K Video Playback

DMA-BUF zero-copy, GPU backend — no CPU cost.

https://github.com/user-attachments/assets/275c6d9a-fced-44f6-8f43-3bbd2984d672

---

## Features

### Working Now

| Feature | Description |
|---------|-------------|
| **GPU Text Rendering** | Hardware-accelerated text via wgpu (Vulkan/Metal/DX12/OpenGL) |
| **Video Playback** | GStreamer + VA-API hardware decode with DMA-BUF zero-copy |
| **Cursor Animations** | 8 modes with 7 movement styles and configurable spring trail |
| **Scroll Animations** | 21 scroll effects with 5 easing functions |
| **Buffer Transitions** | 10 buffer-switch effects (crossfade, slide, page-curl, etc.) |
| **DMA-BUF Zero-Copy** | GPU-to-GPU texture sharing via Vulkan HAL (no CPU readback) |
| **Inline Images** | GPU-accelerated image rendering in buffers |
| **Inline WebKit** | WPE WebKit browser views embedded in buffers |

### Animations

All animations run on the GPU render thread at display refresh rate, independent of the Emacs redisplay. Configure everything from Elisp.

#### Cursor

**8 particle/visual modes** (Neovide-inspired):

| Mode | Description |
|------|-------------|
| `none` | No animation, instant movement |
| `smooth` | Smooth interpolated movement (default) |
| `railgun` | Particles shoot backward from cursor |
| `torpedo` | Comet-like trail follows cursor |
| `pixiedust` | Sparkly particles scatter around cursor |
| `sonicboom` | Shockwave ring expands from cursor |
| `ripple` | Concentric rings emanate outward |
| `wireframe` | Animated outline glow |

**7 movement styles** controlling how the cursor interpolates between positions:

| Style | Description |
|-------|-------------|
| `exponential` | Smooth deceleration, no fixed duration (uses speed param) |
| `spring` | Critically-damped spring, Neovide-like feel (default) |
| `ease-out-quad` | Gentle deceleration curve |
| `ease-out-cubic` | Stronger deceleration curve |
| `ease-out-expo` | Sharp deceleration curve |
| `ease-in-out-cubic` | Smooth S-curve |
| `linear` | Constant speed |

The spring style also supports a **4-corner trail effect** where leading corners snap ahead and trailing corners stretch behind, controlled by a `trail-size` parameter (0.0-1.0).

#### Buffer Switch (Crossfade/Transition)

**10 buffer-switch effects** triggered when the visible buffer changes:

| Effect | Description |
|--------|-------------|
| `none` | Instant switch |
| `crossfade` | Alpha blend between old and new (default) |
| `slide-left/right/up/down` | Directional slide transitions |
| `scale-fade` | Scale and fade |
| `push` | New buffer pushes old buffer out |
| `blur` | Blur transition |
| `page-curl` | 3D page-turning effect |

#### Scroll

**21 scroll animation effects** organized into categories:

| # | Effect | Category | Description |
|---|--------|----------|-------------|
| 0 | `slide` | 2D | Content slides in scroll direction (default) |
| 1 | `crossfade` | 2D | Alpha blend between old and new positions |
| 2 | `scale-zoom` | 2D | Destination zooms from 95% to 100% |
| 3 | `fade-edges` | 2D | Lines fade at viewport edges |
| 4 | `cascade` | 2D | Lines drop in with stagger delay |
| 5 | `parallax` | 2D | Layers scroll at different speeds |
| 6 | `tilt` | 3D | Subtle 3D perspective tilt |
| 7 | `page-curl` | 3D | Page turning effect |
| 8 | `card-flip` | 3D | Card flips around X-axis |
| 9 | `cylinder-roll` | 3D | Content wraps around cylinder |
| 10 | `wobbly` | Deformation | Jelly-like deformation |
| 11 | `wave` | Deformation | Sine-wave distortion |
| 12 | `per-line-spring` | Deformation | Each line springs independently |
| 13 | `liquid` | Deformation | Noise-based fluid distortion |
| 14 | `motion-blur` | Post-process | Vertical blur during scroll |
| 15 | `chromatic-aberration` | Post-process | RGB channel separation |
| 16 | `ghost-trails` | Post-process | Semi-transparent afterimages |
| 17 | `color-temperature` | Post-process | Warm/cool tint by direction |
| 18 | `crt-scanlines` | Post-process | Retro scanline overlay |
| 19 | `depth-of-field` | Post-process | Center sharp, edges dim |
| 20 | `typewriter-reveal` | Creative | Lines appear left-to-right |

**5 scroll easing functions:**

| # | Easing | Description |
|---|--------|-------------|
| 0 | `ease-out-quad` | Standard deceleration (default) |
| 1 | `ease-out-cubic` | Stronger deceleration |
| 2 | `spring` | Critically damped spring with overshoot |
| 3 | `linear` | Constant speed |
| 4 | `ease-in-out-cubic` | Smooth S-curve |

#### Configuration

```elisp
;; All-in-one configuration:
;; (neomacs-set-animation-config
;;   CURSOR-ENABLED CURSOR-SPEED CURSOR-STYLE CURSOR-DURATION
;;   CROSSFADE-ENABLED CROSSFADE-DURATION
;;   SCROLL-ENABLED SCROLL-DURATION
;;   &optional SCROLL-EFFECT SCROLL-EASING TRAIL-SIZE)

;; Example: spring cursor, crossfade buffer switch, page-curl scroll with spring easing
(neomacs-set-animation-config t 15.0 'spring 150 t 200 t 150 7 2 0.7)

;; Example: fast linear cursor, no crossfade, wobbly scroll
(neomacs-set-animation-config t 20.0 'linear 100 nil 200 t 200 10 0 0.0)
```

### The Ambitious Vision

Neomacs aims to be **the most capable and beautiful Emacs ever built**, rewriting its internals in Rust:

- **Rich media** — 4K video, PDF rendering, image manipulation directly in buffers
- **GPU-native** — hardware-accelerated rendering, shader effects, 120fps animations
- **GPU terminal** — Rust-based terminal emulator replacing slow `term.el`/`ansi-term`/vterm
- **Cross-platform** — Linux (Vulkan), macOS (Metal), Windows (Vulkan/DX12)
- **Rust core** — rewrite Emacs C internals in Rust for memory safety and performance
- **Multi-threaded Elisp** — true concurrency for the Lisp machine, enabling parallel Elisp execution
- **10x faster Elisp** — Rust-optimized Lisp interpreter/compiler to dramatically speed up Elisp

The goal: **Make Emacs as powerful and beautiful as it deserves to be.**

---

## Architecture

Neomacs is rewriting Emacs from C to Rust with clean module boundaries. The goal is a
layered architecture where the Elisp runtime is a self-contained core, editor subsystems
are independent modules communicating through defined APIs, and the rendering engine runs
on a separate GPU thread.

### Current State

The rendering engine and layout engine are already in Rust. The Emacs C core still runs
the Elisp evaluator, GC, and editor subsystems — connected to Rust via FFI channels.

### Target Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                         Neomacs (Rust)                          │
│                                                                 │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                  Elisp Runtime Core                      │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │   │
│  │  │ Evaluator   │  │ Bytecode VM │  │ GC/Allocator│       │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘       │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │   │
│  │  │ LispObject  │  │Symbol Table │  │ Type System │       │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘       │   │
│  └──────────────────────────────────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌──────────────────────────────────────────────────────────┐   │
│  │                       Runtime API                        │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │   │
│  │  │register_type│  │register_root│  │define_func  │       │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘       │   │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐       │   │
│  │  │  run_hook   │  │  specbind   │  │signal_error │       │   │
│  │  └─────────────┘  └─────────────┘  └─────────────┘       │   │
│  └──────────────────────────────────────────────────────────┘   │
│                             │                                   │
│                             ▼                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                     Editor Modules                        │  │
│  │  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐  ┌───────┐│  │
│  │  │ Buffer │  │ Window │  │ Frame  │  │Keyboard│  │Process││  │
│  │  └────────┘  └────────┘  └────────┘  └────────┘  └───────┘│  │
│  │  ┌────────┐  ┌────────┐  ┌────────┐  ┌────────┐  ┌───────┐│  │
│  │  │ Font   │  │ Image  │  │File IO │  │ Reader │  │ Data  ││  │
│  │  └────────┘  └────────┘  └────────┘  └────────┘  └───────┘│  │
│  └───────────────────────────────────────────────────────────┘  │
│                             │                                   │
│                             ▼                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                    Rendering Engine                       │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │  │
│  │  │Layout Engine│  │wgpu Renderer│  │ Animations  │        │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘        │  │
│  │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐        │  │
│  │  │    winit    │  │   WebKit    │  │ GStreamer   │        │  │
│  │  └─────────────┘  └─────────────┘  └─────────────┘        │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                      Threading                            │  │
│  │   ┌────────────┐                       ┌────────────┐     │  │
│  │   │EmacsThread │                       │RenderThread│     │  │
│  │   └────────────┘                       └────────────┘     │  │
│  │      │                                      ▲             │  │
│  │      ├── FrameGlyphBuffer (crossbeam) ──────┘             │  │
│  │      └── InputEvent (crossbeam) ────────────────────┐     │  │
│  │                                                     │     │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                 │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │                       Backends                            │  │
│  │   ┌──────────┐         ┌──────────┐       ┌──────────┐    │  │
│  │   │  Vulkan  │         │  Metal   │       │ DX12/GL  │    │  │
│  │   └──────────┘         └──────────┘       └──────────┘    │  │
│  └───────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

**Design principles:**

- **Elisp Runtime Core** is a self-contained Rust crate. It owns LispObject, the
  evaluator, bytecode VM, GC, specpdl, and symbol table. It does NOT know about
  buffers, windows, frames, or any editor concept.
- **Runtime API** is a trait-based interface. Editor modules register their types
  (with GC trace descriptors), roots, and primitives. The GC traces registered
  types generically — no hardcoded `mark_kboards()` or `mark_terminals()`.
- **Editor Modules** are independent. Each owns its data structures and exposes
  them to Lisp through the Runtime API. Modules do not reach into each other's
  internals.
- **Rendering Engine** runs on a separate GPU thread, communicating via crossbeam
  channels (`FrameGlyphBuffer` down, `InputEvent` up). Already implemented.

### Why Rust?

- **Memory safety** without garbage collection
- **Zero-cost abstractions** for high-performance rendering
- **Excellent FFI** with C (Emacs core)
- **Modern tooling** (Cargo, async, traits)
- **Growing ecosystem** for graphics (wgpu, winit, cosmic-text)

### Why wgpu?

- **Cross-platform** — single API for Vulkan, Metal, DX12, and OpenGL
- **Safe Rust API** — no unsafe Vulkan/Metal code in application
- **WebGPU standard** — future-proof API design
- **Active development** — used by Firefox, Bevy, and many others

For an in-depth analysis of the current Emacs C architecture, why it's hard to rewrite,
and why Elisp is slow, see [docs/elisp-core-analysis.md](docs/elisp-core-analysis.md).

---

## Building

### Prerequisites

- **Rust** (stable, 1.95+)
- **GStreamer** (optional, for video playback)
- **WPE WebKit** (optional, for inline browser, Linux only)
- **VA-API** (optional, for hardware video decode on Linux)
- **GNU Emacs** (optional, for pre-compiling .el files — speeds up bootstrap ~17x)

Build commands in this README are run from the repository root.

### Quick Start

```bash
# Optional (recommended): use the repo dev shell (handles all dependencies)
nix develop --accept-flake-config

# Build Neomacs (compiles Rust, bootstraps Elisp, generates pdump)
cargo xtask fresh-build --release

# Run
./target/release/neomacs
```

### Testing

After a release fresh build, run the main parity suites with:

```bash
cargo nextest run -p neovm-core --no-fail-fast
cargo nextest run -p neovm-oracle-tests --no-fail-fast
cargo nextest run -p neomacs-tui-tests --release --no-fail-fast
```

The TUI harness follows Cargo's active test profile when choosing the
Neomacs binary. A plain `cargo nextest run -p neomacs-tui-tests` uses
`target/debug/neomacs`; after `cargo xtask fresh-build --release`, use
`--release` so the harness uses `target/release/neomacs` and its matching
final image.

### Linux (Arch Linux)

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
  ncurses gnutls libxml2 sqlite jansson tree-sitter \
  gmp acl libxpm \
  libgccjit

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build Neomacs (compiles Rust, bootstraps Elisp, generates pdump)
cargo xtask fresh-build --release

# Run
./target/release/neomacs
```

### macOS (Experimental)

macOS support is experimental — see [issue #22](https://github.com/eval-exec/neomacs/issues/22) for status.

```bash
# Install dependencies (Homebrew)
brew install pkgconf \
  glib cairo \
  gstreamer gst-plugins-base gst-plugins-good \
  jpeg-turbo libtiff giflib libpng librsvg webp \
  gnutls libxml2 sqlite jansson tree-sitter gmp

# gmp-mpfr-sys is built with system GMP support. Its build script probes
# GMP with the C compiler directly, so Homebrew's keg must be visible to
# both the C compiler and linker.
export CPATH="$(brew --prefix gmp)/include${CPATH:+:$CPATH}"
export LIBRARY_PATH="$(brew --prefix gmp)/lib${LIBRARY_PATH:+:$LIBRARY_PATH}"

# Install Rust (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build Neomacs
cargo xtask fresh-build --release

# Run
./target/release/neomacs
```

### NixOS / Nix

Neomacs uses [nix-wpe-webkit](https://github.com/eval-exec/nix-wpe-webkit) for the WPE WebKit dependency. Pre-built binaries are available via Cachix (~60MB download instead of ~1 hour build).

The `flake.nix` includes `nixConfig` for the Cachix cache. Pass `--accept-flake-config` to use it automatically, or configure it system-wide:

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

#### Build with Nix

**Option 1** — Trust the `nixConfig` in `flake.nix` (simplest):

```bash
nix build --accept-flake-config

# Or enter development shell
nix develop --accept-flake-config
```

**Option 2** — Pass Cachix flags directly:

```bash
nix build \
  --extra-substituters "https://nix-wpe-webkit.cachix.org" \
  --extra-trusted-public-keys "nix-wpe-webkit.cachix.org-1:ItCjHkz1Y5QcwqI9cTGNWHzcox4EqcXqKvOygxpwYHE="
```

> **Note:** Both options require your user to be in `trusted-users` in `/etc/nix/nix.conf`
> (e.g., `trusted-users = root @wheel your-username`), or configure the cache system-wide
> as shown above.

#### Manual build (inside dev shell)

```bash
cargo xtask fresh-build --release
```

---

## Platform Support

- **Linux** – primary supported platform. The steps above document a validated Arch Linux workflow, but other distributions should follow similar dependency installation with their package manager.
- **macOS** – experimental. See [issue #22](https://github.com/eval-exec/neomacs/issues/22) for status.
- **Windows** – planned.
- **Mobile (Android/iOS)** – planned.
- **WebAssembly (WASM)** – planned.


## Roadmap

The Rust display engine rewrite is **complete** — the Rust layout engine is now the only rendering path, bypassing `xdisp.c` entirely. See [docs/rust-display-engine.md](docs/rust-display-engine.md) for the design document.

**Completed:**
- GPU text rendering, face resolution, syntax highlighting
- Display properties, overlays, invisible text, line numbers
- Mode-line, header-line, tab-line
- Variable-width fonts, BiDi text layout
- Word-wrap, truncation, fringes, margins
- Images, video, WebKit embedding

**In progress:**
- Rewriting the entire Emacs C core in Rust (Elisp runtime, evaluator, bytecode VM, GC, buffer/window/frame subsystems)
- macOS and cross-platform support
- TUI (terminal) renderer

**Planned:**
- True multi-threaded Elisp
- JIT compilation and inline caching for 10x Elisp performance

---

## Contributing

Contributions welcome! Areas where help is needed:

- **Graphics programmers** — shader effects, rendering optimizations
- **Rust developers** — architecture, performance, safety
- **Emacs hackers** — Lisp API design, integration testing
- **Documentation** — tutorials, API docs, examples

---

## Acknowledgments

Built with:
- [wgpu](https://wgpu.rs/) — Cross-platform GPU rendering (Vulkan/Metal/DX12/GL)
- [winit](https://github.com/rust-windowing/winit) — Cross-platform window management
- [cosmic-text](https://github.com/pop-os/cosmic-text) — Pure Rust text shaping
- [GStreamer](https://gstreamer.freedesktop.org/) — Video playback with VA-API
- [ash](https://github.com/ash-rs/ash) — Vulkan bindings for DMA-BUF import
- Inspired by [Neovide](https://neovide.dev/) cursor animations

---

## License

GNU General Public License v3.0 (same as Emacs)

## Star History

[![Star History Chart](https://api.star-history.com/image?repos=eval-exec/neomacs&type=date&legend=top-left)](https://www.star-history.com/?repos=eval-exec%2Fneomacs&type=date&legend=top-left)
