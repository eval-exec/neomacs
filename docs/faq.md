# FAQ

## Why rewrite Emacs at all?

Emacs is a 40-year-old C codebase that hasn't kept up with modern hardware or
software engineering:

- **Display engine** — ~50,000 lines of C in `xdisp.c`, designed for text terminals in
  the 1980s. CPU-only rendering, no GPU acceleration, no native video/animations, no
  smooth visual effects:
  - **Large images** — rendering slows down significantly
  - **Video playback** — not natively supported
  - **Modern animations** — no smooth cursor movement, buffer transitions, or visual effects
  - **Web content** — limited browser integration
  - **GPU utilization** — everything runs on CPU while your GPU sits idle
- **Elisp performance** — no inline caching, stop-the-world GC, dynamic dispatch
  overhead. Even with native-comp (AOT), Elisp lacks runtime JIT optimization,
  speculative inlining, and concurrent GC
- **Large files and long lines** — a single gap buffer holds the entire file, and
  per-line work (redisplay, bidi, syntax, font-lock) scales with line length. A
  minified one-liner or a hundred-MB log still stalls the editor despite the
  mitigations added in Emacs 29 (`long-line-threshold`, locked narrowing, `so-long`),
  because the data structure never changed — the ecosystem's answer is to avoid the
  problem (`so-long`, VLF) rather than fix it
- **Everything blocks the UI** — Elisp runs on a single thread, so one slow call
  freezes the entire editor
- **Unsafe C codebase** — ~300,000 lines of unsafe C with manual memory management,
  monolithic architecture (runtime and editor entangled)

For the deep dive, see [Elisp core analysis](elisp-core-analysis.md).

## Why a fork, not from scratch?

NEO Emacs aims for 100% compatibility with official GNU Emacs — every config, package,
and workflow should just work. By forking, we keep the original Emacs behavior as a
reference and test oracle: we verify that each Rust rewrite produces identical
behavior, ensuring nothing breaks as subsystems are replaced one by one. Oracle test
suites, TUI grid comparisons, and GUI parity checks continuously diff NEO Emacs against
GNU Emacs.

## Where does the fork come from?

NEO Emacs is a hard fork of GNU Emacs, forked from commit
[`705c0e3729`](https://git.savannah.gnu.org/cgit/emacs.git/commit/?id=705c0e3729bf53db9e84ae7c8b932ebc3b2da934).
The NEO Emacs source tree (the Lisp side) was synced up to GNU Emacs commit
[`a360712c9d2`](https://git.savannah.gnu.org/cgit/emacs.git/commit/?id=a360712c9d272d950d8d8255ef74570f7e90b7d9),
tagged `emacs-31.1`. The changes are too invasive to ever be accepted upstream, so
we did not preserve the original git history to keep the repository lightweight. If
you need the full Emacs git history for reference, open an issue, and we can re-add it.

## Is the C core really gone?

Yes. The shipped `neomacs` binary is pure Rust: the Elisp evaluator, bytecode VM, GC,
portable dump, buffers, windows, keyboard, processes, layout, and rendering are all
Rust. The `lisp/` directory — the Elisp half of Emacs — is retained from GNU Emacs
and kept in sync, which is what makes package compatibility possible.

## Will my config and packages work?

That is the contract NEO Emacs is built around: 100% compatibility on observable
behavior — semantics, behavior, and logical display — with freedom to be better below
that line (GPU rendering, Rust internals, faster startup). NEO Emacs is alpha, so
gaps still exist; when you find one, that's a bug — please
[report it](https://github.com/eval-exec/neomacs/issues).

## Why Rust? Why wgpu?

See [ARCHITECTURE.md](ARCHITECTURE.md#why-rust).
