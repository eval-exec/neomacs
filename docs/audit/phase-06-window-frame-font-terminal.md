# Phase 6 Audit: Window / Frame / Font / Terminal

**Date**: 2026-03-28

## GNU source ownership

Primary GNU source files:

- `src/window.c`
- `src/frame.c`
- `src/terminal.c`
- `src/font.c`
- `src/fontset.c`
- higher-level GNU Lisp in `lisp/window.el`, `lisp/frame.el`, and related files

GNU's design concentrates low-level window/frame state in C data structures and
keeps higher-level window commands in GNU Lisp.

## Neomacs source ownership

Window model:

- `neovm-core/src/window/mod.rs`
- `neovm-core/src/window/display.rs`
- `neovm-core/src/window/history.rs`
- `neovm-core/src/window/parameters.rs`

Window/frame Lisp bridge:

- `neovm-core/src/emacs_core/display/window_cmds/mod.rs`

Font and frame/terminal primitives:

- `neovm-core/src/emacs_core/display/font/mod.rs`
- `neovm-core/src/emacs_core/display/fontset/mod.rs`
- `neovm-core/src/emacs_core/display/terminal/mod.rs`
- `neovm-core/src/emacs_core/display/display/mod.rs`
- `neovm-core/src/emacs_core/display/frame_vars/mod.rs`

Layout-side font behavior:

- `neomacs-layout-engine/src/emacs_ffi.rs`
- `neomacs-layout-engine/src/neovm_bridge.rs`
- `neomacs-layout-engine/src/font_loader.rs`
- `neomacs-layout-engine/src/font_match.rs`
- `neomacs-layout-engine/src/font_metrics.rs`
- `neomacs-layout-engine/src/fontconfig.rs`

## Audit result

Status is **improving, but still not GNU-equal by structure**.

Good:

- Window semantics have been moving into `neovm-core/src/window/`.
- GNU-oracle coverage around window history, display defaults, `vscroll`, and
  `window-old-point` has improved materially.
- `neovm-core/src/window/mod.rs` now carries real GNU-style per-window state
  such as `old_point`, `vscroll`, history, margins, and display fields.
- GNU `lisp/window.el` remains loaded rather than reimplemented wholesale.

Bad:

- `window_cmds/mod.rs` is still larger than the model layer
  (`5840` lines versus `2357` in `src/window/mod.rs`) and still owns more
  behavior than GNU `window.c`-style structure would justify.
- Not all GNU window fields are modeled yet.
- Frame/font/terminal source ownership is still more diffuse than GNU's model.
- Font matching and display-facing font behavior are split between core and
  layout-engine code.

## Long-term ideal design

The ideal design is:

- `neovm-core/src/window/` becomes the source-level equivalent of GNU's
  `window.c` data ownership.
- `window_cmds/mod.rs` becomes a thin Lisp bridge.
- `neovm-core/src/emacs_core/display/font/mod.rs` and
  `neovm-core/src/emacs_core/display/fontset/mod.rs` own Lisp-visible font and fontset
  semantics.
- `neomacs-layout-engine` consumes already-resolved window/frame/font state
  through `neovm_bridge.rs` instead of reading legacy C-struct layouts or
  owning semantic fallback rules independently.
- GNU Lisp continues to own high-level `window.el`/`frame.el` behavior.

## Required work

- Continue moving window state out of builtin code and into `src/window/`.
- Model the remaining GNU window state that still has no equivalent field.
- Audit frame parameters and terminal semantics at the same source level.
- Tighten the boundary between core font semantics and layout-engine font
  application.

## Exit criteria

- Window-owned state lives in `src/window/`.
- Frame/font/terminal primitives match GNU ownership and behavior.
- Layout consumes model-owned state instead of owning semantic window/frame
  data itself.
- GNU Lisp still owns the high-level window/frame command layer.
