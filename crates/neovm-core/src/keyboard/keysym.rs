//! X11 keysym structure: GNU's band predicates, the synthetic identity this
//! port gives a native key that has no keysym, and the name lookup that keeps
//! such a key bindable.
//!
//! GNU decides what a keystroke is by *range* before anything else.  Its
//! backends classify with the X protocol's own macros (`IsCursorKey`,
//! `IsMiscFunctionKey`, `IsKeypadKey`, `IsFunctionKey`,
//! `src/pgtkterm.c:5218-5221`), and `keyboard.c`'s `modify_event_symbol`
//! names whatever is left.  The order matters because a keysym is not a
//! character: XK_F13 is 0xffca, and U+FFCA is a halfwidth hangul letter — the
//! same number in two different domains.  Range checks first, character
//! interpretation only for the keysym ranges that really are characters.
//!
//! `XK_VoidSymbol` (0xffffff) is "no key" and is never named.

/// The F-key block, which is exactly F1..F35: XK_F1 is 0xffbe and XK_F35 is
/// 0xffe0, so `keysym - 0xffbe + 1` is the function-key number.
pub(crate) const FUNCTION_KEY_BASE: u32 = 0xffbe;
pub(crate) const FUNCTION_KEY_LAST: u32 = 0xffe0;

// The three band predicates below are part of GNU's classification and are
// asserted by this module's tests, but a keysym in any of these bands is
// claimed by a current `NamedKey` arm, so only the F block needs a predicate
// of its own at the call site.  They are kept because the naming order is
// stated in terms of them, and the next band to need naming will want them.
/// `IsCursorKey` (`src/pgtkterm.c:5218`): Left, Up, Right, Down, Home, End,
/// Prior, Next, Begin, Select, … — the keys that already have a `NamedKey`.
#[allow(dead_code)]
pub(crate) fn is_cursor_key(keysym: u32) -> bool {
    (0xff50..0xff60).contains(&keysym)
}

/// `IsMiscFunctionKey` (`src/pgtkterm.c:5219`): Undo, Redo, Menu, Find,
/// Cancel, Help, Break, Mode_switch, Num_Lock …
#[allow(dead_code)]
pub(crate) fn is_misc_function_key(keysym: u32) -> bool {
    (0xff60..0xff6c).contains(&keysym)
}

/// `IsKeypadKey` (`src/pgtkterm.c:5220`): the numeric keypad block.
#[allow(dead_code)]
pub(crate) fn is_keypad_key(keysym: u32) -> bool {
    (0xff80..0xffbe).contains(&keysym)
}

/// The F-key block; the caller turns these into `NamedKey::F(n)`.
pub(crate) fn is_function_key(keysym: u32) -> bool {
    (FUNCTION_KEY_BASE..=FUNCTION_KEY_LAST).contains(&keysym)
}

/// `IsModifierKey`: X11's modifier block, 0xffe1-0xffee **inclusive**
/// (Shift_L is 0xffe1 and Hyper_R is 0xffee).  These are state, not
/// keystrokes; the frontend reports them through `ModifiersChanged`.
pub(crate) fn is_modifier_key(keysym: u32) -> bool {
    (0xffe1..=0xffee).contains(&keysym)
}

/// X's special block, 0xfd00-0xffff: the 3270 keysyms (0xfd01-0xfd1e, which
/// GNU names `3270_Attn`, `3270_EraseEOF` and friends), the ISO/kbd specials,
/// the cursor, misc and keypad bands, and the F-key block.
///
/// The 3270 keysyms have to be in here rather than left to the character arm:
/// 0xfd0e is `XK_3270_Attn`, and reading it as a code point would type U+FD0E
/// — the same trap as F13/U+FFCA, one block down.  GNU names every keysym it
/// did not get a character for, and it gets no character here.
pub(crate) fn is_named_block(keysym: u32) -> bool {
    (0xfd00..=0xffff).contains(&keysym)
}

/// X11's vendor space: bit 28, which covers the `XF86keysym.h` block
/// (0x1008ff00-0x1008ffff) and every vendor extension.  GNU accepts these
/// outright — "Any `vendor-specific` key is ok" (`src/xterm.c:20614`) — and
/// names them from the toolkit, so their names keep the vendor prefix.
///
/// The Unicode keysym block (0x01000000-0x0110ffff) does not have this bit.
pub(crate) fn is_vendor_keysym(keysym: u32) -> bool {
    keysym & (1 << 28) != 0
}

/// A keysym that names a key rather than a character: the special block, the
/// vendor space, or one of the reserved bands for a native key that has no
/// keysym.  The Unicode block is handled before this and is not included.
pub(crate) fn is_named_keysym(keysym: u32) -> bool {
    is_named_block(keysym) || is_vendor_keysym(keysym) || keysym >= NATIVE_MACOS_BASE
}

