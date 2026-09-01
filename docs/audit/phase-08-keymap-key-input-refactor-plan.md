# Phase 8 Plan: Keymap and Key Input Refactor

**Date**: 2026-03-29

## Goal

Make Neomacs keymap and key-input semantics source-level equivalent to GNU
Emacs.

This does **not** require copying GNU's platform event plumbing.  It **does**
require that from `read_char` upward, Neomacs has the same semantic ownership
shape GNU Emacs has:

- `src/keyboard.c` owns key/event normalization, input queues, recent-key
  history, macro playback, translation maps, `read_char`, and
  `read_key_sequence`.
- `src/keymap.c` owns keymap normalization, active-map construction, key lookup,
  command remapping, prefix handling, and keymap parent/autoload semantics.
- GNU Lisp keeps Lisp-owned helpers in `lisp/keymap.el`, `lisp/subr.el`, and
  nearby files.

Neomacs can keep a different GUI/runtime frontend, but the semantic boundary
must converge on GNU.

## GNU Source Ownership

Primary GNU source files:

- `src/keyboard.c`
- `src/keymap.c`
- `src/callint.c`
- `src/macros.c`
- `lisp/keymap.el`
- `lisp/subr.el`

Important GNU ownership facts:

- `active_maps` and `current-active-maps` decide map precedence, not ad hoc
  callers.
- `key-binding` is a thin wrapper over `current-active-maps` plus
  `lookup-key`/`command-remapping`.
- `read_key_sequence` owns replay, translation-map application, raw vs cooked
  key tracking, mouse-prefix handling, and command remapping.
- `event-modifiers`, `event-basic-type`, and other higher-level helpers stay
  Lisp-owned.

## Current Neomacs Ownership

Current semantic ownership is split across:

- `neovm-core/src/emacs_core/commands/keymap/mod.rs`
- `neovm-core/src/emacs_core/lisp/native/builtins/keymaps.rs`
- `neovm-core/src/emacs_core/commands/interactive/mod.rs`
- `neovm-core/src/emacs_core/commands/kbd/mod.rs`
- `neovm-core/src/keyboard.rs`
- `neomacs-bin/src/input_bridge.rs`
- `neomacs-display-runtime/src/render_thread/input.rs`

This is not GNU-shaped enough yet.

## Current Audit Result

### Good

- Neomacs already uses GNU-style Lisp keymap objects.
- Minor-mode precedence is intentionally close to GNU.
- Translation-map order is intentionally close to GNU.
- Some higher-level event helpers are still Lisp-owned, which is the right
  direction.

### Bad

- `current-active-maps` is still too thin and ignores important GNU inputs:
  `overriding-local-map`, `overriding-terminal-local-map`, and position-based
  `local-map` / `keymap` properties.
- `key-binding` duplicates active-map logic instead of delegating to one
  authoritative active-map owner.
- Command remapping still follows a thinner local/global/minor lookup path than
  GNU.
- Event representation and key parsing are duplicated across multiple Rust
  modules.
- `read_key_sequence` is evaluator-owned glue instead of keyboard-subsystem
  ownership.
- Keymap autoload and parent-cycle behavior are not yet GNU-like.

### Refreshed audit on latest upstream

Latest upstream re-check on 2026-03-29 at `origin/main` `3bf363f32` still did
not change the keymap/input owner boundary. The newer upstream work was signal
runtime ownership and condition handling, not keyboard/keymap owner refactors.

So the keymap/input audit remains:

- the biggest remaining mismatch is still GNU `keyboard.c` ownership
- terminal-local translation maps are still modeled as ordinary globals instead
  of keyboard/terminal-local state
- keymap lookup/remap/active-map logic is still split across `keymap.rs`,
  `builtins/keymaps.rs`, and `interactive.rs`
- modifier canonicalization is still duplicated and not GNU-canonical
- echo-keystrokes timing/help behavior is still not GNU-shaped

