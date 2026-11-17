//! The region's character count is END - START, so turning a buffer slice
//! into a string must not count the copied bytes again.  The counter,
//! `BUFFER_SLICE_RECOUNT_BYTES`, observes the two routes that did the
//! recount (`lisp_string_from_buffer_bytes` and
//! `Buffer::buffer_substring_lisp_string_no_properties_range`); it does not
//! see a recount reached by another helper.  In this debug build
//! `from_emacs_bytes_with_chars` still checks the supplied count with a
//! `debug_assert`, which release builds skip.

use std::sync::atomic::Ordering;

use super::super::{
    checked_buffer_substring_for_char_region_in_manager, current_buffer_substring_text,
};

use crate::buffer::buffer::BUFFER_SLICE_RECOUNT_BYTES;
use crate::buffer::{BufferId, BufferManager, EmacsByteRange, LispCharPos1};

use neovm_value::heap_types::LispString;
use neovm_value::value::{Value, get_string_text_properties_table_for_value};

/// About 1 MB of 70-char lines with a two-byte `é` every ten characters,
/// the shape `insert-buffer-substring` copies between buffers.
fn multibyte_source_text() -> String {
    let line: String = (0..70)
        .map(|i| match i {
            69 => '\n',
            i if i % 10 == 5 => 'é',
            i if i % 7 == 3 => ' ',
            i => char::from(b'a' + (i % 26) as u8),
        })
        .collect();
    line.repeat(1_000_000 / line.len() + 1)
}

fn source_buffer(mgr: &mut BufferManager) -> BufferId {
    let id = mgr.create_buffer(" source");
    let buf = mgr.get_mut(id).expect("source buffer");
    buf.insert(&multibyte_source_text());
    let face_range = EmacsByteRange::new(
        buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(100)),
        buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(5_000)),
    );
    assert!(buf.text_props_put_property_in_emacs_byte_range(
        face_range,
        Value::symbol("face"),
        Value::symbol("bold"),
    ));
    id
}

/// The string the slice path built before it took the caller's character
/// count: the copied bytes counted again, plus the sliced properties.
fn recounted_reference(mgr: &BufferManager, id: BufferId, start: i64, end: i64) -> Value {
    let buf = mgr.get(id).expect("source buffer");
    let (lo, hi) = (start.min(end), start.max(end));
    let range = EmacsByteRange::new(
        buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(lo)),
        buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(hi)),
    );
    let value = Value::heap_string(LispString::from_emacs_bytes(
        buf.buffer_substring_bytes_range(range),
    ));
    let props = buf.text_props_slice_emacs_byte_range(range);
    if !props.is_empty() {
        neovm_value::value::set_string_text_properties_table_for_value(value, props);
    }
    value
}

fn assert_same_string(actual: Value, expected: Value) {
    let actual_string = actual.as_lisp_string().expect("string result");
    let expected_string = expected.as_lisp_string().expect("string reference");
    assert_eq!(actual_string.as_bytes(), expected_string.as_bytes());
    assert_eq!(actual_string.schars(), expected_string.schars());
    assert_eq!(actual_string.sbytes(), expected_string.sbytes());
    assert_eq!(actual_string.is_multibyte(), expected_string.is_multibyte());
    let runs = |value: Value| {
        get_string_text_properties_table_for_value(value)
            .map(|table| table.interval_plist_runs_for_test())
            .unwrap_or_default()
    };
    assert_eq!(runs(actual), runs(expected));
}

fn substring_counting_recount(
    mgr: &BufferManager,
    id: BufferId,
    start: i64,
    end: i64,
) -> (Value, usize) {
    BUFFER_SLICE_RECOUNT_BYTES.store(0, Ordering::Relaxed);
    let value = checked_buffer_substring_for_char_region_in_manager(
        mgr,
        Some(id),
        LispCharPos1::new(start),
        LispCharPos1::new(end),
        Value::fixnum(start),
        Value::fixnum(end),
    )
    .expect("region inside the accessible portion");
    (value, BUFFER_SLICE_RECOUNT_BYTES.load(Ordering::Relaxed))
}

