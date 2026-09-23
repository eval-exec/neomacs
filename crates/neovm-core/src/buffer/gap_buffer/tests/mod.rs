use super::*;
use crate::buffer::{CharLen, CharPos0, EmacsBytePos, EmacsByteRange};

fn byte_to_char(buf: &GapBuffer, byte_pos: usize) -> usize {
    buf.emacs_byte_pos_to_char_pos(EmacsBytePos::new(byte_pos))
        .get()
}

fn char_to_byte(buf: &GapBuffer, char_pos: usize) -> usize {
    buf.char_pos_to_emacs_byte_pos(CharPos0::new(char_pos))
        .get()
}

fn byte_at(buf: &GapBuffer, byte_pos: usize) -> u8 {
    buf.byte_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
}

fn char_at(buf: &GapBuffer, byte_pos: usize) -> Option<char> {
    buf.char_at_emacs_byte_pos(EmacsBytePos::new(byte_pos))
}

fn gap_text_range(buf: &GapBuffer, start: usize, end: usize) -> String {
    buf.text_emacs_byte_range(EmacsByteRange::from_usize(start, end))
}

fn gap_copy_bytes_to(buf: &GapBuffer, start: usize, end: usize, out: &mut Vec<u8>) {
    buf.copy_emacs_byte_range_to(EmacsByteRange::from_usize(start, end), out);
}

fn gap_insert_emacs_bytes(buf: &mut GapBuffer, byte_pos: usize, bytes: &[u8]) {
    buf.insert_emacs_bytes_at_emacs_byte_pos(EmacsBytePos::new(byte_pos), bytes);
}

fn gap_insert_emacs_bytes_with_char_len(
    buf: &mut GapBuffer,
    byte_pos: usize,
    bytes: &[u8],
    nchars: usize,
) {
    buf.insert_emacs_bytes_at_emacs_byte_pos_with_char_len(
        EmacsBytePos::new(byte_pos),
        bytes,
        CharLen::new(nchars),
    );
}

fn gap_insert_storage_string(buf: &mut GapBuffer, byte_pos: usize, text: &str) {
    buf.insert_storage_string_at_emacs_byte_pos(EmacsBytePos::new(byte_pos), text);
}

fn gap_delete_emacs_range(buf: &mut GapBuffer, start: usize, end: usize) {
    buf.delete_emacs_byte_range(EmacsByteRange::from_usize(start, end));
}

fn gap_delete_emacs_range_with_char_len(
    buf: &mut GapBuffer,
    start: usize,
    end: usize,
    nchars: usize,
) {
    buf.delete_emacs_byte_range_with_char_len(
        EmacsByteRange::from_usize(start, end),
        CharLen::new(nchars),
    );
}

fn gap_replace_same_len_emacs_bytes(
    buf: &mut GapBuffer,
    start: usize,
    end: usize,
    replacement: &[u8],
) {
    buf.replace_same_len_emacs_byte_range(EmacsByteRange::from_usize(start, end), replacement);
}

fn gap_move_to_emacs_byte_and_char(buf: &mut GapBuffer, byte_pos: usize, char_pos: usize) {
    buf.move_gap_to_emacs_byte_pos_and_char_pos(
        EmacsBytePos::new(byte_pos),
        CharPos0::new(char_pos),
    );
}

// -----------------------------------------------------------------------
// Construction & basic queries
// -----------------------------------------------------------------------

#[test]
fn new_buffer_is_empty() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::new();
    assert_eq!(buf.len(), 0);
    assert!(buf.is_empty());
    assert_eq!(buf.char_count(), 0);
    assert_eq!(buf.to_string(), "");
}

#[test]
fn from_str_ascii() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hello");
    assert_eq!(buf.len(), 5);
    assert_eq!(buf.char_count(), 5);
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn from_str_empty() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("");
    assert_eq!(buf.len(), 0);
    assert!(buf.is_empty());
    assert_eq!(buf.to_string(), "");
}

// -----------------------------------------------------------------------
// insert_str
// -----------------------------------------------------------------------

#[test]
fn insert_at_beginning() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("world");
    gap_insert_storage_string(&mut buf, 0, "hello ");
    assert_eq!(buf.to_string(), "hello world");
}

