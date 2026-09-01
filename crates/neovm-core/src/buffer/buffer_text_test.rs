use std::str::FromStr;

use crate::buffer::text::{ImplementedBufferTextBackendKind, TextBackendDebugLayout};
use crate::buffer::{
    BufferTextBackendKind, CharLen, CharPos0, CharRange, EmacsByteLen, EmacsBytePos,
    EmacsByteRange, TextEditRange, TextExtent, TextMetrics,
};
use crate::emacs_core::value::Value;

use super::BufferText;

fn implemented_kind(kind: BufferTextBackendKind) -> ImplementedBufferTextBackendKind {
    kind.implemented()
        .expect("test backend should be implemented")
}

fn emacs_byte_pos(pos: usize) -> EmacsBytePos {
    EmacsBytePos::new(pos)
}

fn emacs_byte_range(start: usize, end: usize) -> EmacsByteRange {
    EmacsByteRange::from_usize(start, end)
}

fn full_emacs_byte_range(text: &BufferText) -> EmacsByteRange {
    EmacsByteRange::from_start_len(EmacsBytePos::ZERO, text.emacs_byte_len())
}

fn char_pos_to_byte_pos(text: &BufferText, target: usize) -> usize {
    text.char_pos_to_emacs_byte_pos(CharPos0::new(target)).get()
}

fn byte_pos_to_char_pos(text: &BufferText, target: usize) -> usize {
    text.emacs_byte_pos_to_char_pos(EmacsBytePos::new(target))
        .get()
}

fn insert_storage_string(text: &mut BufferText, pos: EmacsBytePos, value: &str) {
    if value.is_empty() {
        return;
    }
    let multibyte = text.is_multibyte();
    let bytes = crate::emacs_core::string_escape::storage_string_to_buffer_bytes(value, multibyte);
    let extent = TextExtent::from_emacs_bytes(&bytes, multibyte);
    text.insert_measured_emacs_bytes(pos, &bytes, extent);
}

fn delete_emacs_byte_range(text: &mut BufferText, range: EmacsByteRange) {
    if range.is_empty() {
        return;
    }
    let edit_range = text.edit_range_for_emacs_byte_range(range);
    text.delete_measured_range(edit_range);
}

#[test]
fn backend_kind_defaults_to_gap_buffer_with_stable_symbol_spelling() {
    crate::test_utils::init_test_tracing();
    let text = BufferText::new();

    assert_eq!(text.backend_kind(), BufferTextBackendKind::GapBuffer);
    assert_eq!(text.backend_kind().symbol_name(), "gap-buffer");
    assert_eq!(u8::from(BufferTextBackendKind::GapBuffer), 0);
    assert_eq!(u8::from(BufferTextBackendKind::PieceTree), 1);
    assert_eq!(u8::from(BufferTextBackendKind::Rope), 2);
    assert_eq!(
        BufferTextBackendKind::try_from(0),
        Ok(BufferTextBackendKind::GapBuffer)
    );
    assert_eq!(
        BufferTextBackendKind::try_from(1),
        Ok(BufferTextBackendKind::PieceTree)
    );
    assert_eq!(
        BufferTextBackendKind::try_from(2),
        Ok(BufferTextBackendKind::Rope)
    );
    assert!(text.backend_kind().is_implemented());
    assert_eq!(
        BufferTextBackendKind::from_str("piece-tree"),
        Ok(BufferTextBackendKind::PieceTree)
    );
    assert_eq!(
        BufferTextBackendKind::implemented_variants().collect::<Vec<_>>(),
        vec![
            BufferTextBackendKind::GapBuffer,
            BufferTextBackendKind::PieceTree,
            BufferTextBackendKind::Rope,
        ]
    );
    assert_eq!(
        BufferTextBackendKind::non_gap_implemented_variants().collect::<Vec<_>>(),
        vec![
            BufferTextBackendKind::PieceTree,
            BufferTextBackendKind::Rope,
        ]
    );
    assert_eq!(
        ImplementedBufferTextBackendKind::variants()
            .map(ImplementedBufferTextBackendKind::public_kind)
            .collect::<Vec<_>>(),
        BufferTextBackendKind::implemented_variants().collect::<Vec<_>>()
    );
    assert_eq!(
        ImplementedBufferTextBackendKind::PIECE_TREE.symbol_name(),
        "piece-tree"
    );
    assert!(BufferTextBackendKind::PieceTree.is_implemented());
    assert!(BufferTextBackendKind::Rope.is_implemented());
}

#[test]
fn buffer_text_can_use_non_gap_backends() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::non_gap_implemented_variants() {
        let layout = match kind {
            BufferTextBackendKind::GapBuffer => unreachable!("filtered above"),
            BufferTextBackendKind::PieceTree => {
                TextBackendDebugLayout::PieceTree(TextMetrics::from_usize(5, 6))
            }
            BufferTextBackendKind::Rope => {
                TextBackendDebugLayout::Rope(TextMetrics::from_usize(5, 6))
            }
        };
        let mut text = BufferText::from_str_with_backend_kind("abécd", implemented_kind(kind));

        assert_eq!(text.backend_kind(), kind);
        assert_eq!(text.backend_debug_layout(), layout);
        assert!(text.gap_debug_layout().is_none());

        let insert_pos = char_pos_to_byte_pos(&text, 2);
        insert_storage_string(&mut text, emacs_byte_pos(insert_pos), "XY");
        assert_eq!(text.to_string(), "abXYécd");

        let delete_start = char_pos_to_byte_pos(&text, 1);
        let delete_end = char_pos_to_byte_pos(&text, 4);
        delete_emacs_byte_range(&mut text, emacs_byte_range(delete_start, delete_end));
        assert_eq!(text.to_string(), "aécd");
        assert_eq!(
            text.backend_debug_layout().metrics(),
            TextMetrics::from_usize(4, 5)
        );
    }
}

