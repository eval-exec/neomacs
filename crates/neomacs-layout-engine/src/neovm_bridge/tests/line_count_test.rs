//! Backend/range checks for the scalar and memchr line-count arms.
use super::*;
use neovm_core::buffer::{BufferId, BufferTextBackendKind};

fn range(start: usize, end: usize) -> EmacsByteRange {
    EmacsByteRange::new(EmacsBytePos::new(start), EmacsBytePos::new(end))
}

fn buffer(kind: BufferTextBackendKind, text: &str) -> Buffer {
    let mut buffer = Buffer::try_new_standalone_with_text_backend_kind(
        BufferId(400 + u64::from(u8::from(kind))),
        Value::string("*line-count-arms*"),
        kind,
    )
    .expect("implemented text backend");
    buffer.insert(text);
    buffer.widen();
    buffer
}

fn check_counts<B: LayoutBufferView>(access: &RustBufferAccess<'_, B>, cases: &[(i64, i64, i64)]) {
    for &(start, end, expected) in cases {
        for mode in [
            LayoutLineCountMode::Off,
            LayoutLineCountMode::On,
            LayoutLineCountMode::Verify,
        ] {
            let count = clamped_layout_emacs_byte_range(access.view(), start, end)
                .map_or(0, |range| access.count_line_chunks(range, mode));
            assert_eq!(count, expected, "{mode:?} [{start},{end})");
        }
        // The public entry still applies the existing optional index before
        // scanning, regardless of which numeric process mode was selected.
        assert_eq!(access.count_lines(start, end), expected, "[{start},{end})");
    }
}

#[test]
fn scalar_and_memchr_counts_match_across_fragmented_non_ascii_text() {
    let _runtime = Context::new();
    let text = "pré\n日本\nomega\rhidden\nfin\n";
    let gap = text.find("omega").unwrap();
    let newline = text.find('\n').unwrap();
    let fin = text.find("fin").unwrap();
    for kind in BufferTextBackendKind::implemented_variants() {
        let mut buffer = buffer(kind, text);
        buffer.goto_emacs_byte_pos(EmacsBytePos::new(gap));
        buffer.insert("temporary");
        buffer.delete_emacs_byte_range(range(gap, gap + "temporary".len()));
        assert_eq!(buffer.buffer_string(), text);
        let snapshot = LayoutBufferSnapshot::from_buffer(&buffer);
        if kind == BufferTextBackendKind::GapBuffer {
            let mut chunks = 0;
            snapshot
                .layout_try_for_each_emacs_byte_range_chunk(range(0, text.len()), |_| {
                    chunks += 1;
                    Ok::<(), std::convert::Infallible>(())
                })
                .unwrap();
            assert!(chunks >= 2, "the range must cross the relocated gap");
        }
        check_counts(
            &RustBufferAccess::new(&snapshot),
            &[
                (0, text.len() as i64, 4),
                (0, newline as i64, 0),
                (0, newline as i64 + 1, 1),
                (newline as i64, newline as i64 + 1, 1),
                (gap as i64 - 1, text.len() as i64, 3),
                (gap as i64, fin as i64, 1),
            ],
        );
        // The frozen reader must keep its original counts after live edits.
        buffer.goto_emacs_byte_pos(EmacsBytePos::new(gap));
        buffer.insert("\n\n");
        check_counts(&RustBufferAccess::new(&snapshot), &[(0, i64::MAX, 4)]);
        check_counts(&RustBufferAccess::new(&buffer), &[(0, i64::MAX, 6)]);
    }
}

#[test]
fn line_count_preserves_narrowed_origins_and_explicit_widened_ranges() {
    let _runtime = Context::new();
    let text = "outside\né\n日本\nend\nhidden\n";
    let start = text.find('é').unwrap();
    let end = text.find("hidden").unwrap();
    for kind in BufferTextBackendKind::implemented_variants() {
        let mut buffer = buffer(kind, text);
        buffer.narrow_to_emacs_byte_range(range(start, end));
        let snapshot = LayoutBufferSnapshot::from_buffer(&buffer);
        let access = RustBufferAccess::new(&snapshot);
        assert_eq!(access.begv(), start as i64);
        assert_eq!(access.zv(), end as i64);
        check_counts(
            &access,
            &[
                (access.begv(), access.zv(), 3),
                (0, access.zv(), 4),
                (0, i64::MAX, 5),
            ],
        );
    }
}

#[test]
fn line_count_preserves_empty_reversed_negative_and_clamped_ranges() {
    let _runtime = Context::new();
    for kind in BufferTextBackendKind::implemented_variants() {
        let buffer = buffer(kind, "a\r\nb\nc");
        let snapshot = LayoutBufferSnapshot::from_buffer(&buffer);
        check_counts(
            &RustBufferAccess::new(&snapshot),
            &[
                (0, 0, 0),
                (4, 2, 0),
                (-1, 7, 0),
                (0, -1, 0),
                (i64::MIN, i64::MAX, 0),
                (i64::MAX, i64::MAX, 0),
                (0, i64::MAX, 2),
                (3, i64::MAX, 1),
                (1, 2, 0),
                (1, 3, 1),
            ],
        );
    }
}

#[test]
fn line_count_knob_defaults_to_memchr_and_preserves_scalar_override() {
    assert_eq!(
        LayoutLineCountMode::from_setting(None),
        LayoutLineCountMode::On
    );
    for setting in ["off", "0", "unknown", ""] {
        assert_eq!(
            LayoutLineCountMode::from_setting(Some(setting)),
            LayoutLineCountMode::Off
        );
    }
    for setting in ["on", "1", "true", "yes", " ON "] {
        assert_eq!(
            LayoutLineCountMode::from_setting(Some(setting)),
            LayoutLineCountMode::On
        );
    }
    assert_eq!(
        LayoutLineCountMode::from_setting(Some(" verify ")),
        LayoutLineCountMode::Verify
    );
}