One narrow active-map mismatch is now closed on current `main`: live-window
`POSITION` arguments now use that window's buffer and point in the shared
`keymap.rs` owner for both `current-active-maps` and `key-binding`, matching
GNU `keymap.c` more closely instead of silently defaulting back to the current
buffer.

### Latest kmacro ownership progress

The latest keyboard-macro refactors moved another GNU `macros.c` ownership
boundary into the keyboard runtime:

- live recording/playback state now lives on the keyboard runtime rather than
  higher-level kmacro metadata
- append replay, repeat counts, and command-loop playback now go through the
  shared macro runner
- macro teardown now restores outer execution state and runs
  `kbd-macro-termination-hook` from one shared low-level path, closer to GNU
  `Fexecute_kbd_macro` plus `pop_kbd_macro`
- symbol-valued macro events like `[ignore]` now always go through the command
  loop and keymap lookup; the old callable-symbol direct-execution fallback is
  gone
- `execute-kbd-macro` now follows GNU-style symbol function indirection and
  only accepts a final string or vector, instead of treating direct lists as
  macros

That means the remaining kmacro gaps are narrower:

- the full command-loop/input ownership for every macro event shape is still
  not complete

## Deep Refactor Architecture

The rest of Phase 8 should not be "fix one behavior at a time".  It should
move Neomacs to the same ownership model GNU Emacs uses.

### Design rule

Neomacs may keep different frontend/platform plumbing.

But from the first Lisp-visible event upward, the code should converge on this
GNU split:

- frontend/runtime crates:
  deliver raw platform facts only
- `keyboard` owner:
  owns command-loop keyboard state and `read_char` / `read_key_sequence`
- `keymap` owner:
  owns keymap normalization, active maps, lookup, remapping, and where-is
- GNU Lisp:
  keeps help and convenience layers

### Current owner mismatch

Today the same semantic responsibilities are split across too many places:

- `src/keyboard.rs`
  owns command-loop input blocking, but still delegates binding decisions back
  into higher layers
- `src/emacs_core/lisp/reader/mod.rs`
  still carries thin `read-key-sequence` runtime behavior that belongs with the
  keyboard owner
- `src/emacs_core/commands/interactive/mod.rs`
  still owns `key-binding`, `minor-mode-key-binding`, remap walkers, and
  `where-is` helpers that should live with the keymap owner
- `src/emacs_core/lisp/native/builtins/keymaps.rs`
  mixes Lisp builtin wrappers with real keymap semantics
- `src/emacs_core/commands/keymap/mod.rs`
  owns some true keymap semantics, but not yet the whole GNU `keymap.c`
  surface
- `src/emacs_core/runtime/eval/mod.rs`
  still exposes terminal-local translation maps through Lisp variable cells,
  even though the keyboard owner now mirrors them as keyboard-local runtime
  state

That split is the main reason the system still behaves "compatible enough"
instead of being same by construction.

### Target runtime structures

#### 1. `KBoard`-equivalent keyboard state

Neomacs needs one explicit keyboard/terminal-local runtime owner, even if the
type name is not literally `KBoard`.

That owner should contain:

- `input-decode-map`
- `local-function-key-map`
- unread/replayed cooked event queue
- unread raw event queue
- recent-key / lossage history
- `this-command-keys` and raw-command-key buffers
- keyboard macro playback/recording state
- delayed `switch-frame` / delayed window-selection events
- echo-keystrokes timing state
- shift-translation bookkeeping:
  `this-command-keys-shift-translated`
- replay state for translation rescans

These are GNU `keyboard.c` responsibilities and should stop living as loose
pieces on `Context`, the obarray, or helper modules.

#### 2. Global keyboard translation state

These should remain global, matching GNU:

- `function-key-map`
- `key-translation-map`

But `input-decode-map` and `local-function-key-map` should stop being modeled
as ordinary obarray globals.  They should be terminal-local keyboard state with
the same semantics GNU documents.

#### 3. One authoritative keymap runtime