#[test]
fn public_backend_kind_helpers_select_and_convert_storage() {
    crate::test_utils::init_test_tracing();
    let text = BufferText::try_from_str_with_backend_kind("abc", BufferTextBackendKind::Rope)
        .expect("rope backend should be available");

    assert_eq!(text.backend_kind(), BufferTextBackendKind::Rope);
    assert_eq!(text.to_string(), "abc");

    text.try_convert_backend_kind(BufferTextBackendKind::PieceTree)
        .expect("piece-tree backend should be available");
    assert_eq!(text.backend_kind(), BufferTextBackendKind::PieceTree);
    assert_eq!(text.to_string(), "abc");

    let empty = BufferText::try_new_with_backend_kind(BufferTextBackendKind::Rope)
        .expect("rope backend should be available");
    assert_eq!(empty.backend_kind(), BufferTextBackendKind::Rope);
    assert!(empty.is_empty());
}

#[test]
fn shared_clone_observes_backend_conversion_and_semantic_edits() {
    crate::test_utils::init_test_tracing();
    let text = BufferText::from_str("aé\nz");
    let mut shared = text.shared_clone();
    let deep = text.clone();
    assert!(text.shares_storage_with(&shared));
    assert!(!text.shares_storage_with(&deep));

    text.try_convert_backend_kind(BufferTextBackendKind::Rope)
        .expect("rope backend should be available");
    assert_eq!(shared.backend_kind(), BufferTextBackendKind::Rope);
    assert_eq!(deep.backend_kind(), BufferTextBackendKind::GapBuffer);

    let insert_at = emacs_byte_pos("aé".len());
    insert_storage_string(&mut shared, insert_at, "Ω");
    assert_eq!(text.to_string(), "aéΩ\nz");
    assert_eq!(shared.metrics(), text.metrics());
    assert_eq!(deep.to_string(), "aé\nz");

    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let prop_start = emacs_byte_pos(char_pos_to_byte_pos(&text, 1));
    let prop_end = emacs_byte_pos(char_pos_to_byte_pos(&text, 4));
    assert!(text.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::new(prop_start, prop_end),
        face,
        bold,
    ));

    let inside_prop = emacs_byte_pos(char_pos_to_byte_pos(&shared, 2));
    assert_eq!(
        shared.text_props_get_property_at_emacs_byte_pos(inside_prop, face),
        Some(bold)
    );
    assert_eq!(
        deep.text_props_get_property_at_emacs_byte_pos(inside_prop, face),
        None
    );
}

#[test]
fn bounded_text_property_scan_rounds_up_a_multibyte_limit_to_a_character_boundary() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::implemented_variants() {
        let text = BufferText::try_from_str_with_backend_kind("a好bc", kind)
            .expect("implemented text backend");
        let name = Value::symbol("invisible");
        let hidden = Value::symbol("hidden");
        let property_start = text.char_pos_to_emacs_byte_pos(CharPos0::new(2));
        let property_end = text.char_pos_to_emacs_byte_pos(CharPos0::new(3));
        assert!(text.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(property_start, property_end),
            name,
            hidden,
        ));

        let inside_multibyte_character = EmacsBytePos::new(2);
        assert_eq!(
            text.text_props_next_single_change_after_emacs_byte_pos_bounded(
                EmacsBytePos::ZERO,
                name,
                inside_multibyte_character,
            ),
            Some(text.char_pos_to_emacs_byte_pos(CharPos0::new(2))),
            "{kind:?} must publish a soft scan boundary at a valid character boundary",
        );
    }
}

#[test]
fn non_gap_backends_preserve_virtual_gap_compatibility_state() {
    crate::test_utils::init_test_tracing();
    let non_gap = BufferTextBackendKind::non_gap_implemented_variants().collect::<Vec<_>>();
    assert!(
        non_gap.len() >= 2,
        "test expects at least two non-gap backends"
    );

    for &kind in &non_gap {
        let mut text = BufferText::from_str("abc");
        let initial_gap_position = text.gap_position_lisp();
        let initial_gap_size = text.gap_size_lisp();
        assert_eq!(initial_gap_position, 4);

        text.try_convert_backend_kind(kind)
            .expect("backend should be implemented");
        assert_eq!(text.backend_kind(), kind);
        assert_eq!(text.gap_position_lisp(), initial_gap_position);
        assert_eq!(text.gap_size_lisp(), initial_gap_size);

        let end_pos = emacs_byte_pos(text.emacs_byte_len().get());
        insert_storage_string(&mut text, end_pos, "d");
        assert_eq!(text.gap_position_lisp(), initial_gap_position + 1);
        assert_eq!(text.gap_size_lisp(), initial_gap_size - 1);

        let other = non_gap
            .iter()
            .copied()
            .find(|candidate| *candidate != kind)
            .expect("second non-gap backend should exist");
        text.try_convert_backend_kind(other)
            .expect("second backend should be implemented");
        assert_eq!(text.backend_kind(), other);
        assert_eq!(text.gap_position_lisp(), initial_gap_position + 1);
        assert_eq!(text.gap_size_lisp(), initial_gap_size - 1);
    }
}

