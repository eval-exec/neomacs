//! The text line index against a naive model: random texts, random edits,
//! every query compared with a recount.

use super::*;

/// A small deterministic generator (xorshift64*), so a failing seed replays.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub(crate) fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// Characters a random text is made of: ASCII, line ends, two- and
/// three-byte characters and a raw eight-bit byte in its C1 form.
const MULTIBYTE_PIECES: &[&[u8]] = &[
    b"a",
    b"b",
    b" ",
    b"\n",
    b"\n",
    b"\r",
    b"\r\n",
    "\u{e9}".as_bytes(),
    "\u{4e2d}".as_bytes(),
    &[0xC1, 0x80],
];

pub(crate) fn random_bytes(rng: &mut Rng, chars: usize, multibyte: bool) -> Vec<u8> {
    let mut out = Vec::new();
    for _ in 0..chars {
        if multibyte {
            out.extend_from_slice(MULTIBYTE_PIECES[rng.below(MULTIBYTE_PIECES.len())]);
        } else {
            out.push(match rng.below(8) {
                0 | 1 => b'\n',
                2 => b'\r',
                3 => 0xE9,
                _ => b'a' + rng.below(26) as u8,
            });
        }
    }
    out
}

/// The char-boundary byte positions of BYTES (every position when unibyte).
pub(crate) fn boundaries(bytes: &[u8], multibyte: bool) -> Vec<usize> {
    (0..=bytes.len())
        .filter(|&i| !multibyte || i == bytes.len() || (bytes[i] & 0xC0) != 0x80)
        .collect()
}

fn naive_line_ends(bytes: &[u8], end: usize, line_end: LineEnd) -> u64 {
    bytes[..end]
        .iter()
        .filter(|&&b| b == b'\n' || (line_end == LineEnd::NewlineOrCarriageReturn && b == b'\r'))
        .count() as u64
}

fn naive_newline_position(bytes: &[u8], t: u64) -> Option<usize> {
    if t == 0 {
        return None;
    }
    bytes
        .iter()
        .enumerate()
        .filter(|&(_, &b)| b == b'\n')
        .nth((t - 1) as usize)
        .map(|(i, _)| i)
}

fn plain(bytes: &[u8], multibyte: bool, piece: usize) -> PlainText<'_> {
    PlainText {
        bytes,
        multibyte,
        piece,
    }
}

/// Every query on INDEX against the naive answers for BYTES.
fn check_queries(index: &TextLineIndex, bytes: &[u8], multibyte: bool, piece: usize) {
    let text = plain(bytes, multibyte, piece);
    index
        .check_all(&text)
        .unwrap_or_else(|err| panic!("index invalid: {err}"));
    let step = (bytes.len() / 97).max(1);
    for b in (0..=bytes.len()).step_by(step).chain([bytes.len()]) {
        for line_end in [LineEnd::Newline, LineEnd::NewlineOrCarriageReturn] {
            assert_eq!(
                index.line_ends_before(&text, b, line_end),
                naive_line_ends(bytes, b, line_end),
                "line ends before {b} ({line_end:?})"
            );
        }
    }
    let newlines = naive_line_ends(bytes, bytes.len(), LineEnd::Newline);
    let step = (newlines / 53).max(1);
    for t in (0..=newlines + 1)
        .step_by(step as usize)
        .chain([newlines, newlines + 1])
    {
        assert_eq!(
            index.newline_position(&text, t),
            naive_newline_position(bytes, t),
            "newline {t}"
        );
    }
}

#[test]
fn build_counts_bytes_chars_newlines_and_carriage_returns() {
    crate::test_utils::init_test_tracing();
    let bytes = "a\u{e9}\nb\r\n\u{4e2d}\n".as_bytes();
    for chunk in [8, 9, 64] {
        for piece in [1, 3, 1000] {
            let text = plain(bytes, true, piece);
            let index = TextLineIndex::build(&text, chunk);
            assert_eq!(
                index.total(),
                LineSums {
                    bytes: bytes.len() as u64,
                    chars: 8,
                    newlines: 3,
                    crs: 1,
                }
            );
            check_queries(&index, bytes, true, piece);
        }
    }
    let unibyte = [b'a', 0xE9, b'\n', 0x80, b'\r'];
    let index = TextLineIndex::build(&plain(&unibyte, false, 2), 8);
    assert_eq!(index.total().chars, 5);
    assert_eq!(index.total().newlines, 1);
    assert_eq!(index.total().crs, 1);
}

#[test]
fn empty_text_builds_an_empty_index_that_accepts_insertions() {
    crate::test_utils::init_test_tracing();
    let mut bytes = Vec::new();
    let mut index = TextLineIndex::build(&plain(&bytes, true, 4), 8);
    assert_eq!(index.chunk_count(), 0);
    assert_eq!(
        index.line_ends_before(&plain(&bytes, true, 4), 0, LineEnd::Newline),
        0
    );
    assert_eq!(index.newline_position(&plain(&bytes, true, 4), 1), None);
    let inserted = b"x\ny\n";
    bytes.extend_from_slice(inserted);
    index.note_insert(&plain(&bytes, true, 4), 0, inserted);
    check_queries(&index, &bytes, true, 4);
    // Delete everything again.
    index.note_delete(&plain(&bytes, true, 4), 0, bytes.len());
    bytes.clear();
    assert_eq!(index.chunk_count(), 0);
    check_queries(&index, &bytes, true, 4);
}

