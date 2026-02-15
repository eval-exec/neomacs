# NeoVM / Neomacs Plan

Last updated: 2026-02-15

## Done

- Full suite re-verified after JSON keyword-error and display batch-semantics slices:
  - `cd test/neovm/vm-compat && make check-all-neovm` (pass)
- Full suite re-verified after JSON buffer builtin routing + cl_lib JSON cleanup:
  - `cd test/neovm/vm-compat && make check-all-neovm` (pass)
- Full suite baseline verified after JSON compatibility slice:
  - `cd test/neovm/vm-compat && NEOVM_ORACLE_EMACS=/nix/store/hql3zwz5b4ywd2qwx8jssp4dyb7nx4cb-emacs-30.2/bin/emacs make check-all-neovm` (pass)
- Full suite baseline verified after `set-time-zone-rule` / `current-time-zone` slice:
  - `cd test/neovm/vm-compat && NEOVM_ORACLE_EMACS=/nix/store/hql3zwz5b4ywd2qwx8jssp4dyb7nx4cb-emacs-30.2/bin/emacs make check-all-neovm` (pass)
- Full suite baseline verified after `safe-date-to-time` slice:
  - `cd test/neovm/vm-compat && NEOVM_ORACLE_EMACS=/nix/store/hql3zwz5b4ywd2qwx8jssp4dyb7nx4cb-emacs-30.2/bin/emacs make check-all-neovm` (pass)
- Full suite baseline verified after latest cleanup:
  - `cd test/neovm/vm-compat && make check-all-neovm` (pass)
- Full suite re-verified after terminal parameter semantics slice:
  - `cd test/neovm/vm-compat && NEOVM_ORACLE_EMACS=/nix/store/hql3zwz5b4ywd2qwx8jssp4dyb7nx4cb-emacs-30.2/bin/emacs make check-all-neovm` (pass)
- Added a builtin-registry-wide `fboundp` parity checker against GNU Emacs `-Q`:
  - `test/neovm/vm-compat/check-builtin-registry-fboundp.sh`
  - allowlist: `test/neovm/vm-compat/cases/builtin-registry-fboundp-allowlist.txt`
- Added `make` target:
  - `make -C test/neovm/vm-compat check-builtin-registry-fboundp`
- Wired the parity gate into CI:
  - `.github/workflows/vm-compat.yml`
- Documented the gate in:
  - `test/neovm/vm-compat/README.md`
  - `docs/elisp-vm-design.md`
- Removed multiple unexposed/stale builtin implementations to keep dispatch surface aligned with the registry and oracle policy:
  - `custom-group-p` implementation cleanup
  - `selected-terminal` implementation cleanup
  - `display-line-numbers-update-width` implementation cleanup
  - `register-to-string` implementation cleanup
  - `extract-rectangle-line` implementation cleanup
  - removed stale misc stubs superseded by evaluator paths
  - removed unexposed bookmark helper builtin implementations
  - removed unexposed `string-repeat` helper implementation
  - removed unexposed `bool-vector-complement` implementation
  - removed unexposed `base64url-decode-string` helper
  - removed unexposed `hash-table-keys` / `hash-table-values` helpers
  - removed unexposed `downcase-char` helper
  - removed unexposed `find-coding-system` helper
  - removed unexposed `word-at-point` helper
  - removed stale, unreferenced `elisp/terminal` shim module files
  - removed stale, unreferenced pure-JSON buffer stub helpers from `elisp/json.rs` (`json-insert`/`json-parse-buffer`; replaced by active evaluator-backed implementations in the JSON module)
  - removed duplicate legacy JSON parser/serializer + tests from `elisp/cl_lib.rs` so `elisp/json.rs` is the single JSON implementation path
- Implemented `terminal-parameter` / `set-terminal-parameter` compatibility semantics:
  - persisted terminal parameter values for symbol keys
  - `terminal-parameter` enforces `symbolp` for PARAMETER
  - `set-terminal-parameter` returns `nil` (and ignores non-symbol keys)
  - invalid terminal designators now signal `wrong-type-argument` with `terminal-live-p`
  - `terminal-live-p` now returns `nil` for invalid designator shapes
  - `terminal-name` now rejects invalid terminal designators
  - switched terminal designators to an opaque internal terminal handle
  - `frame-terminal` now rejects invalid frame designators (`wrong-type-argument` on non-nil non-frame)
- Added and enabled new oracle corpus:
  - `test/neovm/vm-compat/cases/terminal-parameter-semantics.forms`
  - `test/neovm/vm-compat/cases/terminal-parameter-semantics.expected.tsv`
  - wired into `test/neovm/vm-compat/cases/default.list`
- Implemented `safe-date-to-time` compatibility subset:
  - parse and convert explicit-offset date formats (ISO/slash and RFC-style)
  - return `(HIGH LOW)` for valid dates, integer `0` for non-parseable/non-string inputs
  - preserve wrong-arity signaling
- Added and enabled new oracle corpus:
  - `test/neovm/vm-compat/cases/safe-date-to-time-semantics.forms`
  - `test/neovm/vm-compat/cases/safe-date-to-time-semantics.expected.tsv`
  - wired into `test/neovm/vm-compat/cases/default.list`
- Implemented `set-time-zone-rule` / `current-time-zone` compatibility subset:
  - process-level time zone rule state (`nil`/`wall`, `t`, integer offsets, TZ strings)
  - deterministic offset-name rendering for fixed offset rules
  - invalid specification signaling (`error "Invalid time zone specification"`)
  - `current-time-zone` now honors explicit `ZONE` argument overrides
