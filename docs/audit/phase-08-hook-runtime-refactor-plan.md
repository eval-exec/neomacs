# Phase 8 Plan: Hook Runtime Refactor

**Date**: 2026-03-30

## Goal

Make Neomacs hook semantics and ownership converge on GNU Emacs.

The target is GNU's split:

- hook storage stays in ordinary Lisp variables
- one generic hook runner owns iteration and stopping semantics
- caller subsystems set the right current buffer/window/frame before running hooks

Neomacs should not keep a second parallel hook framework with different rules.

## GNU Source Ownership

Primary GNU source files:

- `src/eval.c`
- `src/window.c`
- `src/insdel.c`
- `src/intervals.c`
- `lisp/subr.el`

Important GNU ownership facts:

- `add-hook` and `remove-hook` are Lisp-owned in `subr.el`
- generic hook dispatch lives in `eval.c`
- `run-hook-wrapped` stops on the first non-`nil` wrapper result
- buffer/window/edit owners prepare execution context before calling the generic runner
- modification-hook recursion suppression is owned by the edit core, not the generic hook runner

## Current Neomacs Ownership

Current hook behavior is split across too many owners:

- `neovm-core/src/emacs_core/lisp/native/builtins/hooks.rs`
- `neovm-core/src/emacs_core/runtime/bytecode/vm.rs`
- `neovm-core/src/emacs_core/editing/editfns/mod.rs`
- `neovm-core/src/emacs_core/editing/navigation/mod.rs`
- `neovm-core/src/hooks.rs`

This is not GNU-shaped enough.

## Current Audit Result

### Good

- Lisp-facing hook variables already use GNU-style value shapes: `nil`, one
  function, or a list of functions.
- Local hook lists already understand the `t` marker meaning "also run the
  global/default hook value".
- The command system already exposes the main GNU hook entry points.

### Bad

- Generic hook dispatch is duplicated between the evaluator builtin path and the
  bytecode VM path.
- `run-hook-wrapped` still has the wrong stopping semantics.
- Hook callers still bypass one another with ad hoc hook-value lookup helpers.
- Change hooks still do not have GNU `insdel.c` recursion-suppression semantics.
- Window hooks still do not set up GNU-like caller context.
- `neovm-core/src/hooks.rs` is still a parallel non-Lisp hook system with
  runtime concepts that do not match GNU's owner model.

## Deep Refactor Architecture

### Design rule

Neomacs should follow GNU's owner model:

- generic hook iteration lives in one runtime owner
- buffer/window/edit modules remain responsible for context setup
- hook storage remains ordinary Lisp variable state
- there is no competing Rust-native hook manager for Lisp hook semantics

### Target runtime structures

#### 1. One generic hook runtime owner

Introduce one hook runtime module that owns:

- hook-variable value resolution
- local/global hook-function collection
- stopping semantics for:
  - `run-hooks`
  - `run-hook-with-args`
  - `run-hook-with-args-until-success`
  - `run-hook-with-args-until-failure`
  - `run-hook-wrapped`
  - `run-hook-query-error-with-timeout`

The evaluator and VM should both delegate to that owner.

#### 2. One shared runtime variable-resolution path

Hook dispatch should stop using bespoke "dynamic buffer or global" helpers.
It should resolve hook-variable values through the same runtime owner that
already defines visible variable state.

That means:

- alias resolution once
- current-buffer local values when applicable
- default/global obarray values otherwise
- no independent hook-only lookup rules

#### 3. Caller-owned context

After the generic owner exists, callers should converge on GNU:

- `window.c`-like callers select the right frame/window/buffer before running hooks
- `insdel.c`-like callers handle `inhibit-modification-hooks`,
  `first-change-hook`, and error cleanup
- interval/text-property motion code handles point-entered/point-left ownership

The generic hook runner should not grow window/edit special cases.

## Migration Slices

### Slice A: Generic hook owner

Move generic hook iteration into a new `hook_runtime.rs` module and make:

- `builtins/hooks.rs`
- `bytecode/vm.rs`
- generic edit-hook call sites

delegate to it.

This slice should also fix `run-hook-wrapped` and remove duplicated evaluator
vs VM hook semantics.

### Slice B: Window hook callers

Make window hook callers GNU-shaped:

- `run-window-configuration-change-hook`
- `run-window-scroll-functions`

This requires caller-side buffer/window/frame setup, not more generic-hook
branching.

### Slice C: Modification hooks

Make modification hooks match `insdel.c`:

- `inhibit-modification-hooks` rebound during hook execution
- `first-change-hook`
- char-based OLD-LEN semantics
- reset-to-nil on unhandled hook errors where GNU does

### Slice D: Point-motion hooks

Make interval/property motion hook callers match GNU interval ownership and
deduplication semantics.

### Slice E: Remove parallel Rust hook framework

Either delete `neovm-core/src/hooks.rs` or quarantine it as non-Lisp
infrastructure only. It should not represent a second Lisp hook architecture.

## Landed So Far

- Slice A landed: generic hook iteration now lives in the shared
  `hook_runtime.rs` owner and both the evaluator and VM delegate to it.
