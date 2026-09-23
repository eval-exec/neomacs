use super::*;

fn byte_range(start: usize, end: usize) -> EmacsByteRange {
    assert!(start <= end);
    EmacsByteRange::from_start_len(EmacsBytePos::new(start), EmacsByteLen::new(end - start))
}

fn char_range(start: usize, end: usize) -> CharRange {
    assert!(start <= end);
    CharRange::from_start_len(CharPos0::new(start), CharLen::new(end - start))
}

#[test]
fn text_extent_measures_emacs_bytes_for_buffer_mode() {
    let multibyte = TextExtent::from_emacs_bytes("aé🙂".as_bytes(), true);
    assert_eq!(multibyte.chars().get(), 3);
    assert_eq!(multibyte.emacs_bytes().get(), "aé🙂".len());

    let unibyte = TextExtent::from_emacs_bytes(&[0xFF, b'a', 0x80], false);
    assert_eq!(unibyte.chars().get(), 3);
    assert_eq!(unibyte.emacs_bytes().get(), 3);
}

#[test]
fn text_insertion_reports_typed_end_positions() {
    let insertion = TextInsertion::new(
        EmacsBytePos::new(20),
        CharPos0::new(10),
        TextExtent::from_usize(3, 7),
    );

    assert_eq!(insertion.byte_end(), EmacsBytePos::new(27));
    assert_eq!(insertion.char_end(), CharPos0::new(13));
}

#[test]
fn text_edit_range_can_be_built_from_start_and_extent() {
    let range = TextEditRange::from_start_extent(
        EmacsBytePos::new(20),
        CharPos0::new(10),
        TextExtent::from_usize(3, 7),
    );

    assert_eq!(range.byte_range(), byte_range(20, 27));
    assert_eq!(range.char_range(), char_range(10, 13));
}

#[test]
fn text_edit_range_reports_relative_byte_index_range() {
    let base = TextEditRange::from_usize(20, 40, 10, 30);
    let inner = TextEditRange::from_usize(25, 32, 15, 22);

    assert_eq!(inner.byte_index_range_relative_to(base), 5..12);
}

#[test]
fn text_edit_range_builds_typed_subrange_from_offsets() {
    let base = TextEditRange::from_usize(20, 40, 10, 30);
    let subrange = base.subrange_from_start_offsets(
        EmacsByteLen::new(5),
        CharLen::new(3),
        TextExtent::from_usize(2, 7),
    );

    assert_eq!(subrange, TextEditRange::from_usize(25, 32, 13, 15));
}

#[test]
fn text_change_can_keep_distinct_before_and_after_ranges() {
    let before_range = TextEditRange::from_usize(10, 20, 5, 15);
    let after_range = TextEditRange::from_usize(10, 14, 5, 9);
    let change = TextChange::unchanged_extent_with_after_range(before_range, after_range);

    assert_eq!(change.before_start_byte(), EmacsBytePos::new(10));
    assert_eq!(change.before_end_byte(), EmacsBytePos::new(20));
    assert_eq!(change.after_start_byte(), EmacsBytePos::new(10));
    assert_eq!(change.after_end_byte(), EmacsBytePos::new(14));
    assert_eq!(change.old_char_len(), CharLen::new(4));
}

#[test]
fn text_transposition_keeps_byte_and_char_ranges_together() {
    let transposition = TextTransposition::from_usize(0, 3, 0, 2, 8, 13, 6, 9);

    assert_eq!(transposition.first().byte_range(), byte_range(0, 3));
    assert_eq!(transposition.first().char_range(), char_range(0, 2));
    assert_eq!(transposition.second().byte_range(), byte_range(8, 13));
    assert_eq!(transposition.second().char_range(), char_range(6, 9));
    assert_eq!(transposition.byte_span(), byte_range(0, 13));
    assert_eq!(transposition.char_span(), char_range(0, 9));
    assert_eq!(
        transposition.span_edit_range(),
        TextEditRange::from_usize(0, 13, 0, 9)
    );
    assert_eq!(transposition.middle_byte_range(), byte_range(3, 8));
    assert_eq!(transposition.changed_chars(), CharLen::new(9));
}

#[test]
fn text_transposition_moves_byte_and_char_positions_separately() {
    let transposition = TextTransposition::from_usize(0, 4, 0, 2, 9, 12, 7, 9);

    assert_eq!(
        transposition.transpose_byte_pos(EmacsBytePos::new(1)),
        EmacsBytePos::new(9)
    );
    assert_eq!(
        transposition.transpose_byte_pos(EmacsBytePos::new(6)),
        EmacsBytePos::new(5)
    );
    assert_eq!(
        transposition.transpose_byte_pos(EmacsBytePos::new(10)),
        EmacsBytePos::new(1)
    );
    assert_eq!(
        transposition.transpose_char_pos(CharPos0::new(1)),
        CharPos0::new(8)
    );
    assert_eq!(
        transposition.transpose_char_pos(CharPos0::new(5)),
        CharPos0::new(5)
    );
    assert_eq!(
        transposition.transpose_char_pos(CharPos0::new(8)),
        CharPos0::new(1)
    );
    assert_eq!(
        transposition.transpose_anchor(TextPositionAnchor::from_usize(8, 10)),
        TextPositionAnchor::from_usize(1, 1)
    );
}

#[test]
fn empty_text_edit_range_keeps_coordinate_spaces_together() {
    let range = TextEditRange::empty_at(EmacsBytePos::new(20), CharPos0::new(10));

    assert_eq!(range.byte_range(), byte_range(20, 20));
    assert_eq!(range.char_range(), char_range(10, 10));
    assert!(range.is_empty());
}

#[test]
fn text_replacement_keeps_old_and_new_extents_together() {
    let replacement = TextReplacement::new(
        TextEditRange::from_usize(20, 36, 10, 18),
        TextExtent::from_usize(3, 5),
    );

    assert_eq!(replacement.byte_start().get(), 20);
    assert_eq!(replacement.old_byte_len().get(), 16);
    assert_eq!(replacement.old_char_len().get(), 8);
    assert_eq!(replacement.new_byte_len().get(), 5);
    assert_eq!(replacement.new_char_len().get(), 3);
    assert_eq!(replacement.changed_chars_usize(), 8);
}

#[test]
fn text_replacement_changed_chars_uses_inserted_extent_when_larger() {
    let replacement = TextReplacement::new(
        TextEditRange::from_usize(20, 24, 10, 12),
        TextExtent::from_usize(5, 9),
    );

    assert_eq!(replacement.changed_chars_usize(), 5);
}

#[test]
fn text_change_reports_before_and_after_hook_ranges() {
    let replacement = TextReplacement::new(
        TextEditRange::from_usize(20, 36, 10, 18),
        TextExtent::from_usize(3, 5),
    );
    let change = TextChange::replacement(replacement);

    assert_eq!(change.before_byte_range(), byte_range(20, 36));
    assert_eq!(change.after_start_byte(), EmacsBytePos::new(20));
    assert_eq!(change.after_end_byte(), EmacsBytePos::new(25));
    assert_eq!(change.old_char_len(), CharLen::new(8));
}

#[test]
fn text_change_unchanged_extent_keeps_after_range_equal() {
    let range = TextEditRange::from_usize(20, 36, 10, 18);
    let change = TextChange::unchanged_extent(range);

    assert_eq!(change.before_byte_range(), byte_range(20, 36));
    assert_eq!(change.after_start_byte(), EmacsBytePos::new(20));
    assert_eq!(change.after_end_byte(), EmacsBytePos::new(36));
    assert_eq!(change.old_char_len(), CharLen::new(8));
}
