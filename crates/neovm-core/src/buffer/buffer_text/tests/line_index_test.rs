//! The text line index inside `BufferText` (`NEOVM_TEXT_LINE_INDEX`): every
//! edit path keeps it equal to a recount, every query answers what a scan
//! answers, snapshots share it, and it builds safely under a held borrow.

use super::*;
use crate::buffer::text::TextReplacement;
use crate::buffer::text_index::tests::{Rng, boundaries, random_bytes};
use crate::buffer::text_index::{
    TextLineIndexConfig, TextLineIndexMode, with_text_line_index_config,
};
use crate::buffer::text_props::TextPropertyTable;
use crate::buffer::text_snapshot::{TextSnapshotMode, set_text_snapshot_mode_override};

use super::super::LineEnd;

/// Every buffer and every query uses the index; chunks of 16 bytes so that
/// small texts have many; every answer is checked against a scan.
fn eager_verify() -> TextLineIndexConfig {
    TextLineIndexConfig::eager(TextLineIndexMode::Verify, 16)
}

fn every_backend() -> impl Iterator<Item = ImplementedBufferTextBackendKind> {
    BufferTextBackendKind::implemented_variants().map(implemented_kind)
}

fn text_with_bytes(
    kind: ImplementedBufferTextBackendKind,
    bytes: &[u8],
    multibyte: bool,
) -> BufferText {
    let mut text = BufferText::new_with_backend_kind(kind);
    text.set_multibyte(multibyte);
    let extent = TextExtent::from_emacs_bytes(bytes, multibyte);
    text.insert_measured_emacs_bytes(EmacsBytePos::ZERO, bytes, extent);
    text
}

fn all_bytes(text: &BufferText) -> Vec<u8> {
    let mut out = Vec::new();
    text.copy_emacs_byte_range_to(full_emacs_byte_range(text), &mut out);
    out
}

/// What the scans answer, computed on the model bytes.
fn model_count(bytes: &[u8], from: usize, limit: usize, line_end: LineEnd) -> usize {
    let (from, limit) = (from.min(bytes.len()), limit.min(bytes.len()));
    if from >= limit {
        return 0;
    }
    bytes[from..limit]
        .iter()
        .filter(|&&b| b == b'\n' || (line_end == LineEnd::NewlineOrCarriageReturn && b == b'\r'))
        .count()
}

fn model_nth(bytes: &[u8], from: usize, limit: usize, n: usize) -> (usize, usize) {
    let (from, limit) = (from.min(bytes.len()), limit.min(bytes.len()));
    if n == 0 || from >= limit {
        return (from, 0);
    }
    let mut crossed = 0;
    let mut past = from;
    for (i, &b) in bytes[from..limit].iter().enumerate() {
        if b == b'\n' {
            crossed += 1;
            past = from + i + 1;
            if crossed == n {
                break;
            }
        }
    }
    (past, crossed)
}

fn model_backward(bytes: &[u8], from: usize, floor: usize, n: usize) -> (usize, usize) {
    let bol = |pos: usize| {
        bytes[floor..pos]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(floor, |i| floor + i + 1)
    };
    let mut pos = from;
    let mut moved = 0;
    for _ in 0..n {
        let start = bol(pos);
        if start <= floor {
            pos = floor;
            break;
        }
        pos = bol(start - 1);
        moved += 1;
    }
    (pos, moved)
}

/// Random queries on TEXT against the model answers.
fn check_queries(text: &BufferText, rng: &mut Rng) {
    let bytes = all_bytes(text);
    text.check_line_index_for_test()
        .unwrap_or_else(|err| panic!("index invalid: {err}"));
    let marks = boundaries(&bytes, text.is_multibyte());
    for _ in 0..12 {
        let a = marks[rng.below(marks.len())];
        let b = marks[rng.below(marks.len())];
        let (from, limit) = (a.min(b), a.max(b));
        for line_end in [LineEnd::Newline, LineEnd::NewlineOrCarriageReturn] {
            assert_eq!(
                text.count_line_ends_emacs_byte(
                    emacs_byte_pos(from),
                    emacs_byte_pos(limit),
                    line_end
                ),
                model_count(&bytes, from, limit, line_end),
                "count [{from}, {limit}) {line_end:?}"
            );
        }
        let n = rng.below(40);
        let (end, crossed) =
            text.nth_newline_emacs_byte(emacs_byte_pos(from), emacs_byte_pos(limit), n);
        assert_eq!(
            (end.get(), crossed),
            model_nth(&bytes, from, limit, n),
            "nth [{from}, {limit}) n={n}"
        );
        if n > 0 {
            let got = text
                .lines_backward_emacs_byte(emacs_byte_pos(limit), emacs_byte_pos(from), n)
                .map(|(pos, moved)| (pos.get(), moved));
            assert_eq!(
                got,
                Some(model_backward(&bytes, limit, from, n)),
                "backward [{from}, {limit}] n={n}"
            );
        }
    }
}