- Added and enabled new oracle corpus:
  - `test/neovm/vm-compat/cases/time-zone-rule-semantics.forms`
  - `test/neovm/vm-compat/cases/time-zone-rule-semantics.expected.tsv`
  - wired into `test/neovm/vm-compat/cases/default.list`
- Implemented JSON compatibility adjustments:
  - `json-serialize` alist key typing now follows Emacs `symbolp` expectations
  - `json-parse-string` `:object-type 'alist` now emits symbol keys
  - parser now signals `json-end-of-file` for empty/whitespace-only input
  - trailing-content signal now uses `json-trailing-content`
- Added and enabled new oracle corpus:
  - `test/neovm/vm-compat/cases/json-semantics.forms`
  - `test/neovm/vm-compat/cases/json-semantics.expected.tsv`
  - wired into `test/neovm/vm-compat/cases/default.list`
- Aligned evaluator-backed buffer JSON builtins with Emacs behavior:
  - `json-parse-buffer` now parses from current point, allows trailing content, and advances point to the end of parsed value
  - `json-insert` now uses `json-serialize` keyword semantics (`:null-object`, `:false-object`)
  - dispatch for `json-parse-buffer` / `json-insert` now routes to `elisp/json.rs` implementations
- Aligned JSON keyword-argument error semantics with Emacs:
  - odd JSON plist tails now signal `(wrong-type-argument plistp ...)`
  - unknown JSON keyword args now signal `error` with Emacs-compatible message + offending value payload
  - invalid `:array-type` / `:object-type` values now signal Emacs-compatible `error` payloads
- Aligned display query builtins with Emacs batch-context defaults:
  - display queries now report terminal-style values in vm-compat context (`display-graphic-p`, `display-color-p`, pixel/mm size, color cells/planes/class/backing store, `x-display-list`)
  - preserved terminal-handle parameter semantics separately (`terminal-parameter` corpus remains green)
- Aligned terminal handle and terminal-name presentation semantics:
  - `terminal-name` now returns `\"initial_terminal\"` for the current terminal
  - `prin1-to-string` / `format` / error-message rendering now print terminal handles as `#<terminal ...>` (parity with GNU Emacs handle style)
- Added and enabled new oracle corpus:
  - `test/neovm/vm-compat/cases/json-buffer-semantics.forms`
  - `test/neovm/vm-compat/cases/json-buffer-semantics.expected.tsv`
  - wired into `test/neovm/vm-compat/cases/default.list`
- Added and enabled new oracle corpus:
  - `test/neovm/vm-compat/cases/json-keyword-errors.forms`
  - `test/neovm/vm-compat/cases/json-keyword-errors.expected.tsv`
  - wired into `test/neovm/vm-compat/cases/default.list`
- Added and enabled new oracle corpus:
  - `test/neovm/vm-compat/cases/display-batch-semantics.forms`
  - `test/neovm/vm-compat/cases/display-batch-semantics.expected.tsv`
  - wired into `test/neovm/vm-compat/cases/default.list`
- Added and enabled new oracle corpus:
  - `test/neovm/vm-compat/cases/terminal-handle-printing.forms`
  - `test/neovm/vm-compat/cases/terminal-handle-printing.expected.tsv`
  - wired into `test/neovm/vm-compat/cases/default.list`
- Aligned monitor attribute builtins with batch-style oracle semantics:
  - `display-monitor-attributes-list` / `frame-monitor-attributes` now return batch-size geometry/workarea `(0 0 80 25)`, `mm-size` as `(nil nil)`, and stable key shape (`geometry`, `workarea`, `mm-size`, `frames`)
  - invalid non-`nil` monitor designators now signal `error` (oracle-aligned class for `get-device-terminal` argument failures)
  - `frame-monitor-attributes` accepts terminal handles for terminal-associated monitor lookup semantics
- Added and enabled new oracle corpus:
  - `test/neovm/vm-compat/cases/display-monitor-semantics.forms`
  - `test/neovm/vm-compat/cases/display-monitor-semantics.expected.tsv`
  - wired into `test/neovm/vm-compat/cases/default.list`
- Aligned generic value printer with terminal-handle rendering:
  - top-level/result printing now renders terminal handles as `#<terminal ...>` instead of raw vector internals
  - byte-printing path (`print_value_bytes`) now mirrors the same terminal-handle representation
- Expanded oracle corpus for terminal-handle top-level rendering:
  - `test/neovm/vm-compat/cases/terminal-handle-printing.forms` now includes raw `(frame-terminal)` / `(car (terminal-list))` output checks
  - refreshed `test/neovm/vm-compat/cases/terminal-handle-printing.expected.tsv`
- Kept branch green with targeted Rust tests and vm-compat checks after each slice.

## Doing

- Continue compatibility-first maintenance with small commit slices:
  - keep builtin surface and registry in lock-step
  - run oracle/parity checks after each behavior-affecting change
  - remove dead helper code that is not part of exposed compatibility surface
- Reduce vm-compat operator friction for large case sets (small Makefile UX improvements).
  - added list-driven targets: `record-list`, `check-list`, `check-neovm-list` with `LIST=cases/<name>.list`

## Next

1. Run a full vm-compat verification pass (`check-all-neovm`) after the latest cleanup batch and record results.
2. Expand focused oracle corpora for remaining high-risk areas still carrying stubs (search/input/minibuffer/display edges).
3. Prioritize one high-impact stub-to-real implementation slice with oracle lock-in, then repeat.
4. Keep Rust backend behind compile-time switch and preserve Emacs C core as default backend.
