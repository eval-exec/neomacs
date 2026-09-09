# Rust precedents for terminal capability linkage and access

Research date: 2026-09-09. Scope: compare Neomacs PR #363 with primary-source examples, separating a similar native-linking problem from alternative terminal architectures. This note does not change the implementation.

## Findings

| Project | Problem and approach | Relevance to Neomacs |
| --- | --- | --- |
| ncurses-rs | PR #124 fixed openSUSE linkage by preserving the libraries and search paths returned by pkg-config, including separate ncurses and tinfo libraries. | Direct precedent for the distribution-dependent split-library problem. It does not establish the identical independently compiled binary/library layout of Neomacs. |
| pancurses | Its Unix dependency is ncurses, so native bindings and discovery live in a dependency. | Precedent for giving a dedicated dependency responsibility for native linkage. |
| WezTerm / termwiz | Uses the published `terminfo` crate to read capability databases and expand parameterized strings in Rust. | Avoids native terminfo linkage and native expansion state altogether. |
| fish | Replaced ncurses with `rust-terminfo` in PR #10269, eliminating native discovery and `cur_term` ownership; subsequently moved away from database-driven terminal behavior. | Strong precedent for removing both build complexity and C interoperability, with explicit compatibility tradeoffs. |

## Native-linking precedent