#[test]
fn insert_at_end() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello");
    gap_insert_storage_string(&mut buf, 5, " world");
    assert_eq!(buf.to_string(), "hello world");
}

#[test]
fn insert_in_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("helo");
    gap_insert_storage_string(&mut buf, 2, "l");
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn insert_into_empty_buffer() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::new();
    gap_insert_storage_string(&mut buf, 0, "abc");
    assert_eq!(buf.to_string(), "abc");
    assert_eq!(buf.len(), 3);
}

#[test]
fn insert_empty_string_is_noop() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello");
    gap_insert_storage_string(&mut buf, 3, "");
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn multiple_sequential_inserts() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::new();
    gap_insert_storage_string(&mut buf, 0, "a");
    gap_insert_storage_string(&mut buf, 1, "b");
    gap_insert_storage_string(&mut buf, 2, "c");
    gap_insert_storage_string(&mut buf, 3, "d");
    assert_eq!(buf.to_string(), "abcd");
}

#[test]
fn insert_larger_than_gap() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::new();
    let long = "x".repeat(256);
    gap_insert_storage_string(&mut buf, 0, &long);
    assert_eq!(buf.to_string(), long);
    assert_eq!(buf.len(), 256);
}

// -----------------------------------------------------------------------
// delete_range
// -----------------------------------------------------------------------

#[test]
fn delete_from_beginning() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello world");
    gap_delete_emacs_range(&mut buf, 0, 6);
    assert_eq!(buf.to_string(), "world");
}

#[test]
fn delete_from_end() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello world");
    gap_delete_emacs_range(&mut buf, 5, 11);
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn delete_from_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello world");
    gap_delete_emacs_range(&mut buf, 5, 6); // delete the space
    assert_eq!(buf.to_string(), "helloworld");
}

#[test]
fn delete_everything() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello");
    gap_delete_emacs_range(&mut buf, 0, 5);
    assert_eq!(buf.to_string(), "");
    assert!(buf.is_empty());
}

#[test]
fn delete_empty_range_is_noop() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello");
    gap_delete_emacs_range(&mut buf, 2, 2);
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn delete_then_insert() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello world");
    gap_delete_emacs_range(&mut buf, 5, 11);
    gap_insert_storage_string(&mut buf, 5, " rust");
    assert_eq!(buf.to_string(), "hello rust");
}

// -----------------------------------------------------------------------
// replace_same_len_emacs_bytes
// -----------------------------------------------------------------------

#[test]
fn replace_same_len_preserves_gap_position_before_and_after_gap() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("abécd日本");
    let gap_byte = char_to_byte(&buf, 3);
    buf.move_gap_to(gap_byte);
    let gap = (buf.gpt(), buf.gpt_byte());

    let before_start = char_to_byte(&buf, 2);
    let before_end = char_to_byte(&buf, 3);
    gap_replace_same_len_emacs_bytes(&mut buf, before_start, before_end, "ß".as_bytes());

    assert_eq!(buf.to_string(), "abßcd日本");
    assert_eq!((buf.gpt(), buf.gpt_byte()), gap);
    assert_eq!(buf.char_count(), 7);
    assert_eq!(buf.emacs_byte_len(), "abßcd日本".len());

    let after_start = char_to_byte(&buf, 5);
    let after_end = char_to_byte(&buf, 6);
    gap_replace_same_len_emacs_bytes(&mut buf, after_start, after_end, "界".as_bytes());

    assert_eq!(buf.to_string(), "abßcd界本");
    assert_eq!((buf.gpt(), buf.gpt_byte()), gap);
    assert_eq!(buf.char_count(), 7);
    assert_eq!(buf.emacs_byte_len(), "abßcd界本".len());
}

#[test]
fn replace_same_len_preserves_gap_position_when_range_straddles_gap() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("abécd日本");
    let gap_byte = char_to_byte(&buf, 3);
    buf.move_gap_to(gap_byte);
    let gap = (buf.gpt(), buf.gpt_byte());

    let start = char_to_byte(&buf, 2);
    let end = char_to_byte(&buf, 4);
    gap_replace_same_len_emacs_bytes(&mut buf, start, end, "ßx".as_bytes());

    assert_eq!(buf.to_string(), "abßxd日本");
    assert_eq!((buf.gpt(), buf.gpt_byte()), gap);
    assert_eq!(buf.char_count(), 7);
    assert_eq!(buf.emacs_byte_len(), "abßxd日本".len());
}

