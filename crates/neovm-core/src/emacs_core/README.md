# `emacs_core` source layout

`emacs_core` is organized by ownership rather than by the historical GNU Emacs
C filenames. Every subsystem owns a directory containing its implementation,
tests, and private helpers.

```text
emacs_core/
├── commands/  # input, keymaps, minibuffer, interactive command invocation
├── display/   # frames, windows, redisplay, fonts, images, terminal surfaces
├── editing/   # buffers, markers, undo, modes, and editing operations
├── lisp/      # reader, loader, language facilities, native subroutines, docs
├── runtime/   # evaluator, VM, values, symbols, GC, JIT, and portable dumps
├── system/    # files, processes, networking, time, and platform integration
├── tests/     # cross-subsystem and architectural tests
├── text/      # characters, coding, syntax, regex, search, and text properties
└── mod.rs     # stable public module-path facade
```

## Rules

- Add a subsystem as `<domain>/<subsystem>/mod.rs`, never as a loose file in
  `emacs_core/` or directly inside a domain directory.
- Keep out-of-line subsystem tests in `<subsystem>/tests/`; small white-box
  unit tests may remain inline in `mod.rs`. Cross-subsystem tests belong in
  `emacs_core/tests/`.
- Put Rust-backed Elisp implementations in the subsystem's `mod.rs` and their
  declarations in `subrs.rs`. Use `define_subrs!` so the const `SubrBatch` and
  its registrar are generated from the same typed declarations (plus typed
  dispatch metadata when the evaluator requires it).
- Treat `emacs_core/mod.rs` as wiring and a compatibility facade. Physical moves
  must not force callers to change stable paths such as
  `crate::emacs_core::eval`.
- Prefer a precise subsystem name over catch-all modules such as `misc` or
  `extra`. Existing compatibility modules are migration boundaries, not homes
  for new code.

The architectural layout tests enforce the no-loose-files rules.
