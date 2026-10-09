use super::*;

#[test]
fn foreign_marker_byte_probe_is_bounded_on_a_continuation_byte() {
    for kind in implemented_text_backends() {
        let buf = buf_with_text_backend("é", kind);
        let value = buf.char_code_after_foreign_marker_byte_pos(EmacsBytePos::new(1));
        assert_eq!(
            value,
            Some(crate::emacs_core::emacs_char::byte8_to_char(0xA9)),
            "{kind:?}"
        );
        assert_eq!(
            buf.char_code_after_foreign_marker_byte_pos(EmacsBytePos::new(2)),
            None,
            "{kind:?}"
        );
        assert_eq!(
            buf.char_code_after_foreign_marker_byte_pos(EmacsBytePos::new(0)),
            Some(233),
            "{kind:?}"
        );
    }
}
