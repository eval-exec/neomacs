# Numeric terminfo follow-up

Base: merged PR #363, `1a16429025c80b221141b417b5c42a7702d2f1ae`.
Branch: `fix/gnu-terminal-parity`.

## Change

The original guard rejected computed constants, character divisors, locally
assigned variables, and long programs with shallow stacks. The first follow-up
accepted these through a constant analysis. The continuation accepts safe calls
using actual parameters, inherited variables, wrapping arithmetic, and branches
with differing divisors. It also models native stack overflow instead of rejecting
all overflowing programs.

The regex checks every directive, including unreachable branches, to exclude
pointer-consuming formats, embedded NULs, malformed constants and unsupported
conversions. Only division/remainder programs need arithmetic preflight. Native
ncurses remains responsible for output formatting and variable updates.

Preflight runs under the native mutex. A generated numeric read-only probe obtains
referenced native variables, then Rust evaluates the supplied parameters and
executed conditional path before the real call. The lock spans all three steps;
no other database load or expansion can change the context between them.
Lowercase variables reset per call in newer ncurses and persist in Apple's older
version; the probe observes the respective initial state without assigning it.
Rejected expansions do not commit the program's variable assignments.

The safety check follows native byte-oriented conditional skipping and rejects
jumps into validated tokens. It uses ncurses' 20-slot stack, drops overflowing
pushes, supplies zero on underflow, and increments parameters only once for `%i`.
Division by zero produces zero, as ncurses does. Actual `INT_MIN / -1` and
`INT_MIN % -1` are rejected before entering C.

## Remaining limits and assumptions

Explicit `%p` programs use the actual initial empty stack. For implicit termcap
programs, preflight checks every possible initial argument count from zero to
nine, because native versions infer that count differently. A call can still be
rejected if an impossible count would trap. This is a conservative restriction,
not complete numeric acceptance.

Addition, subtraction, multiplication, and `%i` use 32-bit wrapping arithmetic,
matching the supported native implementations observed in compatibility tests.
Signed overflow is undefined in abstract C; this does not establish safety for
arbitrary compilers or other tparm implementations. Width and precision remain
bounded at 10000; numeric constants must fit a nonnegative i32. String parameters
remain unsupported. Native variables belong to the current native terminal
context, not individual capability snapshots.

Published Rust expanders were evaluated in the [crate survey](rust-terminfo-project-precedents.md).
Their arithmetic, boolean, formatting, or variable behavior differed from GNU's
native interpreter. We retain published `regex` and ncurses, with no vendored
interpreter and no additional unsafe blocks or FFI declarations.

## Validation

Fixtures compile custom entries with `tic` and compare output with `tput`, the
ncurses interpreter used by GNU's terminfo build. Cases cover computed and
character divisors, actual parameters, branch selection, native variable lifetime,
zero division, wrapping arithmetic, dropped pushes, repeated `%i`, concurrency,
and rejected traps without state mutation.

```sh
cargo nextest run -p neomacs-terminfo --locked
cargo nextest run -p neomacs-terminfo --locked --run-ignored only
cargo clippy -p neomacs-terminfo --all-targets --locked -- -D warnings
```

The host-database scan is opt-in because it depends on installed entries. CI covers
Linux, Apple system ncurses, and Homebrew ncurses. Latest platform checks and
remaining GNU compatibility work are recorded in the [backend continuation](terminal-backend-followup.md).