// -----------------------------------------------------------------------
// byte_at / char_at
// -----------------------------------------------------------------------

#[test]
fn byte_at_ascii() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("abcde");
    assert_eq!(byte_at(&buf, 0), b'a');
    assert_eq!(byte_at(&buf, 4), b'e');
}

#[test]
fn byte_at_after_gap_move() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("abcde");
    buf.move_gap_to(2);
    // Logical content unchanged.
    assert_eq!(byte_at(&buf, 0), b'a');
    assert_eq!(byte_at(&buf, 2), b'c');
    assert_eq!(byte_at(&buf, 4), b'e');
}

#[test]
fn char_at_ascii() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hello");
    assert_eq!(char_at(&buf, 0), Some('h'));
    assert_eq!(char_at(&buf, 4), Some('o'));
    assert_eq!(char_at(&buf, 5), None);
}

#[test]
#[should_panic]
fn byte_at_out_of_range_panics() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hi");
    byte_at(&buf, 2);
}

// -----------------------------------------------------------------------
// text_range
// -----------------------------------------------------------------------

#[test]
fn text_range_full() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hello world");
    assert_eq!(gap_text_range(&buf, 0, 11), "hello world");
}

#[test]
fn text_range_prefix() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hello world");
    assert_eq!(gap_text_range(&buf, 0, 5), "hello");
}

#[test]
fn text_range_suffix() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hello world");
    assert_eq!(gap_text_range(&buf, 6, 11), "world");
}

#[test]
fn text_range_spanning_gap() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello world");
    buf.move_gap_to(5);
    // Range spans the gap.
    assert_eq!(gap_text_range(&buf, 3, 8), "lo wo");
}

#[test]
fn text_range_empty() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hello");
    assert_eq!(gap_text_range(&buf, 2, 2), "");
}

// -----------------------------------------------------------------------
// move_gap_to
// -----------------------------------------------------------------------

#[test]
fn move_gap_to_start() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello");
    buf.move_gap_to(0);
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn move_gap_to_end() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello");
    buf.move_gap_to(5);
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn move_gap_around() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("abcdef");
    buf.move_gap_to(3);
    assert_eq!(buf.to_string(), "abcdef");
    buf.move_gap_to(0);
    assert_eq!(buf.to_string(), "abcdef");
    buf.move_gap_to(6);
    assert_eq!(buf.to_string(), "abcdef");
    buf.move_gap_to(2);
    assert_eq!(buf.to_string(), "abcdef");
}

// -----------------------------------------------------------------------
// ensure_gap
// -----------------------------------------------------------------------

#[test]
fn ensure_gap_grows() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello");
    let old_gap = buf.gap_size();
    buf.ensure_gap(old_gap + 100);
    assert!(buf.gap_size() >= old_gap + 100);
    // Content must be preserved.
    assert_eq!(buf.to_string(), "hello");
}

#[test]
fn ensure_gap_noop_when_large_enough() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello");
    let old_gap = buf.gap_size();
    buf.ensure_gap(1);
    assert_eq!(buf.gap_size(), old_gap);
}

// -----------------------------------------------------------------------
// Multibyte / UTF-8 (CJK, emoji)
// -----------------------------------------------------------------------

#[test]
fn multibyte_cjk() {
    crate::test_utils::init_test_tracing();
    // Each CJK character is 3 bytes in UTF-8.
    let text = "\u{4F60}\u{597D}\u{4E16}\u{754C}"; // 你好世界
    let buf = GapBuffer::from_str(text);
    assert_eq!(buf.len(), 12); // 4 chars * 3 bytes
    assert_eq!(buf.char_count(), 4);
    assert_eq!(buf.to_string(), text);

    // char_at at byte boundaries
    assert_eq!(char_at(&buf, 0), Some('\u{4F60}')); // 你
    assert_eq!(char_at(&buf, 3), Some('\u{597D}')); // 好
    assert_eq!(char_at(&buf, 6), Some('\u{4E16}')); // 世
    assert_eq!(char_at(&buf, 9), Some('\u{754C}')); // 界
}

