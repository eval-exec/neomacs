//! Copy-on-write text snapshots (P3.5 stage A, `NEOMACS_TEXT_SNAPSHOT`).

use std::rc::Rc;

use super::*;
use crate::buffer::text_snapshot::{
    TextSnapshotMode, buffer_text_cow_copies, parse_text_snapshot_knob,
    set_text_snapshot_mode_override,
};

/// Run F with MODE forced on this thread, then return to the knob.
fn with_mode<R>(mode: TextSnapshotMode, f: impl FnOnce() -> R) -> R {
    set_text_snapshot_mode_override(Some(mode));
    let result = f();
    set_text_snapshot_mode_override(None);
    result
}

fn shares_bytes(a: &BufferText, b: &BufferText) -> bool {
    Rc::ptr_eq(&a.storage.borrow().backend, &b.storage.borrow().backend)
}

fn text_of(text: &BufferText) -> String {
    text.full_text_string()
}

fn every_backend() -> impl Iterator<Item = ImplementedBufferTextBackendKind> {
    BufferTextBackendKind::implemented_variants().map(implemented_kind)
}

#[test]
fn knob_parses_modes_and_defaults_to_copy() {
    crate::test_utils::init_test_tracing();
    assert_eq!(parse_text_snapshot_knob(None), TextSnapshotMode::Copy);
    assert_eq!(parse_text_snapshot_knob(Some("")), TextSnapshotMode::Copy);
    assert_eq!(
        parse_text_snapshot_knob(Some("copy")),
        TextSnapshotMode::Copy
    );
    assert_eq!(
        parse_text_snapshot_knob(Some("off")),
        TextSnapshotMode::Copy
    );
    assert_eq!(
        parse_text_snapshot_knob(Some(" Share ")),
        TextSnapshotMode::Share
    );
    assert_eq!(
        parse_text_snapshot_knob(Some("on")),
        TextSnapshotMode::Share
    );
    assert_eq!(
        parse_text_snapshot_knob(Some("bogus")),
        TextSnapshotMode::Copy
    );
}

#[test]
fn share_snapshot_is_an_rc_bump_and_copy_snapshot_is_not() {
    crate::test_utils::init_test_tracing();
    for kind in every_backend() {
        let text = BufferText::from_str_with_backend_kind("hello world", kind);
        let shared = with_mode(TextSnapshotMode::Share, || text.clone());
        assert!(shares_bytes(&text, &shared), "{kind:?}: share mode shares");
        let copied = with_mode(TextSnapshotMode::Copy, || text.clone());
        assert!(!shares_bytes(&text, &copied), "{kind:?}: copy mode copies");
        assert_eq!(text_of(&shared), "hello world");
        assert_eq!(text_of(&copied), "hello world");
    }
}

#[test]
fn share_snapshot_keeps_old_text_across_insert_delete_and_replace() {
    crate::test_utils::init_test_tracing();
    for kind in every_backend() {
        let mut text = BufferText::from_str_with_backend_kind("abc\u{e9}def\nghi", kind);
        let before = text_of(&text);

        let snapshot = with_mode(TextSnapshotMode::Share, || text.clone());
        let copies = buffer_text_cow_copies();
        insert_storage_string(&mut text, emacs_byte_pos(3), "XY\u{4e2d}");
        assert_eq!(
            buffer_text_cow_copies(),
            copies + 1,
            "{kind:?}: the first mutation copies once"
        );
        assert!(!shares_bytes(&text, &snapshot));
        // Later mutations find the live bytes unshared: no further copy.
        let range = emacs_byte_range(0, 2);
        delete_emacs_byte_range(&mut text, range);
        insert_storage_string(&mut text, emacs_byte_pos(0), "Q");
        assert_eq!(
            buffer_text_cow_copies(),
            copies + 1,
            "{kind:?}: exactly one copy"
        );

        assert_eq!(text_of(&snapshot), before, "{kind:?}: snapshot unchanged");
        assert_eq!(snapshot.char_count(), CharLen::new(before.chars().count()));
        assert_eq!(
            text_of(&text),
            format!("Qc{}", &"XY\u{4e2d}\u{e9}def\nghi"),
            "{kind:?}: live text edited"
        );
        // Position conversions on the snapshot answer for the old text.
        for (charpos, ch) in before.chars().enumerate() {
            let byte = char_pos_to_byte_pos(&snapshot, charpos);
            assert_eq!(byte_pos_to_char_pos(&snapshot, byte), charpos);
            assert_eq!(
                snapshot.char_at_emacs_byte_pos(emacs_byte_pos(byte)),
                Some(ch),
                "{kind:?}: char {charpos}"
            );
        }
    }
}

