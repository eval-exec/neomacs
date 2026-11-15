//! `compute-motion` scans its span of the buffer in place.

use super::super::*;
use crate::buffer::{BufferId, LispCharPos1};
use neovm_value::heap_types::LispString;

fn buffer_with(text: &str) -> Buffer {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new_standalone(BufferId(1), Value::string("compute-motion"));
    buf.insert(text);
    buf.widen();
    buf
}

fn origin(pos: EmacsBytePos) -> MotionScan {
    MotionScan {
        pos,
        hpos: 0,
        vpos: 0,
        prev_hpos: 0,
        contin: false,
    }
}

fn limits(width: i64) -> MotionLimits {
    MotionLimits {
        to_hpos: i64::MAX,
        to_vpos: i64::MAX,
        width,
        tab_width: 8,
    }
}

fn scan_all(buf: &Buffer, limits: MotionLimits) -> MotionScan {
    let span = buf.accessible_emacs_byte_range();
    scan_compute_motion(buf, span, origin(span.start()), limits)
}

fn at(pos: usize, hpos: i64, vpos: i64, prev_hpos: i64, contin: bool) -> MotionScan {
    MotionScan {
        pos: EmacsBytePos::new(pos),
        hpos,
        vpos,
        prev_hpos,
        contin,
    }
}

#[test]
fn compute_motion_copies_no_buffer_text_for_a_short_span() {
    let buf = buffer_with(&"a".repeat(1 << 20));
    let from = buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(500_000));
    let to = buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(500_010));
    let copied = crate::buffer::buffer_text::text_bytes_copied();
    let scan = scan_compute_motion(
        &buf,
        EmacsByteRange::new(from, to),
        origin(from),
        limits(80),
    );
    let copied = crate::buffer::buffer_text::text_bytes_copied() - copied;
    assert_eq!(scan, at(from.get() + 10, 10, 0, 9, false));
    assert!(
        copied < 4096,
        "a 10-character span copied {copied} bytes out of a 1 MB buffer"
    );
}

#[test]
fn compute_motion_counts_tabs_newlines_and_wide_characters() {
    let buf = buffer_with("ab\tc\nxyz");
    assert_eq!(scan_all(&buf, limits(80)), at(8, 3, 1, 2, false));

    // é is one column, 漢 two.
    let buf = buffer_with("é漢x");
    assert_eq!(scan_all(&buf, limits(80)), at(6, 4, 0, 3, false));
}

#[test]
fn compute_motion_continues_lines_at_width() {
    let buf = buffer_with("abcdef");
    assert_eq!(scan_all(&buf, limits(4)), at(6, 2, 1, 1, true));
}

#[test]
fn compute_motion_stops_at_topos() {
    let buf = buffer_with("abcdef");
    let stop = MotionLimits {
        to_hpos: 3,
        to_vpos: 0,
        ..limits(80)
    };
    assert_eq!(scan_all(&buf, stop), at(3, 3, 0, 2, false));
}

#[test]
fn compute_motion_scans_across_the_gap() {
    let mut buf = buffer_with("漢字");
    buf.goto_emacs_byte_pos(EmacsBytePos::new(3));
    buf.insert("ab");
    assert_eq!(scan_all(&buf, limits(80)), at(8, 6, 0, 4, false));
}

#[test]
fn compute_motion_counts_a_raw_byte_by_its_own_length() {
    // An eight-bit raw byte (#x3FFF80, bytes C0 80) before the span.
    let mut buf = buffer_with("");
    buf.insert_lisp_string(&LispString::from_emacs_bytes(b"\xC0\x80abc\tdef".to_vec()));
    let span = EmacsByteRange::new(EmacsBytePos::new(2), EmacsBytePos::new(9));
    assert_eq!(
        scan_compute_motion(&buf, span, origin(span.start()), limits(80)),
        at(9, 11, 0, 10, false)
    );
}

#[test]
fn compute_motion_reads_unibyte_text_byte_by_byte() {
    // The byte 0xE9 is one column and one byte.
    let mut buf = buffer_with("");
    buf.set_multibyte_value(false);
    buf.insert_lisp_string(&LispString::from_unibyte(b"a\t\xE9b".to_vec()));
    assert_eq!(scan_all(&buf, limits(80)), at(4, 10, 0, 9, false));
}