/// Low range reserved for a native key that has no keysym at all.
///
/// macOS, Windows, Android and OpenHarmony hand the frontend a scancode or a
/// virtual-key code, not a keysym.  X11's keysym space stops below 0x20000000
/// (the Unicode range tops out at 0x0110ffff and the vendor bit at bit 28),
/// so these bands cannot collide with a real keysym, and a key from one of
/// those platforms keeps a stable identity instead of being dropped.
const NATIVE_BAND_MASK: u32 = 0x0000_ffff;
const NATIVE_MACOS_BASE: u32 = 0x2000_0000;
const NATIVE_WINDOWS_BASE: u32 = 0x2100_0000;
const NATIVE_ANDROID_BASE: u32 = 0x2200_0000;
const NATIVE_OHOS_BASE: u32 = 0x2300_0000;

/// Identity for a macOS key that winit could not name (`NativeKey::MacOS`,
/// an Apple scancode).
pub fn native_key_macos(scancode: u16) -> u32 {
    NATIVE_MACOS_BASE | (scancode as u32 & NATIVE_BAND_MASK)
}

/// Identity for a Windows key that winit could not name
/// (`NativeKey::Windows`, a virtual-key code).
pub fn native_key_windows(virtual_key: u16) -> u32 {
    NATIVE_WINDOWS_BASE | (virtual_key as u32 & NATIVE_BAND_MASK)
}

/// Identity for an Android key that winit could not name
/// (`NativeKey::Android`, a keycode).
pub fn native_key_android(keycode: u32) -> u32 {
    NATIVE_ANDROID_BASE | (keycode & NATIVE_BAND_MASK)
}

/// Identity for an OpenHarmony key that winit could not name.
pub fn native_key_ohos(keycode: u32) -> u32 {
    NATIVE_OHOS_BASE | (keycode & NATIVE_BAND_MASK)
}

/// Name a reserved-band key after the platform and number it came from, the
/// way GNU's last resort names an unnamed keysym after its number.
fn native_key_name(keysym: u32) -> Option<String> {
    let (platform, _base) = [
        ("mac", NATIVE_MACOS_BASE),
        ("win", NATIVE_WINDOWS_BASE),
        ("android", NATIVE_ANDROID_BASE),
        ("ohos", NATIVE_OHOS_BASE),
    ]
    .into_iter()
    .find(|(_, base)| keysym & !NATIVE_BAND_MASK == *base)?;
    Some(format!("{platform}-{}", keysym & NATIVE_BAND_MASK))
}

/// GNU's own names for the X11 special block, ported from
/// `lispy_function_keys` (`src/keyboard.c:5510-5591`) and
/// `iso_lispy_function_keys` (`src/keyboard.c:5596-5613`) and indexed exactly
/// as C indexes them: `keysym - 0xff00` and `keysym - 0xfe00`.  `""` is C's
/// `0`, "this table has no name for it", which is what sends GNU on to the
/// toolkit.  `#[rustfmt::skip]` keeps the C row layout so the port can be read
/// against the source line by line.
///
/// The block cannot simply be lowercased: GNU spells `XK_Henkan_Mode` `henkan`
/// and `XK_Kana_Lock` `kana-lock`, so lowercasing `XKeysymToString` would give
/// `henkan_mode`, which no GNU config binds.  Verified against the pinned GNU
/// Emacs by sending each keysym with xdotool and printing what `read-event`
/// returned (`Henkan_Mode` → `henkan`, `Zenkaku_Hankaku` →
/// `zenkaku-hankaku`, `KP_Enter` → `kp-enter`).
#[rustfmt::skip]
static LISPY_FUNCTION_KEYS: [&str; 256] = [
    "", "", "", "", "", "", "", "", // 0x00
    "backspace", "tab", "linefeed", "clear", "", "return", "", "", // 0x08
    "", "", "", "pause", "", "", "", "", // 0x10
    "", "", "", "escape", "", "", "", "", // 0x18
    "", "kanji", "muhenkan", "henkan", "romaji", "hiragana", "katakana", "hiragana-katakana", // 0x20
    "zenkaku", "hankaku", "zenkaku-hankaku", "touroku", "massyo", "kana-lock", "kana-shift", "eisu-shift", // 0x28
    "eisu-toggle", "", "", "", "", "", "", "", // 0x30
    "", "", "", "", "", "", "", "", // 0x38
    "", "", "", "", "", "", "", "", // 0x40
    "", "", "", "", "", "", "", "", // 0x48
    "home", "left", "up", "right", "down", "prior", "next", "end", // 0x50
    "begin", "", "", "", "", "", "", "", // 0x58
    "select", "print", "execute", "insert", "", "undo", "redo", "menu", // 0x60
    "find", "cancel", "help", "break", "", "", "", "", // 0x68
    "", "", "", "", "backtab", "", "", "", // 0x70
    "", "", "", "", "", "", "", "kp-numlock", // 0x78
    "kp-space", "", "", "", "", "", "", "", // 0x80
    "", "kp-tab", "", "", "", "kp-enter", "", "", // 0x88
    "", "kp-f1", "kp-f2", "kp-f3", "kp-f4", "kp-home", "kp-left", "kp-up", // 0x90
    "kp-right", "kp-down", "kp-prior", "kp-next", "kp-end", "kp-begin", "kp-insert", "kp-delete", // 0x98
    "", "", "", "", "", "", "", "", // 0xa0
    "", "", "kp-multiply", "kp-add", "kp-separator", "kp-subtract", "kp-decimal", "kp-divide", // 0xa8
    "kp-0", "kp-1", "kp-2", "kp-3", "kp-4", "kp-5", "kp-6", "kp-7", // 0xb0
    "kp-8", "kp-9", "", "", "", "kp-equal", "f1", "f2", // 0xb8
    "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", // 0xc0
    "f11", "f12", "f13", "f14", "f15", "f16", "f17", "f18", // 0xc8
    "f19", "f20", "f21", "f22", "f23", "f24", "f25", "f26", // 0xd0
    "f27", "f28", "f29", "f30", "f31", "f32", "f33", "f34", // 0xd8
    "f35", "", "", "", "", "", "", "", // 0xe0
    "", "", "", "", "", "", "", "", // 0xe8
    "", "", "", "", "", "", "", "", // 0xf0
    "", "", "", "", "", "", "", "delete", // 0xf8
];

