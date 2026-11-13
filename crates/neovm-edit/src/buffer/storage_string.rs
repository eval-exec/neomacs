//! Lisp string storage encoding (stage-3 C7).
//!
//! NeoVM keeps Lisp strings as Rust `str` with sentinel code points for
//! Emacs raw bytes and non-Unicode character codes.  The buffer substrate
//! decodes that storage on every insert, so the scan and decode cluster
//! lives here as plain data and pure functions; the formatting side stays in
//! `emacs_core::string_escape`, which re-exports these items so the
//! historical `emacs_core::string_escape` path keeps working.

use neovm_value::emacs_char;

const RAW_BYTE_SENTINEL_BASE: u32 = 0xE000;
const RAW_BYTE_SENTINEL_MIN: u32 = 0xE080;
const RAW_BYTE_SENTINEL_MAX: u32 = 0xE0FF;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const RAW_BYTE_CHAR_MIN: u32 = 0x3FFF80;
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
const RAW_BYTE_CHAR_MAX: u32 = 0x3FFFFF;
// Unibyte raw bytes 0x80..0xFF are stored as U+E380..U+E3FF (BASE + byte); see
// `bytes_to_unibyte_storage_string`. Bytes 0x00..0x7F stay ASCII, so the
// sentinel range is U+E380..U+E3FF — NOT U+E300..U+E37F, which are genuine
// Private-Use-Area characters (e.g. nerd-font weather icons U+E322). Treating
// the wider range as sentinels corrupted those glyphs and broke byte-compiled
// `.elc` syntax (issue #131).
const UNIBYTE_BYTE_SENTINEL_BASE: u32 = 0xE300;
const UNIBYTE_BYTE_SENTINEL_MIN: u32 = 0xE380;
const UNIBYTE_BYTE_SENTINEL_MAX: u32 = 0xE3FF;

const EXT_SEQ_PREFIX: u32 = 0xE100;
const EXT_SEQ_LEN_BASE: u32 = 0xE110;
const EXT_SEQ_BYTE_BASE: u32 = 0xE200;
const EXT_SEQ_MAX_LEN: u32 = 6;

/// Encode non-Unicode Emacs character codes as NeoVM internal sentinels.
///
/// Returns `None` for Unicode scalar values (which can be stored directly).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn encode_nonunicode_char_for_storage(code: u32) -> Option<String> {
    if code <= 0x10FFFF && char::from_u32(code).is_some() {
        return None;
    }

    if (RAW_BYTE_CHAR_MIN..=RAW_BYTE_CHAR_MAX).contains(&code) {
        // Emacs raw-byte chars map 0x3FFF80..0x3FFFFF -> 0x80..0xFF.
        let raw = code - 0x3FFF00;
        let ch = char::from_u32(RAW_BYTE_SENTINEL_BASE + raw).expect("valid raw-byte sentinel");
        return Some(ch.to_string());
    }

    if code <= emacs_char::MAX_CHAR {
        let bytes = encode_emacs_extended_utf8(code);
        return Some(encode_extended_sequence_for_storage(&bytes));
    }

    None
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn encode_emacs_extended_utf8(code: u32) -> Vec<u8> {
    if code <= 0x7F {
        vec![code as u8]
    } else if code <= 0x7FF {
        vec![0xC0 | ((code >> 6) as u8), 0x80 | ((code & 0x3F) as u8)]
    } else if code <= 0xFFFF {
        vec![
            0xE0 | ((code >> 12) as u8),
            0x80 | (((code >> 6) & 0x3F) as u8),
            0x80 | ((code & 0x3F) as u8),
        ]
    } else if code <= 0x1FFFFF {
        vec![
            0xF0 | (((code >> 18) & 0x07) as u8),
            0x80 | (((code >> 12) & 0x3F) as u8),
            0x80 | (((code >> 6) & 0x3F) as u8),
            0x80 | ((code & 0x3F) as u8),
        ]
    } else if code <= 0x3FFFFFF {
        vec![
            0xF8 | (((code >> 24) & 0x03) as u8),
            0x80 | (((code >> 18) & 0x3F) as u8),
            0x80 | (((code >> 12) & 0x3F) as u8),
            0x80 | (((code >> 6) & 0x3F) as u8),
            0x80 | ((code & 0x3F) as u8),
        ]
    } else {
        vec![
            0xFC | (((code >> 30) & 0x01) as u8),
            0x80 | (((code >> 24) & 0x3F) as u8),
            0x80 | (((code >> 18) & 0x3F) as u8),
            0x80 | (((code >> 12) & 0x3F) as u8),
            0x80 | (((code >> 6) & 0x3F) as u8),
            0x80 | ((code & 0x3F) as u8),
        ]
    }
}

