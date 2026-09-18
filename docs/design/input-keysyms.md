# Keysym identity and function-key naming

A key that no table enumerated must still be bindable. GNU's model is not a
list of supported keys: its backends hand `keyboard.c` whatever the toolkit
reported, and `modify_event_symbol` (`src/keyboard.c:7749-7805`) names it from
`system-key-alist`, then from the toolkit's keysym name (`XKeysymToString` on
X11 `src/xterm.c:14365`, `gdk_keyval_name` on Wayland/GTK `src/pgtkterm.c:391`,
per-platform on w32/NS), and finally from the key's own number as `key-N`.
There is no unnamed-key case, so the worst outcome of an exotic key is a
binding the user has to write, never silence.

A port that keeps a table of supported keys and drops what is missing turns
each unnoticed key into a bug report — and a silent one, because a dropped key
is indistinguishable from a key the program does not support. That is what
`translate_key` did: F13 and above, the XF86 block, Undo/Redo/Menu, and every
native key winit could not name all fell through one `_ => 0`.

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

## Spelling

Names are the ones GNU configs already write, because GNU's `kbd` and
`read-kbd-macro` are loaded from `lisp/subr.el` and `lisp/edmacro.el` rather
than reimplemented:

- the standard block is lowercased the way GNU's own `lispy_function_keys`
  spells it — `f13`, `undo`, `menu`, `find`, `cancel`;
- the vendor block keeps its keysym spelling — `XF86Back`, `XF86Copy`,
  `XF86Paste` — which is what `(kbd "<XF86Back>")` matches;
- anything the registry does not name becomes `key-<number>`.

## Where it lives

`render_thread/input.rs` (frontend) gives every `Key` an identity and no
longer relies on an unlisted key falling through to zero; modifiers are listed
explicitly so that "a modifier" and "not in the table" stay different answers.
`keyboard/keysym.rs` holds the band predicates, the reserved-band helpers and
the registry-backed name lookup; `keyboard.rs` turns a keysym into a `Key`
and, for the keys no `NamedKey` variant enumerates, a `Key::Function` carrying
the name GNU would give it — which `commands/keymap` interns as the event
symbol, so `[undo]`, `[XF86Back]` and `[key-268963840]` are ordinary bindable
events.

The registry table comes from the `keysymdefs` crate (pure Rust, no runtime
dependencies, generated from `keysymdef.h` and `XF86keysym.h`), so naming works
on every platform and needs neither X11 nor xkbcommon at runtime. If it ever
lags a newer keysym, that key falls through to `key-N` and stays bindable,
which is the same place an unnamed keysym already lands.

## Known gap

A key winit *does* name but this table does not yet spell — the media, launch
and volume families, whose keysyms live in the XF86 block — is logged
(`key has no keysym mapping yet`) and still returns zero. Closing it properly
means either carrying the name through the frontend transport, or generating
the winit-name-to-keysym table from the two registries the way GDK and libX11
generate theirs. Only 82 of winit's 307 named keys share a spelling with the
keysym registry, so the mapping cannot simply be derived by name.
