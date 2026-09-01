use super::TextBackend;
use crate::buffer::position::{CharPos0, CharRange, EmacsByteLen, EmacsBytePos, EmacsByteRange};
use crate::buffer::text::{
    ImplementedBufferTextBackendKind, TextEditRange, TextExtent, TextReplacement,
};
use proptest::prelude::*;

fn byte_range(start: usize, end: usize) -> EmacsByteRange {
    assert!(start <= end);
    EmacsByteRange::from_start_len(EmacsBytePos::new(start), EmacsByteLen::new(end - start))
}

#[track_caller]
fn assert_backend_matches_gap(
    kind: ImplementedBufferTextBackendKind,
    backend: &TextBackend,
    gap: &TextBackend,
) {
    assert_eq!(
        backend.metrics(),
        gap.metrics(),
        "{kind:?} metrics diverged"
    );
    assert_eq!(
        backend.metrics().is_empty(),
        gap.metrics().is_empty(),
        "{kind:?} emptiness diverged"
    );
    assert_eq!(
        backend.is_multibyte(),
        gap.is_multibyte(),
        "{kind:?} multibyte flag diverged"
    );
    let backend_len = backend.metrics().emacs_bytes_usize();
    let gap_len = gap.metrics().emacs_bytes_usize();
    assert_eq!(backend_len, gap_len, "{kind:?} byte length diverged");
    assert_eq!(
        backend.to_string(),
        gap.to_string(),
        "{kind:?} display text diverged"
    );
    assert_eq!(
        backend.dump_text(),
        gap.dump_text(),
        "{kind:?} dump bytes diverged"
    );

    let full = byte_range(0, backend_len);
    assert_eq!(
        copied_range(backend, full),
        copied_range(gap, full),
        "{kind:?} full byte copy diverged"
    );

    for byte_pos in 0..backend_len {
        let pos = EmacsBytePos::new(byte_pos);
        assert_eq!(
            backend.byte_at_emacs_byte_pos(pos),
            gap.byte_at_emacs_byte_pos(pos),
            "{kind:?} byte_at({byte_pos}) diverged"
        );
        assert_eq!(
            backend.emacs_byte_at_pos(pos),
            gap.emacs_byte_at_pos(pos),
            "{kind:?} emacs_byte_at({byte_pos}) diverged"
        );
    }
    assert_eq!(
        backend.emacs_byte_at_pos(EmacsBytePos::new(backend_len)),
        None,
        "{kind:?} end byte lookup should be nil"
    );

    let mut char_boundaries = Vec::new();
    for char_pos in 0..=backend.metrics().chars_usize() {
        let char_pos0 = CharPos0::new(char_pos);
        let backend_byte = backend.char_pos_to_emacs_byte_pos(char_pos0);
        let gap_byte = gap.char_pos_to_emacs_byte_pos(char_pos0);
        assert_eq!(
            backend_byte, gap_byte,
            "{kind:?} char_to_byte({char_pos}) diverged"
        );
        assert_eq!(
            backend.emacs_byte_pos_to_char_pos(backend_byte),
            char_pos0,
            "{kind:?} byte_to_char(char boundary {char_pos}) did not round trip"
        );

        char_boundaries.push(backend_byte.get());

        if char_pos < backend.metrics().chars_usize() {
            assert_eq!(
                backend.char_code_at_emacs_byte_pos(backend_byte),
                gap.char_code_at_emacs_byte_pos(gap_byte),
                "{kind:?} char code at char {char_pos} diverged"
            );
            assert_eq!(
                backend.char_at_emacs_byte_pos(backend_byte),
                gap.char_at_emacs_byte_pos(gap_byte),
                "{kind:?} char at char {char_pos} diverged"
            );
        }
    }

    for &start in &char_boundaries {
        for &end in char_boundaries.iter().filter(|&&end| start <= end) {
            let range = byte_range(start, end);
            assert_eq!(
                copied_range(backend, range),
                copied_range(gap, range),
                "{kind:?} copy range {start}..{end} diverged"
            );
            assert_eq!(
                chunked_range(backend, range).concat(),
                copied_range(backend, range),
                "{kind:?} chunks for range {start}..{end} did not flatten to copied bytes"
            );
            assert_contiguous_contract(kind, backend, range);
            if backend.is_multibyte() {
                assert_eq!(
                    backend.text_emacs_byte_range(range),
                    gap.text_emacs_byte_range(range),
                    "{kind:?} text range {start}..{end} diverged"
                );
            }
        }
    }
}

