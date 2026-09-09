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
The optimized ANSI renderer remains available for the primary ANSI terminal.

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

The backend uses published `crossterm` and `crossterm_winapi` APIs, adding no local
unsafe code. Microsoft's [windows-version](https://docs.rs/windows-version/0.1.7/windows_version/)
provides the OS-version check without a handwritten native call. The latter was already a transitive dependency. The
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
cross-compile with cargo-xwin. CI adds a native console test in a newly allocated
Windows console, including legacy fallback and restoration. That test has not
been executed on Windows locally; cross-compilation is not runtime evidence.

This branch does not establish full GNU terminal parity. Remaining work includes:

- Native macOS and Windows execution, real terminal screen comparisons, and a
  Windows input/modifier audit.
- Physical serial-terminal padding delays. Existing capability normalization
  removes padding markers; the new byte painter retains that limitation.
- GNU attribute postprocessing (`sg`/`ug`, underline-as-standout, `se`/`me`) and
  unusual terminal quirks such as teleray. Cursor shape is preserved for ANSI
  terminals, but native legacy Windows cursor-size control remains unimplemented.
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
  locked dependency graph. Native Windows execution remains pending CI.
- A broad changed-crate library run was interrupted after repeated Lisp bootstrap
  `file-missing` errors: 118 tests passed; failing and interrupted tests are not
  counted as successful validation. This reproduces the earlier checkout's
  bootstrap failure class, but is not a passing full application suite.

Independent Standards and Spec reviews found no remaining actionable findings in
the reviewed fixes. Legacy cursor size and the other documented parity gaps remain
open; a clean review does not establish complete GNU compatibility.