/// The ISO 9995 block, same shape, indexed by `keysym - 0xfe00`.  Everything
/// below `iso-lefttab` is empty in GNU's table; the block's other entries are
/// the ISO margin/cursor/emphasis keys.
#[rustfmt::skip]
static ISO_LISPY_FUNCTION_KEYS: [&str; 53] = [
    "", "", "", "", "", "", "", "", // 0x00
    "", "", "", "", "", "", "", "", // 0x08
    "", "", "", "", "", "", "", "", // 0x10
    "", "", "", "", "", "", "", "", // 0x18
    "iso-lefttab", "iso-move-line-up", "iso-move-line-down", "iso-partial-line-up", "iso-partial-line-down", "iso-partial-space-left", "iso-partial-space-right", "iso-set-margin-left", // 0x20
    "iso-set-margin-right", "iso-release-margin-left", "iso-release-margin-right", "iso-release-both-margins", "iso-fast-cursor-left", "iso-fast-cursor-right", "iso-fast-cursor-up", "iso-fast-cursor-down", // 0x28
    "iso-continuous-underline", "iso-discontinuous-underline", "iso-emphasize", "iso-center-object", "iso-enter", // 0x30
];

/// GNU's table name for a keysym in the 0xff00 block, when the table has one.
/// Total on every input: a keysym below the block is simply not in it.
fn lispy_function_key_name(keysym: u32) -> Option<&'static str> {
    let offset = keysym.checked_sub(0xff00)?;
    let name = *LISPY_FUNCTION_KEYS.get(offset as usize)?;
    (!name.is_empty()).then_some(name)
}

/// The function-key symbol GNU would give this keysym, or `None` when even
/// GNU's last resort is the caller's to synthesize (`key-N`).
///
/// The tiers are `make_lispy_event`'s (`src/keyboard.c:6382-6412`) and
/// `modify_event_symbol`'s (`src/keyboard.c:7742-7823`), in order: the ISO
/// block, GNU's own table for the 0xff00 block, the reserved bands for keys
/// that never had a keysym, and finally the toolkit's keysym name — here the
/// X11 registry, the one `XKeysymToString` searches — taken **verbatim**,
/// which is what keeps `XF86Back`, `Scroll_Lock` and `3270_Attn` spelled the
/// way GNU spells them.  (`XF86AudioRaiseVolume` and `Scroll_Lock` were both
/// confirmed against the pinned GNU Emacs.)
///
/// The ISO block is the odd one out: GNU sends the whole 0xfe00-0xfeff range to
/// a 53-entry table and falls through to `key-<index>` — the *stripped* number,
/// not the keysym — and returns nil past the table's end, dropping the event.
/// No key this port can produce lands there (XKB consumes the group-switch
/// keysyms server-side, and winit names the rest), so the past-the-end case is
/// left to the caller's `key-N` rather than modelled.
pub(crate) fn function_key_name(keysym: u32) -> Option<String> {
    if let Some(name) = native_key_name(keysym) {
        return Some(name);
    }
    if (0xfe00..0xff00).contains(&keysym) {
        let index = (keysym - 0xfe00) as usize;
        return match ISO_LISPY_FUNCTION_KEYS.get(index) {
            Some(name) if !name.is_empty() => Some((*name).to_owned()),
            Some(_) => Some(format!("key-{index}")),
            None => None,
        };
    }
    if let Some(name) = lispy_function_key_name(keysym) {
        return Some(name.to_owned());
    }
    let item = keysymdefs::get_item_by_keysym(keysym)?;
    // `XK_F13` names the key `F13` and `XF86XK_Back` names it `XF86Back`:
    // X drops the `XK_` infix, keeping the vendor prefix intact.
    let name = item.name().replace("XK_", "");
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
#[path = "keysym/tests/keysym_test.rs"]
mod tests;