#[test]
fn multibyte_emoji() {
    crate::test_utils::init_test_tracing();
    // Emoji are 4 bytes in UTF-8.
    let text = "\u{1F600}\u{1F60D}"; // two emoji
    let buf = GapBuffer::from_str(text);
    assert_eq!(buf.len(), 8);
    assert_eq!(buf.char_count(), 2);
    assert_eq!(char_at(&buf, 0), Some('\u{1F600}'));
    assert_eq!(char_at(&buf, 4), Some('\u{1F60D}'));
}

#[test]
fn insert_multibyte_in_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("ab");
    gap_insert_storage_string(&mut buf, 1, "\u{1F600}"); // insert emoji between a and b
    assert_eq!(buf.to_string(), "a\u{1F600}b");
    assert_eq!(buf.len(), 6); // 1 + 4 + 1
    assert_eq!(buf.char_count(), 3);
}

#[test]
fn delete_multibyte_char() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("a\u{4F60}b"); // a你b
    // Delete the CJK char (bytes 1..4).
    gap_delete_emacs_range(&mut buf, 1, 4);
    assert_eq!(buf.to_string(), "ab");
}

#[test]
fn text_range_multibyte_spanning_gap() {
    crate::test_utils::init_test_tracing();
    let text = "\u{4F60}\u{597D}\u{4E16}\u{754C}"; // 你好世界
    let mut buf = GapBuffer::from_str(text);
    buf.move_gap_to(6); // gap between 好 and 世
    assert_eq!(gap_text_range(&buf, 0, 6), "\u{4F60}\u{597D}");
    assert_eq!(gap_text_range(&buf, 6, 12), "\u{4E16}\u{754C}");
    assert_eq!(gap_text_range(&buf, 3, 9), "\u{597D}\u{4E16}");
}

#[test]
fn mixed_ascii_and_multibyte() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello\u{4E16}\u{754C}!");
    // "hello世界!" — 5 + 3 + 3 + 1 = 12 bytes, 8 chars
    assert_eq!(buf.len(), 12);
    assert_eq!(buf.char_count(), 8);

    gap_insert_storage_string(&mut buf, 5, " ");
    assert_eq!(buf.to_string(), "hello \u{4E16}\u{754C}!");
    assert_eq!(buf.len(), 13);

    gap_delete_emacs_range(&mut buf, 6, 12); // delete "世界"
    assert_eq!(buf.to_string(), "hello !");
}

// -----------------------------------------------------------------------
// byte_to_char / char_to_byte
// -----------------------------------------------------------------------

#[test]
fn byte_char_roundtrip_ascii() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hello");
    for i in 0..=5 {
        assert_eq!(byte_to_char(&buf, i), i);
        assert_eq!(char_to_byte(&buf, i), i);
    }
}

#[test]
fn byte_to_char_cjk() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("\u{4F60}\u{597D}\u{4E16}"); // 你好世
    assert_eq!(byte_to_char(&buf, 0), 0);
    assert_eq!(byte_to_char(&buf, 3), 1);
    assert_eq!(byte_to_char(&buf, 6), 2);
    assert_eq!(byte_to_char(&buf, 9), 3);
}

#[test]
fn char_to_byte_cjk() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("\u{4F60}\u{597D}\u{4E16}"); // 你好世
    assert_eq!(char_to_byte(&buf, 0), 0);
    assert_eq!(char_to_byte(&buf, 1), 3);
    assert_eq!(char_to_byte(&buf, 2), 6);
    assert_eq!(char_to_byte(&buf, 3), 9);
}

#[test]
fn byte_char_roundtrip_mixed() {
    crate::test_utils::init_test_tracing();
    // "a你b" — byte offsets: a=0, 你=1..4, b=4
    let buf = GapBuffer::from_str("a\u{4F60}b");
    assert_eq!(byte_to_char(&buf, 0), 0); // before 'a'
    assert_eq!(byte_to_char(&buf, 1), 1); // before '你'
    assert_eq!(byte_to_char(&buf, 4), 2); // before 'b'
    assert_eq!(byte_to_char(&buf, 5), 3); // end

    assert_eq!(char_to_byte(&buf, 0), 0);
    assert_eq!(char_to_byte(&buf, 1), 1);
    assert_eq!(char_to_byte(&buf, 2), 4);
    assert_eq!(char_to_byte(&buf, 3), 5);
}

