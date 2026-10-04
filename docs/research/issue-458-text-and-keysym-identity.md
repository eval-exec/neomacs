# Issue #458: preserve text identity across frontend transport

## Reproduction and cause

Use only the reporter's [issue body](https://github.com/eval-exec/neomacs/issues/458).
A failing regression sent `translate_committed_text("，", 0)` through the core
keyboard boundary: expected `Key::Char('，')`, observed
`Key::Function("key-65292")`. This ran before production edits. A real Pinyin
commit under private Xvfb and fcitx5 also failed against the existing release
binary: the comma command reached Neomacs but inserted nothing.

The input method supplies **text**, not an untyped X11 keysym. Both
`Ime::Commit` and keyboard committed-text translation cast each character to
`u32`, then transported it through the keysym classifier. The logical-key
fallback and the non-Unix crossterm mapper also discarded character identity.
This makes U+FF08 become Backspace, U+FF09 Tab, U+FF0D Return, U+FF1B Escape,
and U+FFCA F13. Recognizing unnamed printable registry holes would fix comma
but leave these collisions and modifier collisions unresolved.

## GNU Emacs and primary sources

Study the requested mirror at `/home/exec/Projects/github.com/emacs-mirror/emacs`:

- `src/gtkutil.c:xg_im_context_commit` decodes UTF-8 into a
  `MULTIBYTE_CHAR_KEYSTROKE_EVENT` with decoded text in `arg`; it does not feed
  the character values into the function-key classifier. This is the
  reporter's GNU GTK comparison path.
- `src/xterm.c`, KeyPress handling: `XLookupChars` clears the keysym and
  modifiers; the Unicode keysym block is explicitly decoded before the
  non-ASCII function-key classifier. Keysyms and committed strings remain
  distinct inputs. A genuine bare `0xff0c` is therefore not proof that text
  should be inferred from the registry; it is the transport provenance that
  was lost in Neomacs.
- `src/keyboard.c:make_lispy_event` and `modify_event_symbol` cook character
  events and name function-key events separately.

The [X.Org XIM protocol, section 4.18](https://xorg.freedesktop.org/archive/X11R7.5/doc/libX11/xim.html)
separates `XLookupChars`, `XLookupKeySym`, and `XLookupBoth`: a commit can
carry a string, keysyms, or both. The
[winit IME contract](https://docs.rs/winit/latest/winit/event/enum.Ime.html)
defines `Commit` as text for insertion. These support preserving the toolkit's
text facts rather than reconstructing them from numeric ranges.
The [fcitx5 Unicode addon configuration](https://github.com/fcitx/fcitx5/blob/master/src/modules/unicode/unicode.h)
defines its direct Unicode entry shortcut; the native regression pins
`Control+Shift+U` in its private configuration to obtain genuine IME commits.

## Design

`FrontendKey` is a closed sum type: `Character(char)` or `Keysym(u32)`.
`InputEvent::Key` carries it end to end, alongside modifiers, press state, and
source frame. Rust's `char` excludes invalid Unicode scalars; exhaustive
matches force consumers to choose between text and key identity. No implicit
integer conversion erases this choice.

The core cooks `Character` using the existing `FrontendCharacterInput`
modifier policy. `Keysym` retains existing GNU key naming, including F13–F35,
3270, ISO, modifier suppression, and unknown vendor keys. IME commits have
zero modifiers; ordinary keyboard text and logical command chords retain
their existing modifier policy. Modifier cooking selects ordinary policy
for characters even when their numeric values overlap function-key bands.
Unix TTY input remains raw bytes decoded by `keyboard-coding-system`;
non-Unix crossterm characters now use the same typed character transport.

## Isolation and verification

`neomacs-infra::display::start_xvfb` provides an authenticated loopback X11
server. It now exports empty `WAYLAND_DISPLAY` and `WAYLAND_SOCKET`, because
winit 0.31 prefers inherited Wayland connections and no longer reads
`WINIT_UNIX_BACKEND`. It also pins `GDK_BACKEND=x11` for GTK clients. Failing
harness assertions first demonstrated that these isolation facts were absent.
The fcitx scenario owns its D-Bus session,
configuration, and runtime directory. It verifies the editor window's PID
on the private X11 server before sending XTEST input.

Tests cover core character/key collisions and modifiers, frontend text
translation, GNU Lisp event parity, native fcitx Pinyin punctuation plus
fcitx Unicode commits and an F13 binding, final GUI redisplay snapshots,
and real UTF-8 TUI input compared to GNU display and saved text.

Commands (the debug binary is rebuilt from this checkout):

```sh
cargo build -p neomacs --bins
cargo test -p neovm-core --lib keyboard::
cargo test -p neomacs-display-runtime --lib
cargo check -p neomacs-display-runtime --features webview
cargo test -p neomacs --lib input_bridge::tests::
cargo test -p neomacs-gui-tests --test harness_contract
NEOVM_ENABLE_ORACLE_PROPTEST=1 NEOVM_BINARY_PATH="$PWD/target/debug/neomacs" \
  cargo test -p neovm-oracle-tests --lib oracle_prop_kbd_event_fullwidth_punctuation_character_identity
NEOMACS_TUI_NEOMACS_BIN="$PWD/target/debug/neomacs" \
  cargo test -p neomacs-tui-tests --test tui fullwidth_punctuation_self_inserts_like_gnu
NEOMACS_GUI_TEST_BACKEND=x11 NEOMACS_GUI_TEST_BINARY="$PWD/target/debug/neomacs" \
  cargo test -p neomacs-gui-tests --test native_frame_focus
```

The fcitx scenario requires fcitx5 with its Pinyin and Unicode addons, dbus-daemon,
xdotool, Xvfb, and xauth. It is opt-in with `NEOMACS_GUI_TEST_BACKEND=x11`.
Its fixture also works with `NEOMACS_GUI_TEST_BINARY=emacs` to check the same
native input against GNU Emacs.

Verified on Linux/X11: 83 core keyboard tests, 1,073 runtime unit tests
(five existing opt-in tests ignored), 24 input-bridge tests, and 18 harness
contract tests passed. The enabled GNU oracle regression, paired GNU/Neomacs
TUI regression, and both native GUI tests passed. The fcitx GUI scenario also
passed against GNU Emacs 31.1. The crossterm mapper is exercised by unit tests;
native macOS and Windows were not run. The optional WebView feature compiles;
its numeric API conversion stays at the WebView boundary. Formatting and
whitespace checks passed.