#[test]
fn share_snapshot_dropped_before_the_edit_copies_nothing() {
    crate::test_utils::init_test_tracing();
    for kind in every_backend() {
        let mut text = BufferText::from_str_with_backend_kind("steady typing", kind);
        let copies = buffer_text_cow_copies();
        // A redisplay per keystroke: snapshot, read, drop, then type.
        for i in 0..50 {
            let snapshot = with_mode(TextSnapshotMode::Share, || text.clone());
            assert_eq!(snapshot.char_count(), text.char_count());
            drop(snapshot);
            let end = text.emacs_byte_len().get();
            insert_storage_string(
                &mut text,
                emacs_byte_pos(end),
                if i % 2 == 0 { "a" } else { "b" },
            );
        }
        assert_eq!(
            buffer_text_cow_copies(),
            copies,
            "{kind:?}: steady-state typing must not copy"
        );
        assert_eq!(text.char_count(), CharLen::new("steady typing".len() + 50));
    }
}

#[test]
fn copy_snapshot_never_counts_a_cow_copy() {
    crate::test_utils::init_test_tracing();
    let mut text = BufferText::from_str("copy mode");
    let copies = buffer_text_cow_copies();
    let snapshot = with_mode(TextSnapshotMode::Copy, || text.clone());
    insert_storage_string(&mut text, emacs_byte_pos(0), "x");
    assert_eq!(buffer_text_cow_copies(), copies);
    assert_eq!(text_of(&snapshot), "copy mode");
    assert_eq!(text_of(&text), "xcopy mode");
}

#[test]
fn gap_motion_under_a_shared_snapshot_copies_only_when_the_gap_moves() {
    crate::test_utils::init_test_tracing();
    let mut text = BufferText::from_str("0123456789abcdefghij");
    // Park the gap mid-buffer.
    insert_storage_string(&mut text, emacs_byte_pos(10), "-");
    let full = full_emacs_byte_range(&text);
    let head = emacs_byte_range(0, 5);
    assert!(text.has_contiguous_emacs_byte_range(head));
    assert!(!text.has_contiguous_emacs_byte_range(full));

    let snapshot = with_mode(TextSnapshotMode::Share, || text.clone());
    let copies = buffer_text_cow_copies();
    // Already contiguous: nothing moves, nothing is copied.
    assert!(text.try_make_emacs_byte_range_contiguous(head));
    assert_eq!(buffer_text_cow_copies(), copies);
    assert!(shares_bytes(&text, &snapshot));
    // The gap must move: the live side copies, the snapshot keeps its gap.
    let snapshot_gap = snapshot.gap_debug_layout();
    assert!(text.try_make_emacs_byte_range_contiguous(full));
    assert_eq!(buffer_text_cow_copies(), copies + 1);
    assert!(text.has_contiguous_emacs_byte_range(full));
    assert_eq!(snapshot.gap_debug_layout(), snapshot_gap);
    assert_eq!(text_of(&snapshot), text_of(&text));
}

#[test]
fn share_snapshot_starts_with_an_empty_anchor_ring_and_converts_correctly() {
    crate::test_utils::init_test_tracing();
    // Multibyte text long enough that scans record anchors: each walk below
    // is far longer than POSITION_ANCHOR_STRIDE.
    let unit = "\u{e9}".repeat(64) + "abcd" + &"\u{65e5}\u{672c}".repeat(16) + "\n";
    let source: String = unit.repeat(400);
    let text = BufferText::from_str(&source);
    let chars = source.chars().count();
    for k in 1..20 {
        char_pos_to_byte_pos(&text, k * chars / 20);
    }
    assert!(text.scan_anchor_ring_len_for_test() > 0, "live ring warmed");

    let copied = with_mode(TextSnapshotMode::Copy, || text.clone());
    assert_eq!(
        copied.scan_anchor_ring_len_for_test(),
        text.scan_anchor_ring_len_for_test(),
        "copy mode keeps the old snapshot exactly"
    );
    let shared = with_mode(TextSnapshotMode::Share, || text.clone());
    assert_eq!(shared.scan_anchor_ring_len_for_test(), 0, "not copied");
    for target in (0..chars).step_by(1013) {
        let expected = char_pos_to_byte_pos(&text, target);
        assert_eq!(char_pos_to_byte_pos(&shared, target), expected);
        assert_eq!(byte_pos_to_char_pos(&shared, expected), target);
    }
}

#[test]
fn share_snapshot_survives_set_multibyte_and_backend_conversion() {
    crate::test_utils::init_test_tracing();
    let text = BufferText::from_str("abc");
    let snapshot = with_mode(TextSnapshotMode::Share, || text.clone());
    let copies = buffer_text_cow_copies();
    text.convert_backend_kind(ImplementedBufferTextBackendKind::ROPE);
    // A conversion replaces the backend outright; nothing is copied on write.
    assert_eq!(buffer_text_cow_copies(), copies);
    assert!(!shares_bytes(&text, &snapshot));
    assert_eq!(snapshot.backend_kind(), BufferTextBackendKind::GapBuffer);
    assert_eq!(text_of(&snapshot), "abc");

    let snapshot = with_mode(TextSnapshotMode::Share, || text.clone());
    text.set_multibyte(false);
    assert_eq!(buffer_text_cow_copies(), copies + 1);
    assert!(snapshot.is_multibyte());
    assert!(!text.is_multibyte());
}
