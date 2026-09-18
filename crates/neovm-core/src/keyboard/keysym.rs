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

/// X's special block, 0xfe00-0xffff: the ISO/kbd specials, the cursor, misc
/// and keypad bands, the F-key block, and `XK_VoidSymbol`.
pub(crate) fn is_named_block(keysym: u32) -> bool {
    (0xfe00..=0xffff).contains(&keysym)
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

/// The function-key symbol GNU would give this keysym, or `None` when even
/// GNU's last resort is the caller's to synthesize (`key-N`).
///
/// Two tiers, mirroring `modify_event_symbol` (`src/keyboard.c:7749-7805`):
/// the toolkit's keysym name (here the X11 registry, the same table
/// `XKeysymToString` searches) and the reserved bands for keys that never had
/// a keysym.  Vendor keysyms keep their spelling — `XF86Back`, exactly what
/// `(kbd "<XF86Back>")` matches — while the standard block is lowercased the
/// way GNU's own `lispy_function_keys` spells those keys (`undo`, `menu`,
/// `find`, `f13`).
pub(crate) fn function_key_name(keysym: u32) -> Option<String> {
    if let Some(name) = native_key_name(keysym) {
        return Some(name);
    }
    let item = keysymdefs::get_item_by_keysym(keysym)?;
    // `XK_F13` names the key `F13` and `XF86XK_Back` names it `XF86Back`:
    // X drops the `XK_` infix, keeping the vendor prefix intact.
    let name = item.name().replace("XK_", "");
    if name.is_empty() {
        return None;
    }
    Some(if is_vendor_keysym(keysym) {
        name
    } else {
        name.to_lowercase()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bands_match_gnu_ranges() {
        assert!(is_function_key(0xffbe) && is_function_key(0xffe0));
        assert!(!is_function_key(0xffbd) && !is_function_key(0xffe1));
        assert!(is_misc_function_key(0xff65) && is_misc_function_key(0xff6b));
        assert!(!is_misc_function_key(0xff5f) && !is_misc_function_key(0xff6c));
        assert!(is_modifier_key(0xffe1) && is_modifier_key(0xffee));
        assert!(!is_modifier_key(0xffe0) && !is_modifier_key(0xffef));
    }

    #[test]
    fn native_bands_are_outside_the_keysym_space() {
        for keysym in [
            native_key_macos(0x24),
            native_key_windows(0x5d),
            native_key_android(4),
            native_key_ohos(7),
        ] {
            assert!(
                keysym >= 0x2000_0000,
                "{keysym:#x} must not collide with a keysym"
            );
            assert!(keysymdefs::get_item_by_keysym(keysym).is_none());
        }
    }

    #[test]
    fn names_follow_gnu_spelling() {
        assert_eq!(function_key_name(0xffca).as_deref(), Some("f13"));
        assert_eq!(function_key_name(0xffe0).as_deref(), Some("f35"));
        assert_eq!(function_key_name(0xff65).as_deref(), Some("undo"));
        assert_eq!(function_key_name(0xff67).as_deref(), Some("menu"));
        assert_eq!(function_key_name(0x1008ff26).as_deref(), Some("XF86Back"));
        assert_eq!(function_key_name(0x1008ff6d).as_deref(), Some("XF86Paste"));
        assert_eq!(
            function_key_name(native_key_macos(0x24)).as_deref(),
            Some("mac-36")
        );
        assert_eq!(
            function_key_name(native_key_windows(0x5d)).as_deref(),
            Some("win-93")
        );
    }
}