#[test]
fn backend_conversion_to_gap_preserves_virtual_gap_compatibility_state() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::non_gap_implemented_variants() {
        let mut text = BufferText::from_str("abéc");

        text.try_convert_backend_kind(kind)
            .expect("backend should be implemented");
        insert_storage_string(&mut text, emacs_byte_pos(0), "Ω");
        let virtual_gap_position = text.gap_position_lisp();
        let virtual_gap_size = text.gap_size_lisp();
        assert_eq!(text.to_string(), "Ωabéc");

        text.try_convert_backend_kind(BufferTextBackendKind::GapBuffer)
            .expect("gap backend should be implemented");
        let real_gap = text
            .gap_debug_layout()
            .expect("converted backend should be a real gap buffer")
            .compat_state();
        assert_eq!(
            real_gap.lisp_position(),
            virtual_gap_position,
            "{kind:?} virtual gap position should become the real GPT"
        );
        assert_eq!(
            real_gap.lisp_size(),
            virtual_gap_size,
            "{kind:?} virtual gap size should become the real GAP_SIZE"
        );
        assert_eq!(text.gap_position_lisp(), virtual_gap_position);
        assert_eq!(text.gap_size_lisp(), virtual_gap_size);

        text.try_convert_backend_kind(kind)
            .expect("backend should convert back to non-gap");
        assert_eq!(text.gap_position_lisp(), virtual_gap_position);
        assert_eq!(text.gap_size_lisp(), virtual_gap_size);
    }
}

#[test]
fn non_gap_same_len_replace_preserves_virtual_gap_byte_position() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::non_gap_implemented_variants() {
        let mut text = BufferText::from_str("éxq");
        text.try_convert_backend_kind(kind)
            .expect("backend should be implemented");
        let initial_gap_size = text.gap_size_lisp();
        assert_eq!(text.gap_position_lisp(), 4);

        text.replace_same_len_emacs_byte_range(emacs_byte_range(0, "éx".len()), "€".as_bytes());

        assert_eq!(text.to_string(), "€q");
        assert_eq!(
            text.gap_position_lisp(),
            3,
            "{kind:?} should keep the virtual gap at the same byte position"
        );
        assert_eq!(text.gap_size_lisp(), initial_gap_size);
    }
}

#[test]
fn edit_measurement_boundary_is_backend_neutral() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::implemented_variants() {
        let text = BufferText::from_str_with_backend_kind("aé\n🙂z", implemented_kind(kind));
        let start = char_pos_to_byte_pos(&text, 1);
        let end = char_pos_to_byte_pos(&text, 4);

        let range = text.edit_range_for_emacs_byte_range(EmacsByteRange::from_usize(start, end));
        assert_eq!(
            range.byte_range(),
            EmacsByteRange::from_usize(start, end),
            "{kind:?} byte range changed while measuring edit range"
        );
        assert_eq!(
            range.char_start(),
            CharPos0::new(1),
            "{kind:?} edit start char position diverged"
        );
        assert_eq!(
            range.char_end(),
            CharPos0::new(4),
            "{kind:?} edit end char position diverged"
        );
        assert_eq!(
            range.extent(),
            TextExtent::from_usize(3, end - start),
            "{kind:?} edit extent diverged"
        );
        assert_eq!(
            text.edit_range_for_char_range(CharRange::from_usize(1, 4)),
            range,
            "{kind:?} char range edit measurement diverged"
        );

        let byte_pos = EmacsBytePos::new(start);
        let empty = text.edit_range_at_emacs_byte_pos(byte_pos);
        assert_eq!(
            empty,
            TextEditRange::empty_at(byte_pos, CharPos0::new(1)),
            "{kind:?} empty edit range diverged"
        );
        assert_eq!(
            text.edit_range_for_char_range(CharRange::from_usize(1, 1)),
            empty,
            "{kind:?} empty char range edit measurement diverged"
        );

        let extent = TextExtent::from_emacs_bytes("Ωx".as_bytes(), true);
        let insertion = text.insertion_at_emacs_byte_pos(byte_pos, extent);
        assert_eq!(insertion.byte_pos(), byte_pos, "{kind:?}");
        assert_eq!(insertion.char_pos(), CharPos0::new(1), "{kind:?}");
        assert_eq!(
            insertion.byte_end(),
            EmacsBytePos::new(start + "Ωx".len()),
            "{kind:?}"
        );
        assert_eq!(insertion.char_end(), CharPos0::new(3), "{kind:?}");
    }
}

