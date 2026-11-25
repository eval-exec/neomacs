//! Keys as the frontend transports them to the evaluator.
//!
//! The runtime's input tests assert that a toolkit key becomes the
//! [`TransportedKey`] below; the display host asserts what the evaluator
//! cooks from that same value (`render_key_transport_to_input_event`).

use neomacs_display_protocol::{FrontendKey, ModifierBits};

/// One key event's transported identity and modifier bits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportedKey {
    pub key: FrontendKey,
    pub modifiers: ModifierBits,
}

/// Fullwidth characters that toolkits and terminals deliver as text. Each
/// one is transported as [`fullwidth`] and must reach the evaluator as that
/// character, not as a keysym that happens to share its code.
pub const FULLWIDTH_TEXT: &str = "，（）；－ｦￊ";

/// The transported form of each character of [`FULLWIDTH_TEXT`]: the
/// character itself, with no modifier.
pub const fn fullwidth(character: char) -> TransportedKey {
    TransportedKey {
        key: FrontendKey::Character(character),
        modifiers: ModifierBits::NONE,
    }
}

/// `Ctrl+Alt+n`. Windows folds Ctrl+Alt into AltGr and reports the layout's
/// key value without the command modifiers, so the toolkit delivers `n`;
/// GNU's `make_lispy_event` cooks the chord to `C-M-n`
/// (`src/keyboard.c:6328-6337`, issue #756).
pub const CTRL_ALT_N: TransportedKey = TransportedKey {
    key: FrontendKey::Character('n'),
    modifiers: ModifierBits::from_bits(ModifierBits::CTRL.bits() | ModifierBits::META.bits()),
};

/// `Ctrl+Alt+Shift+3`, which the toolkit delivers as the shifted `#`: GNU's
/// `(kbd "C-M-#")`, where Shift does not survive as a modifier bit
/// (`src/keyboard.c:2178-2211`, issue #756).
pub const CTRL_ALT_SHIFT_3: TransportedKey = TransportedKey {
    key: FrontendKey::Character('#'),
    modifiers: ModifierBits::from_bits(
        ModifierBits::CTRL.bits() | ModifierBits::META.bits() | ModifierBits::SHIFT.bits(),
    ),
};