The keymap owner should absorb all of the following:

- `get_keymap`
- keymap autoload policy
- parent-cycle checks
- `access_keymap`
- prefix-keymap composition across inheritance
- `current_minor_maps`
- `current-active-maps`
- `lookup-key`
- `key-binding`
- `minor-mode-key-binding`
- command remapping on active maps
- `where-is-internal`

This should be a real ownership move, not just helper extraction.

### Target call graph

The target runtime path should look like this:

1. frontend transport emits raw input facts
2. keyboard owner normalizes them into Lisp-visible Emacs event values
3. `read_char` consumes keyboard/terminal-local queues, timers, redisplay, and
   macro playback
4. `read_key_sequence` performs GNU-style replay/translation scanning
5. `read_key_sequence` asks the keymap owner for active maps and key lookup
6. keymap owner returns prefix/binding/remap answers
7. keyboard owner updates cooked/raw command-key history and echo state
8. interactive/callint layer only consumes the finished sequence/binding

That is much closer to GNU:

- `keyboard.c` drives the read loop
- `keymap.c` answers keymap questions
- `callint.c` consumes the results

### What must move out of `interactive.rs`

`interactive.rs` should shrink to interactive-command concerns only:

- `call-interactively`
- interactive spec parsing
- commandp / called-interactively-p
- command invocation argument resolution
- lightweight wrappers over keymap/keyboard-owned primitives

It should stop owning:

- active-map construction
- remap search across active maps
- minor-mode keymap walkers
- `where-is` search helpers
- structural keymap resolution

### What must move out of `reader.rs`

`reader.rs` should stop carrying keyboard semantics.

It can stay as the Lisp builtin wrapper surface for:

- `read-char`
- `read-key`
- `read-key-sequence`
- `read-key-sequence-vector`

But those builtins should just validate args and delegate into the keyboard
owner.  The real semantics should not be duplicated there.

### What must unify

There should be exactly one authoritative implementation for each of these:

- modifier canonicalization
- event symbol parsing/canonical symbol construction
- key description rendering
- host-event to Emacs-event normalization
- keymap resolution from symbols/autoloads/handles

If the same logic appears in both `keyboard.rs` and `keymap.rs`, or in both
`keymap.rs` and `interactive.rs`, the owner split is still wrong.

## Migration Order

The refactor should land in coherent owner-boundary slices, in this order.

### Slice A: finish `get_keymap` ownership

Commit the existing local WIP that centralizes keymap resolution, autoload
policy, and parent-cycle semantics under the keymap owner.

This is the right first step because every later slice depends on one
authoritative keymap-normalization path.

### Slice B: introduce keyboard-local state owner

Create the `KBoard`-equivalent runtime structure and move terminal-local
translation maps plus command-loop keyboard state onto it.

This slice should move ownership, not just field locations.

Current status:

- completed: unread/pending input queues, command-key history, recent input
  history, and command-loop keyboard-macro playback state now live under the
  keyboard owner
- completed: `input-decode-map` and `local-function-key-map` now have explicit
  keyboard-local runtime slots, and evaluator assignment keeps those slots in
  sync with the Lisp-visible variables
- remaining: terminal-local variable access still flows through ordinary symbol
  cells instead of a real GNU-style `DEFVAR_KBOARD` / `kboard` access path

### Slice C: move active maps and remap walkers into the keymap owner

After Slice B, move all remaining active-map and remap logic out of
`interactive.rs` and `builtins/keymaps.rs` into `keymap.rs`.

At the end of this slice:

- `keyboard.rs` asks the keymap owner for active maps and lookups
- `interactive.rs` no longer performs its own map walking

Current status:

- completed: `current-active-maps` owner logic now lives in `keymap.rs`,
  including position-sensitive `keymap` / `local-map` text properties,
  minor-mode map collection, and overriding local-map precedence
- completed: shared command-remapping walkers and remap normalization now live
  in `keymap.rs`, and `interactive.rs` now delegates instead of re-owning that
  traversal logic
