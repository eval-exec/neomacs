# Phase 5 Audit: Editing Commands

**Date**: 2026-03-28

## GNU source ownership

Primary GNU source files:

- `src/editfns.c`
- `src/cmds.c`
- `src/undo.c`
- `src/indent.c`
- `src/minibuf.c`

And many higher-level editing commands are GNU Lisp-owned in the `lisp/`
tree rather than C-owned.

## Neomacs source ownership

- `neovm-core/src/emacs_core/editing/editfns/mod.rs`
- `neovm-core/src/emacs_core/editing/indent/mod.rs`
- `neovm-core/src/emacs_core/commands/minibuffer/mod.rs`
- `neovm-core/src/emacs_core/commands/interactive/mod.rs`
- `neovm-core/src/emacs_core/commands/kmacro/mod.rs`
- `neovm-core/src/emacs_core/editing/navigation/mod.rs`
- `neovm-core/src/emacs_core/editing/rect/mod.rs`
- `neovm-core/src/emacs_core/commands/register/mod.rs`
- `neovm-core/src/emacs_core/lisp/native/builtins/`
- editing-adjacent motion/window commands in
  `neovm-core/src/emacs_core/display/window_cmds/mod.rs`

## Audit result

Status is **partially compatible and dependency-heavy**.

This phase depends strongly on:

- Phase 2 buffer correctness
- Phase 4 reader/load correctness
- Phase 8 command/input correctness

That means editing commands cannot be audited in isolation.

## Source-level risk

The biggest source-level risk here is not a missing function. It is **wrong
ownership**:

- If GNU owns a command in C, Rust can own it.
- If GNU owns a command in Lisp, Neomacs should load the GNU Lisp.
- A second risk is over-centralization: `window_cmds/mod.rs` is currently a
  very large mixed command module, so editing-adjacent behavior can drift into
  the window bridge even when GNU splits that ownership across C primitives and
  GNU Lisp.

Any Rust fallback that shadows a GNU Lisp-owned editing command becomes a
long-term semantic liability.

## Long-term ideal design

The ideal design is:

- `neovm-core/src/emacs_core/` owns only the GNU C-owned editing primitives.
- High-level editing commands continue to come from GNU Lisp sources.
- The Rust side exposes the correct primitive substrate and stays out of the
  way where GNU Lisp already defines the semantics.
- Window switching, buffer switching, and motion commands should stay in their
  correct owning subsystem instead of accumulating in one fallback module.

## Required work

- Audit Rust-owned editing commands against GNU C ownership.
- Find and remove any unnecessary Rust shadowing of GNU Lisp-owned editing
  behavior.
- Keep shrinking editing-adjacent behavior in `window_cmds/mod.rs` when GNU
  actually owns that behavior in Lisp or in a different C subsystem.
- Keep adding GNU differential tests for:
  region/mark operations, indentation primitives, undo-visible editing changes,
  minibuffer behavior, and interactive editing contracts.

## Exit criteria

- Source ownership matches GNU:
  Rust for GNU C-owned primitives, GNU Lisp for GNU Lisp-owned commands.
- Editing behavior is proven together with real command invocation paths.
