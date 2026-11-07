use crate::buffer::{BufferText, BufferTextBackendKind, EmacsBytePos, EmacsByteRange};
use neovm_value::value::Value;
use neovm_value::value::positions::CharPos0;

#[test]
fn byte_caps_still_clamp_and_align_before_raw_absence_or_presence_queries() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::implemented_variants() {
        for content in ["abcdef", "a好bcé"] {
            let text = BufferText::try_from_str_with_backend_kind(content, kind)
                .expect("implemented text backend");
            let invisible = Value::symbol("invisible");
            for pos in 0..content.chars().count() {
                text.text_props_put_property_in_emacs_byte_range(
                    EmacsByteRange::new(
                        text.char_pos_to_emacs_byte_pos(CharPos0::new(pos)),
                        text.char_pos_to_emacs_byte_pos(CharPos0::new(pos + 1)),
                    ),
                    Value::symbol("face"),
                    Value::symbol(if pos % 2 == 0 { "bold" } else { "italic" }),
                );
            }
            for limit in [EmacsBytePos::new(2), EmacsBytePos::new(1000)] {
                assert_eq!(
                    text.text_props_next_single_change_after_emacs_byte_pos_bounded(
                        EmacsBytePos::ZERO,
                        invisible,
                        limit,
                    ),
                    None,
                    "{kind:?}: {content:?}",
                );
            }
            let start = text.char_pos_to_emacs_byte_pos(CharPos0::new(2));
            let end = text.char_pos_to_emacs_byte_pos(CharPos0::new(3));
            text.text_props_put_property_in_emacs_byte_range(
                EmacsByteRange::new(start, end),
                invisible,
                Value::T,
            );
            for limit in [EmacsBytePos::new(2), EmacsBytePos::new(1000)] {
                assert_eq!(
                    text.text_props_next_single_change_after_emacs_byte_pos_bounded(
                        EmacsBytePos::ZERO,
                        invisible,
                        limit,
                    ),
                    Some(start),
                    "{kind:?}: {content:?}",
                );
            }
            assert_eq!(
                text.text_props_get_property_at_emacs_byte_pos(start, invisible),
                Some(Value::T)
            );
            assert_eq!(
                text.text_props_get_property_at_emacs_byte_pos(end, invisible),
                None
            );
        }
    }
}