/// Random edits through all four measured mutators on every backend.
#[test]
fn every_edit_path_keeps_the_index_equal_to_a_recount() {
    crate::test_utils::init_test_tracing();
    with_text_line_index_config(eager_verify(), || {
        for kind in every_backend() {
            for seed in 0..6u64 {
                let mut rng = Rng::new(seed * 31 + u64::from(u8::from(kind.public_kind())));
                let multibyte = seed % 3 != 0;
                let initial = random_bytes(&mut rng, 150, multibyte);
                let mut text = text_with_bytes(kind, &initial, multibyte);
                // A query builds the index.
                let _ =
                    text.count_newlines_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos());
                assert!(
                    text.has_line_index_for_test(),
                    "{kind:?}: built on first query"
                );
                for _ in 0..120 {
                    let bytes = all_bytes(&text);
                    let marks = boundaries(&bytes, multibyte);
                    let a = marks[rng.below(marks.len())];
                    let b = marks[rng.below(marks.len())];
                    let range = emacs_byte_range(a.min(b), a.max(b));
                    let len = if rng.below(5) == 0 {
                        rng.below(60)
                    } else {
                        rng.below(3)
                    };
                    let new = random_bytes(&mut rng, len, multibyte);
                    let extent = TextExtent::from_emacs_bytes(&new, multibyte);
                    match rng.below(4) {
                        0 => text.insert_measured_emacs_bytes(range.start(), &new, extent),
                        1 => delete_emacs_byte_range(&mut text, range),
                        2 => {
                            let old = text.edit_range_for_emacs_byte_range(range);
                            text.replace_measured_range(TextReplacement::new(old, extent), &new);
                        }
                        _ => {
                            // Same byte length: overwrite with ASCII and line ends.
                            let same: Vec<u8> = (0..range.len().get())
                                .map(|_| [b'x', b'\n', b'\r'][rng.below(3)])
                                .collect();
                            if !same.is_empty() {
                                let old = text.edit_range_for_emacs_byte_range(range);
                                let extent = TextExtent::from_emacs_bytes(&same, multibyte);
                                text.replace_same_len_measured_range(
                                    TextReplacement::new(old, extent),
                                    &same,
                                );
                            }
                        }
                    }
                    assert!(
                        text.has_line_index_for_test(),
                        "{kind:?}: small edits keep it"
                    );
                    check_queries(&text, &mut rng);
                }
            }
        }
    });
}

#[test]
fn the_index_is_off_by_default_and_never_built() {
    crate::test_utils::init_test_tracing();
    let config = TextLineIndexConfig {
        min_buffer_bytes: 0,
        min_query_bytes: 0,
        min_query_lines: 0,
        ..TextLineIndexConfig::with_mode(TextLineIndexMode::Off)
    };
    with_text_line_index_config(config, || {
        let text = BufferText::from_str(&"line\n".repeat(1000));
        assert_eq!(
            text.count_newlines_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos()),
            1000
        );
        assert_eq!(
            text.nth_newline_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos(), 500),
            (emacs_byte_pos(2500), 500)
        );
        assert_eq!(
            text.lines_backward_emacs_byte(text.emacs_byte_end_pos(), EmacsBytePos::ZERO, 500),
            None
        );
        assert!(!text.has_line_index_for_test());
        let snapshot = text.clone();
        assert!(!snapshot.has_line_index_for_test());
    });
}

