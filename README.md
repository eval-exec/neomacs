"I started Neomacs because I love Emacs, I respect Emacs, and I want to evolve the legendary Emacs into the ultimate modern powerhouse." — *Eval Exec*

<p align="center">
  <i>✨ "While other editors can save your files, only Emacs can save your soul." ✨</i>
</p>


<p align="center">
  <img src="assets/banner.svg" alt="NEOMACS banner"/>
</p>

<p align="center">
  <b>Emacs, rewritten in Rust — unleashed on the GPU, fixing what 40 years of C never could.</b>
  <br/>
  Built for 100% compatibility with the Emacs ecosystem you already have — your config, your packages, your muscle memory.
</p>


<p align="center">
  <a href="https://github.com/eval-exec/neomacs/actions/workflows/ci.yml"><img src="https://github.com/eval-exec/neomacs/actions/workflows/ci.yml/badge.svg" alt="CI"/></a>
  <a href="https://github.com/eval-exec/neomacs/releases/latest"><img src="https://img.shields.io/github/v/release/eval-exec/neomacs?label=release" alt="Latest release"/></a>
  <a href="https://github.com/eval-exec/neomacs/releases"><img src="https://img.shields.io/github/downloads/eval-exec/neomacs/total?label=downloads" alt="Downloads"/></a>
  <a href="COPYING"><img src="https://img.shields.io/github/license/eval-exec/neomacs" alt="License: GPL-3.0"/></a>
  <a href="https://github.com/eval-exec/neomacs/discussions"><img src="https://img.shields.io/github/discussions/eval-exec/neomacs" alt="Discussions"/></a>
  <a href="https://x.com/evil_exec"><img src="https://img.shields.io/badge/X-Eval%20Exec-000000?logo=x&logoColor=white" alt="X: Eval-Exec"></a>
<a href="https://github.com/sponsors/eval-exec">
  <img
    src="https://img.shields.io/badge/Sponsor-eval--exec-ea4aaa?style=flat&logo=githubsponsors&logoColor=white"
    alt="Sponsor NEO Emacs"
  />
</a>

  
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#showcase">Showcase</a> ·
  <a href="docs/animations.md">Animations</a> ·
  <a href="docs/ARCHITECTURE.md">Architecture</a> ·
  <a href="docs/faq.md">FAQ</a> ·
  <a href="https://github.com/eval-exec/neomacs/discussions">Discussions</a>
</p>

NEO Emacs keeps everything that makes Emacs *Emacs* — your `init.el`, your packages, your
muscle memory — and rebuilds the machinery underneath: the display engine runs on the
GPU, and the ~300,000-line C core is reimplemented in Rust. GNU Emacs itself serves as
the test oracle, so every rewritten subsystem is verified to behave identically.
([Why rewrite Emacs?](docs/faq.md))