#[track_caller]
fn assert_contiguous_contract(
    kind: ImplementedBufferTextBackendKind,
    backend: &TextBackend,
    range: EmacsByteRange,
) {
    let has_contiguous = backend.has_contiguous_emacs_byte_range(range);
    let contiguous = backend.with_contiguous_emacs_byte_range(range, |slice| slice.to_vec());
    assert_eq!(
        contiguous.is_some(),
        has_contiguous,
        "{kind:?} contiguous predicate/result diverged for range {}..{}",
        range.start().get(),
        range.end().get()
    );
    if let Some(slice) = contiguous {
        assert_eq!(
            slice,
            copied_range(backend, range),
            "{kind:?} contiguous slice bytes diverged for range {}..{}",
            range.start().get(),
            range.end().get()
        );
    }
}

fn copied_range(backend: &TextBackend, range: EmacsByteRange) -> Vec<u8> {
    let mut out = Vec::new();
    backend.copy_emacs_byte_range_to(range, &mut out);
    out
}

fn chunked_range(backend: &TextBackend, range: EmacsByteRange) -> Vec<Vec<u8>> {
    let mut chunks = Vec::new();
    backend
        .for_each_emacs_byte_range_chunk(range, |chunk| {
            chunks.push(chunk.to_vec());
            Ok::<(), ()>(())
        })
        .expect("infallible chunk visitor");
    chunks
}

fn extent_for_bytes(bytes: &[u8], multibyte: bool) -> TextExtent {
    TextExtent::from_emacs_bytes(bytes, multibyte)
}

fn buffer_bytes_for_text(text: &str, multibyte: bool) -> Vec<u8> {
    crate::emacs_core::string_escape::storage_string_to_buffer_bytes(text, multibyte)
}

fn insert_text(backend: &mut TextBackend, byte_pos: EmacsBytePos, text: &str) {
    let bytes = buffer_bytes_for_text(text, backend.is_multibyte());
    backend.insert_measured_emacs_bytes(
        byte_pos,
        &bytes,
        extent_for_bytes(&bytes, backend.is_multibyte()),
    );
}

fn insert_bytes(backend: &mut TextBackend, byte_pos: EmacsBytePos, bytes: &[u8], chars: usize) {
    backend.insert_measured_emacs_bytes(
        byte_pos,
        bytes,
        TextExtent::from_usize(chars, bytes.len()),
    );
}

fn delete_char_range(backend: &mut TextBackend, char_range: CharRange) {
    let start = char_to_byte(backend, char_range.start().get());
    let end = char_to_byte(backend, char_range.end().get());
    backend.delete_measured_range(TextEditRange::new(
        byte_range(start, end),
        char_range.start(),
        char_range.end(),
    ));
}

fn delete_byte_range(backend: &mut TextBackend, byte_range: EmacsByteRange) {
    let start_char = byte_to_char(backend, byte_range.start().get());
    let end_char = byte_to_char(backend, byte_range.end().get());
    backend.delete_measured_range(TextEditRange::new(
        byte_range,
        CharPos0::new(start_char),
        CharPos0::new(end_char),
    ));
}

fn replace_char_range(backend: &mut TextBackend, char_range: CharRange, text: &str) {
    let start = char_to_byte(backend, char_range.start().get());
    let end = char_to_byte(backend, char_range.end().get());
    let bytes = buffer_bytes_for_text(text, backend.is_multibyte());
    let range = TextEditRange::new(byte_range(start, end), char_range.start(), char_range.end());
    backend.replace_measured_range(
        TextReplacement::new(range, extent_for_bytes(&bytes, backend.is_multibyte())),
        &bytes,
    );
}

fn replace_byte_range(backend: &mut TextBackend, byte_range: EmacsByteRange, bytes: &[u8]) {
    let start_char = byte_to_char(backend, byte_range.start().get());
    let end_char = byte_to_char(backend, byte_range.end().get());
    let range = TextEditRange::new(
        byte_range,
        CharPos0::new(start_char),
        CharPos0::new(end_char),
    );
    backend.replace_measured_range(
        TextReplacement::new(range, TextExtent::from_usize(bytes.len(), bytes.len())),
        bytes,
    );
}

