//! Shared Lisp string escaping helpers.

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
pub(crate) fn encode_nonunicode_char_for_storage(code: u32) -> Option<String> {
    if code <= 0x10FFFF && char::from_u32(code).is_some() {
        return None;
    }

    if (RAW_BYTE_CHAR_MIN..=RAW_BYTE_CHAR_MAX).contains(&code) {
        // Emacs raw-byte chars map 0x3FFF80..0x3FFFFF -> 0x80..0xFF.
        let raw = code - 0x3FFF00;
        let ch = char::from_u32(RAW_BYTE_SENTINEL_BASE + raw).expect("valid raw-byte sentinel");
        return Some(ch.to_string());
    }

    if code <= crate::emacs_core::emacs_char::MAX_CHAR {
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

fn push_unibyte_literal_byte(out: &mut Vec<u8>, byte: u8) {
    use crate::emacs_core::emacs_char;
    match byte {
        b'"' => out.extend_from_slice(br#"\""#),
        b'\\' => out.extend_from_slice(br#"\\"#),
        // GNU `print_object' (print.c) prints a unibyte string's high byte as
        // `printchar (BYTE8_TO_CHAR (c))' when `print-escape-nonascii' is nil --
        // i.e. it emits the raw byte (in its byte8 internal encoding), not an
        // octal escape.  Octal escaping only happens when the print target is
        // multibyte (which sets `print-escape-nonascii'), handled by the caller.
        b if b >= 0x80 => {
            let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
            let n = emacs_char::char_string(emacs_char::byte8_to_char(b), &mut buf);
            out.extend_from_slice(&buf[..n]);
        }
        b => out.push(b),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct StorageUnit {
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
pub(crate) fn scan_storage_units(s: &str, multibyte: bool) -> Vec<StorageUnit> {
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

        let width = crate::encoding::char_width(ch);
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
pub(crate) fn scan_storage_units_auto(s: &str) -> Vec<StorageUnit> {
    scan_storage_units(s, !storage_string_contains_unibyte_bytes(s))
}

/// [`decode_storage_char_codes`] with multibyte-ness inferred from content (see
/// [`scan_storage_units_auto`]). For callers holding a bare storage string.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn decode_storage_char_codes_auto(s: &str) -> Vec<u32> {
    decode_storage_char_codes(s, !storage_string_contains_unibyte_bytes(s))
}

/// Return the Emacs character code at a storage byte position and the next
/// storage byte position. See [`scan_storage_units`] for how `multibyte`
/// governs the U+E300..E3FF range.
pub(crate) fn storage_code_step(s: &str, pos: usize, multibyte: bool) -> Option<(u32, usize)> {
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

fn storage_has_special_units(s: &str) -> bool {
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

pub(crate) fn storage_string_contains_unibyte_bytes(s: &str) -> bool {
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
pub(crate) fn bytes_to_unibyte_storage_string(bytes: &[u8]) -> String {
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

pub(crate) fn storage_string_to_buffer_bytes(s: &str, multibyte: bool) -> Vec<u8> {
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
        let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
        let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
        bytes.extend_from_slice(&buf[..len]);
    }
    bytes
}

pub(crate) fn decode_storage_units(s: &str, multibyte: bool) -> Vec<(u32, usize)> {
    if !storage_has_special_units(s) {
        return s
            .chars()
            .map(|ch| (ch as u32, crate::encoding::char_width(ch)))
            .collect();
    }
    scan_storage_units(s, multibyte)
        .into_iter()
        .map(|unit| (unit.code, unit.display_width))
        .collect()
}

/// Decode NeoVM string storage into Emacs character codes. `multibyte` governs
/// the U+E300..E3FF range (see [`scan_storage_units`]).
pub(crate) fn decode_storage_char_codes(s: &str, multibyte: bool) -> Vec<u32> {
    decode_storage_units(s, multibyte)
        .into_iter()
        .map(|(cp, _)| cp)
        .collect()
}

/// Compute Emacs-like display width for NeoVM string storage.
/// Count logical Emacs characters in NeoVM string storage.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn storage_char_len(s: &str) -> usize {
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
pub(crate) fn storage_byte_len(s: &str) -> usize {
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
pub(crate) fn storage_byte_to_logical_byte(s: &str, storage_byte_pos: usize) -> usize {
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
pub(crate) fn storage_logical_byte_to_storage_byte(s: &str, logical_byte_pos: usize) -> usize {
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

fn push_octal_escape(out: &mut Vec<u8>, byte: u8) {
    out.push(b'\\');
    out.push(b'0' + ((byte >> 6) & 7));
    out.push(b'0' + ((byte >> 3) & 7));
    out.push(b'0' + (byte & 7));
}

/// Render a unibyte string's bytes the way GNU's `print_string`
/// (`src/print.c`) does under `princ` (escapeflag=false) when the string is a
/// nested element printed via `print_object`: raw eight-bit bytes (`>= 0x80`)
/// are octal-escaped as `\NNN` (via `string_escape_byte8`), while ASCII and
/// control bytes pass through verbatim and there are no surrounding quotes.
/// Used by the princ printer for nested strings (e.g. a byte-code object's
/// code string), so `(format "%s" (byte-compile …))` matches GNU byte-for-byte.
pub(crate) fn octal_escape_unibyte_eight_bit(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    for &b in bytes {
        if b >= 0x80 {
            push_octal_escape(&mut out, b);
        } else {
            out.push(b);
        }
    }
    out
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn push_escaped_literal_byte(out: &mut Vec<u8>, byte: u8) {
    match byte {
        b'"' => out.extend_from_slice(br#"\""#),
        b'\\' => out.extend_from_slice(br#"\\"#),
        0x08 => out.extend_from_slice(br#"\b"#),
        b'\t' => out.extend_from_slice(br#"\t"#),
        b'\n' => out.extend_from_slice(br#"\n"#),
        0x0B => out.extend_from_slice(br#"\v"#),
        0x0C => out.extend_from_slice(br#"\f"#),
        b'\r' => out.extend_from_slice(br#"\r"#),
        0x07 => out.extend_from_slice(br#"\a"#),
        0x1B => out.extend_from_slice(br#"\e"#),
        b if b < 0x20 || b == 0x7F => push_octal_escape(out, b),
        b => out.push(b),
    }
}

use super::print::PrintOptions;

/// Format a `LispString` as an Emacs Lisp string literal (UTF-8 `String` output).
pub(crate) fn format_lisp_string_emacs(
    ls: &crate::heap_types::LispString,
    options: &PrintOptions,
) -> String {
    let bytes = format_lisp_string_bytes_inner_emacs(ls.as_bytes(), ls.is_multibyte(), options);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Format a `LispString` as an Emacs Lisp string literal byte sequence.
pub(crate) fn format_lisp_string_bytes_emacs(
    ls: &crate::heap_types::LispString,
    options: &PrintOptions,
) -> Vec<u8> {
    format_lisp_string_bytes_inner_emacs(ls.as_bytes(), ls.is_multibyte(), options)
}

/// Push an octal escape for a character code, choosing 1-3 digits.
/// If the next character after the octal escape is an octal digit ('0'–'7'),
/// we must use 3 digits to avoid misinterpretation (matching GNU's `octalout`).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn push_octal_escape_contextual(out: &mut Vec<u8>, byte: u8, next_char: Option<char>) {
    let need_three_digits = byte > 0o77
        || next_char.is_some_and(|nc| {
            let nc_u32 = nc as u32;
            // Check if the next character is an ASCII octal digit.
            // For sentinel chars, don't force 3 digits.
            nc_u32 < 0x80 && (b'0'..=b'7').contains(&(nc_u32 as u8))
        });
    let need_two_digits = byte > 0o7;

    out.push(b'\\');
    if need_three_digits {
        out.push(b'0' + ((byte >> 6) & 7));
        out.push(b'0' + ((byte >> 3) & 7));
        out.push(b'0' + (byte & 7));
    } else if need_two_digits {
        out.push(b'0' + ((byte >> 3) & 7));
        out.push(b'0' + (byte & 7));
    } else {
        out.push(b'0' + (byte & 7));
    }
}

/// Like `push_octal_escape_contextual` but uses a `u32` Emacs character code
/// for the "next character" peek (avoids requiring Rust `char`).
pub(super) fn push_octal_escape_contextual_u32(
    out: &mut Vec<u8>,
    byte: u8,
    next_code: Option<u32>,
) {
    let need_three_digits =
        byte > 0o77 || next_code.is_some_and(|nc| nc < 0x80 && (b'0'..=b'7').contains(&(nc as u8)));
    let need_two_digits = byte > 0o7;

    out.push(b'\\');
    if need_three_digits {
        out.push(b'0' + ((byte >> 6) & 7));
        out.push(b'0' + ((byte >> 3) & 7));
        out.push(b'0' + (byte & 7));
    } else if need_two_digits {
        out.push(b'0' + ((byte >> 3) & 7));
        out.push(b'0' + (byte & 7));
    } else {
        out.push(b'0' + (byte & 7));
    }
}

/// Format Emacs-encoded bytes as a Lisp string literal.
///
/// This is the canonical string printer.  It accepts `&[u8]` in Emacs internal
/// encoding (UTF-8 superset with C0/C1 overlong sequences for raw bytes).
/// The `is_multibyte` flag controls whether raw-byte characters are printed
/// with octal escapes (multibyte) or as literal bytes (unibyte).
pub(crate) fn format_lisp_string_bytes_inner_emacs(
    data: &[u8],
    is_multibyte: bool,
    options: &PrintOptions,
) -> Vec<u8> {
    use crate::emacs_core::emacs_char;

    let mut out = Vec::with_capacity(data.len() + 2);
    out.push(b'"');

    // Track whether we just emitted a hex escape.  If so, the next
    // character might be taken as part of the hex literal; GNU Emacs
    // inserts `\ ` (backslash-space) to disambiguate.
    let mut need_nonhex = false;

    let mut pos = 0;
    while pos < data.len() {
        if is_multibyte {
            let (code, len) = emacs_char::string_char(&data[pos..]);
            // Peek at the next character for contextual octal escaping
            let next_code = if pos + len < data.len() {
                let (nc, _) = emacs_char::string_char(&data[pos + len..]);
                Some(nc)
            } else {
                None
            };
            pos += len;

            // Raw-byte character in multibyte string → octal escape
            if emacs_char::char_byte8_p(code) {
                let byte = emacs_char::char_to_byte8(code);
                push_octal_escape_contextual_u32(&mut out, byte, next_code);
                need_nonhex = false;
                continue;
            }

            // print-escape-multibyte: non-ASCII in multibyte strings → \xNNNN
            if options.print_escape_multibyte && code > 0x7F {
                let hex = format!("\\x{:04x}", code);
                out.extend_from_slice(hex.as_bytes());
                need_nonhex = true;
                continue;
            }

            // ASCII-range handling
            if code <= 0x7F {
                let b = code as u8;
                if is_hex_digit(b) && need_nonhex {
                    out.extend_from_slice(b"\\ ");
                    out.push(b);
                    need_nonhex = false;
                    continue;
                }
                if b == b'\n' && options.print_escape_newlines {
                    out.extend_from_slice(b"\\n");
                    need_nonhex = false;
                    continue;
                }
                if b == 0x0c && options.print_escape_newlines {
                    out.extend_from_slice(b"\\f");
                    need_nonhex = false;
                    continue;
                }
                if b == b'"' || b == b'\\' {
                    out.push(b'\\');
                    out.push(b);
                    need_nonhex = false;
                    continue;
                }
                if options.print_escape_control_characters && (b < 0x20 || b == 0x7f) {
                    push_octal_escape_contextual_u32(&mut out, b, next_code);
                    need_nonhex = false;
                    continue;
                }
                out.push(b);
                need_nonhex = false;
                continue;
            }

            // Non-ASCII Unicode character: emit as Emacs encoding bytes
            let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
            let n = emacs_char::char_string(code, &mut buf);
            out.extend_from_slice(&buf[..n]);
            need_nonhex = false;
        } else {
            // Unibyte string: each byte is one character
            let byte = data[pos];
            let next_byte = data.get(pos + 1).copied();
            pos += 1;

            if byte >= 0x80 {
                if options.print_escape_nonascii {
                    push_octal_escape_contextual_u32(&mut out, byte, next_byte.map(|b| b as u32));
                } else {
                    push_unibyte_literal_byte(&mut out, byte);
                }
                need_nonhex = false;
                continue;
            }

            // ASCII byte
            if is_hex_digit(byte) && need_nonhex {
                out.extend_from_slice(b"\\ ");
                out.push(byte);
                need_nonhex = false;
                continue;
            }
            if byte == b'\n' && options.print_escape_newlines {
                out.extend_from_slice(b"\\n");
                need_nonhex = false;
                continue;
            }
            if byte == 0x0c && options.print_escape_newlines {
                out.extend_from_slice(b"\\f");
                need_nonhex = false;
                continue;
            }
            if byte == b'"' || byte == b'\\' {
                out.push(b'\\');
                out.push(byte);
                need_nonhex = false;
                continue;
            }
            if options.print_escape_control_characters && (byte < 0x20 || byte == 0x7f) {
                push_octal_escape_contextual_u32(&mut out, byte, next_byte.map(|b| b as u32));
                need_nonhex = false;
                continue;
            }
            out.push(byte);
            need_nonhex = false;
        }
    }

    out.push(b'"');
    out
}

/// Backward-compat wrapper: format a Rust `&str` (old sentinel-encoded strings).
/// This delegates to the new Emacs-byte-based formatter via a simple UTF-8 →
/// Emacs encoding pass (for pure Unicode text this is a no-op).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn format_lisp_string_bytes_inner(s: &str, options: &PrintOptions) -> Vec<u8> {
    if storage_has_special_units(s) {
        use crate::emacs_core::emacs_char;

        let units = scan_storage_units_auto(s);
        let is_multibyte = units.iter().any(|unit| unit.code > 0xFF);
        let mut data = Vec::with_capacity(s.len());
        for unit in units {
            if !is_multibyte {
                data.push(unit.code as u8);
                continue;
            }

            let mut buf = [0u8; emacs_char::MAX_MULTIBYTE_LENGTH];
            let n = emacs_char::char_string(unit.code, &mut buf);
            data.extend_from_slice(&buf[..n]);
        }
        return format_lisp_string_bytes_inner_emacs(&data, is_multibyte, options);
    }

    format_lisp_string_bytes_inner_emacs(s.as_bytes(), true, options)
}

fn is_hex_digit(b: u8) -> bool {
    b.is_ascii_hexdigit()
}

// ---------------------------------------------------------------------------
// Emacs-encoding-native utilities (replace sentinel-based versions)
// ---------------------------------------------------------------------------

/// Compute Emacs display width from bytes in Emacs internal encoding.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn display_width_emacs(data: &[u8], is_multibyte: bool) -> usize {
    use crate::emacs_core::emacs_char;
    if is_multibyte {
        let mut width = 0usize;
        let mut pos = 0;
        while pos < data.len() {
            let (code, len) = emacs_char::string_char(&data[pos..]);
            pos += len;
            if emacs_char::char_byte8_p(code) {
                width += 4; // raw bytes display as \xNN (4 chars)
            } else if let Some(ch) = char::from_u32(code) {
                width += crate::encoding::char_width(ch);
            } else {
                width += 1;
            }
        }
        width
    } else {
        let mut width = 0usize;
        for &b in data {
            if b < 0x80 {
                if let Some(ch) = char::from_u32(b as u32) {
                    width += crate::encoding::char_width(ch);
                } else {
                    width += 1;
                }
            } else {
                width += 1; // unibyte non-ASCII bytes are 1 column
            }
        }
        width
    }
}

/// Decode Emacs-encoded bytes to character code + display width pairs.
pub(crate) fn decode_units_emacs(data: &[u8], is_multibyte: bool) -> Vec<(u32, usize)> {
    use crate::emacs_core::emacs_char;
    if is_multibyte {
        let mut out = Vec::new();
        let mut pos = 0;
        while pos < data.len() {
            let (code, len) = emacs_char::string_char(&data[pos..]);
            pos += len;
            let width = if emacs_char::char_byte8_p(code) {
                4
            } else if let Some(ch) = char::from_u32(code) {
                crate::encoding::char_width(ch)
            } else {
                1
            };
            out.push((code, width));
        }
        out
    } else {
        data.iter()
            .map(|&b| {
                let width = if b < 0x80 {
                    char::from_u32(b as u32)
                        .map(crate::encoding::char_width)
                        .unwrap_or(1)
                } else {
                    1
                };
                (b as u32, width)
            })
            .collect()
    }
}
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
