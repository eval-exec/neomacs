# Decision: neomacs-terminfo

Date: 2026-09-09. PR #363 baseline: `7384bd804cae123e1ff0b3a95b423a7fbc9ca1de`.

## Decision

Use a small private `neomacs-terminfo` library to own ncurses discovery, native calls, owned capability snapshots, and numeric expansion. Keep editor policy in `neomacs`. Use published `pkg-config` and `regex` dependencies; do not vendor or fork an interpreter.

The application compiles `main.rs` independently as a binary and through its library. Moving native calls into a dependency gives both targets the native linkage through Cargo's library mechanism. This replaces the PR's manual replay of native `-l` arguments. Cargo documents this arrangement explicitly. [Cargo native linkage](https://doc.rust-lang.org/cargo/reference/build-scripts.html#rustc-link-lib)

This follows the ownership pattern used by `libgit2-sys` and `openssl-sys`: a dependency owns native discovery and linkage. Their bundling options and larger abstraction layers are unnecessary here. [git2-rs build script](https://github.com/rust-lang/git2-rs/blob/master/libgit2-sys/build.rs), [rust-openssl discovery](https://github.com/sfackler/rust-openssl/blob/master/openssl-sys/build/find_normal.rs)

## Why retain ncurses

GNU Emacs uses native termcap lookup and tparm expansion. Native `tgetstr("me")` can differ from raw terminfo `sgr0`, so changing the database reader is a behavior change. Pure Rust expansion was also evaluated independently of lookup. Published candidates failed compatibility probes needed by the existing renderer; see [research](terminfo-native-research.md).

`regex` validates a conservative numeric directive grammar; ncurses remains the interpreter. This reuses an established Rust parser engine without maintaining a second terminfo interpreter. It does not eliminate native code or claim a complete safety audit of ncurses.

## Interface and ownership

`Database::load(term, queries)` copies a finite query set under one native lock. Snapshots contain only Rust-owned bytes, numbers, and flags. String and boolean names explicitly distinguish termcap from terminfo. Missing queries behave like missing capabilities. No handles or raw pointers escape.

`expand_numeric(bytes, [i32; 9])` validates every directive, including skipped branches, before calling native tparm with nine C long arguments. String directives are rejected. Native variable state remains process/current-terminal state, as in the original GNU-style wrapper; it is not attached to snapshots. Loading another terminal selects that native context.

All unsafe code is restricted to one private module. Loading, lookup, expansion, and copying share one mutex. The crate must remain the sole ncurses caller in Neomacs: unrelated raw bindings cannot share its lock. The fixed 32 KiB string arena is removed; native strings are copied before releasing the lock. ncurses owns its bounded termcap cache, which must not be manually freed behind its back.

Application code retains color precedence, padding removal, rendition comparisons, terminal suitability diagnostics, and keymap policy. RGB and Tc use terminfo boolean lookup, matching GNU; Su remains a termcap flag.

## Build and validation

Linux probes ncursesw then ncurses; macOS probes ncurses then ncursesw. pkg-config emits native link metadata. The dependency passes discovered runtime library directories to the application build script, which preserves rpaths for Nix and other nonstandard prefixes. Without pkg-config, the existing unsplit-library fallback remains with an explicit warning; split installations require valid pkg-config metadata. Other platforms return UnsupportedPlatform.

Use `cargo nextest run -p neomacs-terminfo --locked` for fixture, expansion, failure-recovery, and concurrency checks. The fixture requires ncurses `tic`. `python3 crates/neomacs-terminfo/tests/linkage.py` builds actual consumer executables and runs nextest against synthetic split/unsplit, shared/static libraries. Application policy tests remain in the application crate.

Linux runtime validation does not establish macOS runtime compatibility, and these checks are not a sanitizer audit. A future Rust backend needs lookup and expansion comparisons against ncurses before adoption.

Numeric division and remainder use a bounded constant/stack analysis before
native expansion. This accepts literal and character divisors, computed
constants, and variables assigned within the program. Both conditional paths
must prove safe: inherited variables and parameters remain unknown, and
arithmetic overflow discards a constant fact. A divisor that could trigger
native INT_MIN / -1 (or remainder) is rejected. ncurses still produces all
output and owns variable state. See [follow-up](terminfo-numeric-followup.md).

## Recorded validation

On Linux, the final focused nextest runs passed 38 application terminal tests
and all six new crate tests. The four linkage fixture configurations passed.
The crate passed Clippy with warnings denied, and application library checking
with default features passed (with existing dependency warnings).

The broad application library run used `--no-default-features --features
neo-term`; it reached 118 passing tests before bootstrap tests reported Lisp
`file-missing` errors. Remaining work in that run was interrupted. This is not
a passing full-suite result. macOS and Windows cross-checks could not proceed
because the active Nix toolchain did not contain their standard libraries.

A final full application executable build did not complete: files disappeared
from the original private checkout during compilation, including incremental
cache files and its `.git` pointer. The committed checkout was restored inside
its locked worktree metadata directory. The successful focused tests and
synthetic executable linkage checks preceded this incident; a full application
executable link and startup check remain unverified.
