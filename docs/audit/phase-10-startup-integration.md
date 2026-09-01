# Phase 10 Audit: Startup & Integration

**Date**: 2026-03-28

## GNU source ownership

Primary GNU startup sources:

- `src/emacs.c`
- `src/lread.c`
- `lisp/loadup.el`
- `lisp/startup.el`
- related runtime integration through package/init/session/window-system Lisp

## Neomacs source ownership

Startup and CLI:

- `neomacs-bin/src/main.rs`
- `neomacs-bin/src/input_bridge.rs`
- `neomacs-bin/src/tty_frontend.rs`

Bootstrap/load/runtime state:

- `neovm-core/src/emacs_core/lisp/load/mod.rs`
- `neovm-core/src/emacs_core/lisp/lread/mod.rs`
- `neovm-core/src/emacs_core/runtime/pdump/`

GUI startup/runtime:

- `neomacs-display-runtime/src/lib.rs`
- `neomacs-display-runtime/src/render_thread/bootstrap.rs`
- `neomacs-display-runtime/src/render_thread/lifecycle.rs`
- `neomacs-display-runtime/src/render_thread/frame_ingest.rs`

Real user config surface:

- `~/.config/init.el`
- `~/.config/doom/`

## Audit result

Status is **directionally close in bootstrap, not yet proven in full GUI
integration**.

Good:

- Neomacs intentionally runs the same top-level GNU Lisp startup path where it
  can.
- There is already a dedicated bootstrap audit doc.

Bad:

- `neomacs-bin/src/main.rs` still performs substantial pre-startup mutation
  before entering `recursive_edit`: it disables GC, sets `dump-mode` to `nil`,
  bootstraps buffers/frames, starts the render thread, installs an input bridge,
  and forces variables such as `inhibit-startup-screen`.
- `neovm-core/src/emacs_core/lisp/load/mod.rs` still applies runtime repair/shim logic
  through `apply_runtime_startup_state()` and post-`loadup.el`
  `term/common-win` / `term/neo-win` loading.
- Real startup success is more than "batch bootstrap passes".
- GUI startup, real init files, package activation, Doom dashboard rendering,
  input after startup, and renderer/runtime behavior are still practical
  integration gates.
- Because Neomacs splits display/runtime/front-end concerns differently from
  GNU, startup integration bugs can come from several crates.

## Long-term ideal design

The ideal design is:

- `neomacs-bin` owns platform/bootstrap orchestration only.
- `neovm-core` owns Lisp/runtime startup semantics.
- `neomacs-display-runtime` owns GUI lifecycle only.
- Real user configs, including Doom, load without semantic special-casing.

In the long term, every pre-`recursive_edit` mutation in `main.rs` should be in
one of two categories only:

- unavoidable host/bootstrap setup
- behavior that GNU already exposes at the Lisp boundary

Anything else should be treated as technical debt to remove or prove necessary.

The final proof of this phase is not a design diagram. It is:

- Neomacs loads a real config like Doom.
- The Doom dashboard appears.
- Basic interactive commands work through the real GUI input path.

## Required work

- Build and keep a repeatable GUI startup harness with:
  redirected logs, window detection, screenshots, and input smoke tests.
- Keep a source-level checklist of every startup shim in `main.rs` and
  `load.rs`, then either remove it or justify it with GNU-compatible semantics.
- Investigate every startup or GUI failure by tracing it back to the owning GNU
  subsystem.
- Keep Doom startup as a practical end-to-end integration target, not as a
  substitute for module-level audits.

## Exit criteria

- Real GNU-style configs, including Doom, load successfully.
- Doom dashboard renders in the GUI window.
- Basic operations such as `M-x`, split window, typing, and buffer switching
  work through the real GUI/input path.
- Startup no longer depends on unexplained pre-Lisp repair logic.
