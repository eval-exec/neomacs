# Phase 7 Audit: Display Engine

**Date**: 2026-03-28

## GNU source ownership

Primary GNU source files:

- `src/xdisp.c`
- `src/dispnew.c`
- `src/xfaces.c`
- `src/fringe.c`
- backend display files such as `src/term.c` and platform backends

GNU has one semantic redisplay core, even though backends differ.

## Neomacs source ownership

Core-facing display semantics:

- `neovm-core/src/emacs_core/display/display/mod.rs`
- `neovm-core/src/emacs_core/display/dispnew/`
- `neovm-core/src/emacs_core/display/xdisp/mod.rs`
- `neovm-core/src/emacs_core/display/xfaces/mod.rs`

Layout:

- `neomacs-layout-engine/src/lib.rs`
- `neomacs-layout-engine/src/neovm_bridge.rs`
- `neomacs-layout-engine/src/engine.rs`
- `neomacs-layout-engine/src/status_line.rs`
- `neomacs-layout-engine/src/hit_test.rs`

GUI runtime:

- `neomacs-display-runtime/src/render_thread/`
- `neomacs-display-runtime/src/text/`
- `neomacs-display-runtime/src/backend/`

Renderer:

- `neomacs-renderer-wgpu/src/renderer/`
- `neomacs-renderer-wgpu/src/glyph_atlas.rs`
- `neomacs-renderer-wgpu/src/image_cache.rs`

Duplicated semantic support in runtime:

- `neomacs-display-runtime/src/core/`
- `neomacs-display-runtime/src/core/mod.rs`
- `neomacs-display-runtime/src/core/gap_buffer.rs`
- `neomacs-display-runtime/src/core/marker.rs`
- `neomacs-display-runtime/src/core/itree.rs`
- `neomacs-display-runtime/src/core/textprop.rs`
- `neomacs-display-runtime/src/core/search.rs`
- `neomacs-display-runtime/src/core/syntax_table.rs`

## Audit result

Status is **the largest remaining source-level architecture gap**.

Good:

- There is already real face/window-display audit coverage.
- The renderer/runtime stack is capable enough to expose real GUI bugs.

Bad:

- Neomacs currently spreads redisplay semantics across too many crates.
- `neomacs-layout-engine` declares itself as replacing `xdisp.c`, while
  `neovm-core` still owns `xdisp`, `dispnew`, and `xfaces` semantics.
- `neomacs-display-runtime/src/core/` duplicates semantic text helpers that do
  not belong in the display runtime long-term.

## Long-term ideal design

The ideal design is:

- `neovm-core` owns redisplay semantics and produces authoritative frame/window
  display state.
- `neomacs-layout-engine` performs layout over that semantic state, ideally
  through `neovm_bridge.rs` or an equivalent Rust-side snapshot boundary.
- `neomacs-display-runtime` performs GUI event/runtime orchestration.
- `neomacs-renderer-wgpu` renders pixels only.

In other words:

- VM/core owns meaning
- layout owns placement
- runtime owns orchestration
- renderer owns drawing

That boundary is much closer to GNU's semantic ownership than the current
overlap-heavy design.

## Required work

- Remove semantic duplication from `neomacs-display-runtime/src/core/`.
- Continue moving face, fringe, display-property, and redisplay invalidation
  semantics toward `neovm-core`.
- Make layout-engine and runtime consume semantic snapshots instead of
  re-deciding semantics.
- Use GUI smoke tests and Doom startup as integration checks after semantic
  ownership is clarified.

## Exit criteria

- One semantic display owner in `neovm-core`.
- Layout/runtime/renderer boundaries stop overlapping semantically.
- GUI behavior is explainable in terms of core-owned redisplay state.