fn replace_same_len(backend: &mut TextBackend, byte_range: EmacsByteRange, bytes: &[u8]) {
    assert_eq!(
        byte_range.end().get() - byte_range.start().get(),
        bytes.len()
    );
    let old_range = TextEditRange::new(
        byte_range,
        backend.emacs_byte_pos_to_char_pos(byte_range.start()),
        backend.emacs_byte_pos_to_char_pos(byte_range.end()),
    );
    let new_extent = TextExtent::from_emacs_bytes(bytes, backend.is_multibyte());
    backend.replace_same_len_measured_range(TextReplacement::new(old_range, new_extent), bytes);
}

fn char_to_byte(backend: &TextBackend, char_pos: usize) -> usize {
    backend
        .char_pos_to_emacs_byte_pos(CharPos0::new(char_pos))
        .get()
}

fn byte_to_char(backend: &TextBackend, byte_pos: usize) -> usize {
    backend
        .emacs_byte_pos_to_char_pos(EmacsBytePos::new(byte_pos))
        .get()
}

fn sample_insert(seed: u8) -> &'static str {
    match seed % 8 {
        0 => "a",
        1 => "XYZ",
        2 => "é",
        3 => "日本",
        4 => "\n",
        5 => "🙂",
        6 => "ßΩ",
        _ => "end",
    }
}

fn sample_unibyte_insert(seed: u8) -> Vec<u8> {
    match seed % 7 {
        0 => vec![b'a'],
        1 => vec![0xFF],
        2 => vec![b'\n'],
        3 => vec![0x80, b'Z'],
        4 => vec![b'X', b'Y', b'Z'],
        5 => vec![0, 1, 2],
        _ => vec![seed, seed.wrapping_add(1)],
    }
}

fn replacement_bytes_for_len(len: usize, seed: u8) -> Option<Vec<u8>> {
    let candidates = ["Q", "z", "\n", "é", "ß", "日", "界", "🙂", "🚀"];
    let matches: Vec<Vec<u8>> = candidates
        .iter()
        .map(|candidate| buffer_bytes_for_text(candidate, true))
        .filter(|bytes| bytes.len() == len)
        .collect();
    (!matches.is_empty()).then(|| matches[seed as usize % matches.len()].clone())
}

#[test]
fn implemented_backends_match_gap_for_scripted_multibyte_edits() {
    crate::test_utils::init_test_tracing();
    for kind in ImplementedBufferTextBackendKind::variants() {
        let mut backend = TextBackend::from_str("abécd日本\nΩ", kind);
        let mut gap =
            TextBackend::from_str("abécd日本\nΩ", ImplementedBufferTextBackendKind::GAP_BUFFER);
        assert_backend_matches_gap(kind, &backend, &gap);

        let insert_pos = char_to_byte(&backend, 2);
        insert_text(&mut backend, EmacsBytePos::new(insert_pos), "XYZ");
        insert_text(&mut gap, EmacsBytePos::new(insert_pos), "XYZ");
        assert_backend_matches_gap(kind, &backend, &gap);

        delete_char_range(&mut backend, CharRange::from_usize(1, 5));
        delete_char_range(&mut gap, CharRange::from_usize(1, 5));
        assert_backend_matches_gap(kind, &backend, &gap);

        replace_char_range(&mut backend, CharRange::from_usize(2, 4), "🙂z");
        replace_char_range(&mut gap, CharRange::from_usize(2, 4), "🙂z");
        assert_backend_matches_gap(kind, &backend, &gap);

        let start = char_to_byte(&backend, 0);
        let end = char_to_byte(&backend, 1);
        replace_same_len(&mut backend, EmacsByteRange::from_usize(start, end), b"Q");
        replace_same_len(&mut gap, EmacsByteRange::from_usize(start, end), b"Q");
        assert_backend_matches_gap(kind, &backend, &gap);

        backend.set_multibyte(false);
        gap.set_multibyte(false);
        assert_backend_matches_gap(kind, &backend, &gap);

        backend.set_multibyte(true);
        gap.set_multibyte(true);
        assert_backend_matches_gap(kind, &backend, &gap);
    }
}

