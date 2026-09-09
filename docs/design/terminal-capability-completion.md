# Terminal capability and padding completion

This implements the five follow-ups agreed after the native backend checks:
magic-cookie exclusions, underline-as-standout, paired highlighting exits,
terminal padding, and legacy Windows cursor sizing.

## Capability selection

Follow GNU `src/term.c:init_tty` in order: disable standout/underline when their
`sg`/`ug` numbers are nonnegative; borrow underline and its exit if standout is
missing; use `me` when the chosen standout exit is missing; disable standout if
no exit can be found. Keep the resolved exit with the capability snapshot.
Rendering closes standout before subsequent plain text even without `me`.

## Padding output

Retain padding in attribute, color, cursor, and lifecycle capabilities. Carry
control ranges separately from glyph text, so a buffer containing `$<10/>` is
printed literally. Entries with padded capabilities use the native painter,
including entries whose cursor addressing otherwise qualifies for ANSI output.
Clear-screen padding scales with the screen height; row operations use one line.

`neomacs-terminfo::Padding` duplicates the output descriptor to discover that
terminal's baud rate. Each padded control uses a temporary `setupterm` context
under the same mutex as lookup and tparm. The previous context is restored and
only the temporary context is freed; it never enters tgetent's cache. This keeps
secondary terminals independent and preserves native expansion state. Unpadded
controls and text remain buffered together with no native output call.

Native `tputs` emits pad bytes or sleeps for `npc`. The callback flushes the Rust
writer before a possible native sleep, records I/O errors, and contains panics
until C returns and native state has been restored. Numeric bounds are checked
before C's padding arithmetic. Malformed/unbounded delay specifications and
BSD leading-delay notation are conservatively rejected by this output bridge.
The writer must not call back into the terminfo crate.

The native implementation, including its version-specific optional-padding
behavior, is intentional. In particular some ncurses SP builds apply optional
padding in standalone tputs despite xon/pb; other builds suppress it. GNU uses
the same library API. A temporary context rereads the terminal entry for padded
output, so changes to the installed database can affect its padding policy.
Physical serial hardware testing remains useful beyond the pseudo-terminal tests.

## Existing libraries and unsafe boundary

Use the existing system ncurses instead of adding another terminfo parser.
[tinf's Rust tputs](https://docs.rs/tinf/0.14.0/tinf/fn.tputs.html) was considered;
reusing native ncurses preserves GNU's library behavior and existing dependency
choice. [Termwiz's renderer](https://github.com/wezterm/wezterm/blob/main/termwiz/src/render/terminfo.rs)
would replace broader rendering policy and does not supply these GNU selection
rules as a small reusable interface.

Windows retains Crossterm handle ownership. A small adapter uses Microsoft's
[windows-sys bindings](https://docs.rs/windows-sys/0.61.2/windows_sys/Win32/System/Console/index.html)
for cursor information, which Crossterm's safe API does not expose. One unsafe
setter is used in production; a second getter is compiled only for native tests.
The terminfo C boundary remains entirely in `neomacs-terminfo::native`.
No vendored source or custom Windows declarations are introduced.

Legacy cursors use 99% height for a block (GNU's console default) and 25% for an
underline. A requested vertical bar falls back to a block because this native API
only supports horizontal height, as documented by
[Microsoft](https://learn.microsoft.com/en-us/windows/console/console-cursor-info-str).
The original buffer's cursor is untouched; tests verify restoration after leaving.
This adds renderer sizing, not GNU's separate Lisp `set-cursor-size` primitive.

## Verification

Focused nextest tests cover capability selection (including zero-sized cookies),
paired exits into ordinary text, native output without interpreting literal text,
padding multiplication, pad characters, npc delays, and recovery after writer
errors/panics. Windows native tests query actual cursor size and visibility and
verify the original buffer after teardown. GitHub CI runs Linux, Apple ncurses,
Homebrew ncurses, the macOS app, and Windows console tests.

Validation results for this change are recorded after the runs finish; the older
successful run in terminal-backend-followup.md predates this implementation.

The independent Spec review found a duplicate secondary-session snapshot that
lacked the device attachment. The session now renders from the device's sole
capability snapshot. An open/render/suspend/resume pseudo-terminal regression
covers this path and is included in native macOS CI. The Standards review found
no actionable violations.