#[test]
fn byte_char_conversion_with_gap_in_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("a\u{4F60}b\u{597D}c");
    // Move gap to middle of the text.
    buf.move_gap_to(4); // between 你 and b
    // Conversions should be unaffected by gap position.
    assert_eq!(byte_to_char(&buf, 0), 0);
    assert_eq!(byte_to_char(&buf, 1), 1);
    assert_eq!(byte_to_char(&buf, 4), 2);
    assert_eq!(byte_to_char(&buf, 5), 3);
    assert_eq!(byte_to_char(&buf, 8), 4);

    assert_eq!(char_to_byte(&buf, 0), 0);
    assert_eq!(char_to_byte(&buf, 1), 1);
    assert_eq!(char_to_byte(&buf, 2), 4);
    assert_eq!(char_to_byte(&buf, 3), 5);
    assert_eq!(char_to_byte(&buf, 4), 8);
}

#[test]
fn byte_char_conversion_empty() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::new();
    assert_eq!(byte_to_char(&buf, 0), 0);
    assert_eq!(char_to_byte(&buf, 0), 0);
}

#[test]
fn byte_to_char_emoji() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("x\u{1F600}y"); // x😀y
    // byte offsets: x=0, 😀=1..5, y=5
    assert_eq!(byte_to_char(&buf, 0), 0);
    assert_eq!(byte_to_char(&buf, 1), 1);
    assert_eq!(byte_to_char(&buf, 5), 2);
    assert_eq!(byte_to_char(&buf, 6), 3);
}

#[test]
fn byte_char_conversion_unibyte_storage_sentinels() {
    crate::test_utils::init_test_tracing();
    let storage =
        crate::emacs_core::string_escape::bytes_to_unibyte_storage_string(&[0x80, b'A', 0xFF]);
    let buf = GapBuffer::from_str(&storage);
    assert_eq!(buf.char_count(), 3);
    assert_eq!(char_to_byte(&buf, 0), 0);
    assert_eq!(char_to_byte(&buf, 1), 1);
    assert_eq!(char_to_byte(&buf, 2), 2);
    assert_eq!(char_to_byte(&buf, 3), 3);
    assert_eq!(byte_to_char(&buf, 0), 0);
    assert_eq!(byte_to_char(&buf, 1), 1);
    assert_eq!(byte_to_char(&buf, 2), 2);
    assert_eq!(byte_to_char(&buf, 3), 3);
}

#[test]
fn byte_char_conversion_unibyte_storage_sentinels_after_gap_move() {
    crate::test_utils::init_test_tracing();
    let storage =
        crate::emacs_core::string_escape::bytes_to_unibyte_storage_string(&[0x80, b'A', 0xFF]);
    let mut buf = GapBuffer::from_str(&storage);
    buf.move_gap_to(2);
    assert_eq!(buf.char_count(), 3);
    assert_eq!(char_to_byte(&buf, 0), 0);
    assert_eq!(char_to_byte(&buf, 1), 1);
    assert_eq!(char_to_byte(&buf, 2), 2);
    assert_eq!(char_to_byte(&buf, 3), 3);
    assert_eq!(byte_to_char(&buf, 0), 0);
    assert_eq!(byte_to_char(&buf, 1), 1);
    assert_eq!(byte_to_char(&buf, 2), 2);
    assert_eq!(byte_to_char(&buf, 3), 3);
}

// -----------------------------------------------------------------------
// Edge cases
// -----------------------------------------------------------------------

#[test]
fn repeated_insert_delete_cycle() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::new();
    for i in 0..100 {
        let s = format!("{i}");
        let end = buf.len();
        gap_insert_storage_string(&mut buf, end, &s);
    }
    let full = buf.to_string();
    assert!(!full.is_empty());

    // Delete everything one byte at a time from the front.
    while !buf.is_empty() {
        gap_delete_emacs_range(&mut buf, 0, 1);
    }
    assert!(buf.is_empty());
    assert_eq!(buf.to_string(), "");
}

