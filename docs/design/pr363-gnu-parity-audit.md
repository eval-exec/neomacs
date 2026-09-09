# PR 363 GNU terminal parity audit

Date: 2026-09-09. Audited Neomacs commit: `1a16429025c80b221141b417b5c42a7702d2f1ae` (merged PR 363). Comparison: `git diff 1a16429025^...1a16429025`. GNU source: `a360712c9d272d950d8d8255ef74570f7e90b7d9`, the local Emacs 31.1 reference.

Requirement: “what gnu/emacs can do, neomacs need can do too.” Scope is terminal capability loading, numeric expansion, and platform integration relevant to PR 363. This is not a complete editor/GUI parity audit. This records the audit before implementation. The [numeric follow-up](terminfo-numeric-followup.md) fixes the four demonstrated regressions; the older platform gaps below remain open.

## Main conclusion

The new numeric validator introduces a demonstrated GNU-compatibility regression. Linux/macOS capability handling also has older gaps, and Windows does not have equivalent console capability initialization. Keeping ncurses preserves many native behaviors, but it is not by itself proof of cross-platform parity.

## Introduced by PR 363: valid numeric programs are rejected

**P1, `crates/neomacs-terminfo/src/numeric.rs:19` and `:73`.** Division/remainder must immediately follow a nonnegative numeric literal. A conservative total-push limit also rejects programs whose actual stack never approaches the native limit. GNU's `src/terminfo.c:44` forwards programs to native tparm without these restrictions.

The audit fixture compiles real `setaf` entries with tic, obtains expected bytes from ncurses tput, and compares them with the Rust API using the same numeric parameter (7):

| Format | Native output | Neomacs |
| --- | --- | --- |
| `%p1%{2}%{2}%+%/%d` | `1` | InvalidNumericFormat |
| `%p1%'A'%/%d` | `0` | InvalidNumericFormat |
| `%{4}%Pa%p1%ga%/%d` | `1` | InvalidNumericFormat |
| `%p1%d` repeated 20 times, then `%p1%{2}%/%d` | twenty `7` characters, then `3` | InvalidNumericFormat |

These are benign numeric programs with nonzero positive divisors. The last program consumes its values as it goes and has a shallow stack. The regressions do not require string arguments or hazardous arithmetic.

Impact is not limited to an exposed helper: `terminal_capabilities.rs:22` converts the error to None, and `neomacs-display-runtime/src/backend/tty/rif.rs:2903` emits no color sequence for a resolved entry when expansion returns None. Styled underline construction can similarly discard styles if expansion fails. The `ground_sequence` documentation's suggestion that None means a malformed native format is no longer accurate.

Do not fix this by simply removing validation: the native variadic interface still needs protection against string arguments and arithmetic hazards. The replacement must accept GNU-supported numeric behavior while maintaining a justified safety boundary.

## Existing platform gaps, not introduced by PR 363

### Windows color model — P1

`tty_init.rs:63-85` chooses zero colors when TERM is empty, 256 for a name containing 256color, or eight otherwise. Windows always takes this fallback because the database backend is unavailable. GNU's `src/w32console.c:633-655` enables output VT processing and selects 16777216 colors when that succeeds, or 16 for legacy output. This count is visible to Lisp as well as the renderer. See the [Windows audit](pr363-windows-audit.md).

### Windows output mode — P1

`tty_init.rs:318-325` enables crossterm input raw mode, then writes ANSI sequences directly. The reviewed output path does not establish Windows output VT processing or provide GNU's native-console fallback. GNU checks SetConsoleMode and uses separate console drawing hooks. The affected case is an output handle with VT processing initially disabled or unavailable; native Windows execution is still required to measure its visible failure.

A real Windows input path exists through crossterm events. The absence of terminfo key lookup does not prove Windows key handling is broken. Full key/modifier parity remains unverified.

### Non-ANSI terminals — P1 under the stated parity requirement

`terminal_capabilities.rs:497-507` rejects cursor addressing that is not ANSI CSI row/column. GNU's `src/cm.c:455-469` accepts absolute cursor positioning or sufficient relative movement. Installed `vt52` has a valid non-ANSI cup capability: `tput -T vt52 cup 1 2` produced bytes `1b 59 21 22`. Neomacs rejects that encoding. Supporting it requires capability-driven renderer output; weakening the startup check alone would produce incorrect output.

### Missing database or unknown TERM — P2 behavior difference

`terminal_capabilities.rs:79-85` converts load errors to None, and `:498-499` treats that as a successful suitability check. `tty_init.rs:63-85` then guesses capabilities. GNU `src/term.c:4494-4520` instead reports initialization failures. This was an explicit existing Neomacs policy, but it differs from the current strict parity requirement.

### Attribute postprocessing — P2

`terminal_capabilities.rs:150-168` uses raw standout and underline strings without GNU's magic-cookie (`sg`/`ug`) checks, underline-as-standout fallback, or `se`/`me` checks before accepting standout. GNU performs those transformations at `src/term.c:4834-4864`. The current query set also lacks `sg`, `ug`, and `se`. This is a source-confirmed pre-existing gap; end-to-end terminal fixtures have not been run for it.

The Windows fallback additionally advertises styled underlines beyond GNU's Windows initialization. This is recorded as a capability-policy difference in the Windows note, rather than proof that modern Windows terminals cannot render them.

## Standards review

No documented standards violations or new native lookup/linkage defect were found. One maintenance concern remains: the snapshot query list repeats names used in separate resolvers, while an unrequested query is indistinguishable from an absent capability. A new resolver lookup can silently fail if its snapshot query is forgotten. The current query set was checked and covers all current production reads, including key fallback names.

## Executed validation

Command in the private checkout:

```sh
CARGO_TARGET_DIR=target/audit363 cargo nextest run -p neomacs-terminfo --locked --no-fail-fast --success-output immediate
```

Result: **13 tests, 9 passed and 4 failed**. All six existing crate tests passed. The new tests consist of four failing parity comparisons, two passing control comparisons, and a passing installed-database acceptance scan.

The scan checked **3533 capability values representing 86 unique programs** across the database exposed by this host's toe command. No loaded program was rejected. Six enumeration records, representing `ibm327x` and `unknown` repeated across database paths, could not be loaded and were reported separately; tput also reports ibm327x as unknown. This scan establishes format acceptance at selected parameters, not exhaustive output parity or proof that every deployed custom entry works.

The executable fixtures use ncurses tput as the expansion oracle because GNU's terminfo build calls the same native interpreter. They are not full editor screen comparisons. No macOS or Windows runtime tests were performed. Existing Linux native lookup/concurrency tests passed; no complete native memory-safety audit was attempted.

Audit regression tests are retained at `crates/neomacs-terminfo/tests/gnu_numeric.rs`. They failed at the audited commit and pass with the numeric follow-up.

## Recommended order

1. Resolve the introduced numeric-expansion restrictions without exposing unchecked native formats; keep the four regression tests.
2. Implement and test Windows console/VT capability negotiation and output behavior.
3. Restore GNU terminal capability postprocessing and address non-ANSI rendering support.
4. Add native Linux/macOS/Windows coverage, including custom entries and failure paths, before declaring platform parity.
