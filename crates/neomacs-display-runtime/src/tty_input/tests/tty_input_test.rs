use super::*;
use crossterm::event::{KeyEventKind, KeyEventState};
use neomacs_display_protocol::FrontendKey;

#[cfg(unix)]
#[test]
fn unix_tty_read_batch_is_forwarded_as_raw_bytes() {
    let event = raw_tty_input_event(b"\x1b[A".to_vec());

    assert!(matches!(
        event,
        InputEvent::RawTtyBytes {
            bytes,
            emacs_frame_id: 0,
        } if bytes == b"\x1b[A"
    ));
}

fn key_event(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent {
        code,
        modifiers,
        kind: KeyEventKind::Press,
        state: KeyEventState::empty(),
    }
}

fn key_parts(code: KeyCode, modifiers: KeyModifiers) -> (FrontendKey, u32) {
    match map_key_event(key_event(code, modifiers)).expect("key event") {
        InputEvent::Key { key, modifiers, .. } => (key, modifiers),
        _ => panic!("expected key event"),
    }
}

#[test]
fn tty_control_digit_aliases_are_raw_control_bytes() {
    let cases = [
        ('2', 0x00),
        ('3', 0x1b),
        ('4', 0x1c),
        ('5', 0x1d),
        ('6', 0x1e),
        ('7', 0x1f),
        ('8', 0x7f),
        ('/', 0x1f),
    ];

    for (input, expected) in cases {
        let (keysym, modifiers) = key_parts(KeyCode::Char(input), KeyModifiers::CONTROL);
        assert_eq!(keysym, FrontendKey::Keysym(expected), "input C-{input}");
        assert_eq!(
            modifiers & neomacs_display_protocol::ModifierBits::CTRL.bits(),
            0,
            "input C-{input}"
        );
    }
}

#[test]
fn tty_meta_control_alias_preserves_meta_only() {
    let (keysym, modifiers) = key_parts(
        KeyCode::Char('4'),
        KeyModifiers::CONTROL | KeyModifiers::ALT,
    );

    assert_eq!(keysym, FrontendKey::Keysym(0x1c));
    assert_eq!(
        modifiers & neomacs_display_protocol::ModifierBits::CTRL.bits(),
        0
    );
    assert_ne!(
        modifiers & neomacs_display_protocol::ModifierBits::META.bits(),
        0
    );
}

#[test]
fn tty_backspace_is_raw_del_byte() {
    let (keysym, modifiers) = key_parts(KeyCode::Backspace, KeyModifiers::ALT);

    assert_eq!(keysym, FrontendKey::Keysym(0x7f));
    assert_ne!(
        modifiers & neomacs_display_protocol::ModifierBits::META.bits(),
        0
    );
}

/// The runtime's half of a split test (DECISIONS D44): a terminal's
/// fullwidth character is transported as the character with no modifier.
/// The display host pins what the evaluator cooks from that transported key
/// (`key_transport_contract_test.rs`).
#[test]
fn crossterm_fullwidth_characters_reach_core_as_text() {
    use crate::testing::key_transport::{self, TransportedKey};
    use neomacs_display_protocol::ModifierBits;
    for character in key_transport::FULLWIDTH_TEXT.chars() {
        let (key, modifiers) = key_parts(KeyCode::Char(character), KeyModifiers::NONE);
        assert_eq!(
            TransportedKey {
                key,
                modifiers: ModifierBits::from_bits(modifiers),
            },
            key_transport::fullwidth(character)
        );
    }
}