- completed: `builtins/keymaps.rs` now acts as Lisp builtin surface over the
  keymap owner instead of carrying its own active-map implementation
- completed: active key lookup/remap resolution now lives in `keymap.rs`, and
  both `interactive.rs::key-binding` and `keyboard.rs::read_key_sequence` call
  the shared keymap-owner resolver instead of routing through interactive
  builtin glue
- completed: `minor-mode-key-binding` lookup and `where-is-internal` keymap
  selection now live under the keymap owner instead of `interactive.rs`
- remaining: `interactive.rs` still keeps `where-is-internal` sequence
  collection/formatting glue, and the bigger GNU gap is still Slice D's
  `keyboard.c`-shaped replay/rescan state machine

### Slice D: replace the current thin `read_key_sequence`

Rebuild `read_key_sequence` in the keyboard owner around GNU shape:

- replay/rescan state
- suffix translation scanning
- translation functions
- delayed frame-switch handling
- raw vs cooked command-key history
- `dont-downcase-last`
- `this-command-keys-shift-translated`
- fake/prefixed mouse events

This is the biggest semantic slice in Phase 8.

Current status:

- completed: `read-key-sequence` prompt / `DONT-DOWNCASE-LAST` /
  `CAN-RETURN-SWITCH-FRAME` option parsing now flows through one
  keyboard-owner options type instead of ad hoc wrapper paths
- completed: function-valued translation bindings now run inside the keyboard
  owner, including prompt delivery to translation functions
- completed: suffix translation replay now keeps reading when a translated
  suffix is still only a prefix key sequence, instead of incorrectly bailing
  out early with an undefined binding
- completed: shift/downcase fallback for uppercase chars and shifted function
  keys now replays the buffered key sequence inside the keyboard owner instead
  of reading a fresh event, and it now drives
  `this-command-keys-shift-translated`
- completed: `dont-downcase-last` and undefined-sequence restore now run on the
  keyboard-owned current sequence buffer instead of evaluator-local return-path
  glue
- completed: `switch-frame` event transport and deferral now live in the
  keyboard owner too: frontend focus events preserve `emacs_frame_id`,
  `read_char` emits GNU-shaped `(switch-frame FRAME)` events, and
  `read_key_sequence` now defers them through keyboard-owned selection-event
  state when the current sequence cannot return them yet
- completed: delayed window-selection events now use the same low-level owner
  path: the keyboard runtime carries generic selection events,
  `read_char` can surface GNU-shaped `(select-window (WINDOW))` events, and
  `read_key_sequence` now defers or returns them through the same
  switch-frame-kind path GNU uses in `keyboard.c`
- completed: mouse target-frame identity now survives frontend transport into
  the keyboard owner, mouse posn synthesis is frame/window aware instead of
  always falling back to the selected buffer, clicked-window buffer-local maps
  now participate in active-map lookup, and non-text mouse areas such as the
  mode line now prefix key lookup through the keyboard-owned sequence path
- completed: mouse sequences that begin in another window now switch the
  keyboard owner's current-buffer context to the clicked window's buffer for
  the duration of the sequence, so clicked-window buffer-local minor-mode maps
  are resolved the GNU way instead of leaking the previously selected buffer's
  minor-mode state into mouse lookup
- completed: `call-interactively` / `command-execute` explicit `KEYS` vectors
  now honor the GNU `interactive "@"` prefix too, selecting the window from
  the first parameterized mouse event before later interactive args are read
- remaining: full GNU replay/rescan for the remaining non-text mouse-event
  edge cases, especially fake-prefix provenance/backtracking and the broader
  unbound-mouse fallback ladder

### Slice E: unify modifier canonicalization

After `read_key_sequence` is GNU-shaped, unify:

- event-symbol modifier ordering
- key-description rendering
- transport-to-event modifier naming

Everything should route through one GNU-canonical modifier order.

### Slice F: final cleanup

