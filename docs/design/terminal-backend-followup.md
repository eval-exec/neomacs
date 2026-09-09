# Terminal backend continuation

Branch: `fix/gnu-terminal-parity`, rebased onto `origin/main` at `842c3fb7f`.
This continues the [PR #363 GNU parity audit](pr363-gnu-parity-audit.md).

## Output boundary

`TtyRif::paint` gives a platform painter the dimensions, dirty cell rows, and
cursor. The painter owns terminal operations and flushing. Only a successful
finish commits the screen model; errors retain the desired frame and force a full
retry. Tests exercise failed flush, full retry, unchanged frames and dirty rows.
Byte output resets known insert/rendition modes before painting so a partial
write cannot leave the retry in insert mode or with stale highlighting. This
cannot guarantee recovery from every terminal's partially received control code.
The optimized ANSI renderer remains available for the primary ANSI terminal
when its capabilities do not require padding.

Unix non-ANSI and secondary terminals render with their own capability snapshot.
Absolute cursor addressing is expanded by native tparm. Relative addressing uses
home, or repeated upward movement and carriage return/left movement to anchor,
then down/right. GNU's `nl`, `bc`, and `bs` fallbacks are honored. On a backward
wrapping terminal, relative-only addressing currently requires home or carriage
return. Lifecycle output uses the entry's screen, keypad, and cursor controls.
Missing TERM and an unreadable terminal description now fail initialization.

Bottom-right output on an autowrapping terminal follows GNU's suffix-first,
insert-first-glyph strategy. A trailing default blank can use erase-to-end instead.
When neither operation is available, painting reports an error and retains the
frame for retry. The renderer does not silently scroll the physical screen and
commit an incorrect model. Secondary terminals use their own attribute sequences,
even when their cursor addressing is ANSI.

## Windows

The backend uses published `crossterm` and `crossterm_winapi` APIs. Cursor sizing
uses a small `windows-sys` adapter; see the [completion](terminal-capability-completion.md). `crossterm_winapi` was already a transitive dependency. Microsoft's
[windows-version](https://docs.rs/windows-version/0.1.7/windows_version/)
provides the OS-version check without a handwritten native call. The
[crate research](windows-terminal-renderer-research.md) explains why termwiz's
public renderer and crossterm's generic style commands did not fit these needs.

Mode detection probes the actual output console handle, restores its original
mode after the probe, requires GNU's Windows version threshold (later than
10.0.15063), and publishes 16777216 colors for VT or 16 for legacy output.
VT sessions enable output processing before emitting ANSI. Legacy sessions own an
alternate screen buffer, use native cursor/attribute/Unicode operations, and
restore the original screen and mode on exit. Automatic wrapping is disabled in
the legacy alternate buffer to prevent bottom-right scrolling. ANSI-order realized
palette indices are translated to native BGR bits; default inverse colors follow
GNU's realized-face behavior.

## Validation and remaining work

Linux fixtures cover VT52 bytes and lifecycle, relative motion aliases and anchors,
bottom-right insertion/erase, and wide first glyphs. Windows library and test code
cross-compile with cargo-xwin. Native GitHub CI also executes the macOS application
fixtures and Windows VT/legacy console tests successfully; see the recorded run
below. Windows tests allocate a real console and run serially to avoid sharing
console state between concurrent tests.

This branch does not establish full GNU terminal parity. Remaining work includes:

- Full editor screen comparisons on real terminals and a Windows input/modifier
  audit. The hosted smoke tests do not establish complete interactive parity.
- Unusual terminal quirks such as teleray. The [five capability follow-ups](terminal-capability-completion.md)
  implement cookie exclusions, highlighting fallbacks, native padding, and
  legacy Windows cursor sizing. Physical serial hardware remains to be exercised.
- Windows Lisp console APIs and `w32console.el` palette initialization. The output
  backend uses Neomacs' existing ANSI-order realized palette; it does not claim
  compatibility with every GNU-specific console primitive.
- Some implicit numeric formats, bounded formatting fields, and native compiler
  assumptions described in the [numeric follow-up](terminfo-numeric-followup.md).

The legacy console uses crossterm_winapi's safe Unicode writer, which does not
expose the native count of UTF-16 units written. Error returns force a repaint,
but detecting an otherwise successful partial native write needs an upstream API
improvement or a different safe writer.

## Recorded local results

- Linux: all 19 numeric tests, the opt-in installed-capability scan, and 43
  focused application terminal tests pass with cargo nextest.
- All 882 enabled display-runtime library tests pass; two remain ignored.
- Numeric crate Clippy passes with warnings denied; the workflow passes actionlint.
- Windows x86_64 MSVC library and test code pass cargo-xwin checking with the
  locked dependency graph. Native Windows execution is recorded separately below.
- A broad changed-crate library run was interrupted after repeated Lisp bootstrap
  `file-missing` errors: 118 tests passed; failing and interrupted tests are not
  counted as successful validation. This reproduces the earlier checkout's
  bootstrap failure class, but is not a passing full application suite.

Independent Standards and Spec reviews found no remaining actionable findings in
the reviewed fixes. The documented remaining parity gaps still apply; a clean review does not
establish complete GNU compatibility.

## Native GitHub CI results

On 2026-09-09, all five focused jobs passed in
[run 34338028185](https://github.com/eval-exec/neomacs/actions/runs/34338028185),
testing commit `b529663e752463dc92caaed3fb08e9f000c029bd`.

| Native environment | Checks | Passed |
| --- | --- | ---: |
| Linux | Numeric expansion and snapshots | 19 |
| macOS, Apple system ncurses | Numeric expansion and snapshots | 19 |
| macOS, Homebrew ncurses | Numeric expansion and snapshots | 19 |
| macOS ARM64 | Application terminal output and capability policy | 43 |
| Windows MSVC x86_64 | Palette mapping, native legacy restoration, VT negotiation/output/restoration | 3 |

The Windows job forces legacy mode for one test even though the hosted console
supports VT. This exercises both implementations on a modern Windows runner;
it does not test every older Windows release. The opt-in installed-database scan
remains ignored in CI. All Rust tests run through cargo nextest.

The first run exposed assumptions about installed terminfo contents in three
application tests: Apple's tmux entry lacked `Smulx`, its Linux palette used a
different brightness-reset code, and its Linux reset string was normalized
differently. Those tests now compile controlled entries with native `tic` and
load them in child processes with an explicit TERMINFO, without unsafe environment
mutation. Assertions remain mandatory. Windows log forwarding also now writes
UTF-8 explicitly so nextest's Unicode separators cannot fail under CP1252.

Run this focused suite without launching the unrelated full CI matrix:

```sh
gh workflow run ci.yml --ref fix/gnu-terminal-parity -f terminal_only=true
```

The native jobs also run during ordinary pull-request and main-branch CI.
