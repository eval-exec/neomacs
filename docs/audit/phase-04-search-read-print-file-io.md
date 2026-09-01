# Phase 4 Audit: Search / Read / Print / File I/O

**Date**: 2026-03-28

## GNU source ownership

Primary GNU source files:

- `src/search.c`
- `src/regex-emacs.c`
- `src/syntax.c`
- `src/lread.c`
- `src/print.c`
- `src/doc.c`
- `src/fileio.c`
- `src/dired.c`
- `src/filelock.c`

`lread.c` and `fileio.c` are especially important because they sit on the
boundary between Lisp semantics and startup/bootstrap behavior.

## Neomacs source ownership

- `neovm-core/src/emacs_core/text/search/mod.rs`
- `neovm-core/src/emacs_core/text/regex/mod.rs`
- `neovm-core/src/emacs_core/text/syntax/mod.rs`
- `neovm-core/src/emacs_core/lisp/reader/mod.rs`
- `neovm-core/src/emacs_core/lisp/lread/mod.rs`
- `neovm-core/src/emacs_core/lisp/load/mod.rs`
- `neovm-core/src/emacs_core/lisp/autoload/mod.rs`
- `neovm-core/src/emacs_core/lisp/print/mod.rs`
- `neovm-core/src/emacs_core/lisp/doc/mod.rs`
- `neovm-core/src/emacs_core/system/fileio/mod.rs`
- `neovm-core/src/emacs_core/editing/dired/mod.rs`

Related support:

- `neovm-core/src/emacs_core/lisp/native/builtins/search.rs`

Duplicated runtime-side logic:

- `neomacs-display-runtime/src/core/search.rs`
- `neomacs-display-runtime/src/core/regex/`
- `neomacs-display-runtime/src/core/syntax_table.rs`

## Audit result

Status is **partially compatible**, with the main remaining risk in
reader/loader semantics.

Good:

- Neomacs has dedicated source modules for all the major GNU areas here.
- Recent load/bootstrap work reduced some known semantic gaps.

Bad:

- `lisp/load/mod.rs` still remains one of the highest-risk files in the entire
  project.
- GNU and Neomacs still do not construct runtime state the same way during
  bootstrap.
- Search/regex/syntax-related logic also still exists in the display runtime,
  which is the wrong long-term owner for Lisp-visible search semantics.
- `.neobc` is a Neomacs-only source path that must stay completely invisible to
  Lisp semantics.
- Reader, printer, search, and file I/O behavior are not yet documented as
  uniformly GNU-equal at edge-case depth.

## Long-term ideal design

The ideal design is:

- `neovm-core/src/emacs_core/lisp/reader/mod.rs`, `lisp/lread/mod.rs`,
  `lisp/autoload/mod.rs`, and `lisp/load/mod.rs` together behave as a
  GNU-compatible reader/loader boundary, even if the internal implementation
  is Rust-native.
- `system/fileio/mod.rs`, `editing/dired/mod.rs`, and `lisp/doc/mod.rs` match
  GNU error shape, side effects, and handler behavior.
- Search, regex, and syntax semantics are owned in `neovm-core`, not in GUI
  runtime code.

## Required work

- Keep `load` and `lread` as first-class source-audit targets.
- Collapse duplicated search/regex/syntax ownership out of
  `neomacs-display-runtime/src/core/`.
- Keep auditing recursive-load limits, autoload shape, load history,
  `load-source-file-function`, and compiled/source file selection.
- Expand regex and syntax differential coverage.
- Audit print/read round-tripping and file handler behavior.
- Treat `.neobc` invisibility as a hard compatibility rule.

## Exit criteria

- Reader/loader semantics match GNU at the Lisp boundary.
- File I/O and doc lookup behavior match GNU under differential tests.
- Search, regex, and syntax edge cases are covered against GNU Emacs.
- No GUI/runtime crate owns Lisp-visible read/search semantics.