Delete remaining duplicate helper paths and make the module boundaries reflect
ownership clearly:

- `keyboard.rs`: command-loop keyboard runtime
- `keymap.rs`: keymap runtime
- `reader.rs`: Lisp builtin wrappers only
- `interactive.rs`: command invocation only

## Refactor Constraints

These constraints matter if the goal is same design and implementation:

- Do not add more ad hoc `symbol_function_of_value` keymap resolution outside
  the keymap owner.
- Do not keep terminal-local maps in the obarray as if they were plain globals.
- Do not let `keyboard.rs` call back up into `interactive.rs` for key binding
  semantics long term.
- Do not implement more help-layer behavior in Rust if GNU keeps it in Lisp.
- Do not add more duplicate modifier-prefix formatters.

## Testing Expansion Required

The next GNU differential coverage must include:

- function-valued bindings in `input-decode-map`
- function-valued bindings in `local-function-key-map`
- suffix-only translation replacement
- `dont-downcase-last`
- `this-command-keys-shift-translated`
- echo-keystrokes / echo-keystrokes-help behavior
- keymap autoload during `lookup-key`, `define-key`, and active-map use

Without those tests, Phase 8 can still drift while appearing compatible.

## Ideal Long-Term Design

### Semantic boundary

- `neomacs-display-runtime` and `neomacs-bin` deliver raw platform facts:
  keycode, text, modifiers, pointer/window payloads, IME payloads.
- `neovm-core` owns the Emacs event model and all Lisp-visible keyboard
  semantics.

### `keyboard` ownership

The keyboard subsystem in `neovm-core` should own:

- raw input queue and unread-command-events integration
- keyboard macro playback and recording state
- recent-key history / lossage / command-key buffers
- translation-map application and replay bookkeeping
- `read_char`
- `read_key_sequence`
- current command key surfaces:
  `this-command-keys`, `this-command-keys-vector`,
  `this-single-command-keys`, `recent-keys`

### `keymap` ownership

The keymap subsystem in `neovm-core` should own:

- `get_keymap`
- `access_keymap`
- `lookup-key`
- `current_minor_maps`
- `current-active-maps`
- `key-binding`
- `minor-mode-key-binding`
- `where-is-internal`
- command remapping over active maps
- keymap parent mutation and cycle checks
- keymap autoload behavior where GNU does it in C

### Lisp ownership

GNU Lisp-owned helpers should stay loaded from GNU Lisp:

- `lisp/keymap.el`
- `lisp/subr.el`
- help/describe-key layers
- keymap convenience APIs/macros

If GNU owns it in Lisp, Neomacs should not permanently re-own it in Rust.

## Refactor Plan

## Progress

Completed on 2026-03-29:

- Slice 1 owner is now substantially better: `current-active-maps`,
  `key-binding`, and command remapping share one active-map constructor, honor
  overriding maps, and honor point-sensitive `local-map` / `keymap` text
  properties.
- Key description parsing/formatting no longer keeps an independent parser in
  `src/keyboard.rs`; it now routes through the GNU-facing `kbd` / Emacs-event
  path.
- Command-loop unread-event and keyboard-macro playback state now store
  Lisp-visible Emacs event values instead of frontend `KeyEvent` structs, which
  is closer to GNU `keyboard.c` ownership and removes repeated re-normalization
  in `read_char`.
- Frontend transport bitmasks and X keysym normalization now have a single
  owner in `neovm-core/src/keyboard.rs`; `neomacs-bin` and the TTY frontend
  forward transport facts instead of keeping their own duplicate key transport
  vocabulary.
- `read_char` and `read_key_sequence` implementation ownership has moved out of
  `emacs_core/eval.rs` and into `src/keyboard.rs`, so keyboard sequencing now
  lives with the command-loop owner instead of evaluator glue.
- Pending host input and GNU idle-epoch state now live on `CommandLoop`, and
  the mouse-event builders moved into `src/keyboard.rs`, so the keyboard owner
  no longer depends on evaluator-owned storage for those parts of command-loop
  state.
