//! The evaluator's half of the runtime's key-transport and terminal-byte
//! tests (DECISIONS D44): what the evaluator makes of the values
//! `neomacs_display_runtime::testing` records for the runtime's half.

use neomacs_display_runtime::testing::key_transport::{
    self, CTRL_ALT_N, CTRL_ALT_SHIFT_3, TransportedKey,
};
use neomacs_display_runtime::testing::raw_bytes::RAW_BYTE_A4_INTERNAL;
use neovm_core::keyboard::{InputEvent, Key};

fn cook(transported: TransportedKey, emacs_frame_id: u64) -> InputEvent {
    neovm_core::keyboard::render_key_transport_to_input_event(
        transported.key,
        transported.modifiers.bits(),
        true,
        emacs_frame_id,
    )
    .expect("a transported key press cooks to an input event")
}

/// Backs `render_thread::input::tests::fullwidth_committed_text_reaches_core_as_a_character`,
/// `render_thread::input::tests::fullwidth_logical_keys_reach_core_as_characters` (frame 42)
/// and `tty_input::tests::crossterm_fullwidth_characters_reach_core_as_text` (frame 0).
#[test]
fn transported_fullwidth_characters_reach_core_as_characters() {
    for emacs_frame_id in [0, 42] {
        for character in key_transport::FULLWIDTH_TEXT.chars() {
            let event = cook(key_transport::fullwidth(character), emacs_frame_id);
            assert!(
                matches!(&event, InputEvent::KeyPress { key, emacs_frame_id: frame }
                    if *frame == emacs_frame_id && key.key == Key::Char(character)),
                "{character:?} on frame {emacs_frame_id}: {event:?}"
            );
        }
    }
}

/// Backs `render_thread::input::tests::ctrl_alt_letter_cooks_to_gnu_c_m_letter`.
#[test]
fn transported_ctrl_alt_n_cooks_to_gnu_c_m_n() {
    let InputEvent::KeyPress {
        key,
        emacs_frame_id,
    } = cook(CTRL_ALT_N, 42)
    else {
        panic!("expected a key press");
    };
    assert_eq!(emacs_frame_id, 42);
    assert_eq!(key.to_description(), "C-M-n");
    // The integer GNU's `(kbd "C-M-n")` produces.
    assert_eq!(key.to_emacs_event_value().as_int(), Some(0x800_000e));
}

/// Backs `render_thread::input::tests::ctrl_alt_shift_3_cooks_to_gnu_c_m_hash`.
#[test]
fn transported_ctrl_alt_shift_3_cooks_to_gnu_c_m_hash() {
    let InputEvent::KeyPress { key, .. } = cook(CTRL_ALT_SHIFT_3, 42) else {
        panic!("expected a key press");
    };
    assert_eq!(key.to_description(), "C-M-#");
    // The integer GNU's `(kbd "C-M-#")` produces.
    assert_eq!(key.to_emacs_event_value().as_int(), Some(0xc00_0023));
}

/// Backs `tty_input::relay_tests::real_reader_preserves_bytes_split_across_actual_host_reads`:
/// the recorded bytes are the evaluator's internal encoding of raw byte 0xa4.
#[test]
fn recorded_raw_byte_a4_is_the_evaluator_internal_encoding() {
    let raw = neovm_core::emacs_core::emacs_char::EmacsChar::from_byte8(0xa4);
    let mut internal = [0; neovm_core::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let count = raw.char_string(&mut internal);
    assert_eq!(&internal[..count], RAW_BYTE_A4_INTERNAL);
}
