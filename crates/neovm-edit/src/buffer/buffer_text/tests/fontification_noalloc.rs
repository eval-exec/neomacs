use super::*;

use crate::buffer::{BufferTextBackendKind, LispCharPos1};

use crate::emacs_core::textprop;

use crate::emacs_core::textprop_entries;

#[test]
fn fontification_no_alloc_conversions_match_all_backends_after_edits() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::implemented_variants() {
        let mut expected = "aé中λ".repeat(600);
        let mut text = BufferText::try_from_str_with_backend_kind(&expected, kind).unwrap();
        assert_eq!(anchor_storage(&text), (0, 0));
        assert_conversions(&text, &expected);

        let insertion = "文Ω";
        let at = expected.char_indices().nth(700).unwrap().0;
        let extent = TextExtent::from_emacs_bytes(insertion.as_bytes(), true);
        text.insert_measured_emacs_bytes(EmacsBytePos::new(at), insertion.as_bytes(), extent);
        expected.insert_str(at, insertion);
        assert_conversions(&text, &expected);

        let start = expected.char_indices().nth(300).unwrap().0;
        let end = expected.char_indices().nth(900).unwrap().0;
        let edit = text.edit_range_for_emacs_byte_range(EmacsByteRange::from_usize(start, end));
        text.delete_measured_range(edit);
        expected.replace_range(start..end, "");
        assert_conversions(&text, &expected);
    }
}

fn anchor_storage(text: &BufferText) -> (usize, usize) {
    let storage = text.storage.borrow();
    let anchors = storage.anchor_cache.borrow();
    (anchors.len(), anchors.capacity())
}

fn assert_conversions(text: &BufferText, expected: &str) {
    let before = anchor_storage(text);
    for char_pos in [
        0,
        1,
        511,
        1025,
        expected.chars().count() - 1,
        expected.chars().count(),
    ] {
        let byte_pos = expected
            .char_indices()
            .nth(char_pos)
            .map_or(expected.len(), |(byte, _)| byte);
        assert_eq!(
            text.char_pos_to_emacs_byte_pos_no_alloc(CharPos0::new(char_pos)),
            EmacsBytePos::new(byte_pos)
        );
        assert_eq!(
            text.emacs_byte_pos_to_char_pos_no_alloc(EmacsBytePos::new(byte_pos)),
            CharPos0::new(char_pos)
        );
    }
    assert_eq!(
        anchor_storage(text),
        before,
        "native conversions grew anchor storage"
    );
    // The existing cached conversion remains a separate reference, outside the
    // native-query storage observation above.
    for char_pos in [1, 511, 1025, expected.chars().count() - 1] {
        let position = CharPos0::new(char_pos);
        assert_eq!(
            text.char_pos_to_emacs_byte_pos_no_alloc(position),
            text.char_pos_to_emacs_byte_pos(position)
        );
    }
}
