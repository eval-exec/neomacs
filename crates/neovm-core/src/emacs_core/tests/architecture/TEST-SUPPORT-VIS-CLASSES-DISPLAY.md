# DISPLAY — per-item class table for the wave-2 TEST-SUPPORT-VIS rows (B15-1)

Companion to `TEST-SUPPORT-VIS-DISPLAY.tsv`.  Three wave-2 A' rows, reviewed
against the three-way rule (lead #6537 / PEER 5812).

Paths are relative to `crates/neovm-display/src/emacs_core/display/`.
Line numbers are as of the B15-1 commit.

| item | kind | class | evidence |
|---|---|---|---|
| `fontset::DEFAULT_FONTSET_NAME` | const | 1 | production caller: fontset/mod.rs:99 (`default_name`), :210/:213 (default-fontset checks), :773/:792 (`resolve_fontset_name_arg`), :807 (`matching_entries_for_char`) |
| `fontset::matching_entries_for_fontset` | fn | 1 | production caller: `matching_entries_for_char` (fontset/mod.rs:806) calls it at :807; the function is at :811 |
| `font::alternative_font_registries` | fn | 3 | deleted.  No production reader: `alternative_font_registry_alist` is written by the `internal-set-alternative-font-registry-alist` builtin (xfaces/mod.rs:5401 -> :5426) but only this test-only lookup model read it, so its tests asserted a parallel model.  The xfaces setter tests and the moved GC/TLS test now assert the production store through `alternative_font_registry_alist_snapshot_for_test` (font/mod.rs:279). |

## Ledger effect

- Rows: 59 -> 58 (the deleted class-3 model).  The ceiling stays 59; it is
  never taken below the row count.
- The two rule-1 items keep their rows: production calls them, and the
  test-support visibility only exposes those production functions to the moved
  core tests.
