use super::*;
use crate::buffer::{BufferText, BufferTextBackendKind, EmacsBytePos, EmacsByteRange};

fn face_only_table() -> TextPropertyTable {
    let mut table = TextPropertyTable::new();
    for pos in 0..100 {
        table.put_property_in_char_range(
            CharRange::from_usize(pos, pos + 1),
            Value::symbol("face"),
            Value::symbol(if pos % 2 == 0 { "bold" } else { "italic" }),
        );
    }
    table
}

fn bounded(table: &TextPropertyTable, pos: usize, name: Value, limit: usize) -> Option<usize> {
    table
        .next_single_property_change_after_char_pos_bounded(
            CharPos0::new(pos),
            name,
            CharPos0::new(limit),
        )
        .map(CharPos0::get)
}

#[test]
fn absent_key_skips_dense_face_intervals_without_soft_stops() {
    let table = face_only_table();
    let invisible = Value::symbol("invisible");
    assert_eq!(
        table.property_name_presence(invisible),
        PropertyNamePresence::DefinitelyAbsent,
    );
    for (pos, limit) in [(0, 1), (0, 31), (30, 32), (90, 150), (100, 150)] {
        assert_eq!(bounded(&table, pos, invisible, limit), None);
    }
}

#[test]
fn assigning_a_key_detaches_the_absence_proof_and_preserves_limits_and_values() {
    let mut table = face_only_table();
    let snapshot = table.clone();
    let invisible = Value::symbol("invisible");
    table.put_property_in_char_range(CharRange::from_usize(70, 80), invisible, Value::T);
    assert_eq!(
        table.property_name_presence(invisible),
        PropertyNamePresence::PossiblyPresent,
    );
    assert_eq!(bounded(&snapshot, 0, invisible, 30), None);
    assert_eq!(bounded(&table, 0, invisible, 30), Some(30));
    assert_eq!(bounded(&table, 0, invisible, 75), Some(70));
    assert_eq!(bounded(&table, 75, invisible, 150), Some(80));
    assert_eq!(
        table.get_property_at_char_pos(CharPos0::new(75), invisible),
        Some(Value::T),
    );
    assert_eq!(
        table.get_property_at_char_pos(CharPos0::new(80), invisible),
        None
    );

    // Removing the last occurrence may retain a conservative positive. Its
    // full bounded walk remains valid, including the existing soft stop.
    table.remove_property_in_char_range(CharRange::from_usize(70, 80), invisible);
    assert_eq!(
        table.property_name_presence(invisible),
        PropertyNamePresence::PossiblyPresent,
    );
    assert_eq!(bounded(&table, 0, invisible, 30), Some(30));
    assert_eq!(bounded(&table, 0, invisible, 150), None);
}

#[test]
fn replacing_or_grafting_properties_makes_a_formerly_absent_key_visible() {
    let invisible = Value::symbol("invisible");
    let mut table = face_only_table();
    table.set_properties_in_char_range(CharRange::from_usize(20, 30), vec![(invisible, Value::T)]);
    assert_eq!(bounded(&table, 0, invisible, 40), Some(20));

    let mut grafted = face_only_table();
    grafted.append_shifted_at_char_offset(&table, CharLen::new(100));
    assert_eq!(bounded(&grafted, 100, invisible, 140), Some(120));
    assert_eq!(bounded(&grafted, 125, invisible, 150), Some(130));
}

#[test]
fn byte_caps_still_clamp_and_align_before_raw_absence_or_presence_queries() {
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