#[test]
fn newline_scans_match_gap_backend_after_edits() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::implemented_variants() {
        let mut text =
            BufferText::from_str_with_backend_kind("α\nβγ\n🙂\nend", implemented_kind(kind));
        let mut gap = BufferText::from_str_with_backend_kind(
            "α\nβγ\n🙂\nend",
            ImplementedBufferTextBackendKind::GAP_BUFFER,
        );

        let insert_pos = emacs_byte_pos("α\nβ".len());
        insert_storage_string(&mut text, insert_pos, "x\ny");
        insert_storage_string(&mut gap, insert_pos, "x\ny");

        let delete_start = emacs_byte_pos("α\nβx\n".len());
        let delete_end = emacs_byte_pos("α\nβx\nyγ".len());
        delete_emacs_byte_range(&mut text, EmacsByteRange::new(delete_start, delete_end));
        delete_emacs_byte_range(&mut gap, EmacsByteRange::new(delete_start, delete_end));

        assert_eq!(text.to_string(), gap.to_string(), "{kind:?}");
        let len = text.emacs_byte_len().get();
        let ranges = [
            (0, len),
            (0, "α\n".len()),
            (1, len.saturating_sub(1)),
            ("α\n".len(), len),
            ("α\nβx\n".len(), len),
            (len, len),
        ];

        for (from, limit) in ranges {
            let from = EmacsBytePos::new(from);
            let limit = EmacsBytePos::new(limit);
            assert_eq!(
                text.next_newline_emacs_byte(from, limit),
                gap.next_newline_emacs_byte(from, limit),
                "{kind:?} next newline diverged for {}..{}",
                from.get(),
                limit.get()
            );
            assert_eq!(
                text.prev_newline_emacs_byte(limit, from),
                gap.prev_newline_emacs_byte(limit, from),
                "{kind:?} previous newline diverged for {}..{}",
                from.get(),
                limit.get()
            );
            assert_eq!(
                text.count_newlines_emacs_byte(from, limit),
                gap.count_newlines_emacs_byte(from, limit),
                "{kind:?} newline count diverged for {}..{}",
                from.get(),
                limit.get()
            );
        }
    }
}

#[test]
fn non_gap_position_conversion_forward_scan_crosses_backend_chunks() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::non_gap_implemented_variants() {
        let mut char_scan =
            BufferText::from_str_with_backend_kind("aé日本🙂zzzz", implemented_kind(kind));
        insert_storage_string(&mut char_scan, emacs_byte_pos(1), "Ω");

        assert_eq!(char_scan.to_string(), "aΩé日本🙂zzzz");
        assert_eq!(
            char_pos_to_byte_pos(&char_scan, 4),
            "aΩé日".len(),
            "{kind:?} char-to-byte forward scan should cross edited chunks"
        );

        let mut byte_scan =
            BufferText::from_str_with_backend_kind("aé日本🙂zzzz", implemented_kind(kind));
        insert_storage_string(&mut byte_scan, emacs_byte_pos(1), "Ω");
        assert_eq!(byte_scan.to_string(), "aΩé日本🙂zzzz");
        assert_eq!(
            byte_pos_to_char_pos(&byte_scan, "aΩé日".len()),
            4,
            "{kind:?} byte-to-char forward scan should cross edited chunks"
        );
    }
}

#[test]
fn non_gap_position_conversion_uses_backend_index_without_anchor_scan() {
    crate::test_utils::init_test_tracing();
    let mut s = String::new();
    for _ in 0..20_000 {
        s.push_str("日");
    }

    for kind in BufferTextBackendKind::non_gap_implemented_variants() {
        let text = BufferText::from_str_with_backend_kind(&s, implemented_kind(kind));
        assert_eq!(text.anchor_cache_len(), 0, "{kind:?}");

        let byte_pos = char_pos_to_byte_pos(&text, 10_000);
        assert_eq!(byte_pos, "日".len() * 10_000, "{kind:?}");
        assert_eq!(text.anchor_cache_len(), 0, "{kind:?}");

        let char_pos = byte_pos_to_char_pos(&text, byte_pos);
        assert_eq!(char_pos, 10_000, "{kind:?}");
        assert_eq!(text.anchor_cache_len(), 0, "{kind:?}");
    }
}

#[test]
fn non_gap_lisp_string_preserves_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::non_gap_implemented_variants() {
        let raw = crate::heap_types::LispString::from_unibyte(vec![0xFF, b'A', 0x80]);
        let text = BufferText::from_lisp_string_with_backend_kind(&raw, implemented_kind(kind));

        assert_eq!(text.backend_kind(), kind);
        assert!(!text.is_multibyte());
        assert_eq!(text.char_count(), CharLen::new(3));

        let mut bytes = Vec::new();
        text.copy_emacs_byte_range_to(full_emacs_byte_range(&text), &mut bytes);
        assert_eq!(bytes, vec![0xFF, b'A', 0x80]);
    }
}

#[test]
fn replace_lisp_string_preserves_non_gap_backend() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::non_gap_implemented_variants() {
        let text = BufferText::from_str_with_backend_kind("abc", implemented_kind(kind));
        let replacement = crate::heap_types::LispString::from_utf8("日本");

        text.replace_lisp_string(
            &replacement,
            crate::buffer::text_props::TextPropertyTable::new(),
        );

        assert_eq!(text.backend_kind(), kind);
        assert_eq!(text.to_string(), "日本");
        assert_eq!(
            text.backend_debug_layout().metrics(),
            TextMetrics::from_usize(2, 6)
        );
    }
}

