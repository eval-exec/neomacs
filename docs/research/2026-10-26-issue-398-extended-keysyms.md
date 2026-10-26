# Issue #398: extended GUI keysym identity

Research for [issue #398](https://github.com/eval-exec/neomacs/issues/398),
2026-10-26. GNU source was studied first at revision
`a360712c9d272d950d8d8255ef74570f7e90b7d9`; its checkout remained read-only.
This note records source/history findings and design limits. Runtime results
must be supplied by the reproduction and verification work, separately.

## GNU reference

PGTK classifies standard function keys through `0xffe0` (F35), accepts vendor
keysyms, and sends them as non-ASCII keyboard events. Modifier state is
excluded. The core event builder consults GNU's special-key tables before
system/toolkit naming and a final numeric fallback. Thus a GUI key is not
limited to the first twelve function keys or to an application's short
allow-list.
[PGTK classification](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/pgtkterm.c#L5398-L5473),
[event naming](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/keyboard.c#L6360-L6418).

GNU's table spells standard key events in its own vocabulary; vendor names
come from the toolkit unchanged. For the reported keys, the expected
identities are:

| Incoming identity | Lisp event |
| --- | --- |
| F13–F35, `0xffca`–`0xffe0` | `f13`–`f35` |
| Undo, `0xff65` | `undo` |
| Redo/Again, `0xff66` | `redo` |
| Menu, `0xff67` | `menu` |
| XF86Back, `0x1008ff26` | `XF86Back` |
| XF86Forward, `0x1008ff27` | `XF86Forward` |
| XF86Copy, `0x1008ff57` | `XF86Copy` |
| XF86Cut, `0x1008ff58` | `XF86Cut` |
| XF86Paste, `0x1008ff6d` | `XF86Paste` |

The standard spellings are explicit in
[GNU's function-key table](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/keyboard.c#L5510-L5591).
PGTK obtains other names from `gdk_keyval_name`, and `modify_event_symbol`
interns that name without lowercasing it.
[PGTK lookup](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/pgtkterm.c#L390-L394),
[fallback naming](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/keyboard.c#L7775-L7810).
The Lisp manual independently describes function keys as symbolic events.
[Function Keys](https://www.gnu.org/software/emacs/manual/html_node/elisp/Function-Keys.html).

## Actual toolkit dependency and mapping

The workspace declares `winit = "=0.31.0-beta.3"`, patched to the
`eval-exec/winit` fork at `2290f91c602f8efdf102972457a25e8b6e36f90c`.
Research against an arbitrary 0.30 release would not establish this build's
behavior. See [Cargo.toml](../../Cargo.toml) and [Cargo.lock](../../Cargo.lock).

That pinned XKB mapper pairs the requested keysyms with `NamedKey::F13` through
`F35`, `Undo`, `Redo`, `ContextMenu`, `BrowserBack`, `BrowserForward`, `Copy`,
`Cut`, and `Paste`. Both X11 and Wayland use this common XKB path. Unknown
nonzero keysyms retain their number in `Key::Unidentified(NativeKey::Xkb(..))`.
[Pinned XKB mapper](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-common/src/xkb/keymap.rs#L505-L926).

The W3C key vocabulary distinguishes logical meaning from physical key
location, including distinct `Undo`, `Redo`, `Again`, `ContextMenu`, and browser
key labels. Those labels are semantic input to toolkit translation; they are
not themselves the Lisp event spelling GNU users bind.
[W3C key values](https://www.w3.org/TR/uievents-key/#keys-editing).

## Existing fixes on main

The requested mappings already exist in current main. History identifies two
explicit fixes for this issue:

- [`62815890a1`](https://github.com/eval-exec/neomacs/commit/62815890a1455ab6ba0f72381161874014de9550)
  replaces the short allow-list, preserves unidentified native identities,
  and classifies named keys before character conversion.
- [`d150806aef`](https://github.com/eval-exec/neomacs/commit/d150806aef511f96fd3c9ebeea5d3cbff62150be)
  covers additional XKB-produced named families and ports GNU naming tables.

Current [frontend translation](../../crates/neomacs-display-runtime/src/render_thread/input.rs),
[core dispatch](../../crates/neovm-core/src/keyboard.rs), and
[keysym classification/naming](../../crates/neovm-core/src/keyboard/keysym.rs)
contain these changes. Existing frontend tests explicitly cover all F13–F35,
the misc-function band and the requested XF86 band. Core tests cover their
names and dispatch.

A source audit of the pinned XKB mapper found 150 distinct active `NamedKey`
variants; all occur in the frontend mapping. Commented-out upstream match
arms were excluded. This is a source-coverage observation, not a successful
runtime reproduction or proof of complete cross-platform parity.

## Type and abstraction implications

`FrontendKey::Character(char)` and `FrontendKey::Keysym(u32)` already preserve
the essential domain distinction. U+FFCA text must not become the F13 keysym
`0xffca`; text and key identity can share an integer while representing
different input. Keep those enum variants through transport rather than
reconstructing the distinction from numeric ranges. The existing
[design note](../design/input-keysyms.md) records this contract and issue #458.

The toolkit's `Key` enum separates named keys, characters, unidentified native
identities and dead-key state. `NativeKey` is non-exhaustive; an external match
requires a future-compatible fallback. A wildcard on the large named-key
table does not provide compile-time proof that every upstream named variant
is supported. If a new defect requires changing that interface, an internal
translation outcome can distinguish a delivered key, intentionally ignored
modifier/dead state, and an unmapped identity; exhaustive matching then checks
those internal outcomes. Do not claim that it automatically checks upstream
mapping completeness.
[Pinned keyboard types](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-core/src/keyboard.rs#L82-L101),
[logical key model](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-core/src/keyboard.rs#L243-L275),
[Rust exhaustive matching](https://doc.rust-lang.org/book/ch06-02-match.html#matches-are-exhaustive).

## Verification limits and adjacent concerns

The correct runtime test follows a real injected key into Lisp `read-event`
and command lookup; testing that `(kbd "<f20>")` parses alone cannot detect
the original native-event drop. TUI transport uses terminal escape sequences
rather than Wayland keysyms, so TUI tests should be described as dispatch and
terminal regressions, not proof of GUI delivery. A genuinely already-fixed
case should be reported as such rather than forcing an unrelated production
change merely to produce a new RED/GREEN pair.

The XKB map is many-to-one: both XF86 and Sun copy keysyms become
`NamedKey::Copy`, and some power/media aliases share named variants. Inverting
the named enum therefore cannot recover every original keysym. This is an
upstream information limit for exact vendor identity, distinct from the
listed #398 keys whose chosen mappings agree with GNU. Extending exact alias
parity would require preserving original backend identity, not another
reverse table. The existing design note also records unverified native
Windows/macOS name differences and a few non-XKB named variants. These are
separate investigations unless runtime evidence ties them to the report.
[Many-to-one mappings and unknown fallback](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-common/src/xkb/keymap.rs#L836-L926),
[existing limits](../design/input-keysyms.md).


## Historical reproduction and current verification

The reported revision is `908a8e7dd5ef18c9324c30ad7cb7e409a92fcc38`.
Its actual `RenderApp::translate_key` function was extracted without changing
its body into a local test harness using the current pinned winit key types.
The wrapper introduces only a test-only `RenderApp` type and assertions.
`cargo nextest run --offline --manifest-path
tmp/issue398/historical-replay/Cargo.toml --test historical_replay
--no-fail-fast` produces one passing F1/F12 control and one failing reported-key
test. Every requested F13–F35, Undo/Redo/Menu and XF86 key returns zero instead
of its expected identity. The 31 actual outputs are preserved in
`tmp/issue398/historical-red.log`.

This is an exact historical translator replay, not a claim to have launched
the author's old native binary. That revision used a different toolkit fork;
rebuilding its entire product would add dependency and runtime changes to the
minimal reproduction. The historical core also accepted F1–F24 only, then
fell through to Unicode conversion for larger ordinary keysyms, while XF86
values exceeded the Unicode range. The frontend allow-list was the first
failure; the existing fixes address both stages.
[Reported translator](https://github.com/eval-exec/neomacs/blob/908a8e7dd5ef18c9324c30ad7cb7e409a92fcc38/crates/neomacs-display-runtime/src/render_thread/input.rs),
[reported core conversion](https://github.com/eval-exec/neomacs/blob/908a8e7dd5ef18c9324c30ad7cb7e409a92fcc38/crates/neovm-core/src/keyboard.rs#L1120-L1175).

The new regression work does not replace those production fixes. It checks
the missing end-to-end contract on current main:

- Core tests send actual typed frontend input through the evaluator's input
  channel, check `read-key-sequence` and the independently named local binding,
  then execute that same sequence through the command loop. All 31 reported
  identities and a modified function key are covered.
- A live batch oracle checks GNU's symbolic spelling, `kbd`, key descriptions,
  binding lookup and base-event identity. It is a Lisp control, not GUI input
  evidence.
- A real PTY test checks configured F13/F35 escape sequences in GNU and
  NeoMacs. Its explicit `input-decode-map` entries avoid claiming that every
  terminal produces those sequences by default.
- GUI tests inject F13–F35, Undo/Redo/Menu, XF86Back/Forward/Copy/Cut/Paste and
  `C-S-f20` through actual native input. Each of the 32 measured keys is sent
  exactly once, with F13 first. The test requires the complete ordered list,
  matching command event/key-description observations from initialized GNU,
  clean exits and graphical artifacts.

## Harness controls

Xvfb's default XKB map made `xdotool key F24` emit `C-f24` to the GNU oracle.
A direct GNU `read-event` probe confirmed that before any NeoMacs comparison.
The regression installs a private XKB map with one unmodified keycode per
keysym and genuine Control/Shift modifiers. The same probe then emitted plain
`f24`. Only the owned private display is changed. `xkbcomp` is now an explicit
Linux CI/development-shell test dependency.

Bindings are installed during fixture loading, before the command reader
starts, as ordinary startup configuration. The timer only prepares the visible
buffer and readiness signal. During harness development, changing buffer-local
bindings from a timer while input was already being read affected the first
lookup; input tracing showed that the first keysym itself had translated
correctly. This adjacent late-keymap behavior is not claimed fixed here.
Temporary tracing and probe acknowledgements were removed; the final test
retains no extra function-key priming or retries of measured keys.

The core fixture must likewise use the real input channel, rather than the
legacy `CommandLoop::enqueue_event` helper queue that the Context reader does
not consume. It displays the local-map buffer before macro dispatch: GNU also
switches command execution to the selected window's buffer. Early fixture
failures were diagnosed against both editors and are not evidence of a new
production mapping bug.


## Final results

All Rust tests below were run with `cargo nextest` in the existing Nix shell.
All generated fixtures, subprocess diagnostics and reproduction artifacts stay
under the repository's `tmp/issue398/`.

| Layer | Result |
| --- | --- |
| Exact historical translator replay | Reported-key test fails on all 31 identities; F1/F12 control passes |
| Current frontend translation selection | 23 passed |
| Full current runtime unit suite | 1,182 passed, five skipped |
| Core keyboard suite, including the two new input-channel tests | 141 passed |
| Live GNU batch binding oracle | One passed, covering 31 identities and a modifier control |
| Actual PTY configured-terminal comparison | One passed, F13 and F35 commands observed in both editors |
| Explicit native Wayland comparison | 32 exact ordered command observations match initialized GNU |
| Explicit native X11 comparison | 32 exact ordered command observations match initialized GNU |

The final combined GUI run passed both explicit backend tests even with
`NEOMACS_GUI_TEST_BACKEND=x11` in the job environment. Thus the original
Wayland path cannot be omitted merely because CI defaults to X11. Native
screenshots and frame snapshots were written; the Wayland screenshot was
inspected and shows every command name, including F13 first and the modified
F20 event last.

Formatting, whitespace checks, `bash -n scripts/ci/setup-linux.sh`, the CI
package-list check, Nix expression parsing and offline development-shell
evaluation pass. Verification used the existing native development build from
the same production tree as the initial main revision `0fbc8b9893`; this change
adds test registration and test/tooling/research files rather than replacing
the existing input implementation. The GNU source checkout remained read-only.

The local evidence includes `historical-red.log`, `current-translation.log`,
`runtime-full.log`, `core-keyboard-channel.log`, `oracle.log`, `tui.log`, and
`gui-final-both-backends.log`. Native macOS/Windows and every physical remapper
configuration were not tested. The production mappings requested by #398 are
verified on current main; platform-specific names, Sun/XF86 aliases and late
keymap changes remain the distinct limitations described above.