#[test]
fn small_texts_short_ranges_and_short_moves_scan() {
    crate::test_utils::init_test_tracing();
    let config = TextLineIndexConfig {
        min_buffer_bytes: 1000,
        min_query_bytes: 100,
        min_query_lines: 10,
        ..TextLineIndexConfig::eager(TextLineIndexMode::Verify, 64)
    };
    with_text_line_index_config(config, || {
        let small = BufferText::from_str(&"ab\n".repeat(100));
        small.count_newlines_emacs_byte(EmacsBytePos::ZERO, small.emacs_byte_end_pos());
        assert!(!small.has_line_index_for_test(), "300 bytes < 1000");

        let text = BufferText::from_str(&"ab\n".repeat(1000));
        text.count_newlines_emacs_byte(EmacsBytePos::ZERO, emacs_byte_pos(99));
        text.nth_newline_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos(), 10);
        text.lines_backward_emacs_byte(text.emacs_byte_end_pos(), EmacsBytePos::ZERO, 10);
        assert!(!text.has_line_index_for_test(), "short queries scan");
        // A count over 100..1000 bytes uses an index but does not build one.
        assert_eq!(
            text.count_newlines_emacs_byte(EmacsBytePos::ZERO, emacs_byte_pos(999)),
            333
        );
        assert!(!text.has_line_index_for_test(), "a 999-byte count scans");
        assert_eq!(
            text.nth_newline_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos(), 11),
            (emacs_byte_pos(33), 11)
        );
        assert!(text.has_line_index_for_test(), "an 11-line move builds it");
        assert_eq!(
            text.indexed_newline_count(EmacsBytePos::ZERO, emacs_byte_pos(999)),
            Some(333),
            "and a 999-byte count then uses it"
        );
    });
    // A count over the whole of a large enough text builds one.
    with_text_line_index_config(config, || {
        let text = BufferText::from_str(&"ab\n".repeat(1000));
        text.count_newlines_emacs_byte(EmacsBytePos::ZERO, emacs_byte_pos(1000));
        assert!(text.has_line_index_for_test());
    });
}

#[test]
fn wholesale_mutations_drop_the_index_and_queries_stay_exact() {
    crate::test_utils::init_test_tracing();
    with_text_line_index_config(eager_verify(), || {
        let built = |text: &BufferText| {
            text.count_newlines_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos());
            assert!(text.has_line_index_for_test());
        };
        let text = BufferText::from_str("a\u{e9}\nb\n\u{4e2d}\r\n");
        built(&text);
        text.set_multibyte(false);
        assert!(
            !text.has_line_index_for_test(),
            "set-buffer-multibyte drops it"
        );
        assert_eq!(
            text.count_newlines_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos()),
            3
        );
        text.set_multibyte(true);
        built(&text);
        text.replace_storage("x\ny", true, TextPropertyTable::new());
        assert!(!text.has_line_index_for_test(), "replace_storage drops it");
        assert_eq!(
            text.count_newlines_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos()),
            1
        );
        built(&text);
        text.convert_backend_kind(ImplementedBufferTextBackendKind::ROPE);
        assert!(
            !text.has_line_index_for_test(),
            "a backend conversion drops it"
        );
        assert_eq!(
            text.nth_newline_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos(), 1),
            (emacs_byte_pos(2), 1)
        );
        // A gap move changes no logical position: the index stays.
        let mut text = BufferText::from_str(&"0123456789\n".repeat(50));
        built(&text);
        insert_storage_string(&mut text, emacs_byte_pos(100), "zz");
        assert!(text.try_make_emacs_byte_range_contiguous(emacs_byte_range(0, 300)));
        assert!(text.has_line_index_for_test());
        text.check_line_index_for_test().unwrap();
    });
}

#[test]
fn an_edit_larger_than_a_quarter_of_the_text_drops_the_index() {
    crate::test_utils::init_test_tracing();
    with_text_line_index_config(eager_verify(), || {
        let mut text = BufferText::from_str(&"line\n".repeat(80_000));
        text.count_newlines_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos());
        assert!(text.has_line_index_for_test());
        // 300 KiB > max(256 KiB, 400 KB / 4): an erase-buffer.
        delete_emacs_byte_range(&mut text, emacs_byte_range(0, 300_000));
        assert!(!text.has_line_index_for_test());
        assert_eq!(
            text.count_newlines_emacs_byte(EmacsBytePos::ZERO, text.emacs_byte_end_pos()),
            20_000
        );
        assert!(text.has_line_index_for_test(), "the next query rebuilds it");
        // 100 KB into 100 KB: below 256 KiB, so it is maintained.
        insert_storage_string(&mut text, emacs_byte_pos(5), &"x\n".repeat(50_000));
        assert!(text.has_line_index_for_test());
        text.check_line_index_for_test().unwrap();
        // 300 KB: dropped.
        insert_storage_string(&mut text, emacs_byte_pos(5), &"y\n".repeat(150_000));
        assert!(!text.has_line_index_for_test());
    });
}

