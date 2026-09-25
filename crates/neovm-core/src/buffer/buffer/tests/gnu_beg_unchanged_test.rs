//! GNU `BEG_UNCHANGED` as the mode-line gate reads it (P3.5 E1): property
//! changes count one char early (`modify_text_properties` runs
//! `BUF_COMPUTE_UNCHANGED (buf, b - 1, e)`, textprop.c:87), character
//! changes do not, and an empty property range records nothing.

use super::*;

fn bytes(buf: &Buffer, start: usize, end: usize) -> crate::buffer::EmacsByteRange {
    crate::buffer::EmacsByteRange::new(
        crate::buffer::EmacsBytePos::new(byte_pos_for_char(buf, start)),
        crate::buffer::EmacsBytePos::new(byte_pos_for_char(buf, end)),
    )
}

#[test]
fn nothing_changed_has_no_gnu_prefix() {
    let buf = buf_with_text("aaaa\nbbbb\n");
    buf.reset_unchanged_region();
    assert_eq!(buf.gnu_beg_unchanged(), None);
}

#[test]
fn a_property_change_counts_from_one_char_before_its_start() {
    let buf = buf_with_text("aaaa\nbbbb\n");
    buf.reset_unchanged_region();
    buf.note_changed_property_region(bytes(&buf, 5, 10));
    assert_eq!(buf.changed_char_range(), Some((5, 10)), "the layout span");
    assert_eq!(buf.gnu_beg_unchanged(), Some(4), "GNU's shifted prefix");
    buf.reset_unchanged_region();
    assert_eq!(buf.gnu_beg_unchanged(), None, "the ack resets both");
}

#[test]
fn a_character_change_is_not_shifted() {
    let mut buf = buf_with_text("aaaa\nbbbb\n");
    buf.reset_unchanged_region();
    buf.goto_emacs_byte_pos(crate::buffer::EmacsBytePos::new(7));
    buf.insert("x");
    assert_eq!(buf.gnu_beg_unchanged(), Some(7));
    // A later property change further right does not move the prefix; one
    // starting at the insertion does, by one.
    buf.note_changed_property_region(bytes(&buf, 9, 10));
    assert_eq!(buf.gnu_beg_unchanged(), Some(7));
    buf.note_changed_property_region(bytes(&buf, 7, 8));
    assert_eq!(buf.gnu_beg_unchanged(), Some(6));
}

#[test]
fn an_empty_property_range_is_not_a_gnu_change() {
    let buf = buf_with_text("aaaa\nbbbb\n");
    buf.reset_unchanged_region();
    buf.note_changed_property_region(bytes(&buf, 10, 10));
    assert_eq!(buf.gnu_beg_unchanged(), Some(10));
}
