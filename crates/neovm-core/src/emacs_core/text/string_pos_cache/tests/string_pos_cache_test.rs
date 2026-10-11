use super::*;
use crate::emacs_core::emacs_char;
use crate::emacs_core::value::Value;
use crate::heap_types::LispString;

/// A multibyte string mixing 1- to 4-byte characters and raw bytes.
fn mixed_bytes(seed: u64, chars: usize) -> Vec<u8> {
    let mut state = seed;
    let mut next = || {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (state >> 33) as u32
    };
    let mut bytes = Vec::new();
    for _ in 0..chars {
        let code = match next() % 6 {
            0 | 1 => b'a' as u32 + next() % 26,
            2 => 0x430 + next() % 32,   // Cyrillic, 2 bytes
            3 => 0x3042 + next() % 80,  // Hiragana, 3 bytes
            4 => 0x1F600 + next() % 64, // emoji, 4 bytes
            _ => emacs_char::byte8_to_char(0x80 + (next() % 128) as u8), // raw byte
        };
        let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = emacs_char::char_string(code, &mut buf);
        bytes.extend_from_slice(&buf[..len]);
    }
    bytes
}

#[test]
fn cached_conversions_agree_with_a_scan_from_the_start() {
    crate::test_utils::init_test_tracing();
    for seed in 0..8u64 {
        reset_string_pos_cache();
        let bytes = mixed_bytes(seed, 700);
        let value = Value::heap_string(LispString::from_emacs_bytes(bytes.clone()));
        let s = value.as_lisp_string().expect("string");
        let schars = s.schars();
        let boundaries: Vec<usize> = (0..=bytes.len())
            .filter(|&b| b == bytes.len() || (bytes[b] & 0xC0) != 0x80)
            .collect();
        assert_eq!(boundaries.len(), schars + 1);
        let mut state = seed.wrapping_add(99);
        for step in 0..3000usize {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            let r = (state >> 33) as usize;
            // Mostly short hops from the last position, sometimes far jumps.
            let char_index = if step % 7 == 0 {
                r % (schars + 3)
            } else {
                (step * 3 + r % 5) % (schars + 1)
            };
            let want = emacs_char::char_to_byte_pos(&bytes, char_index.min(schars));
            assert_eq!(
                string_char_to_byte(value, s, char_index),
                want,
                "seed {seed} char {char_index}"
            );
            let byte_index = r % (bytes.len() + 3);
            let want = emacs_char::byte_to_char_pos(&bytes, byte_index.min(bytes.len()));
            assert_eq!(
                string_byte_to_char(value, s, byte_index),
                want,
                "seed {seed} byte {byte_index}"
            );
        }
    }
}

#[test]
fn a_changed_string_never_answers_from_the_cache() {
    crate::test_utils::init_test_tracing();
    reset_string_pos_cache();
    // "aжb": cache a pair past the 2-byte character, then make the first
    // character 2 bytes and the second 1 byte -- the same SBYTES, shifted
    // boundaries.
    let value = Value::heap_string(LispString::from_emacs_bytes("aжжb".as_bytes().to_vec()));
    let s = value.as_lisp_string().expect("string");
    assert_eq!(string_char_to_byte(value, s, 2), 3);
    let mut owned = s.clone();
    owned.mutate_bytes(|bytes| {
        bytes.clear();
        bytes.extend_from_slice("жaжb".as_bytes());
    });
    let changed = Value::heap_string(owned);
    let t = changed.as_lisp_string().expect("string");
    assert_eq!(string_char_to_byte(changed, t, 2), 3);
    assert_eq!(string_char_to_byte(value, s, 1), 1);
    // In place: the same object, SBYTES and data pointer, other boundaries.
    reset_string_pos_cache();
    let value = Value::heap_string(LispString::from_emacs_bytes("aжжb".as_bytes().to_vec()));
    assert_eq!(
        string_char_to_byte(value, value.as_lisp_string().unwrap(), 2),
        3
    );
    value.with_lisp_string_mut(|string| {
        string.mutate_bytes(|bytes| {
            bytes.clear();
            bytes.extend_from_slice("жaжb".as_bytes());
        })
    });
    let s = value.as_lisp_string().expect("string");
    assert_eq!(string_char_to_byte(value, s, 2), 3);
    assert_eq!(string_char_to_byte(value, s, 1), 2);
    assert_eq!(string_byte_to_char(value, s, 2), 1);
}
