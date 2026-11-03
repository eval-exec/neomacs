use super::{GnuDeclaredCharacterCount, LispString};

#[test]
fn declared_character_midpoint_chooses_backward_payload_boundary() {
    crate::test_utils::init_test_tracing();
    let _context = crate::emacs_core::eval::Context::new();
    let text = LispString::from_emacs_bytes_with_declared_chars(
        "béa".as_bytes().to_vec(),
        GnuDeclaredCharacterCount::try_from(2).expect("test metadata"),
    );
    assert_eq!(text.char_to_byte_pos(0), 0);
    assert_eq!(text.char_to_byte_pos(1), 3);
    assert_eq!(text.char_to_byte_pos(2), 4);
    let ordinary = LispString::from_utf8("bé");
    assert_eq!(ordinary.char_to_byte_pos(1), 1);
}