#[test]
fn insert_buffer_substring_copies_a_multibyte_region_without_recounting_characters() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let id = source_buffer(&mut mgr);
    let (point_min, point_max) = {
        let buf = mgr.get(id).expect("source buffer");
        (
            buf.point_min_lisp_char_pos().as_i64(),
            buf.point_max_lisp_char_pos().as_i64(),
        )
    };
    let whole_bytes = mgr.get(id).expect("source buffer").full_emacs_byte_range();
    assert!(whole_bytes.end().get() >= 1_000_000);

    let (whole, recounted) = substring_counting_recount(&mgr, id, point_min, point_max);
    assert_eq!(
        recounted, 0,
        "the region's character count is END - START; the copied bytes must not be counted again"
    );
    assert_same_string(whole, recounted_reference(&mgr, id, point_min, point_max));

    // Both argument orders, and a region that starts and ends inside the
    // property run.
    for (start, end) in [(4_000, 90_000), (90_000, 4_000), (200, 300), (7, 7)] {
        let (part, recounted) = substring_counting_recount(&mgr, id, start, end);
        assert_eq!(recounted, 0, "region {start}..{end}");
        assert_same_string(part, recounted_reference(&mgr, id, start, end));
    }

    // The copied string lands in the destination buffer with its text and
    // properties, as `insert-buffer-substring' leaves them.
    let dest = mgr.create_buffer(" dest");
    let text = whole.as_lisp_string().expect("string result");
    mgr.get_mut(dest)
        .expect("destination buffer")
        .insert_lisp_string(text);
    let dest_buf = mgr.get(dest).expect("destination buffer");
    let source_buf = mgr.get(id).expect("source buffer");
    assert_eq!(
        dest_buf.buffer_substring_bytes_range(dest_buf.full_emacs_byte_range()),
        source_buf.buffer_substring_bytes_range(whole_bytes)
    );
    assert_eq!(
        dest_buf.point_max_lisp_char_pos().as_i64(),
        point_max - point_min + 1
    );
    assert_eq!(
        dest_buf
            .text_props_slice_emacs_byte_range(dest_buf.full_emacs_byte_range())
            .interval_plist_runs_for_test(),
        source_buf
            .text_props_slice_emacs_byte_range(whole_bytes)
            .interval_plist_runs_for_test()
    );
}

#[test]
fn buffer_substring_of_a_narrowed_multibyte_buffer_does_not_recount_characters() {
    crate::test_utils::init_test_tracing();
    let mut mgr = BufferManager::new();
    let id = source_buffer(&mut mgr);
    mgr.set_current(id);
    let (narrow_start, narrow_end) = (3_001_i64, 800_001_i64);
    {
        let buf = mgr.get_mut(id).expect("source buffer");
        let range = EmacsByteRange::new(
            buf.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(narrow_start)),
            buf.lisp_pos_to_full_buffer_emacs_byte_pos(LispCharPos1::new(narrow_end)),
        );
        buf.narrow_to_emacs_byte_range(range);
        assert_eq!(buf.point_min_lisp_char_pos().as_i64(), narrow_start);
        assert_eq!(buf.point_max_lisp_char_pos().as_i64(), narrow_end);
    }

    // 40-character `buffer-substring' calls (one starting on a two-byte
    // character, one ending at point-max), the whole accessible region,
    // and an empty one; each also with its arguments reversed.
    let regions = [
        (narrow_start, narrow_start + 40),
        (4_326, 4_366),
        (narrow_end - 40, narrow_end),
        (narrow_start, narrow_end),
        (9_000, 9_000),
    ];
    for (lo, hi) in regions {
        for (start, end) in [(lo, hi), (hi, lo)] {
            BUFFER_SLICE_RECOUNT_BYTES.store(0, Ordering::Relaxed);
            let (from, to, text) =
                current_buffer_substring_text(&mgr, [Value::fixnum(start), Value::fixnum(end)])
                    .expect("region inside the accessible portion");
            assert_eq!(
                BUFFER_SLICE_RECOUNT_BYTES.load(Ordering::Relaxed),
                0,
                "region {start}..{end}"
            );
            assert_eq!((from.as_i64(), to.as_i64()), (lo, hi), "ordered positions");
            let buf = mgr.get(id).expect("source buffer");
            let range = EmacsByteRange::new(
                buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(lo)),
                buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(hi)),
            );
            let recounted = buf.buffer_substring_lisp_string_no_properties_range(range);
            assert_eq!(
                text.as_bytes(),
                recounted.as_bytes(),
                "region {start}..{end}"
            );
            assert_eq!(text.schars(), recounted.schars(), "region {start}..{end}");
            assert_eq!(text.schars(), usize::try_from(hi - lo).expect("ordered"));
            assert_eq!(text.sbytes(), recounted.sbytes());
            assert_eq!(text.is_multibyte(), recounted.is_multibyte());
            assert!(
                !text.has_intervals(),
                "properties are added after fontification"
            );
        }
    }

    // Positions outside the narrowing still signal, in either order.
    for (start, end) in [
        (narrow_start - 1, narrow_start + 10),
        (narrow_end + 1, narrow_end - 10),
    ] {
        assert!(
            current_buffer_substring_text(&mgr, [Value::fixnum(start), Value::fixnum(end)])
                .is_err(),
            "region {start}..{end} lies outside the accessible portion"
        );
    }
}