/// A layout snapshot shares the index: no copy per layout, a copy only if
/// the live text is edited while the snapshot still holds it, and each then
/// describes its own text.
#[test]
fn snapshots_share_the_index_and_an_edit_copies_it_only_while_shared() {
    crate::test_utils::init_test_tracing();
    for mode in [TextSnapshotMode::Copy, TextSnapshotMode::Share] {
        set_text_snapshot_mode_override(Some(mode));
        with_text_line_index_config(eager_verify(), || {
            let mut text = BufferText::from_str(&"abc\n".repeat(100));
            let end = text.emacs_byte_end_pos();
            assert_eq!(text.count_newlines_emacs_byte(EmacsBytePos::ZERO, end), 100);
            let snapshot = text.clone();
            assert!(snapshot.shares_line_index_with_for_test(&text), "{mode:?}");
            assert_eq!(
                snapshot.indexed_newline_count(EmacsBytePos::ZERO, end),
                Some(100),
                "{mode:?}: the snapshot's count uses the shared index"
            );
            // Edit the live text while the snapshot lives: one copy.
            insert_storage_string(&mut text, EmacsBytePos::ZERO, "\n\n");
            assert!(!snapshot.shares_line_index_with_for_test(&text));
            assert_eq!(
                snapshot.indexed_newline_count(EmacsBytePos::ZERO, end),
                Some(100)
            );
            assert_eq!(
                text.indexed_newline_count(EmacsBytePos::ZERO, text.emacs_byte_end_pos()),
                Some(102)
            );
            drop(snapshot);
            // Snapshot, drop, edit: the live text owns its index again.
            let snapshot = text.clone();
            drop(snapshot);
            insert_storage_string(&mut text, EmacsBytePos::ZERO, "\n");
            text.check_line_index_for_test().unwrap();
        });
        set_text_snapshot_mode_override(None);
    }
}

/// A snapshot never builds; its query asks the live text to, and the next
/// snapshot shares what the live text built.
#[test]
fn a_snapshot_query_makes_the_live_text_build_at_the_next_snapshot() {
    crate::test_utils::init_test_tracing();
    with_text_line_index_config(eager_verify(), || {
        let text = BufferText::from_str(&"abc\n".repeat(100));
        let end = text.emacs_byte_end_pos();
        let first = text.clone();
        assert_eq!(first.indexed_newline_count(EmacsBytePos::ZERO, end), None);
        assert_eq!(
            first.count_newlines_emacs_byte(EmacsBytePos::ZERO, end),
            100
        );
        assert!(
            !first.has_line_index_for_test(),
            "a snapshot does not build"
        );
        assert!(!text.has_line_index_for_test());
        let second = text.clone();
        assert!(
            text.has_line_index_for_test(),
            "the live text built at the snapshot"
        );
        assert!(second.shares_line_index_with_for_test(&text));
        assert_eq!(
            second.indexed_newline_count(EmacsBytePos::ZERO, end),
            Some(100)
        );
        // No further demand: the next snapshot just shares.
        let third = text.clone();
        assert!(third.shares_line_index_with_for_test(&text));
    });
}

/// The ASCII-prefix trap (2026-09-20): a lazy build under a read borrow
/// redisplay holds must not `borrow_mut` the storage.
#[test]
fn the_lazy_build_is_safe_under_a_held_read_borrow() {
    crate::test_utils::init_test_tracing();
    with_text_line_index_config(eager_verify(), || {
        let text = BufferText::from_str(&"abc\n".repeat(100));
        let end = text.emacs_byte_end_pos();
        let _held = text.storage.borrow();
        let mut counts = Vec::new();
        text.for_each_emacs_byte_range_chunk::<()>(emacs_byte_range(0, 4), |_| {
            counts.push(text.count_newlines_emacs_byte(EmacsBytePos::ZERO, end));
            counts.push(text.nth_newline_emacs_byte(EmacsBytePos::ZERO, end, 50).1);
            Ok(())
        })
        .unwrap();
        assert_eq!(counts, vec![100, 50]);
        assert!(text.has_line_index_for_test());
    });
}

/// Indirect buffers share the storage, and so the index; narrowing only
/// changes the ranges asked about.
#[test]
fn shared_storage_shares_one_index() {
    crate::test_utils::init_test_tracing();
    with_text_line_index_config(eager_verify(), || {
        let mut text = BufferText::from_str(&"abc\n".repeat(100));
        let indirect = text.shared_clone();
        assert_eq!(
            indirect.count_newlines_emacs_byte(emacs_byte_pos(40), emacs_byte_pos(80)),
            10
        );
        assert!(text.has_line_index_for_test());
        insert_storage_string(&mut text, emacs_byte_pos(41), "\n\n\n");
        assert_eq!(
            indirect.count_newlines_emacs_byte(emacs_byte_pos(40), emacs_byte_pos(83)),
            13
        );
        indirect.check_line_index_for_test().unwrap();
    });
}
