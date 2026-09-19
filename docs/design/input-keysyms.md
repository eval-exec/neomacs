# Keysym identity and function-key naming

A key that no table enumerated must still be bindable. GNU's model is not a
list of supported keys: its backends hand `keyboard.c` whatever the toolkit
reported, and `make_lispy_event` (`src/keyboard.c:6358-6412`) plus
`modify_event_symbol` (`src/keyboard.c:7742-7823`) name it — from GNU's own
tables, then `system-key-alist`, then the toolkit's keysym name
(`XKeysymToString` on X11 `src/xterm.c:14365`, `gdk_keyval_name` on
Wayland/GTK `src/pgtkterm.c:391`, per-platform on w32/NS), and finally from the
key's own number as `key-N`. There is no unnamed-key case, so the worst outcome
of an exotic key is a binding the user has to write, never silence.

A port that keeps a table of supported keys and drops what is missing turns
each unnoticed key into a bug report — and a silent one, because a dropped key
is indistinguishable from a key the program does not support. That is what
`translate_key` did: F13 and above, the XF86 block, Undo/Redo/Menu, and every
native key winit could not name all fell through one `_ => 0`. It now spells
every `NamedKey` winit's xkb keymap can produce.

## Keysym is not character

The X11 keysym space is not the Unicode space, and the two overlap by number:
`XK_F13` is 0xffca and U+FFCA is a halfwidth hangul letter; `XK_Redo` is
0xff66 and U+FF66 is a halfwidth katakana letter. A pipeline that decides "is
this a character?" by trying `char::from_u32` on the keysym will therefore type
garbage for those keys. GNU classifies by range first — `IsCursorKey`
(0xff50-0xff5f), `IsMiscFunctionKey` (0xff60-0xff6b), `IsKeypadKey`
(0xff80-0xffbd), `IsFunctionKey` (0xffbe-0xffe0), `IsModifierKey`
(0xffe1-0xffee), all in `src/pgtkterm.c:5218-5221` — and only then asks whether
the keysym is a character. `keyboard/keysym.rs` ports those predicates, and
`keysym_to_key_event` applies them in that order.

The trap is not confined to the 0xff00 block. `0xfd0e` is `XK_3270_Attn`, and
U+FD0E is an Arabic presentation form — so the 3270 keysyms (0xfd01-0xfd1e) sit
inside the named block too, or the character arm would type a letter instead of
producing the `3270_Attn` event GNU produces (verified against the pinned
binary, which names it exactly that).

The F block is worth stating plainly: it is exactly F1..F35, `0xffbe` to
`0xffe0`, so the function-key number is arithmetic (`keysym - 0xffbe + 1`)
rather than a table. F13-F35 are not an edge case; they are the upper half of
the block.

## Identity for a key with no keysym

macOS, Windows, Android and OpenHarmony hand the frontend a scancode, a
virtual-key code, or a keycode — not a keysym. Those get reserved bands
(`0x20000000` and up, above the Unicode keysym block at 0x01000000-0x0110ffff
and the vendor bit at 28, so they cannot collide with a real keysym) and are
named after the platform and number they came from: `mac-36`, `win-93`,
`android-4`. X11 and Wayland need none of this: an unmapped key arrives as
`Key::Unidentified(NativeKey::Xkb(keysym))`, and that keysym is already the
identity this port uses.

## Spelling: GNU's tables first, then the toolkit, verbatim

The names are the ones GNU configs already write, and they come from the same
two sources GNU's do:

- **GNU's own tables.** `lispy_function_keys` (`src/keyboard.c:5510-5591`,
  indexed by `keysym - 0xff00`) and `iso_lispy_function_keys`
  (`src/keyboard.c:5596-5613`, `keysym - 0xfe00`) are ported into
  `keyboard/keysym.rs` row for row. This matters because those names are *not*
  the X11 names lowercased: `XK_Henkan_Mode` is `henkan`, `XK_Kana_Lock` is
  `kana-lock`, `XK_Hiragana_Katakana` is `hiragana-katakana`.
- **The toolkit's name, verbatim.** Where GNU's table has no entry, GNU asks
  `XKeysymToString` and takes what it says — so `Scroll_Lock` keeps its capital
  and underscore, `Multi_key` its underscore, `3270_Attn` its spelling, and the
  vendor block keeps its prefix (`XF86Back`, `XF86AudioRaiseVolume`).
  `keyboard/keysym.rs` reads the same names out of the `keysymdefs` crate
  (generated from `keysymdef.h` and `XF86keysym.h`), verbatim.
