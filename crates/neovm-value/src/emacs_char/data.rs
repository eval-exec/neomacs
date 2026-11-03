//! Lower-layer DATA definitions with restricted core aliases.

use super::*;

/// Decode a character from encoded file bytes using GNU reader arithmetic.
///
/// Unlike internal-string decoding, file input must bound the promised width
/// and check continuation shape. Preserve in-domain noncanonical arithmetic;
/// malformed, truncated, or oversized sequences consume their lead byte only.
/// Empty input is EOF, distinct from a declared string's virtual zero tail.
#[inline]
pub fn decode_file_multibyte_char(bytes: &[u8]) -> Option<(EmacsChar, usize)> {
    let &lead = bytes.first()?;
    let width = bytes_by_char_head(lead);
    Some(
        if bytes.len() >= width && bytes[1..width].iter().all(|b| b & 0xc0 == 0x80) {
            string_char_unchecked(&bytes[..width])
        } else {
            (EmacsChar::from_byte8(lead), 1)
        },
    )
}

/// Decode a GNU-declared multibyte string character within initialized bytes.
///
/// GNU shorthand names retain supplied character counts and may contain raw
/// noncanonical bytes. Trust the lead-byte arithmetic as GNU does, but model a
/// short or exhausted tail with zeros instead of reading outside the payload.
/// That tail is a bounded emulator policy, not portable GNU memory behavior.
/// Invalid heads or out-of-range results use the existing raw-byte fallback.
/// The borrowed bytes and stack padding are local to the calling mutator.
/// Returns a validated [`EmacsChar`]: an out-of-domain decode can
/// never leave this function.
#[inline]
pub fn decode_declared_multibyte_char(bytes: &[u8]) -> (EmacsChar, usize) {
    let Some(&first) = bytes.first() else {
        return (EmacsChar::from_char('\0'), 1);
    };
    if first < 0x80 {
        return (EmacsChar::from_char(char::from(first)), 1);
    }
    // GNU assumes a legal head and in-range result. Its behavior outside
    // those assumptions is undefined; use the existing raw-byte policy.
    if !(0xc0..=0xf8).contains(&first) {
        return (EmacsChar::from_byte8(first), 1);
    }
    // A complete short tail can be decoded directly. Only a truncated
    // character needs the bounded zero-padding policy above.
    let (code, len) =
        if bytes.len() >= MAX_MULTIBYTE_LENGTH || bytes.len() >= bytes_by_char_head(first) {
            decode_multibyte_unchecked(bytes)
        } else {
            let mut tail = [0; MAX_MULTIBYTE_LENGTH];
            tail[..bytes.len()].copy_from_slice(bytes);
            decode_multibyte_unchecked(&tail)
        };
    decoded_character(first, code, len)
}

/// End offset into `hay` of a case-folded match of `needle` starting at byte
/// `at`, comparing decoded Emacs char codes in place so offsets stay in `hay`'s
/// own byte space (correct for unibyte and multibyte; raw bytes compare
/// verbatim). Returns `None` if `needle` does not match at `at`.
pub fn case_fold_match_len(hay: &[u8], at: usize, needle: &[u8]) -> Option<usize> {
    let mut h = at;
    let mut n = 0;
    while n < needle.len() {
        if h >= hay.len() {
            return None;
        }
        let (hc, hl) = string_char(&hay[h..]);
        let (nc, nl) = string_char(&needle[n..]);
        if emacs_downcase_code(hc.code()) != emacs_downcase_code(nc.code()) {
            return None;
        }
        h += hl.max(1);
        n += nl.max(1);
    }
    Some(h)
}

/// End offset into `hay` of a case-folded match of `needle` starting at byte
/// `at`, treating BOTH as UNIBYTE: every byte is one raw character advanced
/// one byte at a time (never decoded as a multibyte sequence).
///
/// Mirrors GNU `simple_search`'s unibyte branch (search.c:1622-1633), which
/// fetches one byte per position and folds it through the case-canonicalization
/// table. Each byte 0x00..=0xFF is treated as the Latin-1 character of that
/// code point for folding (so É 0xC9 folds to é 0xE9 in a unibyte buffer);
/// bytes with no case partner compare verbatim. Critically the advance is
/// byte-by-byte, so a raw byte embedded inside what would be a multibyte lead
/// (e.g. the 0xA9 in a C3 A9 pair) is still found at its own position.
pub fn unibyte_case_fold_match_len(hay: &[u8], at: usize, needle: &[u8]) -> Option<usize> {
    if at + needle.len() > hay.len() {
        return None;
    }
    for (i, &nb) in needle.iter().enumerate() {
        let hb = hay[at + i];
        if emacs_downcase_code(hb as u32) != emacs_downcase_code(nb as u32) {
            return None;
        }
    }
    Some(at + needle.len())
}

pub use super::char_width::{GNU_DEFAULT_WIDE_RANGES, ZERO_WIDTH_RANGES};
