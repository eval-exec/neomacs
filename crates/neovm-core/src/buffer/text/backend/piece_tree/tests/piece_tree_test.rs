use super::*;
use crate::buffer::gap_buffer::GapBuffer;
use proptest::prelude::*;

fn byte_range(start: usize, end: usize) -> EmacsByteRange {
    assert!(start <= end);
    EmacsByteRange::from_start_len(EmacsBytePos::new(start), EmacsByteLen::new(end - start))
}

fn assert_matches_gap(piece: &PieceTreeTextBackend, gap: &GapBuffer) {
    assert_eq!(piece.len(), gap.len());
    assert_eq!(piece.metrics().chars_usize(), gap.char_count());
    assert_eq!(piece.to_string(), gap.to_string());

    let mut piece_bytes = Vec::new();
    let mut gap_bytes = Vec::new();
    copy_piece_bytes(piece, 0, piece.len(), &mut piece_bytes);
    gap.copy_emacs_byte_range_to(byte_range(0, gap.len()), &mut gap_bytes);
    assert_eq!(piece_bytes, gap_bytes);

    for byte_pos in 0..piece.len() {
        assert_eq!(piece_byte_at(piece, byte_pos), gap_byte_at(gap, byte_pos));
        assert_eq!(
            piece_emacs_byte_at(piece, byte_pos),
            gap_emacs_byte_at(gap, byte_pos)
        );
    }
    assert_eq!(piece_emacs_byte_at(piece, piece.len()), None);
    assert_eq!(gap_emacs_byte_at(gap, gap.len()), None);

    for char_pos in 0..=piece.metrics().chars_usize() {
        let piece_byte = piece_char_to_byte(piece, char_pos);
        let gap_byte = gap_char_to_byte(gap, char_pos);
        assert_eq!(piece_byte, gap_byte, "char_to_byte({char_pos})");
        assert_eq!(piece_byte_to_char(piece, piece_byte), char_pos);
        assert_eq!(gap_byte_to_char(gap, gap_byte), char_pos);
        if char_pos < piece.metrics().chars_usize() {
            assert_eq!(
                piece_char_code_at(piece, piece_byte),
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

fn piece_byte_to_char(piece: &PieceTreeTextBackend, byte_pos: usize) -> usize {
    piece
        .emacs_byte_pos_to_char_pos(EmacsBytePos::new(byte_pos))
        .get()
}

fn piece_char_to_byte(piece: &PieceTreeTextBackend, char_pos: usize) -> usize {
    piece
        .char_pos_to_emacs_byte_pos(CharPos0::new(char_pos))
        .get()
}

fn piece_byte_at(piece: &PieceTreeTextBackend, byte_pos: usize) -> u8 {
    piece.byte_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
}

fn piece_emacs_byte_at(piece: &PieceTreeTextBackend, byte_pos: usize) -> Option<u8> {
    piece.emacs_byte_at_pos(EmacsBytePos::new(byte_pos))
}

fn piece_char_code_at(piece: &PieceTreeTextBackend, byte_pos: usize) -> Option<u32> {
    piece.char_code_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
}

fn copy_piece_bytes(piece: &PieceTreeTextBackend, start: usize, end: usize, out: &mut Vec<u8>) {
    piece.copy_emacs_byte_range_to(byte_range(start, end), out);
}

fn insert_piece_str(piece: &mut PieceTreeTextBackend, byte_pos: usize, text: &str) {
    let bytes =
        crate::emacs_core::string_escape::storage_string_to_buffer_bytes(text, piece.multibyte);
    let extent = TextExtent::from_emacs_bytes(&bytes, piece.multibyte);
    piece.insert_measured_emacs_bytes(EmacsBytePos::new(byte_pos), &bytes, extent);
}

fn insert_piece_bytes_both(
    piece: &mut PieceTreeTextBackend,
    byte_pos: usize,
    bytes: &[u8],
    nchars: usize,
) {
    piece.insert_measured_emacs_bytes(
        EmacsBytePos::new(byte_pos),
        bytes,
        TextExtent::from_usize(nchars, bytes.len()),
    );
}

fn delete_piece_range_both(
    piece: &mut PieceTreeTextBackend,
    start: usize,
    end: usize,
    nchars: usize,
) {
    let start_char = piece_byte_to_char(piece, start);
    piece.delete_measured_range(TextEditRange::from_usize(
        start,
        end,
        start_char,
        start_char + nchars,
    ));
}

fn replace_piece_same_len(
    piece: &mut PieceTreeTextBackend,
    start: usize,
    end: usize,
    replacement: &[u8],
) {
    let range = byte_range(start, end);
    let edit_range = TextEditRange::new(
        range,
        piece.emacs_byte_pos_to_char_pos(range.start()),
        piece.emacs_byte_pos_to_char_pos(range.end()),
    );
    piece.replace_same_len_measured_range(
        TextReplacement::new(
            edit_range,
            TextExtent::from_emacs_bytes(replacement, piece.multibyte),
        ),
        replacement,
    );
}

#[test]
fn piece_tree_reports_metrics_and_layout() {
    let backend = PieceTreeTextBackend::from_str("éz");
    assert_eq!(
        backend.debug_layout(),
        TextBackendDebugLayout::PieceTree(TextMetrics::from_usize(2, 3))
    );
    assert_eq!(piece_char_to_byte(&backend, 1), "é".len());
    assert_eq!(piece_byte_to_char(&backend, "é".len()), 1);
}

#[test]
fn piece_tree_insert_delete_and_replace_match_gap_buffer() {
    let mut piece = PieceTreeTextBackend::from_str("abécd日本");
    let mut gap = GapBuffer::from_str("abécd日本");
    assert_matches_gap(&piece, &gap);

    let pos = piece_char_to_byte(&piece, 2);
    insert_piece_str(&mut piece, pos, "XYZ");
    insert_gap_str(&mut gap, pos, "XYZ");
    assert_matches_gap(&piece, &gap);

    let start = piece_char_to_byte(&piece, 1);
    let end = piece_char_to_byte(&piece, 5);
    let nchars = piece_byte_to_char(&piece, end) - piece_byte_to_char(&piece, start);
    delete_piece_range_both(&mut piece, start, end, nchars);
    delete_gap_range_both(&mut gap, start, end, nchars);
    assert_matches_gap(&piece, &gap);

    let start = piece_char_to_byte(&piece, 1);
    let end = piece_char_to_byte(&piece, 2);
    replace_piece_same_len(&mut piece, start, end, "ß".as_bytes());
    replace_gap_same_len(&mut gap, start, end, "ß".as_bytes());
    assert_matches_gap(&piece, &gap);
}

#[test]
fn piece_tree_visits_piece_chunks_without_coalescing() {
    let mut backend = PieceTreeTextBackend::from_str("abcdef");
    insert_piece_str(&mut backend, 3, "XY");
    delete_piece_range_both(&mut backend, 4, 5, 1);

    let mut chunks = Vec::new();
    backend
        .for_each_emacs_byte_range_chunk(byte_range(1, 7), |chunk| {
            chunks.push(chunk.to_vec());
            Ok::<(), ()>(())
        })
        .unwrap();
    assert_eq!(chunks, vec![b"bc".to_vec(), b"X".to_vec(), b"def".to_vec()]);
}

#[test]
fn piece_tree_unibyte_raw_bytes_round_trip() {
    let raw = vec![0xFF, b'A', 0x80];
    let mut backend = PieceTreeTextBackend::from_emacs_bytes(&raw, false);
    insert_piece_bytes_both(&mut backend, 1, b"\n", 1);

    assert!(!backend.is_multibyte());
    assert_eq!(backend.metrics().chars_usize(), 4);
    assert_eq!(backend.metrics().emacs_bytes_usize(), 4);
    assert_eq!(piece_byte_to_char(&backend, 3), 3);
    assert_eq!(piece_char_to_byte(&backend, 4), 4);

    let mut bytes = Vec::new();
    copy_piece_bytes(&backend, 0, backend.len(), &mut bytes);
    assert_eq!(bytes, vec![0xFF, b'\n', b'A', 0x80]);
}

proptest! {
    #[test]
    fn piece_tree_random_edit_sequences_match_gap_buffer(
        ops in prop::collection::vec((0u8..3, 0usize..200, 0usize..200, 0u8..32), 0..80)
    ) {
        let mut piece = PieceTreeTextBackend::from_str("abécd日本");
        let mut gap = GapBuffer::from_str("abécd日本");
        assert_matches_gap(&piece, &gap);

        for (kind, a, b, seed) in ops {
            match kind {
                0 => {
                    let char_pos = a % (piece.metrics().chars_usize() + 1);
                    let byte_pos = piece_char_to_byte(&piece, char_pos);
                    let text = sample_insert(seed);
                    insert_piece_str(&mut piece, byte_pos, text);
                    insert_gap_str(&mut gap, byte_pos, text);
                }
                1 => {
                    if piece.metrics().chars_usize() > 0 {
                        let char_a = a % (piece.metrics().chars_usize() + 1);
                        let char_b = b % (piece.metrics().chars_usize() + 1);
                        let start_char = char_a.min(char_b);
                        let end_char = char_a.max(char_b);
                        let start = piece_char_to_byte(&piece, start_char);
                        let end = piece_char_to_byte(&piece, end_char);
                        let nchars = end_char - start_char;
                        delete_piece_range_both(&mut piece, start, end, nchars);
                        delete_gap_range_both(&mut gap, start, end, nchars);
                    }
                }
                _ => {
                    if piece.metrics().chars_usize() > 0 {
                        let char_pos = a % piece.metrics().chars_usize();
                        let start = piece_char_to_byte(&piece, char_pos);
                        let end = piece_char_to_byte(&piece, char_pos + 1);
                        if let Some(replacement) = replacement_bytes_for_len(end - start, seed) {
                            replace_piece_same_len(&mut piece, start, end, &replacement);
                            replace_gap_same_len(&mut gap, start, end, &replacement);
                        }
                    }
                }
            }
            assert_matches_gap(&piece, &gap);
        }
    }
}

proptest! {
    #[test]
    fn piece_tree_unibyte_random_edit_sequences_match_gap_buffer(
        ops in prop::collection::vec((0u8..3, 0usize..200, 0usize..200, any::<u8>()), 0..80)
    ) {
        let initial = vec![0xFF, b'A', 0x80, b'\n', b'Z'];
        let mut piece = PieceTreeTextBackend::from_emacs_bytes(&initial, false);
        let mut gap = GapBuffer::from_emacs_bytes(&initial, false);
        assert_matches_gap(&piece, &gap);

        for (kind, a, b, seed) in ops {
            match kind {
                0 => {
                    let byte_pos = a % (piece.len() + 1);
                    let bytes = sample_unibyte_insert(seed);
                    insert_piece_bytes_both(&mut piece, byte_pos, &bytes, bytes.len());
                    insert_gap_bytes_both(&mut gap, byte_pos, &bytes, bytes.len());
                }
                1 => {
                    if !piece.is_empty() {
                        let byte_a = a % (piece.len() + 1);
                        let byte_b = b % (piece.len() + 1);
                        let start = byte_a.min(byte_b);
                        let end = byte_a.max(byte_b);
                        delete_piece_range_both(&mut piece, start, end, end - start);
                        delete_gap_range_both(&mut gap, start, end, end - start);
                    }
                }
                _ => {
                    if !piece.is_empty() {
                        let start = a % piece.len();
                        let end = (start + 1 + (b % 4)).min(piece.len());
                        let replacement = vec![seed; end - start];
                        replace_piece_same_len(&mut piece, start, end, &replacement);
                        replace_gap_same_len(&mut gap, start, end, &replacement);
                    }
                }
            }
            assert_matches_gap(&piece, &gap);
        }
    }
}
