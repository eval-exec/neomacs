# Terminfo implementation research

Research date: 2026-09-09. Neomacs baseline: PR #363 at `7384bd804`. GNU source examined locally at `/home/exec/Projects/github.com/emacs-mirror/emacs`, commit `a360712c9d272d950d8d8255ef74570f7e90b7d9`.

## GNU behavior to preserve

GNU `src/terminfo.c` wraps native tparm and copies its output. In `src/term.c`, color precedence is packed setf24/setb24, separate setrgbf/setrgbb, the terminfo RGB flag, then Tc/COLORTERM. The Tc fallback literal contains an unmatched `%;`, which ncurses accepts. Smulx and smxx use terminfo string names; Su uses termcap boolean lookup. These details make a strict new interpreter or a mechanical name substitution potentially incompatible. [GNU terminfo.c](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/terminfo.c), [GNU term.c](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/term.c)

ncurses adjusts termcap `me` relative to raw `sgr0` for alternate-character-set reset compatibility. Retaining native lookup preserves that behavior. [ncurses termcap manual](https://invisible-island.net/ncurses/man/curs_termcap.3x.html)

## Published Rust candidates

We inspected published sources and ran focused expansion probes. The failures below describe the evaluated versions, not all future releases or a general judgment of their projects.

| Candidate | Relevant evidence | Decision |
| --- | --- | --- |
| terminfo 0.9.0 | Used by WezTerm's termwiz; its expander rejects GNU's unmatched `%;` and has incorrect logical negation. Its compiled database parser contains unsafe code. | Established adoption, but not a drop-in replacement. |
| term 1.2.1 | Rust project ancestry; upstream marks it unmaintained. Probes exposed p0/division-by-zero panics, negative boolean differences, and loss of bare dollar text. | Do not adopt or maintain a local fork. |
| nshterm 0.1.2 | Published derivative of term; negative logical negation produced 1 for -1, and incomplete `%p` returned empty success. | Does not solve the compatibility requirement. |
| tinf 0.14.0 | Rejects constants at least 10000, including the 65536 used for packed RGB extraction. | Does not cover existing rendering inputs. |
| terminfokit 0.1.0 | Alpha, forbids unsafe; exposes parsing/analysis but uses i64 arithmetic and errors on division by zero. | Interesting future candidate; differs from ncurses' numeric behavior. |

Primary sources: [terminfo 0.9.0](https://docs.rs/crate/terminfo/0.9.0/source/), [termwiz manifest](https://github.com/wezterm/wezterm/blob/main/termwiz/Cargo.toml), [term 1.2.1](https://docs.rs/crate/term/1.2.1/source/), [nshterm 0.1.2](https://docs.rs/crate/nshterm/0.1.2/source/), [tinf 0.14.0](https://docs.rs/crate/tinf/0.14.0/source/), [terminfokit 0.1.0](https://docs.rs/crate/terminfokit/0.1.0/source/).

The selected implementation uses published regex for numeric syntax validation and ncurses for interpretation. No candidate interpreter is vendored or patched. This balances the preference for less hand-written Rust against GNU compatibility; it is not an assertion that no useful Rust terminal crate exists.

## Native contracts

The old application code had a cumulative 32768-byte output arena with no capacity check, unsynchronized global native access, and an eight-c_int tparm call. The new boundary removes that arena, serializes all operations and copies, and supplies nine c_long values. ncurses tparm and termcap queries use global state; a per-object mutex would not be sufficient. [Threading contracts](https://invisible-island.net/ncurses/man/curs_threads.3x.html), [tparm ABI](https://invisible-island.net/ncurses/man/curs_terminfo.3x.html)

Correct argument width alone is insufficient: `%s` and `%l` can make numeric parameters into pointers. The Rust wrapper checks the entire byte sequence against a numeric-only grammar before calling C. It accepts GNU's unmatched `%;`, caps formatting widths/precision and numeric constants, and rejects string conversions, NULs, and unsupported directives. It does not implement or fully validate stack-machine semantics.

For snapshots, set_curterm(NULL) prevents reuse of the previous terminal by setupterm; tgetent(NULL, name) reuses the same ncurses termcap cache slot. That cache owns and reclaims the previous entry. Calling del_curterm separately would leave its cache with a dangling owner. All returned strings are copied under the same lock, after null/error-sentinel checks. This relies specifically on ncurses, including Apple's ncurses implementation, rather than generic historical BSD termcap. [ncurses lib_termcap.c](https://github.com/mirror/ncurses/blob/master/ncurses/tinfo/lib_termcap.c), [Apple ncurses source](https://github.com/apple-oss-distributions/ncurses)

The lock coordinates this crate's calls only. Native expansion variables remain attached to ncurses' current context, while query snapshots remain independent. Environment mutation and unrelated native bindings require separate process-wide coordination and are outside the interface.

## Validation limits

Checked-in fixtures cover extended/canceled capabilities, native error sentinels, repeated queries exceeding the old arena, interleaved snapshots, failure recovery, and concurrency. Numeric cases include GNU color literals, packed RGB, negative boolean values, zero division, signed overflow, padding text, and the ninth argument. Linkage fixtures cover actual Cargo dependency consumers using split/unsplit shared/static libraries.

These focused Linux checks do not replace a native sanitizer audit or macOS runtime tests. Before adopting a Rust backend, compare database discovery, termcap normalization, and expansion against ncurses across the supported terminal corpus.

Numeric division and remainder require an immediately preceding nonnegative
literal divisor, such as `%{256}%/`. Computed divisors are rejected because
native signed division can trap on INT_MIN / -1. Constants are validated on
directive boundaries so escaped literal text is preserved.

For formats containing division, the validator also bounds all possible stack
pushes to 19 with explicit parameters or 10 with implicit parameters. This
reserves space in ncurses' 20-slot stack, including up to nine implicit values;
an overflowing push must not drop the checked divisor. See
[ncurses stack definition](https://github.com/mirror/ncurses/blob/master/ncurses/term.priv.h)
and [interpreter](https://github.com/mirror/ncurses/blob/master/ncurses/tinfo/lib_tparm.c).