- Resize synchronization for `read_char` / `redisplay` now lives in
  `src/keyboard.rs` too, including pending resize draining and opening GUI
  frame host-size reconciliation.
- Timer timeout selection, GNU timer firing, and process-output polling for the
  command loop now live in `src/keyboard.rs` too, so the `read_char` wait path
  is no longer evaluator-owned.
- Command-key history is now keyboard-owned too: `recent-keys`,
  translated/raw command-key buffers, `set--this-command-keys`, and GNU-style
  event-array rendering for `this-command-keys` now go through the keyboard
  subsystem instead of ad hoc interactive-layer formatting.
- `read_key_sequence` now keeps its in-flight raw and translated event streams
  on keyboard-owned state instead of stack-local vectors, which gives Slice D a
  real owner boundary for later GNU replay/rescan, `dont-downcase-last`, and
  `this-command-keys-shift-translated` work.
- Terminal-local keyboard state now has an explicit `KBoard`-equivalent owner
  inside `src/keyboard.rs`, so unread events, command-key history,
  keyboard-macro playback, and terminal-local translation maps stop living as
  loose top-level keyboard fields.
- `read_key_sequence` now threads a real keyboard-owned options object through
  builtin wrappers, VM/runtime callers, and the keyboard owner, so prompt,
  `dont-downcase-last`, and `can-return-switch-frame` semantics no longer need
  evaluator-local wrapper glue.
- Function-valued translation bindings now execute inside the keyboard owner,
  and suffix translation replay now keeps reading through pending translation
  prefixes instead of returning an incomplete undefined sequence.
- Shift/downcase fallback now lives in the keyboard owner too: uppercase chars
  and shifted function keys replay the buffered sequence locally, update
  `this-command-keys-shift-translated`, and honor `dont-downcase-last` /
  undefined-sequence restoration without bouncing back through evaluator glue.
- Frontend focus events now preserve frame identity into the keyboard owner,
  and GNU-shaped `(switch-frame FRAME)` events plus keyboard-owned
  delayed-selection deferral now live under `src/keyboard.rs` instead of being
  dropped as transport-only focus notifications.
- Keyboard-owned selection events are now generic instead of hardcoded to
  switch-frame, so the same low-level path also handles GNU-shaped
  `(select-window (WINDOW))` events and defers them mid-sequence the same way
  GNU `read_key_sequence` does.
- Frontend mouse transport now preserves target-frame identity too, and the
  keyboard owner now synthesizes GNU-shaped mouse positions against the clicked
  frame/window geometry instead of always using the selected window/current
  buffer.
- Active-map lookup now uses the clicked window's buffer/local-map context for
  mouse events, and the keyboard owner prefixes non-text mouse areas such as
  `mode-line` before calling the keymap owner.
- Mouse sequences that begin in another window now also switch current-buffer
  context to the clicked window's buffer for the duration of the sequence, so
  buffer-local minor-mode maps match GNU's `keyboard.c` replay behavior
  instead of using the previously selected buffer's minor-mode state.

Still open:

- Frontend event transport still passes through a separate Rust `KeyEvent`
  layer before entering the command loop, even though raw keysym/modifier
  normalization is now centralized.
- The moved `read_char` / `read_key_sequence` code still reaches through
  `Context` helper/state surfaces that are evaluator-shaped; the next cleanup is
  the remaining display-host reach-through and the keyboard-macro ownership
  split.
- The keyboard owner still applies translation maps with a thin one-pass loop;
  GNU's remaining replay/rescan behavior, especially the harder mouse-event
  edge cases beyond current area-prefix handling, still needs to move over as
  the rest of Slice D.
- The keyboard owner still lacks GNU's richer fake-prefixed mouse provenance
  tracking, but the unbound-mouse fallback ladder now mirrors GNU's
  drag/double/triple/up/down simplification and drop behavior closely enough
  that prefix-key mouse replay no longer loses earlier keys.