#[test]
fn backend_conversion_preserves_text_side_data_after_edits() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");

    for source_kind in BufferTextBackendKind::implemented_variants() {
        let mut text =
            BufferText::from_str_with_backend_kind("aé\n🙂z", implemented_kind(source_kind));
        insert_storage_string(&mut text, emacs_byte_pos(1), "Ω");
        let prop_start = char_pos_to_byte_pos(&text, 1);
        let prop_end = char_pos_to_byte_pos(&text, 5);
        assert!(text.text_props_put_property_in_emacs_byte_range(
            emacs_byte_range(prop_start, prop_end),
            face,
            bold,
        ));
        text.set_save_modified_tick(77);

        let expected_text = text.to_string();
        let expected_metrics = text.metrics();
        let expected_modified_tick = text.modified_tick();
        let expected_chars_modified_tick = text.chars_modified_tick();
        let expected_save_modified_tick = text.save_modified_tick();
        let expected_gap_position = text.gap_position_lisp();
        let expected_gap_size = text.gap_size_lisp();
        let inside_prop = emacs_byte_pos(char_pos_to_byte_pos(&text, 3));
        let outside_prop = emacs_byte_pos(0);

        for target_kind in BufferTextBackendKind::implemented_variants() {
            text.try_convert_backend_kind(target_kind)
                .expect("backend should be implemented");

            assert_eq!(text.backend_kind(), target_kind);
            assert_eq!(
                text.to_string(),
                expected_text,
                "{source_kind:?}->{target_kind:?}"
            );
            assert_eq!(
                text.metrics(),
                expected_metrics,
                "{source_kind:?}->{target_kind:?}"
            );
            assert_eq!(
                text.modified_tick(),
                expected_modified_tick,
                "{source_kind:?}->{target_kind:?}"
            );
            assert_eq!(
                text.chars_modified_tick(),
                expected_chars_modified_tick,
                "{source_kind:?}->{target_kind:?}"
            );
            assert_eq!(
                text.save_modified_tick(),
                expected_save_modified_tick,
                "{source_kind:?}->{target_kind:?}"
            );
            assert_eq!(
                text.gap_position_lisp(),
                expected_gap_position,
                "{source_kind:?}->{target_kind:?}"
            );
            assert_eq!(
                text.gap_size_lisp(),
                expected_gap_size,
                "{source_kind:?}->{target_kind:?}"
            );
            assert_eq!(
                text.text_props_get_property_at_emacs_byte_pos(inside_prop, face),
                Some(bold),
                "{source_kind:?}->{target_kind:?}"
            );
            assert_eq!(
                text.text_props_get_property_at_emacs_byte_pos(outside_prop, face),
                None,
                "{source_kind:?}->{target_kind:?}"
            );
        }
    }
}

#[test]
fn same_size_replacement_invalidates_position_caches() {
    crate::test_utils::init_test_tracing();
    let left = "a".repeat(6001);
    let right = "b".repeat(6001);
    let original = format!("{left}€{right}");
    let replacement = format!("€{left}{right}");
    let target_char = 6001;

    for kind in BufferTextBackendKind::implemented_variants() {
        let mut text = BufferText::from_str_with_backend_kind(&original, implemented_kind(kind));

        assert_eq!(char_pos_to_byte_pos(&text, target_char), target_char);
        if kind.is_gap_buffer() {
            assert!(
                text.anchor_cache_len() > 0,
                "{kind:?} should cache the long char/byte walk before replacement"
            );
        } else {
            assert_eq!(
                text.anchor_cache_len(),
                0,
                "{kind:?} should use backend-indexed position lookup"
            );
        }

        text.replace_same_len_emacs_byte_range(
            emacs_byte_range(0, original.len()),
            replacement.as_bytes(),
        );

        assert_eq!(
            text.anchor_cache_len(),
            0,
            "{kind:?} should clear stale anchors after same-size replacement"
        );
        assert_eq!(char_pos_to_byte_pos(&text, target_char), target_char + 2);
    }
}

#[test]
fn from_lisp_string_preserves_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let raw = crate::heap_types::LispString::from_unibyte(vec![0xFF, b'A', 0x80]);
    let text = BufferText::from_lisp_string(&raw);

    assert!(!text.is_multibyte());
    assert_eq!(text.emacs_byte_len(), EmacsByteLen::new(3));
    assert_eq!(text.char_count(), CharLen::new(3));

    let mut bytes = Vec::new();
    text.copy_emacs_byte_range_to(full_emacs_byte_range(&text), &mut bytes);
    assert_eq!(bytes, vec![0xFF, b'A', 0x80]);
}

#[test]
fn char_count_tracks_multibyte_inserts_and_deletes() {
    crate::test_utils::init_test_tracing();
    let mut text = BufferText::from_str("ééz");
    assert_eq!(text.char_count(), CharLen::new(3));

    insert_storage_string(&mut text, emacs_byte_pos('é'.len_utf8()), "ß");
    assert_eq!(text.char_count(), CharLen::new(4));

    delete_emacs_byte_range(&mut text, emacs_byte_range(2, 4));
    assert_eq!(text.char_count(), CharLen::new(3));
    assert_eq!(text.to_string(), "ééz");
}

