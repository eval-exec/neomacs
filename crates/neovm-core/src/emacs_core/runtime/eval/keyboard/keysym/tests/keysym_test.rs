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

/// GNU's table names are not lowercased X11 names: `XK_Henkan_Mode` is
/// `henkan` and `XK_Kana_Lock` is `kana-lock`.  Rows marked `oracle` were
/// read off the pinned GNU Emacs — xdotool sent that keysym to a GUI frame
/// running `(read-event)`, which prints the symbol `modify_event_symbol`
/// built; the rest follow from the same table (`src/keyboard.c:5510-5613`),
/// whose indices the port asserts against the source.
#[test]
fn names_are_gnus_table_names_not_lowercased_x11_names() {
    for (keysym, expected) in [
        (0xff23, "henkan"),            // oracle
        (0xff27, "hiragana-katakana"), // oracle
        (0xff2a, "zenkaku-hankaku"),   // oracle
        (0xff8d, "kp-enter"),          // oracle
        // Empty table entries fall through to XKeysymToString, verbatim.
        (0xff14, "Scroll_Lock"),       // oracle
        (0xfd0e, "3270_Attn"),         // oracle
        (0x1008ff26, "XF86Back"),      // oracle
        (0xff3d, "MultipleCandidate"), // oracle
        // The rest of the 0xff00 block, from the ported table.
        (0xff0b, "clear"),
        (0xff21, "kanji"),
        (0xff22, "muhenkan"),
        (0xff24, "romaji"),
        (0xff25, "hiragana"),
        (0xff28, "zenkaku"),
        (0xff29, "hankaku"),
        (0xff2d, "kana-lock"),
        (0xff2f, "eisu-shift"),
        (0xff60, "select"),
        (0xff62, "execute"),
        (0xff6a, "help"),
        // The ISO block is indexed with the offset stripped.
        (0xfe20, "iso-lefttab"),
        (0xfe08, "key-8"),
        // Below the block the table lookup is total rather than an
        // underflow, and the registry names a Latin-1 keysym as itself.
        (0x61, "a"),
    ] {
        assert_eq!(
            function_key_name(keysym).as_deref(),
            Some(expected),
            "keysym {keysym:#06x}"
        );
    }
}