- Slice B core landed: window hook callers now enter GNU-shaped caller context
  instead of treating window hooks as generic hook lookups.
- Slice B follow-up landed: `window-configuration-change-hook` now walks frame
  windows with their own caller context without losing the selected window's
  local hook through an incorrect `set-default` write path.
- Slice B broadened: GNU C-owned caller hooks now run from their subsystem
  owners instead of being mere declared variables. This includes:
  - `kill-buffer-query-functions`
  - `kill-buffer-hook`
  - `minibuffer-setup-hook`
  - `minibuffer-exit-hook`
  - `delete-frame-functions`
  - `after-delete-frame-functions`
  - redisplay-owned `window-buffer-change-functions`
  - `window-size-change-functions`
  - `window-selection-change-functions`
  - `window-state-change-functions`
  - `window-state-change-hook`
  - `frame-window-state-change`
  - `set-frame-window-state-change`
- Slice C core landed: modification hooks now run with
  `inhibit-modification-hooks` rebound, `first-change-hook` runs on the initial
  transition to modified, and change-hook OLD-LEN is now character-based.
- Slice C follow-up landed: shared minibuffer teardown now safely swallows
  exit-hook signals at the runtime owner boundary instead of only in one caller
  path.
- Slice D landed: point-motion hook dispatch now follows GNU interval-boundary
  ownership and duplicate suppression order for `point-left` /
  `point-entered`.
- GC owner follow-up landed: `post-gc-hook` now runs from the real GC
  collection/safe-point boundary with GC explicitly inhibited while the hook
  runs, matching GNU `alloc.c` ownership.
- Buffer lifecycle follow-up landed: buffers now carry GNU-style
  `inhibit_buffer_hooks` state, `get-buffer-create` / `make-indirect-buffer`
  honor it, `kill-buffer` skips lifecycle hooks when inhibited, and
  `buffer-list-update-hook` is now run from real buffer/window owners instead
  of only the indirect-buffer path.
- Shared variable-runtime fix landed: `set-default` no longer overwrites the
  current buffer's local value just because a local binding exists; it now
  updates the default/toplevel cell, which matches GNU's hook-variable owner
  model and unblocks buffer-local window hooks.
- Slice E landed: the dead parallel Rust hook framework was deleted outright by
  removing `neovm-core/src/hooks.rs`.
- Advice-stack/backtrace slice landed: runtime function dispatch now owns a
  GNU-shaped Lisp call stack, `backtrace-frame--internal` / `mapbacktrace`
  read from that runtime owner, and `call-interactively` now routes through a
  real `funcall-interactively` marker frame so GNU `subr.el` /
  `nadvice.el` can recover interactive-call state through backtrace walking.
- Advice-stack follow-up landed: advice-wrapper frames are now labeled with
  their bound symbol at the runtime owner boundary, so GNU `nadvice.el`
  sees the same `apply` / advised-symbol / `funcall-interactively` stack
  shape for `:around` and `:before` advice that it expects when walking
  `called-interactively-p`.
- Focus-hook ownership follow-up landed: raw `focus-in` / `focus-out` events
  now flow through GNU's `special-event-map` ownership boundary into Lisp
  `handle-focus-in` / `handle-focus-out` handlers, leaving frame-focus hook
  semantics in `frame.el` instead of reimplementing them in Rust.
- Frame-close owner follow-up landed: window-manager close requests now keep
  source frame identity through transport, bootstrap `special-event-map`
  with GNU's `delete-frame -> handle-delete-frame` binding, and route the
  resulting `(delete-frame (FRAME))` special event through Lisp `frame.el`
  ownership instead of treating close as a raw `quit`.
- Display/selection owner follow-up landed: `echo-area-clear-hook` now runs
  from the real echo-area clear owner, active-region selection updates now
  flow through the GNU command-loop owner into `gui-set-selection` plus
  `post-select-region-hook`, and monitor snapshots are now primed during GUI
  startup and refreshed through a real frontend monitor-change event that runs
  `display-monitors-changed-functions` from the keyboard/display event owner.
- TTY owner follow-up landed: `suspend-tty` / `resume-tty` now run from a real
  terminal host boundary instead of unconditional GUI-style errors when TTY
  runtime is active. GNU-shaped `suspend-tty-functions` run before the host
  suspend transition, and `resume-tty-functions` run after host resume, while
  the TTY frontend now owns raw-mode pause/resume instead of Rust faking those
  hooks elsewhere.
- Generalized-place advice audit landed: GNU `nadvice.el` ownership is now
  differentially covered for:
  - `add-function`
  - `remove-function`
  - `advice-mapc`
  across:
  - named `symbol-function` places
  - `(local 'VAR)` buffer-local places
  - `(process-filter PROC)` places
  - `(process-sentinel PROC)` places
  The GNU-oracle coverage passed without adding a Neomacs-specific advice
  runtime, which confirms the correct design remains "GNU Lisp owns advice,
  Rust only supplies the C-owned substrate".

## Next

