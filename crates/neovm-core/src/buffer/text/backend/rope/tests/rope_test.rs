use super::*;
use crate::buffer::gap_buffer::GapBuffer;
use proptest::prelude::*;

fn byte_range(start: usize, end: usize) -> EmacsByteRange {
    assert!(start <= end);
    EmacsByteRange::from_start_len(EmacsBytePos::new(start), EmacsByteLen::new(end - start))
}

fn assert_matches_gap(rope: &RopeTextBackend, gap: &GapBuffer) {
    rope.assert_invariants();
    assert_eq!(rope.len(), gap.len());
    assert_eq!(rope.metrics().chars_usize(), gap.char_count());
    assert_eq!(rope.to_string(), gap.to_string());

    let mut rope_bytes = Vec::new();
    let mut gap_bytes = Vec::new();
    rope.copy_emacs_byte_range_to(byte_range(0, rope.len()), &mut rope_bytes);
    gap.copy_emacs_byte_range_to(byte_range(0, gap.len()), &mut gap_bytes);
    assert_eq!(rope_bytes, gap_bytes);

    for byte_pos in 0..rope.len() {
        assert_eq!(rope_byte_at(rope, byte_pos), gap_byte_at(gap, byte_pos));
        assert_eq!(
            rope_emacs_byte_at(rope, byte_pos),
            gap_emacs_byte_at(gap, byte_pos)
        );
    }
    assert_eq!(rope_emacs_byte_at(rope, rope.len()), None);
    assert_eq!(gap_emacs_byte_at(gap, gap.len()), None);

    for char_pos in 0..=rope.metrics().chars_usize() {
        let rope_byte = rope_char_to_byte(rope, char_pos);
        let gap_byte = gap_char_to_byte(gap, char_pos);
        assert_eq!(rope_byte, gap_byte, "char_to_byte({char_pos})");
        assert_eq!(rope_byte_to_char(rope, rope_byte), char_pos);
        assert_eq!(gap_byte_to_char(gap, gap_byte), char_pos);
        if char_pos < rope.metrics().chars_usize() {
            assert_eq!(
                rope_char_code_at(rope, rope_byte),
                gap_char_code_at(gap, gap_byte)
            );
        }
    }
}

fn gap_byte_to_char(gap: &GapBuffer, byte_pos: usize) -> usize {
    gap.emacs_byte_pos_to_char_pos(EmacsBytePos::new(byte_pos))
        .get()
}

fn gap_char_to_byte(gap: &GapBuffer, char_pos: usize) -> usize {
    gap.char_pos_to_emacs_byte_pos(CharPos0::new(char_pos))
        .get()
}

fn gap_byte_at(gap: &GapBuffer, byte_pos: usize) -> u8 {
    gap.byte_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
}

fn gap_emacs_byte_at(gap: &GapBuffer, byte_pos: usize) -> Option<u8> {
    gap.emacs_byte_at_pos(EmacsBytePos::new(byte_pos))
}

fn gap_char_code_at(gap: &GapBuffer, byte_pos: usize) -> Option<u32> {
    gap.char_code_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
}

fn insert_gap_str(gap: &mut GapBuffer, byte_pos: usize, text: &str) {
    gap.insert_storage_string_at_emacs_byte_pos(EmacsBytePos::new(byte_pos), text);
}

fn insert_gap_bytes_both(gap: &mut GapBuffer, byte_pos: usize, bytes: &[u8], nchars: usize) {
    gap.insert_emacs_bytes_at_emacs_byte_pos_with_char_len(
        EmacsBytePos::new(byte_pos),
        bytes,
        crate::buffer::CharLen::new(nchars),
    );
}

fn delete_gap_range_both(gap: &mut GapBuffer, start: usize, end: usize, nchars: usize) {
    gap.delete_emacs_byte_range_with_char_len(
        byte_range(start, end),
        crate::buffer::CharLen::new(nchars),
    );
}

fn replace_gap_same_len(gap: &mut GapBuffer, start: usize, end: usize, replacement: &[u8]) {
    gap.replace_same_len_emacs_byte_range(byte_range(start, end), replacement);
}

fn rope_byte_to_char(rope: &RopeTextBackend, byte_pos: usize) -> usize {
    rope.emacs_byte_pos_to_char_pos(EmacsBytePos::new(byte_pos))
        .get()
}

fn rope_char_to_byte(rope: &RopeTextBackend, char_pos: usize) -> usize {
    rope.char_pos_to_emacs_byte_pos(CharPos0::new(char_pos))
        .get()
}

fn rope_byte_at(rope: &RopeTextBackend, byte_pos: usize) -> u8 {
    rope.byte_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
}

fn rope_emacs_byte_at(rope: &RopeTextBackend, byte_pos: usize) -> Option<u8> {
    rope.emacs_byte_at_pos(EmacsBytePos::new(byte_pos))
}