#[test]
fn gap_moves_correctly_after_multiple_operations() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("the quick brown fox");
    gap_delete_emacs_range(&mut buf, 4, 10); // delete "quick "
    assert_eq!(buf.to_string(), "the brown fox");
    gap_insert_storage_string(&mut buf, 4, "slow ");
    assert_eq!(buf.to_string(), "the slow brown fox");
    gap_delete_emacs_range(&mut buf, 9, 15); // delete "brown "
    assert_eq!(buf.to_string(), "the slow fox");
    gap_insert_storage_string(&mut buf, 9, "red ");
    assert_eq!(buf.to_string(), "the slow red fox");
}

#[test]
fn insert_at_every_position() {
    crate::test_utils::init_test_tracing();
    for pos in 0..=5 {
        let mut buf = GapBuffer::from_str("hello");
        gap_insert_storage_string(&mut buf, pos, "X");
        assert_eq!(buf.len(), 6);
        assert_eq!(byte_at(&buf, pos), b'X');
    }
}

#[test]
fn display_trait() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("display test");
    let s = format!("{buf}");
    assert_eq!(s, "display test");
}

#[test]
fn debug_trait_contains_text() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("dbg");
    let dbg = format!("{buf:?}");
    assert!(dbg.contains("dbg"));
    assert!(dbg.contains("GapBuffer"));
}

#[test]
fn default_is_empty() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::default();
    assert!(buf.is_empty());
}

#[test]
fn clone_is_independent() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("original");
    let clone = buf.clone();
    gap_insert_storage_string(&mut buf, 0, "X");
    assert_eq!(buf.to_string(), "Xoriginal");
    assert_eq!(clone.to_string(), "original");
}

#[test]
#[should_panic]
fn insert_past_end_panics() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hi");
    gap_insert_storage_string(&mut buf, 3, "x");
}

#[test]
#[should_panic]
fn delete_past_end_panics() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hi");
    gap_delete_emacs_range(&mut buf, 0, 3);
}

#[test]
#[should_panic]
fn delete_inverted_range_panics() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hello");
    gap_delete_emacs_range(&mut buf, 3, 1);
}

#[test]
#[should_panic]
fn text_range_past_end_panics() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hi");
    gap_text_range(&buf, 0, 3);
}

#[test]
#[should_panic]
fn move_gap_past_end_panics() {
    crate::test_utils::init_test_tracing();
    let mut buf = GapBuffer::from_str("hi");
    buf.move_gap_to(3);
}

#[test]
#[should_panic]
fn byte_to_char_past_end_panics() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hi");
    byte_to_char(&buf, 3);
}

#[test]
fn char_to_byte_past_end_clamps() {
    crate::test_utils::init_test_tracing();
    let buf = GapBuffer::from_str("hi");
    // char_to_byte clamps to buffer end instead of panicking
    // when char_pos exceeds char_count (for stale positions).
    assert_eq!(char_to_byte(&buf, 3), buf.len());
    assert_eq!(char_to_byte(&buf, 100), buf.len());
}

// -----------------------------------------------------------------------
// copy_bytes_to
// -----------------------------------------------------------------------

#[test]
fn copy_bytes_to_basic() {
    crate::test_utils::init_test_tracing();
    let gb = GapBuffer::from_str("Hello, world!");
    let mut out = Vec::new();
    gap_copy_bytes_to(&gb, 0, 5, &mut out);
    assert_eq!(&out, b"Hello");

    gap_copy_bytes_to(&gb, 7, 13, &mut out);
    assert_eq!(&out, b"world!");
}

#[test]
fn copy_bytes_to_spanning_gap() {
    crate::test_utils::init_test_tracing();
    let mut gb = GapBuffer::from_str("abcdef");
    gb.move_gap_to(3); // gap after "abc"
    let mut out = Vec::new();
    gap_copy_bytes_to(&gb, 1, 5, &mut out); // "bcde" — spans gap
    assert_eq!(&out, b"bcde");
}