#[test]
fn implemented_backends_match_gap_for_scripted_unibyte_edits() {
    crate::test_utils::init_test_tracing();
    let initial = vec![0xFF, b'A', 0x80, b'\n', b'Z'];
    for kind in ImplementedBufferTextBackendKind::variants() {
        let mut backend = TextBackend::from_emacs_bytes(&initial, false, kind);
        let mut gap = TextBackend::from_emacs_bytes(
            &initial,
            false,
            ImplementedBufferTextBackendKind::GAP_BUFFER,
        );
        assert_backend_matches_gap(kind, &backend, &gap);

        insert_bytes(&mut backend, EmacsBytePos::new(1), &[0, b'x', 0xFE], 3);
        insert_bytes(&mut gap, EmacsBytePos::new(1), &[0, b'x', 0xFE], 3);
        assert_backend_matches_gap(kind, &backend, &gap);

        delete_byte_range(&mut backend, EmacsByteRange::from_usize(2, 5));
        delete_byte_range(&mut gap, EmacsByteRange::from_usize(2, 5));
        assert_backend_matches_gap(kind, &backend, &gap);

        replace_byte_range(
            &mut backend,
            EmacsByteRange::from_usize(1, 3),
            &[b'R', b'S', b'T', b'U'],
        );
        replace_byte_range(
            &mut gap,
            EmacsByteRange::from_usize(1, 3),
            &[b'R', b'S', b'T', b'U'],
        );
        assert_backend_matches_gap(kind, &backend, &gap);

        replace_same_len(
            &mut backend,
            EmacsByteRange::from_usize(0, 2),
            &[0xAA, 0xBB],
        );
        replace_same_len(&mut gap, EmacsByteRange::from_usize(0, 2), &[0xAA, 0xBB]);
        assert_backend_matches_gap(kind, &backend, &gap);
    }
}

#[test]
fn backend_dump_round_trips_across_implemented_kinds() {
    crate::test_utils::init_test_tracing();
    for source_kind in ImplementedBufferTextBackendKind::variants() {
        let mut source = TextBackend::from_str("αβ\n日本🙂", source_kind);
        let insert_pos = char_to_byte(&source, 2);
        insert_text(&mut source, EmacsBytePos::new(insert_pos), "XY");
        replace_char_range(&mut source, CharRange::from_usize(1, 3), "Ω");

        let mut gap =
            TextBackend::from_str("αβ\n日本🙂", ImplementedBufferTextBackendKind::GAP_BUFFER);
        let insert_pos = char_to_byte(&gap, 2);
        insert_text(&mut gap, EmacsBytePos::new(insert_pos), "XY");
        replace_char_range(&mut gap, CharRange::from_usize(1, 3), "Ω");
        assert_backend_matches_gap(source_kind, &source, &gap);

        let snapshot = source.snapshot();
        for target_kind in ImplementedBufferTextBackendKind::variants() {
            let loaded = TextBackend::from_snapshot(snapshot.clone(), target_kind);
            assert_backend_matches_gap(target_kind, &loaded, &gap);
        }
    }
}

