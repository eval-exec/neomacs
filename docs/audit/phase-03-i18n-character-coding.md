# Phase 3 Audit: I18n / Character / Coding

**Date**: 2026-03-28

## GNU source ownership

Primary GNU source files:

- `src/character.c`
- `src/charset.c`
- `src/chartab.c`
- `src/coding.c`
- `src/ccl.c`
- `src/composite.c`
- `src/bidi.c`
- `src/category.c`
- `src/casetab.c`
- `src/casefiddle.c`

These files define the semantic text interpretation layer that later feeds
display, file I/O, and editing.

## Neomacs source ownership

VM/core side:

- `neovm-core/src/emacs_core/text/character/mod.rs`
- `neovm-core/src/emacs_core/text/charset/mod.rs`
- `neovm-core/src/emacs_core/text/chartable/mod.rs`
- `neovm-core/src/emacs_core/text/coding/mod.rs`
- `neovm-core/src/emacs_core/text/ccl/mod.rs`
- `neovm-core/src/emacs_core/text/composite/mod.rs`
- `neovm-core/src/emacs_core/text/category/mod.rs`
- `neovm-core/src/emacs_core/text/casetab/mod.rs`
- `neovm-core/src/emacs_core/text/casefiddle/mod.rs`

Layout side:

- `neomacs-layout-engine/src/bidi/`
- `neomacs-layout-engine/src/bidi_layout.rs`
- `neomacs-layout-engine/src/neovm_bridge.rs`
- `neomacs-layout-engine/src/unicode.rs`
- `neomacs-layout-engine/src/font_loader.rs`
- `neomacs-layout-engine/src/font_match.rs`

Runtime-side duplicated semantics:

- `neomacs-display-runtime/src/core/casetab.rs`
- `neomacs-display-runtime/src/core/category.rs`
- `neomacs-display-runtime/src/core/chartab.rs`
- `neomacs-display-runtime/src/core/composite.rs`
- `neomacs-display-runtime/src/core/char_utils.rs`

## Audit result

Status is **split across too many source owners**.

Good:

- NeoVM has dedicated modules for most GNU semantic areas in this phase.
- `neomacs-layout-engine/src/neovm_bridge.rs` is the right architectural
  direction: it reads resolved Rust-side state instead of treating GNU C structs
  as the source of truth.

Bad:

- Part of the text-semantic pipeline lives in `neovm-core`, another part lives
  in `neomacs-layout-engine`, and a third slice is duplicated again in
  `neomacs-display-runtime/src/core/`.
- Char width, bidi, composition, and font fallback are especially likely to
  drift if the layout engine independently decides behavior that should be VM
  semantic state.

## Long-term ideal design

The ideal design is:

- `neovm-core` owns semantic character, coding, case, category, composition,
  and bidi state.
- `neomacs-layout-engine` becomes a layout consumer through
  `neomacs-layout-engine/src/neovm_bridge.rs`; it should implement layout
  algorithms, not redefine language/text semantics.
- `neomacs-display-runtime` should not own category/case/chartab/composition
  semantics at all.

The layout engine can still be highly optimized and GPU-friendly, but it should
consume resolved semantic inputs instead of carrying a parallel text semantic
model.

## Required work

- Define a stricter boundary between `neovm-core` and `neomacs-layout-engine`.
- Remove duplicated i18n-semantic helpers from `neomacs-display-runtime/src/core/`.
- Audit char width, string width, bidi direction, and composition results
  against GNU Emacs.
- Keep font fallback and shaping decisions aligned with GNU face/font semantics,
  not just with renderer convenience.
- Expand multilingual differential tests:
  CJK, Arabic/Hebrew, emoji, combining sequences, coding-system round-trips.

## Exit criteria

- All Lisp-visible character/coding semantics are VM-owned.
- Layout engine uses those semantics rather than inventing its own.
- Display runtime no longer owns duplicated i18n-semantic modules.
- Bidi, width, and composition behavior are proven against GNU Emacs.