#[test]
fn copy_bytes_to_empty_range() {
    crate::test_utils::init_test_tracing();
    let gb = GapBuffer::from_str("test");
    let mut out = vec![1, 2, 3]; // pre-existing contents
    gap_copy_bytes_to(&gb, 2, 2, &mut out);
    assert!(out.is_empty());
}

#[test]
fn copy_emacs_bytes_to_unibyte_storage_sentinels() {
    crate::test_utils::init_test_tracing();
    let storage = crate::emacs_core::string_escape::bytes_to_unibyte_storage_string(&[
        0xFF, b'\n', 0x80, b'A',
    ]);
    let mut gb = GapBuffer::from_str(&storage);
    gb.move_gap_to(2);

    let mut out = Vec::new();
    gb.copy_emacs_byte_range_to(EmacsByteRange::from_usize(0, 4), &mut out);
    assert_eq!(out, vec![0xFF, b'\n', 0x80, b'A']);

    gb.copy_emacs_byte_range_to(EmacsByteRange::from_usize(1, 3), &mut out);
    assert_eq!(out, vec![b'\n', 0x80]);
}

#[test]
fn for_each_emacs_byte_chunk_visits_before_and_after_gap() {
    crate::test_utils::init_test_tracing();
    let mut gb = GapBuffer::from_str("abcdef");
    gb.move_gap_to(3);

    let mut chunks = Vec::new();
    gb.for_each_emacs_byte_range_chunk(EmacsByteRange::from_usize(1, 5), |chunk| {
        chunks.push(chunk.to_vec());
        Ok::<(), ()>(())
    })
    .unwrap();
    assert_eq!(chunks, vec![b"bc".to_vec(), b"de".to_vec()]);
}

#[test]
fn for_each_emacs_byte_chunk_skips_empty_range() {
    crate::test_utils::init_test_tracing();
    let gb = GapBuffer::from_str("abcdef");
    let mut called = false;

    gb.for_each_emacs_byte_range_chunk(EmacsByteRange::from_usize(2, 2), |_| {
        called = true;
        Ok::<(), ()>(())
    })
    .unwrap();
    assert!(!called);
}

#[test]
fn contiguous_emacs_bytes_borrows_before_and_after_gap() {
    crate::test_utils::init_test_tracing();
    let mut gb = GapBuffer::from_str("abcdef");
    gb.move_gap_to(3);

    assert!(gb.has_contiguous_emacs_byte_range(EmacsByteRange::from_usize(0, 3)));
    assert_eq!(
        gb.with_contiguous_emacs_byte_range(EmacsByteRange::from_usize(0, 3), |bytes| bytes
            .to_vec()),
        Some(b"abc".to_vec())
    );

    assert!(gb.has_contiguous_emacs_byte_range(EmacsByteRange::from_usize(3, 6)));
    assert_eq!(
        gb.with_contiguous_emacs_byte_range(EmacsByteRange::from_usize(3, 6), |bytes| bytes
            .to_vec()),
        Some(b"def".to_vec())
    );
}

#[test]
fn contiguous_emacs_bytes_rejects_gap_spanning_range() {
    crate::test_utils::init_test_tracing();
    let mut gb = GapBuffer::from_str("abcdef");
    gb.move_gap_to(3);

    assert!(!gb.has_contiguous_emacs_byte_range(EmacsByteRange::from_usize(1, 5)));
    assert_eq!(
        gb.with_contiguous_emacs_byte_range(EmacsByteRange::from_usize(1, 5), |_| ()),
        None
    );
}

// -----------------------------------------------------------------------
// GNU parity tests (gap-sizing constants)
// -----------------------------------------------------------------------

#[test]
fn new_buffer_has_gnu_default_gap_size() {
    crate::test_utils::init_test_tracing();
    let gb = GapBuffer::new();
    assert_eq!(gb.gap_size(), 20);
}