[ncurses-rs PR #124](https://github.com/jeaye/ncurses-rs/pull/124), merged January 18, 2017, discusses pkg-config returning `-lncurses -ltinfo` on openSUSE. The implementation emits Cargo native search and library metadata for the discovered libraries. Its [build script at revision `70023d5`](https://github.com/jeaye/ncurses-rs/blob/70023d5e41ae704f61849e8e6c7055eaad398bb6/build.rs#L10) retains pkg-config probing and [ncurses/tinfo link handling](https://github.com/jeaye/ncurses-rs/blob/70023d5e41ae704f61849e8e6c7055eaad398bb6/build.rs#L49). This is historical evidence: [the repository is archived](https://github.com/jeaye/ncurses-rs), so this note does not recommend adopting it as a new dependency.

[pancurses at revision `69eaec2`](https://github.com/ihalila/pancurses/blob/69eaec248f29bff8dcdb4acc30be600d93a2d002/Cargo.toml) declares `ncurses = "5.101.0"` on Unix. It reuses an existing binding dependency instead of implementing application-specific library discovery.

Cargo's [official `rustc-link-lib` documentation](https://doc.rust-lang.org/cargo/reference/build-scripts.html#rustc-link-lib) says native linkage is applied to the package's library target when one exists; binaries should access native functionality through that library's public API. That directly supports moving Neomacs's native calls behind a dependency API. Manually replaying raw linker arguments into every executable is not the architectural pattern demonstrated by these examples.

## WezTerm: reuse a Rust database and expander

WezTerm revision `9fa147c9532c7b175335f6453a4dd7ad7e6473b2` declares [`terminfo = "0.9"` in workspace dependencies](https://github.com/wezterm/wezterm/blob/9fa147c9532c7b175335f6453a4dd7ad7e6473b2/Cargo.toml), consumed by [termwiz](https://github.com/wezterm/wezterm/blob/9fa147c9532c7b175335f6453a4dd7ad7e6473b2/termwiz/Cargo.toml#L35). Its [capability layer](https://github.com/wezterm/wezterm/blob/9fa147c9532c7b175335f6453a4dd7ad7e6473b2/termwiz/src/caps/mod.rs#L209) loads `Database::from_name` or `from_env`. Its [renderer](https://github.com/wezterm/wezterm/blob/9fa147c9532c7b175335f6453a4dd7ad7e6473b2/termwiz/src/render/terminfo.rs#L56) calls typed capability expansion and provides ANSI fallbacks. Tests load [included compiled terminal data](https://github.com/wezterm/wezterm/blob/9fa147c9532c7b175335f6453a4dd7ad7e6473b2/termwiz/src/caps/mod.rs#L435), avoiding dependence on the host's database.

This establishes production use of the published Rust crate. It does not prove exact ncurses or GNU Emacs compatibility. At upstream revision `3aba9de91aa0bc16ba77495ebac16333fb5feb62`, its [logical-not evaluator](https://github.com/meh/rust-terminfo/blob/3aba9de91aa0bc16ba77495ebac16333fb5feb62/src/expand.rs#L361) computes `(x != 0)`, opposite to numeric logical negation. Its [compiled parser](https://github.com/meh/rust-terminfo/blob/3aba9de91aa0bc16ba77495ebac16333fb5feb62/src/parser/compiled.rs#L38) also uses unchecked UTF-8 conversion: “Rust implementation” is not a claim of zero unsafe. Its [expansion context](https://github.com/meh/rust-terminfo/blob/3aba9de91aa0bc16ba77495ebac16333fb5feb62/src/expand.rs#L85) makes variable state explicit instead of relying on native global state.

## fish: remove the native dependency, then narrow terminal assumptions

[fish PR #10269](https://github.com/fish-shell/fish-shell/pull/10269), merged February 22, 2024 as `c408342d652368be653eb6e121742e359777b3b7`, deliberately replaced ncurses with `rust-terminfo`. The discussion connects that change to native discovery, cross-compilation and packaging problems, and removal of `cur_term` lifetime management. It also acknowledges loss of hashed-database support and additional database installation requirements on some BSD systems.

The [merged implementation](https://github.com/fish-shell/fish-shell/blob/c408342d652368be653eb6e121742e359777b3b7/src/curses.rs#L203) copies selected capabilities into its own `Term` structure. Its zero/one-argument [tparm compatibility functions](https://github.com/fish-shell/fish-shell/blob/c408342d652368be653eb6e121742e359777b3b7/src/curses.rs#L525) call `terminfo::expand!`; they do not retain C `tparm` or add a regex validator around it.

This is a historical stage, not fish's current architecture. [Issue #11344](https://github.com/fish-shell/fish-shell/issues/11344) proposes dropping terminfo because its remaining differences were of little practical value to fish. At current revision `79f79059dbd9f5071b98418509fd72d59503aafb`, [Cargo.toml](https://github.com/fish-shell/fish-shell/blob/79f79059dbd9f5071b98418509fd72d59503aafb/Cargo.toml) no longer depends on terminfo. Adopting that direction for Neomacs would require an explicit change to its GNU-compatible terminal support goals.

## Assessment of the Neomacs follow-ups

The evidence supports native linkage owned by a library dependency, owned capability data, and reuse of a published Rust database/expander when its compatibility is adequate. It also provides an example of a mature application deliberately accepting narrower compatibility to simplify its implementation.

The evidence does **not** establish that Neomacs's custom regex and arithmetic validation around native `tparm` is standard practice. None of the inspected examples implements that arrangement. It remains Neomacs-specific safety and compatibility work, with an ongoing maintenance burden. The general crate boundary is supported by precedent; the validator needs its own justification and validation.

For the user's preference for less unsafe and less hand-written code, `terminfo` as used by termwiz and historical fish is the strongest concrete alternative found here. Replacing Neomacs's current backend with it would still require evaluating the known expansion differences and termcap behavior; production adoption alone is insufficient evidence of a drop-in replacement. No dependency migration, tests, commits, pushes, or PR comments were performed for this research.

## Additional numeric-expansion survey

For the numeric follow-up, additional published candidates were checked for a
usable numeric API or a public AST that could support a small compatibility
adapter. None inspected supplied a drop-in native-compatible solution:

- [`terminfokit 0.1.0`](https://docs.rs/crate/terminfokit/0.1.0/source/src/expand.rs)
  uses 64-bit arithmetic and errors on zero divisors. Its public `Program`
  exposes bytes and summary analysis, not an operation AST. Running it first
  cannot attest native 32-bit arithmetic or branches.
- [`terminfo-lean 0.1.2`](https://docs.rs/crate/terminfo-lean/0.1.2/source/src/expand.rs)
  retains ordinary integer arithmetic, zero-division panics, and boolean/padding
  behavior that differs from the native renderer contract.
- [`infoterm 0.1.1`](https://docs.rs/crate/infoterm/0.1.1/source/src/expand/exec.rs)
  uses ordinary integer arithmetic, errors on zero division, and keeps its
  operation/parser modules private.

The native [`tiparm_s` interface](https://invisible-island.net/ncurses/man/curs_terminfo.3x.html#h3-Formatting-Output)
checks expected argument counts and pointer types. Its
[shared interpreter](https://github.com/mirror/ncurses/blob/master/ncurses/tinfo/lib_tparm.c)
still performs signed division/remainder directly. It also does not provide a
portable replacement for older system ncurses on macOS. The focused follow-up
therefore keeps native expansion with a conservative, safe Rust proof rather
than adding one of these dependencies.