fn decode_emacs_extended_utf8(bytes: &[u8]) -> Option<u32> {
    match bytes {
        [b0] => Some(*b0 as u32),
        // Emacs raw bytes 0x80..0xFF may appear as overlong C0/C1 sequences.
        [b0, b1] if (*b0 == 0xC0 || *b0 == 0xC1) && (b1 & 0xC0) == 0x80 => {
            Some(0x80 + (((b0 & 0x01) as u32) << 6) + ((b1 & 0x3F) as u32))
        }
        [b0, b1] if (0xC2..=0xDF).contains(b0) && (b1 & 0xC0) == 0x80 => {
            Some((((b0 & 0x1F) as u32) << 6) | ((b1 & 0x3F) as u32))
        }
        [b0, b1, b2] if (b0 & 0xF0) == 0xE0 && (b1 & 0xC0) == 0x80 && (b2 & 0xC0) == 0x80 => {
            Some((((b0 & 0x0F) as u32) << 12) | (((b1 & 0x3F) as u32) << 6) | ((b2 & 0x3F) as u32))
        }
        [b0, b1, b2, b3]
            if (b0 & 0xF8) == 0xF0
                && (b1 & 0xC0) == 0x80
                && (b2 & 0xC0) == 0x80
                && (b3 & 0xC0) == 0x80 =>
        {
            Some(
                (((b0 & 0x07) as u32) << 18)
                    | (((b1 & 0x3F) as u32) << 12)
                    | (((b2 & 0x3F) as u32) << 6)
                    | ((b3 & 0x3F) as u32),
            )
        }
        [b0, b1, b2, b3, b4]
            if (b0 & 0xFC) == 0xF8
                && (b1 & 0xC0) == 0x80
                && (b2 & 0xC0) == 0x80
                && (b3 & 0xC0) == 0x80
                && (b4 & 0xC0) == 0x80 =>
        {
            Some(
                (((b0 & 0x03) as u32) << 24)
                    | (((b1 & 0x3F) as u32) << 18)
                    | (((b2 & 0x3F) as u32) << 12)
                    | (((b3 & 0x3F) as u32) << 6)
                    | ((b4 & 0x3F) as u32),
            )
        }
        [b0, b1, b2, b3, b4, b5]
            if (b0 & 0xFE) == 0xFC
                && (b1 & 0xC0) == 0x80
                && (b2 & 0xC0) == 0x80
                && (b3 & 0xC0) == 0x80
                && (b4 & 0xC0) == 0x80
                && (b5 & 0xC0) == 0x80 =>
        {
            Some(
                (((b0 & 0x01) as u32) << 30)
                    | (((b1 & 0x3F) as u32) << 24)
                    | (((b2 & 0x3F) as u32) << 18)
                    | (((b3 & 0x3F) as u32) << 12)
                    | (((b4 & 0x3F) as u32) << 6)
                    | ((b5 & 0x3F) as u32),
            )
        }
        _ => None,
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn encode_extended_sequence_for_storage(bytes: &[u8]) -> String {
    let mut out = String::new();
    out.push(char::from_u32(EXT_SEQ_PREFIX).expect("valid extended prefix sentinel"));
    let len_char = char::from_u32(EXT_SEQ_LEN_BASE + bytes.len() as u32)
        .expect("valid extended length sentinel");
    out.push(len_char);
    for b in bytes {
        out.push(
            char::from_u32(EXT_SEQ_BYTE_BASE + (*b as u32)).expect("valid extended byte sentinel"),
        );
    }
    out
}

fn decode_extended_sequence_span(s: &str, start: usize) -> Option<(usize, u32)> {
    let mut iter = s[start..].char_indices();
    let (_, prefix) = iter.next()?;
    if prefix as u32 != EXT_SEQ_PREFIX {
        return None;
    }

    let (len_off, len_ch) = iter.next()?;
    let len_code = len_ch as u32;
    if !(EXT_SEQ_LEN_BASE + 1..=EXT_SEQ_LEN_BASE + EXT_SEQ_MAX_LEN).contains(&len_code) {
        return None;
    }
    let len = (len_code - EXT_SEQ_LEN_BASE) as usize;

    let mut bytes = Vec::with_capacity(len);
    let mut end_rel = len_off + len_ch.len_utf8();
    for _ in 0..len {
        let (byte_off, byte_ch) = iter.next()?;
        let byte_code = byte_ch as u32;
        if !(EXT_SEQ_BYTE_BASE..=EXT_SEQ_BYTE_BASE + 0xFF).contains(&byte_code) {
            return None;
        }
        bytes.push((byte_code - EXT_SEQ_BYTE_BASE) as u8);
        end_rel = byte_off + byte_ch.len_utf8();
    }

    let cp = decode_emacs_extended_utf8(&bytes)?;
    Some((start + end_rel, cp))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[doc(hidden)]
pub struct StorageUnit {
    pub storage_start: usize,
    pub storage_end: usize,
    pub code: u32,
    pub display_width: usize,
    pub logical_byte_len: usize,
}

/// Scan a storage string into [`StorageUnit`]s.
///
/// `multibyte` selects how the U+E300..E3FF range is interpreted: in a
/// **unibyte** storage string those code points are the byte-0x80..0xFF
/// sentinels produced by `bytes_to_unibyte_storage_string` and decode to the
/// raw byte; in a **multibyte** storage string that range only ever holds real
/// characters (e.g. nerd-font glyphs), so they pass through unchanged. Decoding
/// them unconditionally was issue #131 (U+E322 → 0x22). The U+E080..E0FF
/// raw-byte and U+E100.. extended sentinels are multibyte-only and always decoded.
#[doc(hidden)]
pub fn scan_storage_units(s: &str, multibyte: bool) -> Vec<StorageUnit> {
    let mut out = Vec::new();
    let mut idx = 0usize;

    while idx < s.len() {
        let ch = s[idx..].chars().next().expect("valid utf-8 char boundary");
        let code = ch as u32;
        let next = idx + ch.len_utf8();

        if (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&code) {
            let raw = (code - RAW_BYTE_SENTINEL_BASE) as u8;
            out.push(StorageUnit {
                storage_start: idx,
                storage_end: next,
                code: 0x3FFF00 + raw as u32,
                display_width: 4,
                logical_byte_len: 2,
            });
            idx = next;
            continue;
        }

        if !multibyte && (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&code) {
            let byte = (code - UNIBYTE_BYTE_SENTINEL_BASE) as u8;
            out.push(StorageUnit {
                storage_start: idx,
                storage_end: next,
                code: byte as u32,
                display_width: 1,
                logical_byte_len: 1,
            });
            idx = next;
            continue;
        }

        if code == EXT_SEQ_PREFIX
            && let Some((end, cp)) = decode_extended_sequence_span(s, idx)
        {
            let byte_len = ((s[idx..end].chars().nth(1).expect("extended len sentinel") as u32)
                - EXT_SEQ_LEN_BASE) as usize;
            out.push(StorageUnit {
                storage_start: idx,
                storage_end: end,
                code: cp,
                display_width: 1,
                logical_byte_len: byte_len,
            });
            idx = end;
            continue;
        }

        let width = emacs_char::char_width(ch);
        out.push(StorageUnit {
            storage_start: idx,
            storage_end: next,
            code,
            display_width: width,
            logical_byte_len: ch.len_utf8(),
        });
        idx = next;
    }

    out
}

/// Scan a storage string, inferring multibyte-ness from its content: a string
/// is treated as unibyte only if it actually contains unibyte byte-sentinels
/// (U+E380..U+E3FF). Used by display/metric/format helpers that receive a bare
/// storage string without an explicit flag. Data paths that know the flag
/// (e.g. `storage_string_to_buffer_bytes`) thread it through `scan_storage_units`
/// directly instead.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn scan_storage_units_auto(s: &str) -> Vec<StorageUnit> {
    scan_storage_units(s, !storage_string_contains_unibyte_bytes(s))
}

/// [`decode_storage_char_codes`] with multibyte-ness inferred from content (see
/// [`scan_storage_units_auto`]). For callers holding a bare storage string.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn decode_storage_char_codes_auto(s: &str) -> Vec<u32> {
    decode_storage_char_codes(s, !storage_string_contains_unibyte_bytes(s))
}

/// Return the Emacs character code at a storage byte position and the next
/// storage byte position. See [`scan_storage_units`] for how `multibyte`
/// governs the U+E300..E3FF range.
#[doc(hidden)]
pub fn storage_code_step(s: &str, pos: usize, multibyte: bool) -> Option<(u32, usize)> {
    if pos >= s.len() {
        return None;
    }

    let ch = s[pos..].chars().next()?;
    let code = ch as u32;
    let next = pos + ch.len_utf8();

    if (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&code) {
        let raw = code - RAW_BYTE_SENTINEL_BASE;
        return Some((0x3FFF00 + raw, next));
    }

    if !multibyte && (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&code) {
        return Some((code - UNIBYTE_BYTE_SENTINEL_BASE, next));
    }

    if code == EXT_SEQ_PREFIX
        && let Some((end, cp)) = decode_extended_sequence_span(s, pos)
    {
        return Some((cp, end));
    }

    Some((code, next))
}

#[doc(hidden)]
pub fn storage_has_special_units(s: &str) -> bool {
    if s.is_ascii() {
        return false;
    }
    if !s.as_bytes().contains(&0xEE) {
        return false;
    }
    s.chars().any(|ch| {
        let code = ch as u32;
        (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&code)
            || (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&code)
            || code == EXT_SEQ_PREFIX
            || (EXT_SEQ_LEN_BASE + 1..=EXT_SEQ_LEN_BASE + EXT_SEQ_MAX_LEN).contains(&code)
            || (EXT_SEQ_BYTE_BASE..=EXT_SEQ_BYTE_BASE + 0xFF).contains(&code)
    })
}

#[doc(hidden)]
pub fn storage_string_contains_unibyte_bytes(s: &str) -> bool {
    if s.is_ascii() {
        return false;
    }
    if !s.as_bytes().contains(&0xEE) {
        return false;
    }
    s.chars()
        .any(|ch| (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&(ch as u32)))
}

/// Encode raw byte values as a unibyte storage string.
///
/// This keeps byte-oriented Elisp semantics for operations like `aref`,
/// `string-bytes`, and `secure-hash` binary output.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn bytes_to_unibyte_storage_string(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    for b in bytes {
        if *b <= 0x7f {
            out.push(char::from(*b));
        } else {
            out.push(
                char::from_u32(UNIBYTE_BYTE_SENTINEL_BASE + (*b as u32))
                    .expect("valid unibyte-byte sentinel"),
            );
        }
    }
    out
}