fn rope_char_code_at(rope: &RopeTextBackend, byte_pos: usize) -> Option<u32> {
    rope.char_code_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
}

fn insert_rope_str(rope: &mut RopeTextBackend, byte_pos: usize, text: &str) {
    let bytes =
        crate::emacs_core::string_escape::storage_string_to_buffer_bytes(text, rope.multibyte);
    let extent = TextExtent::from_emacs_bytes(&bytes, rope.multibyte);
    rope.insert_measured_emacs_bytes(EmacsBytePos::new(byte_pos), &bytes, extent);
}

fn insert_rope_bytes_both(
    rope: &mut RopeTextBackend,
    byte_pos: usize,
    bytes: &[u8],
    nchars: usize,
) {
    rope.insert_measured_emacs_bytes(
        EmacsBytePos::new(byte_pos),
        bytes,
        TextExtent::new(CharLen::new(nchars), EmacsByteLen::new(bytes.len())),
    );
}

fn delete_rope_range_both(rope: &mut RopeTextBackend, start: usize, end: usize, nchars: usize) {
    let start_char = rope_byte_to_char(rope, start);
    rope.delete_measured_range(TextEditRange::from_usize(
        start,
        end,
        start_char,
        start_char + nchars,
    ));
}

fn replace_rope_same_len(rope: &mut RopeTextBackend, start: usize, end: usize, replacement: &[u8]) {
    let range = byte_range(start, end);
    let edit_range = TextEditRange::new(
        range,
        rope.emacs_byte_pos_to_char_pos(range.start()),
        rope.emacs_byte_pos_to_char_pos(range.end()),
    );
    rope.replace_same_len_measured_range(
        TextReplacement::new(
            edit_range,
            TextExtent::from_emacs_bytes(replacement, rope.multibyte),
        ),
        replacement,
    );
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

fn replacement_bytes_for_len(len: usize, seed: u8) -> Option<Vec<u8>> {
    let candidates = ["Q", "z", "\n", "é", "ß", "日", "界", "🙂", "🚀"];
    let matches: Vec<Vec<u8>> = candidates
        .iter()
        .map(|candidate| {
            crate::emacs_core::string_escape::storage_string_to_buffer_bytes(candidate, true)
        })
        .filter(|bytes| bytes.len() == len)
        .collect();
    (!matches.is_empty()).then(|| matches[seed as usize % matches.len()].clone())
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

#[test]
fn rope_reports_metrics_and_layout() {
    let backend = RopeTextBackend::from_str("éz");
    assert_eq!(
        backend.debug_layout(),
        TextBackendDebugLayout::Rope(TextMetrics::from_usize(2, 3))
    );
    assert_eq!(rope_char_to_byte(&backend, 1), "é".len());
    assert_eq!(rope_byte_to_char(&backend, "é".len()), 1);
}

#[test]
fn rope_large_initial_text_uses_multiple_chunks_and_preserves_text() {
    let text = "a".repeat(MAX_LEAF_BYTES.get() * 2 + 17);
    let backend = RopeTextBackend::from_str(&text);
    backend.assert_invariants();
    let mut chunks = Vec::new();
    backend
        .for_each_emacs_byte_range_chunk(byte_range(0, backend.len()), |chunk| {
            chunks.push(chunk.len());
            Ok::<(), ()>(())
        })
        .unwrap();
    assert!(
        chunks.len() > 1,
        "large rope text should be represented by multiple chunks"
    );
    assert_eq!(backend.to_string(), text);
}

#[test]
fn rope_coalesces_adjacent_small_chunks_after_edits() {
    let mut backend = RopeTextBackend::from_str("abcdef");
    insert_rope_str(&mut backend, 3, "XY");
    assert_eq!(backend.debug_chunk_lengths(), vec![8]);

    delete_rope_range_both(&mut backend, 3, 5, 2);
    assert_eq!(backend.to_string(), "abcdef");
    assert_eq!(backend.debug_chunk_lengths(), vec![6]);
    backend.assert_invariants();
}

#[test]
fn rope_insert_delete_and_replace_match_gap_buffer() {
    let mut rope = RopeTextBackend::from_str("abécd日本");
    let mut gap = GapBuffer::from_str("abécd日本");
    assert_matches_gap(&rope, &gap);

    let pos = rope_char_to_byte(&rope, 2);
    insert_rope_str(&mut rope, pos, "XYZ");
    insert_gap_str(&mut gap, pos, "XYZ");
    assert_matches_gap(&rope, &gap);

    let start = rope_char_to_byte(&rope, 1);
    let end = rope_char_to_byte(&rope, 5);
    let nchars = rope_byte_to_char(&rope, end) - rope_byte_to_char(&rope, start);
    delete_rope_range_both(&mut rope, start, end, nchars);
    delete_gap_range_both(&mut gap, start, end, nchars);
    assert_matches_gap(&rope, &gap);

    let start = rope_char_to_byte(&rope, 1);
    let end = rope_char_to_byte(&rope, 2);
    replace_rope_same_len(&mut rope, start, end, "ß".as_bytes());
    replace_gap_same_len(&mut gap, start, end, "ß".as_bytes());
    assert_matches_gap(&rope, &gap);
}

#[test]
fn rope_unibyte_raw_bytes_round_trip() {
    let raw = vec![0xFF, b'A', 0x80];
    let mut backend = RopeTextBackend::from_emacs_bytes(&raw, false);
    insert_rope_bytes_both(&mut backend, 1, b"\n", 1);

    assert!(!backend.is_multibyte());
    assert_eq!(backend.metrics().chars_usize(), 4);
    assert_eq!(backend.metrics().emacs_bytes_usize(), 4);
    assert_eq!(rope_byte_to_char(&backend, 3), 3);
    assert_eq!(rope_char_to_byte(&backend, 4), 4);

    let mut bytes = Vec::new();
    backend.copy_emacs_byte_range_to(byte_range(0, backend.len()), &mut bytes);
    assert_eq!(bytes, vec![0xFF, b'\n', b'A', 0x80]);
}

proptest! {
    #[test]
    fn rope_random_edit_sequences_match_gap_buffer(
        ops in prop::collection::vec((0u8..3, 0usize..200, 0usize..200, 0u8..32), 0..80)
    ) {
        let mut rope = RopeTextBackend::from_str("abécd日本");
        let mut gap = GapBuffer::from_str("abécd日本");
        assert_matches_gap(&rope, &gap);

        for (kind, a, b, seed) in ops {
            match kind {
                0 => {
                    let char_pos = a % (rope.metrics().chars_usize() + 1);
                    let byte_pos = rope_char_to_byte(&rope, char_pos);
                    let text = sample_insert(seed);
                    insert_rope_str(&mut rope, byte_pos, text);
                    insert_gap_str(&mut gap, byte_pos, text);
                }
                1 => {
                    if rope.metrics().chars_usize() > 0 {
                        let char_a = a % (rope.metrics().chars_usize() + 1);
                        let char_b = b % (rope.metrics().chars_usize() + 1);
                        let start_char = char_a.min(char_b);
                        let end_char = char_a.max(char_b);
                        let start = rope_char_to_byte(&rope, start_char);
                        let end = rope_char_to_byte(&rope, end_char);
                        let nchars = end_char - start_char;
                        delete_rope_range_both(&mut rope, start, end, nchars);
                        delete_gap_range_both(&mut gap, start, end, nchars);
                    }
                }
                _ => {
                    if rope.metrics().chars_usize() > 0 {
                        let char_pos = a % rope.metrics().chars_usize();
                        let start = rope_char_to_byte(&rope, char_pos);
                        let end = rope_char_to_byte(&rope, char_pos + 1);
                        if let Some(replacement) = replacement_bytes_for_len(end - start, seed) {
                            replace_rope_same_len(&mut rope, start, end, &replacement);
                            replace_gap_same_len(&mut gap, start, end, &replacement);
                        }
                    }
                }
            }
            assert_matches_gap(&rope, &gap);
        }
    }
}

proptest! {
    #[test]
    fn rope_unibyte_random_edit_sequences_match_gap_buffer(
        ops in prop::collection::vec((0u8..3, 0usize..200, 0usize..200, any::<u8>()), 0..80)
    ) {
        let initial = vec![0xFF, b'A', 0x80, b'\n', b'Z'];
        let mut rope = RopeTextBackend::from_emacs_bytes(&initial, false);
        let mut gap = GapBuffer::from_emacs_bytes(&initial, false);
        assert_matches_gap(&rope, &gap);

        for (kind, a, b, seed) in ops {
            match kind {
                0 => {
                    let byte_pos = a % (rope.len() + 1);
                    let bytes = sample_unibyte_insert(seed);
                    insert_rope_bytes_both(&mut rope, byte_pos, &bytes, bytes.len());
                    insert_gap_bytes_both(&mut gap, byte_pos, &bytes, bytes.len());
                }
                1 => {
                    if !rope.is_empty() {
                        let byte_a = a % (rope.len() + 1);
                        let byte_b = b % (rope.len() + 1);
                        let start = byte_a.min(byte_b);
                        let end = byte_a.max(byte_b);
                        delete_rope_range_both(&mut rope, start, end, end - start);
                        delete_gap_range_both(&mut gap, start, end, end - start);
                    }
                }
                _ => {
                    if !rope.is_empty() {
                        let start = a % rope.len();
                        let end = (start + 1 + (b % 4)).min(rope.len());
                        let replacement = vec![seed; end - start];
                        replace_rope_same_len(&mut rope, start, end, &replacement);
                        replace_gap_same_len(&mut gap, start, end, &replacement);
                    }
                }
            }
            assert_matches_gap(&rope, &gap);
        }
    }
}