#[test]
fn shared_clone_observes_cached_char_count_updates() {
    crate::test_utils::init_test_tracing();
    let mut text = BufferText::from_str("ab");
    let shared = text.shared_clone();
    insert_storage_string(&mut text, emacs_byte_pos(2), "é");
    assert_eq!(text.char_count(), CharLen::new(3));
    assert_eq!(shared.char_count(), CharLen::new(3));
}

#[test]
fn deep_clone_keeps_independent_char_count_cache() {
    crate::test_utils::init_test_tracing();
    let mut text = BufferText::from_str("ab");
    let cloned = text.clone();
    insert_storage_string(&mut text, emacs_byte_pos(2), "é");
    assert_eq!(text.char_count(), CharLen::new(3));
    assert_eq!(cloned.char_count(), CharLen::new(2));
}

#[test]
fn layout_tracks_gnu_style_gap_and_end_positions() {
    crate::test_utils::init_test_tracing();
    let mut text = BufferText::from_str("éz");
    assert_eq!(text.metrics(), TextMetrics::from_usize(2, 3));
    let layout = text
        .gap_debug_layout()
        .expect("default backend is a gap buffer");
    assert_eq!(
        text.backend_debug_layout(),
        TextBackendDebugLayout::Gap(layout)
    );
    assert_eq!(layout.gpt.get(), 2);
    assert_eq!(layout.z.get(), 2);
    assert_eq!(layout.gpt_byte.get(), 3);
    assert_eq!(layout.z_byte.get(), 3);

    insert_storage_string(&mut text, emacs_byte_pos('é'.len_utf8()), "x");
    assert_eq!(text.metrics(), TextMetrics::from_usize(3, 4));
    let layout = text
        .gap_debug_layout()
        .expect("default backend is a gap buffer");
    assert_eq!(
        text.backend_debug_layout(),
        TextBackendDebugLayout::Gap(layout)
    );
    assert_eq!(layout.gpt.get(), 2);
    assert_eq!(layout.z.get(), 3);
    assert_eq!(layout.gpt_byte.get(), 3);
    assert_eq!(layout.z_byte.get(), 4);
    assert_eq!(text.to_string(), "éxz");
}

#[test]
fn emacs_byte_chunks_cross_gap_without_copying_to_single_slice() {
    crate::test_utils::init_test_tracing();
    let mut text = BufferText::from_str("abcdef");
    insert_storage_string(&mut text, emacs_byte_pos(3), "X");
    delete_emacs_byte_range(&mut text, emacs_byte_range(3, 4));

    let layout = text
        .gap_debug_layout()
        .expect("default backend is a gap buffer");
    assert_eq!(layout.gpt_byte.get(), 3);

    let mut chunks = Vec::new();
    text.for_each_emacs_byte_range_chunk(EmacsByteRange::from_usize(1, 5), |chunk| {
        chunks.push(chunk.to_vec());
        Ok::<(), ()>(())
    })
    .unwrap();
    assert_eq!(chunks, vec![b"bc".to_vec(), b"de".to_vec()]);
}

#[test]
fn range_contains_char_code_scans_non_contiguous_backend_chunks() {
    crate::test_utils::init_test_tracing();
    for kind in BufferTextBackendKind::non_gap_implemented_variants() {
        let mut text =
            BufferText::from_str_with_backend_kind(&"a".repeat(1100), implemented_kind(kind));
        let insert_at = text.char_pos_to_emacs_byte_pos(CharPos0::new(550));
        insert_storage_string(&mut text, insert_at, "é");

        let mut chunks = 0;
        text.for_each_emacs_byte_range_chunk(full_emacs_byte_range(&text), |_| {
            chunks += 1;
            Ok::<(), ()>(())
        })
        .unwrap();
        assert!(
            chunks > 1,
            "{kind:?} should expose a multi-chunk range for this test"
        );

        assert!(text.emacs_byte_range_contains_char_code(full_emacs_byte_range(&text), 'é' as u32));
        assert!(
            !text.emacs_byte_range_contains_char_code(full_emacs_byte_range(&text), '日' as u32)
        );
    }
}

#[test]
fn char_pos_to_emacs_byte_pos_matches_oracle() {
    let mut s = String::new();
    for i in 0..5000 {
        if i % 2 == 0 {
            s.push_str("hello ");
        } else {
            s.push_str("日本語 ");
        }
    }
    let text = BufferText::from_str(&s);

    // Oracle: contiguous bytes → char_to_byte_pos.
    let mut bytes = Vec::new();
    text.copy_emacs_byte_range_to(full_emacs_byte_range(&text), &mut bytes);

    for &cp in &[
        0usize,
        1,
        50,
        500,
        5000,
        12345,
        text.char_count().get() - 1,
        text.char_count().get(),
    ] {
        let got = char_pos_to_byte_pos(&text, cp);
        let expected = crate::emacs_core::emacs_char::char_to_byte_pos(&bytes, cp);
        assert_eq!(
            got, expected,
            "charpos {cp}: char_pos_to_emacs_byte_pos returned {got}, oracle said {expected}"
        );
    }
}

#[test]
fn char_pos_to_emacs_byte_pos_invalidates_on_mutation() {
    let mut text = BufferText::from_str("abc");
    let first = char_pos_to_byte_pos(&text, 2);
    assert_eq!(first, 2);

    // Insert "é" (2 bytes in UTF-8) at pos 0 — now charpos 2 sits at bytepos 3.
    insert_storage_string(&mut text, emacs_byte_pos(0), "é");
    let second = char_pos_to_byte_pos(&text, 2);
    assert_eq!(second, 3);
    assert_ne!(first, second, "cache returned stale bytepos after mutation");
}