#[test]
fn chunks_split_above_twice_the_target_and_merge_below_a_quarter() {
    crate::test_utils::init_test_tracing();
    let mut bytes = b"0123456789\n".repeat(20);
    let mut index = TextLineIndex::build(&plain(&bytes, false, 7), 16);
    let before = index.chunk_count();
    // Grow one chunk past 32 bytes: it splits.
    let inserted = b"abcdefghijklmnopqrstuvwxyz\n\n";
    bytes.splice(5..5, inserted.iter().copied());
    index.note_insert(&plain(&bytes, false, 7), 5, inserted);
    assert!(index.chunk_count() > before, "a split adds chunks");
    check_queries(&index, &bytes, false, 7);
    // Shrink a chunk below 4 bytes: it merges.
    let count = index.chunk_count();
    let (k, before_k) = index.locate_byte(100);
    let start = before_k.bytes as usize;
    let len = index.chunks[k].bytes as usize;
    index.note_delete(&plain(&bytes, false, 7), start, start + len - 2);
    bytes.drain(start..start + len - 2);
    assert!(index.chunk_count() < count, "a merge removes a chunk");
    check_queries(&index, &bytes, false, 7);
}

#[test]
fn a_deletion_across_many_chunks_keeps_the_partial_ends() {
    crate::test_utils::init_test_tracing();
    let mut rng = Rng::new(7);
    let mut bytes = random_bytes(&mut rng, 400, true);
    let mut index = TextLineIndex::build(&plain(&bytes, true, 13), 16);
    let marks = boundaries(&bytes, true);
    let start = marks[37];
    let end = marks[marks.len() - 41];
    index.note_delete(&plain(&bytes, true, 13), start, end);
    bytes.drain(start..end);
    check_queries(&index, &bytes, true, 13);
}

/// Random insertions, deletions and replacements, each checked in full.
fn random_edit_run(seed: u64, multibyte: bool, chunk: usize, piece: usize, steps: usize) {
    let mut rng = Rng::new(seed);
    let initial = rng.below(300);
    let mut bytes = random_bytes(&mut rng, initial, multibyte);
    let mut index = TextLineIndex::build(&plain(&bytes, multibyte, piece), chunk);
    check_queries(&index, &bytes, multibyte, piece);
    for step in 0..steps {
        let marks = boundaries(&bytes, multibyte);
        let a = marks[rng.below(marks.len())];
        let b = marks[rng.below(marks.len())];
        let (start, end) = (a.min(b), a.max(b));
        // Mostly short edits, as typing makes, and sometimes long ones.
        let len = if rng.below(4) == 0 {
            rng.below(80)
        } else {
            rng.below(3)
        };
        let inserted = random_bytes(&mut rng, len, multibyte);
        match rng.below(3) {
            0 => {
                bytes.splice(start..start, inserted.iter().copied());
                index.note_insert(&plain(&bytes, multibyte, piece), start, &inserted);
            }
            1 => {
                index.note_delete(&plain(&bytes, multibyte, piece), start, end);
                bytes.drain(start..end);
            }
            _ => {
                index.note_delete(&plain(&bytes, multibyte, piece), start, end);
                bytes.splice(start..end, inserted.iter().copied());
                index.note_insert(&plain(&bytes, multibyte, piece), start, &inserted);
            }
        }
        let text = plain(&bytes, multibyte, piece);
        index
            .check_around(&text, start)
            .unwrap_or_else(|err| panic!("seed {seed} step {step}: {err}"));
        if step % 7 == 0 || step + 1 == steps {
            check_queries(&index, &bytes, multibyte, piece);
        }
    }
}

#[test]
fn random_edits_keep_the_index_equal_to_a_recount() {
    crate::test_utils::init_test_tracing();
    for seed in 0..24 {
        let multibyte = seed % 3 != 0;
        let chunk = [8, 16, 64][seed as usize % 3];
        let piece = [1, 5, 4096][(seed as usize / 3) % 3];
        random_edit_run(seed, multibyte, chunk, piece, 300);
    }
}

#[test]
fn config_parses_modes_sizes_and_defaults_to_off() {
    crate::test_utils::init_test_tracing();
    let none = |_: &str| None;
    assert_eq!(
        TextLineIndexConfig::parse(none),
        TextLineIndexConfig::with_mode(TextLineIndexMode::Off)
    );
    assert_eq!(
        TextLineIndexMode::parse(Some(" Verify ")),
        TextLineIndexMode::Verify
    );
    assert_eq!(TextLineIndexMode::parse(Some("1")), TextLineIndexMode::On);
    assert_eq!(TextLineIndexMode::parse(Some("on")), TextLineIndexMode::On);
    assert_eq!(TextLineIndexMode::parse(Some("0")), TextLineIndexMode::Off);
    assert_eq!(
        TextLineIndexMode::parse(Some("bogus")),
        TextLineIndexMode::Off
    );
    let env = |name: &str| {
        match name {
            "NEOVM_TEXT_LINE_INDEX" => Some("verify"),
            "NEOVM_TEXT_LINE_INDEX_MIN_BYTES" => Some("0"),
            "NEOVM_TEXT_LINE_INDEX_CHUNK" => Some("2"),
            "NEOVM_TEXT_LINE_INDEX_QUERY_BYTES" => Some("17"),
            "NEOVM_TEXT_LINE_INDEX_QUERY_LINES" => Some("junk"),
            "NEOVM_TEXT_LINE_INDEX_STATS" => Some("1"),
            _ => None,
        }
        .map(str::to_owned)
    };
    assert_eq!(
        TextLineIndexConfig::parse(env),
        TextLineIndexConfig {
            mode: TextLineIndexMode::Verify,
            min_buffer_bytes: 0,
            // Clamped up to the smallest target.
            chunk_bytes: TextLineIndexConfig::MIN_CHUNK_BYTES,
            min_query_bytes: 17,
            min_query_lines: TextLineIndexConfig::DEFAULT_MIN_QUERY_LINES,
            stats: true,
        }
    );
}