#[doc(hidden)]
pub fn storage_string_to_buffer_bytes(s: &str, multibyte: bool) -> Vec<u8> {
    let codes = decode_storage_char_codes(s, multibyte);
    if !multibyte {
        return codes
            .into_iter()
            .map(|code| {
                assert!(
                    code <= 0xFF,
                    "unibyte storage contained non-byte character code {code:#X}"
                );
                code as u8
            })
            .collect();
    }

    let mut bytes = Vec::new();
    for code in codes {
        let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = emacs_char::char_string(code, &mut buf);
        bytes.extend_from_slice(&buf[..len]);
    }
    bytes
}

#[doc(hidden)]
pub fn decode_storage_units(s: &str, multibyte: bool) -> Vec<(u32, usize)> {
    if !storage_has_special_units(s) {
        return s
            .chars()
            .map(|ch| (ch as u32, emacs_char::char_width(ch)))
            .collect();
    }
    scan_storage_units(s, multibyte)
        .into_iter()
        .map(|unit| (unit.code, unit.display_width))
        .collect()
}

/// Decode NeoVM string storage into Emacs character codes. `multibyte` governs
/// the U+E300..E3FF range (see [`scan_storage_units`]).
#[doc(hidden)]
pub fn decode_storage_char_codes(s: &str, multibyte: bool) -> Vec<u32> {
    decode_storage_units(s, multibyte)
        .into_iter()
        .map(|(cp, _)| cp)
        .collect()
}