#[test]
fn emacs_byte_pos_to_char_pos_matches_oracle() {
    let mut s = String::new();
    for i in 0..5000 {
        if i % 2 == 0 {
            s.push_str("hello ");
        } else {
            s.push_str("日本語 ");
        }
    }
    let text = BufferText::from_str(&s);

    let mut bytes = Vec::new();
    text.copy_emacs_byte_range_to(full_emacs_byte_range(&text), &mut bytes);

    let byte_len = text.emacs_byte_len().get();
    for &bp in &[0usize, 1, 50, 500, 5000, 12345, byte_len - 1, byte_len] {
        // Oracle valid only on char boundaries — snap bp down to one.
        let mut bp_snapped = bp;
        while bp_snapped > 0 && bp_snapped < bytes.len() && (bytes[bp_snapped] & 0xC0) == 0x80 {
            bp_snapped -= 1;
        }
        let got = byte_pos_to_char_pos(&text, bp_snapped);
        let expected = crate::emacs_core::emacs_char::byte_to_char_pos(&bytes, bp_snapped);
        assert_eq!(got, expected, "bytepos {bp_snapped}");
    }
}

#[test]
fn long_scan_populates_anchor_cache() {
    // 20 000+ multibyte chars, no existing markers.
    // Query at the midpoint so the walk from either BEG or Z is >5000.
    let mut s = String::new();
    for _ in 0..20_000 {
        s.push_str("日");
    }
    let text = BufferText::from_str(&s);

    assert_eq!(text.anchor_cache_len(), 0);

    // 10 000 chars into a 20 000-char buffer — scan from nearest bracket
    // must walk 10 000 positions (> POSITION_ANCHOR_STRIDE=5000).
    let _ = char_pos_to_byte_pos(&text, 10_000);

    assert!(
        text.anchor_cache_len() > 0,
        "expected auto-anchor to have been inserted after long scan (walked > 5000)"
    );
}

#[test]
fn set_multibyte_invalidates_position_caches() {
    let mut s = String::new();
    for _ in 0..20_000 {
        s.push_str("日");
    }
    let text = BufferText::from_str(&s);

    let _ = char_pos_to_byte_pos(&text, 10_000);
    assert!(text.anchor_cache_len() > 0);

    text.set_multibyte(false);

    assert_eq!(text.anchor_cache_len(), 0);
    assert!(!text.is_multibyte());
    assert_eq!(text.char_count().get(), text.emacs_byte_len().get());
}

#[test]
fn replace_lisp_string_invalidates_position_cache() {
    crate::test_utils::init_test_tracing();
    // Build a buffer with a known multibyte char at charpos 2.
    let text = BufferText::from_str("日日日"); // 3 chars, 9 bytes
    let cached_before = char_pos_to_byte_pos(&text, 2);
    assert_eq!(cached_before, 6);

    // Replace with different same-char-and-byte-count content.
    let lisp_string = crate::heap_types::LispString::from_utf8("本本本");
    text.replace_lisp_string(
        &lisp_string,
        crate::buffer::text_props::TextPropertyTable::new(),
    );

    // Same-count replacement would leave a stale pos_cache; verify it was
    // cleared by confirming the conversion is recomputed correctly. (The
    // byte position of charpos 2 must match the new content's layout.)
    let after = char_pos_to_byte_pos(&text, 2);
    assert_eq!(after, 6, "charpos 2 in '本本本' is at bytepos 6");

    // Sanity: the actual bytes at that position are the lead byte of '本'.
    // '本' is 0xE6 0x9C 0xAC. So buffer[6] should be 0xE6.
    let b = text.byte_at_emacs_byte_pos(emacs_byte_pos(6));
    assert_eq!(
        b, 0xE6,
        "post-replace byte at position 6 should be 0xE6 (lead byte of 本)"
    );
}

#[test]
fn replace_lisp_string_handles_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let text = BufferText::from_str("ééz");
    let cached_before = char_pos_to_byte_pos(&text, 2);
    assert_eq!(cached_before, 4);

    let raw = crate::heap_types::LispString::from_unibyte(vec![0xFF, b'A', 0x80]);
    text.replace_lisp_string(&raw, crate::buffer::text_props::TextPropertyTable::new());

    assert!(!text.is_multibyte());
    assert_eq!(text.char_count(), CharLen::new(3));
    assert_eq!(char_pos_to_byte_pos(&text, 2), 2);
    assert_eq!(text.byte_at_emacs_byte_pos(emacs_byte_pos(0)), 0xFF);
    assert_eq!(text.byte_at_emacs_byte_pos(emacs_byte_pos(1)), b'A');
    assert_eq!(text.byte_at_emacs_byte_pos(emacs_byte_pos(2)), 0x80);

    let mut bytes = Vec::new();
    text.copy_emacs_byte_range_to(full_emacs_byte_range(&text), &mut bytes);
    assert_eq!(bytes, vec![0xFF, b'A', 0x80]);
}