- `input-decode-map` and `local-function-key-map` are still mirrored through
  evaluator globals for Lisp visibility; the next step is to make the keyboard
  owner the clearer source of truth for terminal-local translation state.
- Keymap autoload and parent-cycle semantics are still thinner than GNU.

### Slice 1: Active maps and `key-binding`

Make one shared active-map constructor and use it for:

- `current-active-maps`
- `key-binding`
- command remapping over active maps

Minimum GNU parity for this slice:

- honor `overriding-local-map`
- honor `overriding-terminal-local-map`
- honor point-sensitive `local-map` / `keymap` text properties
- honor explicit numeric and marker `POSITION`
- keep active-map order GNU-compatible

This is the highest-value first slice because multiple user-visible key lookup
surfaces depend on it.

### Slice 2: Event model unification

Collapse duplicated key/event parsing layers into one authoritative Emacs event
representation inside `neovm-core`.

That should absorb semantic duplication currently spread across:

- `src/keyboard.rs`
- `src/emacs_core/commands/keymap/mod.rs`
- `src/emacs_core/commands/kbd/mod.rs`

### Slice 3: Move key-sequence reading to keyboard ownership

Refactor `read_char` and `read_key_sequence` out of evaluator glue into the
keyboard subsystem.

This slice must move toward GNU handling of:

- translation maps and replay
- raw vs translated key history
- mouse-prefix and click-buffer behavior
- prefix-help / shift fallback behavior

### Slice 4: Keymap autoload, parent, and cache discipline

Make keymap ownership closer to GNU `get_keymap` and `set-keymap-parent`:

- keymap autoload where GNU autoloads
- parent-cycle protection
- fewer ad hoc symbol-to-keymap resolution paths

### Slice 5: Command-key history and macros

Unify:

- `recent-keys`
- `this-command-keys*`
- keyboard macro state
- unread/replayed key sequence bookkeeping

These should stop being split across evaluator, interactive, and keyboard
helpers.

### Slice 5 progress on current `main`

Latest keyboard-macro refactor pass moved the live runtime owner much closer to
GNU `src/macros.c` / `src/keyboard.c`:

- `KBoard` now owns live recording state, `last-kbd-macro`, executing-macro
  state, and a GNU-shaped finalized boundary for the current command
- `cancel-kbd-macro-events` now truncates only the unfinalized tail of the
  current command instead of being a no-op
- `start-kbd-macro` append mode now re-executes the previous macro when
  `NO-EXEC` is nil, while keeping the new appended definition buffer stable
- `KmacroManager` now keeps only higher-level metadata such as the macro ring
  and counter state
- VM/shared-runtime tests now exercise the same keyboard-owned state instead of
  the old manager-owned split

Remaining GNU mismatches in this slice:

- macro playback is still driven by direct event execution helpers rather than
  the same command-loop/input path GNU uses in `Fexecute_kbd_macro`

## Testing Plan

Each slice should add GNU differential tests before expanding the scope.

### Slice 1 tests

- `current-active-maps` precedence:
  overriding maps, text-property maps, minor maps, local map, global map
- `key-binding` with integer and marker `POSITION`
- `key-binding` and `current-active-maps` at point with `local-map` / `keymap`
  text properties
- command remapping through the same active-map order

### Later slices

- translation-map replay and key-sequence reading
- recent-key history and `this-command-keys*`
- prefix handling and keymap parent/autoload behavior

## Exit Criteria

Phase 8 keymap/key-input work is not done until:

- `keyboard` and `keymap` ownership in `neovm-core` are GNU-shaped
- frontend/runtime only transports raw platform event data
- `current-active-maps`, `key-binding`, and command remapping all share one
  active-map owner
- `read_key_sequence` semantics are keyboard-owned, not evaluator-owned glue
- GNU differential tests cover active-map precedence, translation/replay,
  recent-key history, remapping, and prefix behavior
