# Numeric terminfo follow-up

Base: merged PR #363, `1a16429025c80b221141b417b5c42a7702d2f1ae`.
Branch: `fix/gnu-terminal-parity`.

## Scope

Fix the four introduced numeric rejections demonstrated in the
[GNU parity audit](pr363-gnu-parity-audit.md). Retain native lookup and expansion,
add no unsafe code or vendored interpreter, and use cargo nextest for tests.
This is the first follow-up: Windows console negotiation/legacy output,
non-ANSI rendering, initialization failures, and attribute postprocessing remain
separate open backend work. This branch does not establish full GNU parity.

## Change

The original validator required a literal immediately before division/remainder
and counted every push in the entire program. This rejected computed constants,
character constants, locally assigned variable divisors, and long programs whose
stack stays shallow. The existing renderer treats rejected expansion as absent
output, so these restrictions can suppress configured colors or underline styles.

The regex still validates every directive, including unreachable branches, to
exclude pointer-consuming formats and malformed numeric tokens. Only programs
containing division/remainder need the additional proof. It tracks known integer
constants and maximum live stack depth through each forward control-flow path.
Disagreeing branches, inherited variables, parameters, and overflowing arithmetic
produce unknown values. Both paths must be safe, regardless of the actual input.
The proof accepts character, computed, and locally assigned constant divisors.
It rejects any divisor that could cause native INT_MIN / -1 or INT_MIN % -1.
Zero divisors retain ncurses' result of zero. Stack overflow is rejected because
ncurses drops pushes, which could otherwise discard a checked divisor.

The proof follows ncurses' byte-oriented conditional skipping and rejects jumps
into the middle of a validated token. It does not produce output or read/write
native variable state. ncurses continues to own formatting, termcap parameter
inference, and variable lifetime. No changes to the unsafe module or linkage.

## Tradeoff and remaining limits

This adds a small abstract stack analysis, so it is more code than the original
blanket guard. The researched published Rust expanders are not drop-in GNU
replacements: arithmetic, boolean handling, variable semantics, or padding differ.
The newer native `tiparm_s` checks argument types but still performs the same
potentially trapping signed division. Neither choice eliminates this boundary
without additional compatibility work. See the [crate survey](rust-terminfo-project-precedents.md)
and [native research](terminfo-native-research.md).

The proof deliberately does not evaluate supplied parameters or inherited native
variables. For example `%p1%p2%/%d` is still unsupported even when a particular
call supplies a safe second parameter. It also rejects some safe programs whose
branches establish different safe divisors, whose arithmetic overflows before a
safe divisor, or whose implicit parameter reserve exceeds the stack bound.
This repairs the demonstrated regressions; it is not complete numeric acceptance.
Widths, precision, constants, and string-parameter restrictions remain unchanged.

## Validation

The regression fixtures compile custom entries with `tic`, then compare output
with `tput`, the ncurses interpreter GNU calls through `tparm`. Tests include
computed/character/variable divisors, branch joins and nesting, signed and zero
division, remainder, and long shallow programs. Safety tests reject native traps,
unknown state, unsafe branch joins, and overflowing stacks.

The host-database scan is opt-in because it depends on installed entries. Run:

```sh
cargo nextest run -p neomacs-terminfo --locked
cargo nextest run -p neomacs-terminfo --locked --run-ignored only
cargo clippy -p neomacs-terminfo --all-targets --locked -- -D warnings
```

CI adds native fixture tests on Linux, Apple system ncurses, and Homebrew ncurses.
macOS jobs are added but have not been executed locally. Windows still uses the
unsupported-platform boundary; this change does not add a Windows backend.

Local Linux results: 15 focused tests passed in debug and release profiles; the opt-in host scan accepted 3533
capability values covering 86 distinct programs (six unavailable enumeration
records reported separately). Clippy passed with warnings denied. All four
split/unsplit shared/static linkage fixtures passed. The CI workflow passed
actionlint. These results cover the changed crate, not a full editor test run.

Independent standards and spec reviews found no actionable findings.
