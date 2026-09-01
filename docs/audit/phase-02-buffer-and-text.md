# Phase 2 Audit: Buffer & Text

**Date**: 2026-03-28

## GNU source ownership

Primary GNU source files:

- `src/buffer.c`
- `src/insdel.c`
- `src/marker.c`
- `src/intervals.c`
- `src/textprop.c`
- `src/itree.c`
- `src/region-cache.c`
- `src/undo.c`

GNU design here is centralized:

- `buffer.c`, `insdel.c`, and `marker.c` define the authoritative text and
  marker model.
- `intervals.c` and `textprop.c` define text property semantics on the same
  text core.
- `itree.c` defines overlays on that same text core.
- `undo.c` depends on the same mutation model.

GNU does **not** duplicate these semantics in its display backend.

## Neomacs source ownership

Primary Neomacs semantic files:

- `neovm-core/src/buffer/buffer.rs`
- `neovm-core/src/buffer/buffer_text.rs`
- `neovm-core/src/buffer/gap_buffer.rs`
- `neovm-core/src/buffer/overlay.rs`
- `neovm-core/src/buffer/text_props.rs`
- `neovm-core/src/buffer/undo.rs`
- `neovm-core/src/emacs_core/editing/buffer_vars/mod.rs`
- `neovm-core/src/emacs_core/editing/marker/mod.rs`
- `neovm-core/src/emacs_core/editing/buffer/mod.rs`

Recent cleanup removed the old duplicate text/runtime core from
`neomacs-display-runtime/src/core/`.  That crate now consumes protocol/layout
types instead of exporting a second editor-text subsystem.

## Audit result

Status is **semantically much closer, still not GNU-shaped yet**.

Good:

- `neovm-core/src/buffer/` is already a real semantic buffer subsystem.
- Recent compatibility work moved shared text-property ownership, undo
  ownership, and active marker ownership onto shared buffer text for indirect
  buffers.
- Buffer-local storage now distinguishes slot-backed locals from ordinary Lisp
  local bindings.
- Current-buffer switching now has a real `set_buffer_internal_1/2`-style
  boundary inside the buffer subsystem, with saved `pt`/`begv`/`zv` marker
  state for non-current buffers that share text.
- Overlays are now real heap objects, not cons-encoded handles.
- GNU oracle coverage now includes indirect-buffer shared-text mutation,
  indirect-buffer undo, marker ownership, file-lock transitions, plain
  `insert`, and `insert-and-inherit` stickiness cases using
  `front-sticky` / `rear-nonsticky` / `text-property-default-nonsticky`.
- The deleted duplicate display-runtime text core was the right refactor:
  GNU does not split buffer semantics across a VM crate and a display crate,
  and Neomacs should not either.

Remaining design gaps:

- GNU text properties are stored in interval trees in `intervals.c`.
  Neomacs still uses a boundary-indexed `BTreeMap` in
  `neovm-core/src/buffer/text_props.rs`.  The observable semantics are much
  better now, but the storage model is still not GNU's.
- GNU overlays are stored in the same general interval/itree family as the
  text engine.  Neomacs now has real overlay objects and indexed overlay
  storage, but the structure is still Rust-local rather than GNU's exact
  itree/object ownership model.
- GNU region-cache ownership is inside the core text engine.
  Neomacs still has no equivalent of GNU `region-cache.c`.
- GNU `buffer.c` and `textprop.c` still remain the semantic source of truth
  for some edge cases that Neomacs covers by oracle tests rather than by
  matching storage shape.

## Long-term ideal design

The ideal design is:

- `neovm-core/src/buffer/` is the **only** semantic owner of buffer text,
  markers, overlays, text properties, narrowing, and undo state.
- `neovm-core/src/emacs_core/` exposes those semantics to Lisp.
- `neomacs-display-runtime` consumes snapshots or read-only projections from
  `neovm-core`; it does not re-implement text semantics.
- `BufferText` should grow toward GNU's shared text-object role:
  it should own the authoritative text-property and marker/undo state for
  indirect buffers.
- Buffer-local storage should keep the current slot-backed-versus-Lisp-local
  split and continue moving toward GNU's `buffer.c` ownership boundaries.
- Long-term, `TextPropertyTable` should become a GNU-like interval tree, not a
  boundary map that merely simulates the same visible behavior.

The display runtime may have caches, but those caches must not become a second
buffer model.

## Required work

- Keep `neomacs-display-runtime` as a consumer of VM-owned text state.
- Keep the current shared-text ownership model for indirect buffers and extend
  it rather than reintroducing buffer-local duplication.
- Rewrite `TextPropertyTable` toward an interval-tree structure if the goal is
  source-level parity with GNU rather than only oracle-level parity.
- Extend GNU differential coverage for:
  marker relocation, insertion-type, text property mutation, overlay ordering,
  narrowing, undo boundary behavior, `insert-before-markers-and-inherit`,
  `default-text-properties`, and `char-property-alias-alist`.
- Add explicit GNU oracle cases for interval/root ownership transitions on
  indirect-buffer teardown and for any future interval-tree rewrite.

## Exit criteria

- One authoritative text model in `neovm-core`.
- Overlay, marker, text property, and undo behavior proven against GNU Emacs.
- No display/runtime text core that can drift from VM semantics.
- Buffer-local storage modeled close enough to GNU that `buffer.c` remains the
  obvious source-level spec.
