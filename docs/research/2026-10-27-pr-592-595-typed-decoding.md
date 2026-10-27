# PRs #592 and #595: typed decoding boundaries

Investigated 2026-10-27. GNU checkout revision: `a360712c9d272d950d8d8255ef74570f7e90b7d9`. Read GNU source before investigating the Rust implementation; GNU checkout remained read-only. Reviewed Neomacs heads `ec84e82ce954d45c49586ef809a7bccfcb6341b0` (#592) and `fa4f4d20555a3ceb77aabbe429a3f180d996c2c5` (#595). These are a stack: #595 targets #592's branch.

## GNU contract

GNU distinguishes character-domain validity from encoding validity. `MAX_CHAR` is `0x3fffff`. Canonical five-byte characters occupy `0x200000..=0x3fff7f`; `0x3fff80..=0x3fffff` represents raw bytes using two-byte C0/C1 forms. `multibyte_length` validates encoding and available length. `BYTES_BY_CHAR_HEAD` only determines width. `string_char_and_length` performs arithmetic under assumptions that include a valid leading byte and the appropriate character range. Consequently, continuation-shaped bytes alone do not establish its precondition. [GNU character.h, encoding and decoders](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/character.h#L30-L59) ([validator and decoder](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/character.h#L291-L409)).

`char_string` emits five bytes only through `MAX_5_BYTE_CHAR`; larger valid codes use `BYTE8_STRING`. Invalid codes eventually signal an error, after modifier processing. This supports separating a valid character value from its canonical encoding. [GNU character.c](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/character.c#L98-L142).

`source_file_get` reads the width indicated by the first byte, checks continuations, then calls `STRING_CHAR`. If a continuation is missing or malformed, it pushes subsequent bytes back and returns the raw-byte character for the lead. This path has a validation gap for oversized but continuation-shaped five-byte sequences. The decoder's assumption is not an additional validation step. [GNU lread.c](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/lread.c#L443-L477).

`eassume` makes the compiler assume its predicate even when checking is disabled; checking builds can abort on violation. Results after violating that assumption cannot establish a portable GNU compatibility requirement. [GNU lisp.h](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/lisp.h#L176-L201).

## Local oracle observations

Generated fixtures and captured output are under `tmp/pr595/gnu-probes/`, including `results.json`. Probes used the checkout's `src/emacs -Q --batch`, loaded fixtures with `load-source-file-function` bound to nil, and supplied a lexical-binding cookie so the probe reaches the raw file reader consistently. Every fixture prints the character list of its literal. These are observations from this binary, not guarantees for malformed encodings.

| Bytes in literal | GNU character list |
| --- | --- |
| `F8 88 80 80 80` | `(2097152)` |
| `F8 8F BF BD BF` | `(4194175)` |
| `80 80` | `(4190080)` |
| `F8 B8 96 B8 96` | `(248 136 150 184 150)` |
| `F8 88 80 41 80` | `(4194296 4190592 65 4194176)` |
| `F8 88 80`, followed by the literal's closing quote | `(4194296 4190592)` |

The last fixture exercises a malformed continuation at a literal boundary, not EOF during a raw file character. No claim about GNU's arbitrary-memory handling of declared, truncated multibyte strings follows from these probes.

## Rust invariant and the review disagreement

At the reviewed #595 head, the file branch still admits `F8 B8 96 B8 96` using a width/continuation screen, then calls the newly typed unchecked decoder. Its raw payload is `0xe16e16`, outside `EmacsChar`'s advertised domain. Debug builds assert; optimized builds can retain an invalid wrapper because `debug_assert!` is disabled by default there. This is a correctness invariant violation; a memory-safety claim would require a separate unsafe downstream use. [Rust debug_assert documentation](https://doc.rust-lang.org/std/macro.debug_assert.html).

The two review suggestions are not behaviorally equivalent. Returning raw arithmetic can preserve existing reader behavior, but must validate the result before constructing `EmacsChar`. Replacing the call with `string_char` also validates the domain, but its masked arithmetic changes some malformed inputs: `80 80` becomes zero rather than the existing raw decoder's `0x3fef80`. GNU's observed behavior matches the latter in this checkout, while its head precondition excludes that input. This distinction argues for an explicit compatibility decision instead of treating a change of decoder as a mechanical refactor. [Reviewed reader source](https://github.com/eval-exec/neomacs/blob/fa4f4d20555a3ceb77aabbe429a3f180d996c2c5/crates/neovm-core/src/emacs_core/runtime/value_reader/mod.rs#L3527-L3547), [reviewed decoder source](https://github.com/eval-exec/neomacs/blob/fa4f4d20555a3ceb77aabbe429a3f180d996c2c5/crates/neovm-core/src/emacs_core/text/emacs_char/mod.rs#L772-L889).

## Design recommendation

Use `EmacsChar` at consumer boundaries, with validation owned by the decoder that establishes it. Rust newtypes prevent accidental interchange of a raw integer and a validated character only when constructors preserve the invariant. Keep raw arithmetic narrow and explicit; avoid expanding raw `u32` plumbing across regex, layout, or readers. [Rust newtype guidance](https://doc.rust-lang.org/book/ch20-03-advanced-types.html#type-safety-and-abstraction-with-the-newtype-pattern).

For the file-reader fix, preserve its established width/continuation and raw-arithmetic behavior, then check the character domain before producing a typed result. On failure, consume the lead byte only and return its raw-byte character. This is a recommended Neomacs recovery policy, inferred from the existing malformed-tail policy; GNU does not define the oversized case. Carry the selected character and actual consumed width together so a fallback cannot accidentally discard four following bytes.

Keep declared-multibyte string decoding separate: its documented bounded zero-padding policy intentionally emulates a different source contract. Do not replace it with strict file validation. Likewise, canonical-encoding validation and character-domain validation are separate operations: rejecting every noncanonical but in-domain sequence would exceed this safety repair's scope. [Reviewed declared decoder](https://github.com/eval-exec/neomacs/blob/fa4f4d20555a3ceb77aabbe429a3f180d996c2c5/crates/neovm-core/src/heap_types.rs#L1467-L1511).

Do not introduce `unsafe` merely to mark a logical precondition. If an invariant is later relied on for memory safety, safe inputs must not be able to violate that reliance; Rust's unsafe boundary documents and isolates contracts the compiler cannot check. [Rustonomicon](https://doc.rust-lang.org/nomicon/safe-unsafe-meaning.html).

## Verification implications

Regression tests should first demonstrate the oversized file-reader panic and verify recovery plus the next unread bytes. Preserve canonical five-byte boundaries, C0/C1 bytes, malformed continuations, literal delimiters, EOF truncation, noncanonical in-domain arithmetic, and declared-string padding behavior. Keep oracle assertions on supported GNU inputs; use explicit Neomacs policy expectations for violated GNU assumptions. TUI/GUI loading verifies integration, but cannot replace reader-level byte and position assertions.

The following checkpoint records implementation and executed verification separately from tests that remain pending.

## Implemented repair

The implementation now keeps raw arithmetic private to the character module. Two source contracts remain explicit: `decode_file_multibyte_char` returns `Option<(EmacsChar, usize)>`, with `None` representing file EOF, while `decode_declared_multibyte_char` returns a typed character and preserves bounded padding for supplied-count strings. The heap module reexports the declared decoder to retain its existing accessor seam. These APIs share domain validation without conflating source semantics. [Character decoders](../../crates/neovm-core/src/emacs_core/text/emacs_char/mod.rs), [heap accessor](../../crates/neovm-core/src/heap_types.rs).

Domain validation in `string_char_unchecked` is unconditional, including optimized builds; bounds and continuation shape still belong to its caller. Out-of-domain arithmetic returns the lead-byte character and a consumed width of one. The FileBytes reader advances by this returned width. Unibyte conversion also respects the actual returned width, and byte8 escaping bounds a lone C0/C1 lead before decoding. These fixes preserve subsequent initialized bytes rather than advancing by an invalid sequence's advertised width. [File reader](../../crates/neovm-core/src/emacs_core/runtime/value_reader/mod.rs), [conversion and escaping](../../crates/neovm-core/src/emacs_core/text/emacs_char/mod.rs).

The caller audit exposed a second boundary: shorthand-created symbol names can retain a supplied character count that disagrees with physical decoding. Symbol printing continues to traverse physical bytes (`SBYTES`), now using the declared decoder for bounded tails. Deletion traverses the supplied character count (`SCHARS`), clamps copied spans to initialized storage, and retains `StringCharacterCountKind::Computed` versus `Supplied` when rebuilding a string. The original string is returned unchanged when nothing is removed. [Printer](../../crates/neovm-core/src/emacs_core/lisp/print/mod.rs), [delete](../../crates/neovm-core/src/emacs_core/lisp/native/builtins/cons_list.rs).

GNU independently establishes the traversal distinction: symbol printing loops to `SBYTES(name)`, whereas string deletion loops to `SCHARS(seq)`. The physical bounds and zero-padding behavior for counts beyond the payload remain explicit Neomacs policies; GNU's access to terminators or other storage is not modeled as a portable contract. [GNU symbol printing](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/print.c#L2471-L2524), [GNU deletion](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/fns.c#L2187-L2245).

## Verification evidence as of 2026-10-27

Development and verification after the remote-only instruction run on `ssh treeroot`, in `/home/exec/Projects/github.com/eval-exec/neomacs/tmp/pr595-review-20261027`. Task artifacts use that worktree's `tmp/pr595/`. The original GNU observations above predate that instruction.

The TDD evidence includes the FileBytes regression, byte8 conversion/escaping regressions, and five printer/deletion caller failures. Initial caller attempts stopped at a missing generated `cl-loaddefs.el`; those attempts were prerequisite failures, not the claimed caller RED. After running the repository's loaddefs generator with GNU against the Neomacs worktree, `callers-ready-red.log` records the meaningful caller RED (nine passing tests and five failing caller tests). The generator left the GNU checkout and tracked `ldefs-boot.el` unchanged.

| Verification | Observed result | Remote artifact |
| --- | --- | --- |
| All new focused core regressions | 14 passed | `all-regressions-green.log` |
| Broader affected unit selection | 786 run: 781 passed, 5 failed | `affected-unit-suite.log` |
| Same five failures with production restored to reviewed head `fa4f4d20555a3ceb77aabbe429a3f180d996c2c5` | All five reproduced | `unmodified-production-control.log`, `unmodified-production-control.json` |
| Public GNU supplied-count deletion probe | Exit 0; exact expected result | `gnu-shorthand-delete/result.json` |
| Live GNU oracle integration | 2 passed: encoded file boundaries and supplied-count deletion | `live-oracle-final.log` |
| TUI integration | 2 passed: GNU display parity and oversized recovery, followed by real PTY typing | `tui.log` |
| X11 GUI integration | 1 passed: native typing, frame snapshot, and green-pixel presentation | `gui-x11-ci-artifacts.log` |
| Workspace and all Rust targets | Check passed | `workspace-check.log` |
| Rust formatting and patch whitespace | Passed | `fmt-check-final.log` and `git diff --check` |
| Nix development shell syntax | Parsed successfully; includes the X11 input tool | `nix-dev-shell-parse.log` |

The five broader failures are `delete_char_handles_nonunicode_storage_units`, `delete_char_handles_unibyte_high_bytes`, `vm_delete_char_uses_shared_read_only_and_narrowing_state`, `delete_indentation_rejects_too_many_args`, and `lisp_case_folded_search_engages_the_fast_scans`. Reproducing them with unmodified production separates existing failures from this repair; it does not turn that broader run into an all-passing result.

The GNU deletion probe extracts the public Lisp form from `delete_does_not_visit_bytes_outside_the_supplied_character_count`. It creates a shorthand-expanded symbol name containing initialized lambda-plus-z bytes but a supplied character count of one, then deletes z. GNU returns the original string without visiting the extra physical byte:

```text
OK (1 t 1 (206 187 122) (206 187 122))
```

The remote probe's `form.el`, stdout, empty stderr, and command/result JSON are preserved under `tmp/pr595/gnu-shorthand-delete/`. This supports the below-SCHARS traversal assertion. The above-physical-count case remains a bounded Neomacs policy test.

The new editor fixtures use handcrafted `.elc` headers so Neomacs reaches FileBytes directly. A `.el` source file still passes through source decoding even when `load-source-file-function` is nil, so it would not establish this regression boundary. Oracle coverage compares supported GNU inputs; oversized F8 recovery is checked only in Neomacs. TUI coverage requires subsequent PTY typing; GUI coverage requires subsequent X11 typing plus frame-snapshot and pixel presentation. All five integration checks passed. The GUI driver discovers the editor by its process ID, explicitly focuses its owned X11 window, and injects the key through XTest; it does not rely on a Lisp title becoming a native window title. GUI artifacts live under `tmp/neomacs-gui-tests/` so the existing CI upload captures them.


The first oracle attempt used the plain debug executable without runtime images and exhausted the 60-second test budget during source preload. The supported producer invocation (`neomacs-temacs --batch --no-build-details -l loadup --temacs=pdump`) subsequently generated an isolated final image from the same compiled binary under `tmp/pr595/runtime/`. With that image, both oracle cases completed in about four seconds each; no test timeout was increased. Producer commands, copied-binary hashes, and output are recorded in `produce-debug-runtime.sh`, `runtime/binary.sha256`, and `runtime-producer.log`. The TUI and GUI runs use that same executable/image pair via `runtime-env.sh`.