- **`key-<number>`** when neither names it — GNU's last resort, which the
  caller supplies.

The ISO block is the one place where the index is not the keysym: GNU sends the
whole `0xfe00-0xfeff` range to a 53-entry table and names anything the table
lacks `key-<index>` — the *stripped* number, so `0xfe08` is `key-8`. Past the
table's end (0xfe35 and up) GNU returns nil and the event is dropped. No key
this port produces lands there (XKB consumes the group-switch keysyms
server-side and winit names the rest), but the branch is written out for the
record.

### How the names were checked

Two ways, both against the pinned GNU Emacs (SHA-256-verified against
`parity-reference.toml`):

- the tables are extracted from `src/keyboard.c` mechanically and anchored on
  indices whose keysyms are not in dispute (`0xff23` is `henkan`, `0xffca` is
  `f13`, `0xffff` is `delete`), so a shifted row fails loudly instead of
  renaming every key after it;
- the names were read off the running binary: a GUI frame evaluating
  `(read-event)` while `xdotool` sent the keysym, which prints exactly the
  symbol `modify_event_symbol` built. `Henkan_Mode` → `henkan`,
  `Zenkaku_Hankaku` → `zenkaku-hankaku`, `KP_Enter` → `kp-enter`,
  `Scroll_Lock` → `Scroll_Lock`, `3270_Attn` → `3270_Attn`, `XF86Back` →
  `XF86Back`.

The frontend's keysym values come from winit's own keymap
(`winit-common/src/xkb/keymap.rs`) inverted, so each value is the keysym winit
matched to produce that `NamedKey` — the same number an X11 or Wayland backend
would have handed GNU.

## Where it lives

`render_thread/input.rs` (frontend) gives every `Key` an identity and no longer
relies on an unlisted key falling through to zero; modifiers are listed
explicitly so that "a modifier" and "not in the table" stay different answers.
`keyboard/keysym.rs` holds the band predicates, the reserved-band helpers, the
ported GNU tables and the registry-backed name lookup; `keyboard.rs` turns a
keysym into a `Key` and, for the keys no `NamedKey` variant enumerates, a
`Key::Function` carrying the name GNU would give it — which `commands/keymap`
interns as the event symbol, so `[undo]`, `[XF86Back]`, `[henkan]` and
`[key-268963840]` are ordinary bindable events.

## Remaining gaps

- **A platform's keys are named by that platform's tables.** GNU names a key
  from whichever backend delivered it, and the same physical key has different
  names on different systems: on X11 the volume keys are
  `XF86AudioRaiseVolume` and friends, while on Windows they are `volume-up`,
  `volume-down`, `volume-mute` (`lispy_multimedia_keys`, `src/keyboard.c:5403`,
  indexed by `VK - VK_BROWSER_BACK`). This port names by keysym, so a Windows
  media key currently gets the X11 spelling — bindable, but not the symbol a
  GNU-on-Windows config writes. Doing it properly means letting the Windows and
  macOS backends hand over their *native* identity (the reserved bands exist
  for exactly that) and porting the NT tables (`src/keyboard.c:5209` for VK,
  `:5403` for multimedia) into `native_key_name`. That work needs a Windows or
  macOS machine to verify, which is why it is written down rather than guessed
  at.
- Four `NamedKey`s the Windows/macOS backends can emit have no X11 keysym and
  still reach the `key has no keysym mapping yet` log: `MediaPlayPause`
  (GNU/Windows spells it `media-play-pause`), `Finish`, `Eisu`, and
  `ZoomToggle` — the last two are remote-control values no keyboard produces.
- `system-key-alist` is not consulted — but reading its two users in GNU shows
  it is narrower than the name suggests: `lisp/term/ns-win.el:174` uses it for
  *pseudo*-events (`ns-power-off`, `ns-drag-file` — C-to-Lisp plumbing rather
  than keystrokes) and `lisp/term/x-win.el:252` for vendor keysyms
  (`mute-acute`, `lira`, `reset`). It is not a general renaming hook, so
  nothing user-visible depends on it yet.