- Terminal owner refactor landed: terminal builtins now go through a real
  terminal manager instead of a singleton handle/param/runtime stub. Frames
  now carry a terminal id, `terminal-list` / `frame-terminal` /
  `terminal-live-p` consult live terminal state, and `delete-terminal` now
  runs `delete-terminal-functions`, deletes frames on that terminal, and marks
  the terminal dead in the runtime owner.

- Remaining GNU gap in this area is no longer the hook call itself; it is the
  deeper multi-terminal model from `terminal.c` / `frame.c` / `keyboard.c`:
  multiple live terminals, raw key ingress tagged by source terminal/frame,
  full side-queue semantics across non-current `kboard`s, device-specific
  `delete_terminal_hook`, and the deferred `Qnoelisp` delete path when the
  last frame on a terminal disappears.

- Keyboard owner follow-up landed: the active NeoVM `kboard` now swaps with
  the selected frame's terminal instead of staying globally singleton.
  Terminal-local input decode / local function key maps, unread events, and
  keyboard macro runtime now survive frame-terminal switches.

- Keyboard owner follow-up broadened: command-loop polling now walks parked
  non-current `kboard`s after the active one, so unread events, unread
  selection events, and keyboard-macro playback pending on another terminal
  are no longer invisible just because that terminal is not currently active.
  The remaining keyboard-side GNU gap narrowed further after the next slice:
  raw keypress transport now carries source frame identity end-to-end, and the
  keyboard owner switches to the source frame's terminal before using
  terminal-local decoding state. That brings raw key ingress closer to GNU's
  "current kboard follows input source" model instead of only following the
  selected frame.

- Terminal owner follow-up broadened: `delete-terminal` now runs a
  device-specific host delete hook before generic terminal/frame/kboard
  teardown, matching GNU `terminal.c`'s split between
  `delete-terminal-functions`, `delete_terminal_hook`, and final terminal
  deletion more closely.

- Terminal owner follow-up broadened again: when the last frame on a terminal
  disappears and other frames still exist elsewhere, `delete-frame` now tears
  down that terminal instead of leaving a terminal object with no frames. That
  matches GNU's normal multi-terminal owner path more closely.

- Terminal/frame teardown follow-up landed: terminal-driven frame deletion now
  uses an internal GNU-shaped deferred hook path instead of running frame
  hooks inline from `delete-terminal`. NeoVM now mirrors GNU's split more
  closely:
  - public `delete-terminal` runs `delete-terminal-functions` immediately
  - cascaded frame deletions queue `delete-frame-functions` and
    `after-delete-frame-functions` onto a runtime-owned pending-funcall stack
  - those deferred calls are drained only at safe points, matching GNU's
    `pending_funcalls` ownership model rather than making terminal teardown a
    second eager hook-dispatch path

- The remaining gap is narrower now:
  - thinner `keyboard.c` side-queue behavior across multiple live `kboard`s
  - the fully deferred GNU `Qnoelisp` / "last frame overall" terminal teardown
    path when terminal disappearance is driven from inside the device delete
    owner rather than from public Lisp

- Public frame/terminal ownership follow-up landed: `delete-frame` now honors
  GNU's visible/force split for sole-frame errors instead of silently deleting
  the only frame, and terminal teardown now goes through a shared terminal
  owner helper so public `delete-terminal` and the internal deferred path stop
  drifting.

- Internal terminal/frame ownership follow-up landed: NeoVM now models GNU's
  three-way delete semantics explicitly instead of approximating them as
  "bool force + deferred hooks". The frame/terminal owners now distinguish:
  - public `nil` force
  - public non-`nil` force
  - internal `Qnoelisp`
  That means sole-terminal / sole-frame checks are bypassed only for the
  internal path, delete hooks are deferred only for the internal path, and the
  terminal owner can finish teardown even if the device-specific host delete
  callback fails because the terminal already disappeared.

- Keyboard/input follow-up landed on top of that owner split: input-channel
  disconnect now routes through the terminal owner's internal `Qnoelisp`
  teardown path instead of only setting `command_loop.running = false`. That
  brings NeoVM closer to GNU `keyboard.c`'s "terminal device terminated"
  behavior: delete the terminal through the internal owner, queue the pending
  safe hook calls, and then unwind the command loop.

- Keyboard owner follow-up broadened again: parked `kboard` polling no longer
  invents its own terminal-id sort order. It now asks the terminal owner for
  GNU-shaped keyboard polling order, which follows live terminal-list order
  instead of numeric ids. That moves NeoVM closer to GNU `read_avail_input`
  ownership where terminals are polled by terminal-list order and only then
  by per-terminal unread state.

- audit the remaining declared-but-unwired GNU C-owned hook variables and either
  add real owner call sites or explicitly document them as not yet implemented
- keep broadening subsystem-owned caller coverage instead of adding hook logic
  to the generic runtime owner
- preserve the GNU split where Lisp-owned hooks such as save/frame focus
  behavior stay in Lisp rather than migrating into Rust
- continue widening GNU differential coverage around Lisp-owned advice/hook
  surfaces rather than reimplementing them in Rust
