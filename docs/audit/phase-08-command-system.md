# Phase 8 Audit: Command System

**Date**: 2026-03-28

## GNU source ownership

Primary GNU source files:

- `src/keyboard.c`
- `src/keymap.c`
- `src/macros.c`
- `src/menu.c`
- `src/callint.c`

GNU treats command dispatch, input decoding, keymaps, and interactive argument
handling as one semantic subsystem.

## Neomacs source ownership

VM/core side:

- `neovm-core/src/emacs_core/commands/interactive/mod.rs`
- `neovm-core/src/emacs_core/commands/keymap/mod.rs`
- `neovm-core/src/emacs_core/commands/kbd/mod.rs`
- `neovm-core/src/emacs_core/commands/keyboard/`
- `neovm-core/src/emacs_core/commands/kmacro/mod.rs`

Frontend/runtime side:

- `neomacs-bin/src/input_bridge.rs`
- `neomacs-display-runtime/src/render_thread/input.rs`
- `neomacs-display-runtime/src/render_thread/window_events.rs`
- `neomacs-display-runtime/src/render_thread/pointer_events.rs`

## Audit result

Status is **not yet clean at the source boundary**.

Good:

- The semantic command layer does exist in `neovm-core`.
- GUI/runtime input translation already has concrete files and therefore an
  auditable boundary.

Bad:

- Too much command-adjacent meaning can still leak into frontend/runtime event
  translation.
- `neomacs-display-runtime/src/render_thread/input.rs` already decides key
  symbol translation, committed-text handling, and some mouse/chrome hit-test
  behavior before `neovm-core` sees the event.
- `neomacs-bin/src/input_bridge.rs` then performs another semantic mapping step
  from display events to `neovm-core::keyboard::InputEvent`.
- GNU's keyboard pipeline is more semantically centralized than the current
  Neomacs crate split suggests.
- `xdotool` smoke tests are useful, but they do not replace source-level
  semantic ownership.

## Long-term ideal design

The ideal design is:

- `neovm-core` owns event normalization, key decoding, keymap lookup,
  interactive specs, recursive edit, and command dispatch semantics.
- `neomacs-bin` and `neomacs-display-runtime` deliver raw GUI/TTY events and
  window-system facts.

Platform-specific window chrome hit-testing may stay frontend-side, but Emacs
input-event shape, modifier normalization, text-vs-control interpretation, and
command lookup semantics should stay in `neovm-core`.

That gives Neomacs the same kind of semantic control point GNU has, even if the
frontend implementation differs.

## Required work

- Use the focused execution plan in `phase-08-keymap-key-input-refactor-plan.md`
  for the GNU-shaped keymap/key-input refactor order.
- Keep reducing semantic input logic outside `neovm-core`.
- Move toward a boundary where frontend/runtime sends raw platform events and
  `neovm-core` constructs GNU-style input events.
- Audit input-event object shape against GNU.
- Differential-test command resolution:
  same key sequence, same command, same prefix arg, same interactive args.
- Use GUI automation only after the command semantics are already source-owned
  by `neovm-core`.

## Exit criteria

- Command semantics have one owner: `neovm-core`.
- Frontend/runtime only transport raw event data.
- Input-event normalization no longer depends on split logic across
  `render_thread/input.rs` and `input_bridge.rs`.
- GUI smoke tests confirm, but do not define, command semantics.