/// Compute Emacs-like display width for NeoVM string storage.
/// Count logical Emacs characters in NeoVM string storage.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn storage_char_len(s: &str) -> usize {
    if !storage_has_special_units(s) {
        return if s.is_ascii() {
            s.len()
        } else {
            s.chars().count()
        };
    }
    scan_storage_units_auto(s).len()
}

/// Count Emacs string bytes represented by NeoVM string storage.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn storage_byte_len(s: &str) -> usize {
    if !storage_has_special_units(s) {
        return s.len();
    }
    scan_storage_units_auto(s)
        .into_iter()
        .map(|unit| unit.logical_byte_len)
        .sum()
}

/// Convert a storage-byte boundary to the corresponding logical Emacs byte offset.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn storage_byte_to_logical_byte(s: &str, storage_byte_pos: usize) -> usize {
    if !storage_has_special_units(s) {
        return storage_byte_pos.min(s.len());
    }

    let units = scan_storage_units_auto(s);
    let mut logical = 0usize;
    for unit in &units {
        if storage_byte_pos <= unit.storage_start {
            return logical;
        }
        if storage_byte_pos < unit.storage_end {
            return logical;
        }
        logical += unit.logical_byte_len;
        if storage_byte_pos == unit.storage_end {
            return logical;
        }
    }
    logical
}

/// Convert a logical Emacs byte offset at a character boundary to a storage-byte offset.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
#[doc(hidden)]
pub fn storage_logical_byte_to_storage_byte(s: &str, logical_byte_pos: usize) -> usize {
    if !storage_has_special_units(s) {
        return logical_byte_pos.min(s.len());
    }

    let units = scan_storage_units_auto(s);
    let mut logical = 0usize;
    for unit in &units {
        if logical_byte_pos == logical {
            return unit.storage_start;
        }
        logical += unit.logical_byte_len;
        if logical_byte_pos == logical {
            return unit.storage_end;
        }
        assert!(
            logical_byte_pos > logical,
            "logical byte position {logical_byte_pos} is not at a character boundary"
        );
    }
    assert!(
        logical_byte_pos == logical,
        "logical byte position {logical_byte_pos} exceeds logical length {logical}"
    );
    s.len()
}