proptest! {
    #[test]
    fn non_gap_backends_match_gap_for_random_multibyte_edit_sequences(
        ops in prop::collection::vec((0u8..4, 0usize..200, 0usize..200, 0u8..32), 0..80)
    ) {
        for kind in ImplementedBufferTextBackendKind::non_gap_variants() {
            let mut backend = TextBackend::from_str("abécd日本", kind);
            let mut gap = TextBackend::from_str(
                "abécd日本",
                ImplementedBufferTextBackendKind::GAP_BUFFER,
            );
            assert_backend_matches_gap(kind, &backend, &gap);

            for (op, a, b, seed) in &ops {
                match op {
                    0 => {
                        let char_pos = a % (backend.metrics().chars_usize() + 1);
                        let byte_pos = char_to_byte(&backend, char_pos);
                        let text = sample_insert(*seed);
                        insert_text(&mut backend, EmacsBytePos::new(byte_pos), text);
                        insert_text(&mut gap, EmacsBytePos::new(byte_pos), text);
                    }
                    1 => {
                        if backend.metrics().chars_usize() > 0 {
                            let char_a = a % (backend.metrics().chars_usize() + 1);
                            let char_b = b % (backend.metrics().chars_usize() + 1);
                            let start_char = char_a.min(char_b);
                            let end_char = char_a.max(char_b);
                            delete_char_range(&mut backend, CharRange::from_usize(start_char, end_char));
                            delete_char_range(&mut gap, CharRange::from_usize(start_char, end_char));
                        }
                    }
                    2 => {
                        if backend.metrics().chars_usize() > 0 {
                            let char_pos = a % backend.metrics().chars_usize();
                            let start = char_to_byte(&backend, char_pos);
                            let end = char_to_byte(&backend, char_pos + 1);
                            if let Some(replacement) = replacement_bytes_for_len(end - start, *seed) {
                                replace_same_len(&mut backend, EmacsByteRange::from_usize(start, end), &replacement);
                                replace_same_len(&mut gap, EmacsByteRange::from_usize(start, end), &replacement);
                            }
                        }
                    }
                    _ => {
                        if backend.metrics().chars_usize() > 0 {
                            let char_a = a % (backend.metrics().chars_usize() + 1);
                            let char_b = b % (backend.metrics().chars_usize() + 1);
                            let start_char = char_a.min(char_b);
                            let end_char = char_a.max(char_b);
                            replace_char_range(&mut backend, CharRange::from_usize(start_char, end_char), sample_insert(*seed));
                            replace_char_range(&mut gap, CharRange::from_usize(start_char, end_char), sample_insert(*seed));
                        }
                    }
                }
                assert_backend_matches_gap(kind, &backend, &gap);
            }
        }
    }
}

proptest! {
    #[test]
    fn non_gap_backends_match_gap_for_random_unibyte_edit_sequences(
        ops in prop::collection::vec((0u8..4, 0usize..200, 0usize..200, any::<u8>()), 0..80)
    ) {
        let initial = vec![0xFF, b'A', 0x80, b'\n', b'Z'];
        for kind in ImplementedBufferTextBackendKind::non_gap_variants() {
            let mut backend = TextBackend::from_emacs_bytes(&initial, false, kind);
            let mut gap = TextBackend::from_emacs_bytes(
                &initial,
                false,
                ImplementedBufferTextBackendKind::GAP_BUFFER,
            );
            assert_backend_matches_gap(kind, &backend, &gap);

            for (op, a, b, seed) in &ops {
                match op {
                    0 => {
                        let backend_len = backend.metrics().emacs_bytes_usize();
                        let byte_pos = a % (backend_len + 1);
                        let bytes = sample_unibyte_insert(*seed);
                        insert_bytes(&mut backend, EmacsBytePos::new(byte_pos), &bytes, bytes.len());
                        insert_bytes(&mut gap, EmacsBytePos::new(byte_pos), &bytes, bytes.len());
                    }
                    1 => {
                        if !backend.metrics().is_empty() {
                            let backend_len = backend.metrics().emacs_bytes_usize();
                            let byte_a = a % (backend_len + 1);
                            let byte_b = b % (backend_len + 1);
                            let start = byte_a.min(byte_b);
                            let end = byte_a.max(byte_b);
                            delete_byte_range(&mut backend, EmacsByteRange::from_usize(start, end));
                            delete_byte_range(&mut gap, EmacsByteRange::from_usize(start, end));
                        }
                    }
                    2 => {
                        if !backend.metrics().is_empty() {
                            let backend_len = backend.metrics().emacs_bytes_usize();
                            let start = a % backend_len;
                            let end = (start + 1 + (b % 4)).min(backend_len);
                            let replacement = vec![*seed; end - start];
                            replace_same_len(&mut backend, EmacsByteRange::from_usize(start, end), &replacement);
                            replace_same_len(&mut gap, EmacsByteRange::from_usize(start, end), &replacement);
                        }
                    }
                    _ => {
                        if !backend.metrics().is_empty() {
                            let backend_len = backend.metrics().emacs_bytes_usize();
                            let byte_a = a % (backend_len + 1);
                            let byte_b = b % (backend_len + 1);
                            let start = byte_a.min(byte_b);
                            let end = byte_a.max(byte_b);
                            let bytes = sample_unibyte_insert(*seed);
                            replace_byte_range(&mut backend, EmacsByteRange::from_usize(start, end), &bytes);
                            replace_byte_range(&mut gap, EmacsByteRange::from_usize(start, end), &bytes);
                        }
                    }
                }
                assert_backend_matches_gap(kind, &backend, &gap);
            }
        }
    }
}