#[test]
fn ensure_gap_grows_beyond_requested_minimum() {
    crate::test_utils::init_test_tracing();
    let mut gb = GapBuffer::new();
    // Fill current gap completely so the next ensure_gap must actually grow.
    let filler = vec![b'a'; gb.gap_size()];
    gap_insert_emacs_bytes(&mut gb, 0, &filler);
    assert_eq!(gb.gap_size(), 0);
    gb.ensure_gap(1);
    // GNU adds GAP_BYTES_DFL beyond caller's request.
    assert!(
        gb.gap_size() >= 2000,
        "expected ensure_gap(1) to grow gap to >= 2000, got {}",
        gb.gap_size()
    );
}

#[test]
fn is_char_boundary_matches_oracle_on_large_multibyte_buffer() {
    crate::test_utils::init_test_tracing();
    // Build a large mixed ASCII + CJK buffer.
    let mut s = String::new();
    for i in 0..2000 {
        if i % 3 == 0 {
            s.push_str("日本語");
        } else {
            s.push_str("hello");
        }
    }
    let gb = GapBuffer::from_str(&s);
    // Oracle: walk char boundaries from 0, mark each one.
    let bytes: Vec<u8> = (0..gb.len()).map(|i| byte_at(&gb, i)).collect();
    let mut oracle_boundary = vec![false; gb.len() + 1];
    oracle_boundary[0] = true;
    let mut p = 0usize;
    while p < bytes.len() {
        let (_, len) = crate::emacs_core::emacs_char::string_char(&bytes[p..]);
        p += len;
        oracle_boundary[p] = true;
    }
    // Spot-check positions: for positions that SHOULD be boundaries,
    // byte_to_char must not panic (it asserts on boundary internally).
    for i in (0..gb.len()).step_by(gb.len() / 500 + 1) {
        if oracle_boundary[i] {
            let _ = byte_to_char(&gb, i);
        }
    }
}

#[test]
fn insert_emacs_bytes_both_matches_scanning_variant() {
    crate::test_utils::init_test_tracing();
    let bytes = "Hello, 日本語!".as_bytes().to_vec();
    let nchars = crate::emacs_core::emacs_char::chars_in_multibyte(&bytes);

    let mut a = GapBuffer::new();
    gap_insert_emacs_bytes(&mut a, 0, &bytes);

    let mut b = GapBuffer::new();
    gap_insert_emacs_bytes_with_char_len(&mut b, 0, &bytes, nchars);

    assert_eq!(a.to_string(), b.to_string());
    assert_eq!(a.char_count(), b.char_count());
    assert_eq!(a.emacs_byte_len(), b.emacs_byte_len());
    assert_eq!(a.gpt(), b.gpt());
    assert_eq!(a.gpt_byte(), b.gpt_byte());
}

#[test]
fn delete_range_both_matches_scanning_variant() {
    crate::test_utils::init_test_tracing();
    let source = "Hello, 日本語 world!";
    let mut a = GapBuffer::from_str(source);
    let mut b = GapBuffer::from_str(source);

    // Delete the CJK span.
    let from = 7;
    let to = 7 + "日本語".len();

    // Compute nchars for the deleted slice via oracle.
    let mut tmp = Vec::new();
    gap_copy_bytes_to(&b, from, to, &mut tmp);
    let nchars = crate::emacs_core::emacs_char::chars_in_multibyte(&tmp);

    gap_delete_emacs_range(&mut a, from, to);
    gap_delete_emacs_range_with_char_len(&mut b, from, to, nchars);

    assert_eq!(a.to_string(), b.to_string());
    assert_eq!(a.char_count(), b.char_count());
    assert_eq!(a.emacs_byte_len(), b.emacs_byte_len());
}

#[test]
fn move_gap_both_matches_scanning_variant() {
    crate::test_utils::init_test_tracing();
    let source = "Hello, 日本語 world!";
    let mut a = GapBuffer::from_str(source);
    let mut b = GapBuffer::from_str(source);

    // Move the gap to a position inside the CJK run.
    let bytepos = 7 + "日".len(); // byte position of '本'
    let charpos = byte_to_char(&a, bytepos);

    a.move_gap_to(bytepos);
    gap_move_to_emacs_byte_and_char(&mut b, bytepos, charpos);

    assert_eq!(a.to_string(), b.to_string());
    assert_eq!(a.gpt(), b.gpt());
    assert_eq!(a.gpt_byte(), b.gpt_byte());
    assert_eq!(a.char_count(), b.char_count());
}