> [!IMPORTANT]
> NEO Emacs is a **work in progress**. Expect rough edges, breaking changes, and missing
> features — [bug reports](https://github.com/eval-exec/neomacs/issues) are very welcome.

> [!NOTE]
> NEO Emacs is a hard fork of GNU Emacs (Lisp tree synced to `emacs-31.0.90`). The C core
> has been fully replaced by Rust. Fork provenance and rationale: [FAQ](docs/faq.md).




## Showcase

[![NEO Emacs Showcase Video](https://img.youtube.com/vi/WZRWWuuNZX0/maxresdefault.jpg)](https://youtu.be/WZRWWuuNZX0?si=yHUy1lUDTznUbTMx)

**Cursor, buffer-switch, and scroll animations** — GPU-rendered at display refresh rate:

https://github.com/user-attachments/assets/85b7ee7b-3f4a-4cd2-a84f-86a91d052f11

<details>
<summary><b>More: 4K video, inline browser, inline terminal, rounded box faces…</b></summary>

### Inline 4K Video Playback — DMA-BUF zero-copy, GPU backend

https://github.com/user-attachments/assets/275c6d9a-fced-44f6-8f43-3bbd2984d672

### Inline 4K Images — GPU-decoded, won't block the Emacs main thread

<img width="1447" alt="Inline 4K images in Emacs buffer" src="https://github.com/user-attachments/assets/325719dc-dac4-4bd8-8fd9-e638450a489f" />

### Inline Web Browser (WPE WebKit) — GPU backend, DMA-BUF zero-copy

<img width="1851" alt="Inline WPE WebKit browser in Emacs buffer" src="https://github.com/user-attachments/assets/10e833ca-34b2-4200-b368-09f7510f50d0" />

### Inline Terminal (Alacritty) — GPU-backed terminal embedded in a buffer

<img width="1448" alt="Inline Alacritty terminal in Emacs buffer" src="https://github.com/user-attachments/assets/175ffd75-78b5-46c9-9562-61cfd705e358" />

### GPU Text with Rounded Box Faces

<img width="1868" alt="Round corner box face attribute" src="https://github.com/user-attachments/assets/65db32f0-8852-4091-bd99-d61f839e0c95" />

</details>

## Highlights

- **GPU display engine** — text, images, and effects rendered via wgpu
  (Vulkan · Metal · DX12 · GL); ~4,000 lines of Rust replace ~50,000 lines of `xdisp.c`
- **Rich media in buffers** — inline 4K video (typed Rust GStreamer + VA-API
  integration on Linux), GPU-decoded images, a WPE WebKit browser, and a GPU terminal;
  DMA-BUF paths stay on the GPU, with typed fallbacks where native interop is unavailable
- **Animations everywhere** — 8 cursor modes, 21 scroll effects, 10 buffer
  transitions at display refresh rate, all configurable from Elisp
  ([full catalog](docs/animations.md))
- **Hackable below the Lisp line** — in GNU Emacs, hackability ends where the C
  display engine begins; NEO Emacs opens the whole frontend to Elisp, from render
  effects down to GPU shaders (WGSL) — hack the pixels, not just the text
- **Compatibility as the contract** — your `init.el`, packages, and muscle memory;
  every rewritten subsystem is diffed against GNU Emacs as a test oracle, and
  real-world configs like Doom Emacs are the daily test bed
- **Pure-Rust core** — no C left: the Elisp evaluator, bytecode VM, GC, portable
  dump, and editor internals are all memory-safe, modern Rust
- **GUI or terminal** — the same binary renders on the GPU or in a TTY (`neomacs -nw`)
- **What's next** — true multi-threaded Elisp and a concurrent zero-pause GC
  ([status](#status))

## Install

Prefer system packages? Download them from
**[Releases](https://github.com/eval-exec/neomacs/releases/latest)**:

| Platform | Packages |
|----------|----------|
| **Linux** | full `.deb` · `.rpm` · tarball; minimal AppImage · tarball (x86_64, aarch64) |
| **macOS** *(experimental)* | `.dmg` · `.zip` · `.tar.gz` (Apple Silicon) |
| **Windows** *(experimental)* | installer `.exe` · portable `.zip` (x86_64, aarch64) |

For terminal, batch, and Linux container deployments, the complete application
is also published for amd64 and arm64 on
[`evalexec/neomacs`](https://hub.docker.com/r/evalexec/neomacs) and
[`ghcr.io/eval-exec/neomacs`](https://github.com/eval-exec/neomacs/pkgs/container/neomacs).
See the [Docker guide](docs/docker.md).

The full Linux product links the system GStreamer runtime and provides video.
The `neomacs-minimal-*` artifacts omit video and all GStreamer loader
dependencies; the portable AppImage intentionally uses this minimal product.

<details>
<summary><b>Build from source</b></summary>

```bash
git clone https://github.com/eval-exec/neomacs && cd neomacs

# Optional (recommended): repo dev shell with all dependencies
nix develop --accept-flake-config

# Compiles Rust, bootstraps Elisp, generates the portable dump
cargo xtask fresh-build --release

./target/release/neomacs
```

Run Cargo commands from within the checkout (normally its root). Cargo discovers
`.cargo/config.toml` from the invocation directory; Neomacs uses that config to
provide member crates with the repository root for shared Lisp and test assets.

Platform dependencies (Arch, macOS, Nix/Cachix) and the test suites:
[docs/building.md](docs/building.md).

</details>

## Status

| Area | State |
|------|-------|
| 100% GNU Emacs compatibility (oracle test suites) | 🚧 ~95%, closing the last gaps |
| JIT compilation + inline caching for Elisp | 🚧 working, profiling and improving |
| zero pause GC | 🚧 maturing |
| Elisp-hackable frontend — GPU shaders, surfaces | 🚧 early, expanding |
| Rust Elisp runtime — evaluator, bytecode VM, portable dump | 🚧 refactoring, testing, improving |
| GPU display + layout engine (replaces `xdisp.c`) | 🚧 early, improving |
| Inline images, 4K video, WebKit browser | 🚧 works today, experimental, improving |
| High-performance neo-term (GPU terminal) | 🚧 in development |
| Cursor / scroll / buffer-switch animations | 🚧 polishing, catalog growing |
| Performance | 🔬 profiling, benchmarking, tuning |
| True multi-threaded Elisp | 🔬 designing, researching, experimental attempts |
| TUI renderer (`neomacs -nw`) | 🚧 usable, polishing |
| Cross-platform support | 🚧 Linux & macOS first; Windows awaiting testing; WASM, Android & iOS planned |

## Architecture

Everything runs in Rust across two threads — the Emacs thread owns the Elisp runtime
and editor state, the render thread owns the GPU.

Design principles, the full module map, and why Rust/wgpu:
[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

-----

## ❤️ [Sponsoring](https://github.com/sponsors/eval-exec) ❤️

NEO Emacs is a long-term project that takes significant ongoing work to build, test, and
maintain. If NEO Emacs is useful or exciting to you, please consider supporting its
development on ❤️ [GitHub Sponsors](https://github.com/sponsors/eval-exec) ❤️.

Crypto donations are also welcome — scan a QR code or copy an address:

<table>
  <tr>
    <th align="center" width="240">BTC (Bitcoin)</th>
    <th align="center" width="240">USDC (ERC-20 · Ethereum)</th>
  </tr>
  <tr>
    <td align="center" width="240"><img src="assets/receive_btc_bc1p0y3fzffqrqw08k8nc2ge3dfj02hr22ku8nugnc04ek522gty9kjsyy8lgc.png" width="240" alt="Bitcoin (BTC) donation QR code"/></td>
    <td align="center" width="240"><img src="assets/receive_usdc_erc20_0xF5e8493b611F8D4c07850C2cC77b38dD632620F2.png" width="240" alt="USDC (ERC-20) donation QR code"/></td>
  </tr>
  <tr>
    <td align="center" width="240"><small>bc1p0y3fzf<wbr>fqrqw08k8n<wbr>c2ge3dfj02<wbr>hr22ku8nug<wbr>nc04ek522g<wbr>ty9kjsyy8l<wbr>gc</small></td>
    <td align="center" width="240"><small>0xF5e8493b<wbr>611F8D4c07<wbr>850C2cC77b<wbr>38dD632620<wbr>F2</small></td>
  </tr>
</table>

-----

## Acknowledgments

Built with [wgpu](https://wgpu.rs/) · [winit](https://github.com/rust-windowing/winit) ·
[cosmic-text](https://github.com/pop-os/cosmic-text) — cursor animations inspired by
[Neovide](https://neovide.dev/).

## License

GNU General Public License v3.0 (same as Emacs)