#[test]
fn unchanged_region_accumulator_composes_edits_and_resets() {
    let text = BufferText::new();
    // No edits since the last ack → no dirty region.
    assert_eq!(text.changed_char_range(100), None);

    // A single edit at [5, 10) in a 100-char buffer dirties exactly that span.
    text.note_changed_char_region(5, 10, 100);
    assert_eq!(text.changed_char_range(100), Some((5, 10)));

    // A second, disjoint edit at [50, 60) unions into [5, 60) (the prefix and
    // suffix unchanged-lengths each shrink to their minimum).
    text.note_changed_char_region(50, 60, 100);
    assert_eq!(text.changed_char_range(100), Some((5, 60)));

    // The suffix length is measured from the END, so an insertion that grew the
    // buffer (current_z larger) keeps the unchanged tail aligned.
    text.note_changed_char_region(50, 50, 100); // 4-char insert at 50
    assert_eq!(text.changed_char_range(104), Some((5, 64)));

    // Ack resets to fully-unchanged.
    text.reset_unchanged_region();
    assert_eq!(text.changed_char_range(104), None);
}

#[test]
fn unchanged_region_accumulator_edit_at_buffer_ends() {
    let text = BufferText::new();
    // Insert at the very beginning (start == end == 0): dirty starts at 0.
    text.note_changed_char_region(0, 0, 100);
    // After inserting 3 chars, current_z = 103, the unchanged suffix is all 100
    // original chars, so the dirty span is exactly the 3 inserted chars.
    assert_eq!(text.changed_char_range(103), Some((0, 3)));

    let text = BufferText::new();
    // Delete the last 5 chars [95, 100): dirty extends to the (new) end.
    text.note_changed_char_region(95, 100, 100);
    assert_eq!(text.changed_char_range(95), Some((95, 95)));
}

/// The char<->byte scan anchors survive content edits like markers do
/// (GNU keeps `buf_charpos_to_bytepos` cheap the same way): anchors before
/// an edit keep their coordinates, anchors after it shift, anchors inside a
/// deleted span are forgotten — and every conversion stays exact.
#[test]
fn position_anchors_survive_edits_and_stay_exact() {
    // Wide multibyte text so char != byte everywhere and anchors are worth
    // remembering (each conversion far from an anchor records one).
    let unit = "é".repeat(64) + "abcd" + &"日本".repeat(16) + "\n";
    let body = unit.repeat(400);
    let mut text = BufferText::from_str(&body);
    let total_chars = body.chars().count();
    // Populate anchors with conversions spread across the buffer (each walk
    // is far longer than POSITION_ANCHOR_STRIDE, so every one records one).
    for k in 1..20 {
        let _ = char_pos_to_byte_pos(&text, k * total_chars / 20);
    }
    let anchors_before = text.scan_anchor_ring_len_for_test();
    assert!(
        anchors_before > 0,
        "conversions far from an anchor must record anchors"
    );

    // Insert in the middle (a multibyte char), then delete a span; after each
    // edit the ring must still hold anchors and every conversion must match a
    // from-scratch buffer holding the same content.
    let mid_char = total_chars / 2;
    let mid_byte = char_pos_to_byte_pos(&text, mid_char);
    insert_storage_string(&mut text, EmacsBytePos::new(mid_byte), "ü");
    let mut expected: String = body.chars().take(mid_char).collect();
    expected.push('ü');
    expected.extend(body.chars().skip(mid_char));
    assert!(
        text.scan_anchor_ring_len_for_test() > 0,
        "an insert must not drop the anchors"
    );
    let fresh = BufferText::from_str(&expected);
    let n = expected.chars().count();
    for k in 0..=80 {
        let c = k * n / 80;
        assert_eq!(
            char_pos_to_byte_pos(&text, c),
            char_pos_to_byte_pos(&fresh, c),
            "char->byte after insert at char {c}"
        );
        let b = char_pos_to_byte_pos(&fresh, c);
        assert_eq!(
            byte_pos_to_char_pos(&text, b),
            c,
            "byte->char after insert at byte {b}"
        );
    }

    // Delete 100 chars starting 3 chars after the insertion point.
    let del_start_char = mid_char + 3;
    let del_start = char_pos_to_byte_pos(&text, del_start_char);
    let del_end = char_pos_to_byte_pos(&text, del_start_char + 100);
    delete_emacs_byte_range(
        &mut text,
        EmacsByteRange::new(EmacsBytePos::new(del_start), EmacsBytePos::new(del_end)),
    );
    let expected2: String = expected
        .chars()
        .take(del_start_char)
        .chain(expected.chars().skip(del_start_char + 100))
        .collect();
    assert!(
        text.scan_anchor_ring_len_for_test() > 0,
        "a delete must not drop the anchors"
    );
    let fresh2 = BufferText::from_str(&expected2);
    let n2 = expected2.chars().count();
    for k in 0..=80 {
        let c = k * n2 / 80;
        assert_eq!(
            char_pos_to_byte_pos(&text, c),
            char_pos_to_byte_pos(&fresh2, c),
            "char->byte after delete at char {c}"
        );
        let b = char_pos_to_byte_pos(&fresh2, c);
        assert_eq!(
            byte_pos_to_char_pos(&text, b),
            c,
            "byte->char after delete at byte {b}"
        );
    }
}
