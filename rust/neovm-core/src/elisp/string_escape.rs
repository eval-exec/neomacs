//! Shared Lisp string escaping helpers.

use std::iter::Peekable;
use std::str::Chars;

const RAW_BYTE_SENTINEL_BASE: u32 = 0xE000;
const RAW_BYTE_SENTINEL_MIN: u32 = 0xE080;
const RAW_BYTE_SENTINEL_MAX: u32 = 0xE0FF;
const RAW_BYTE_CHAR_MIN: u32 = 0x3FFF80;
const RAW_BYTE_CHAR_MAX: u32 = 0x3FFFFF;

const EXT_SEQ_PREFIX: u32 = 0xE100;
const EXT_SEQ_LEN_BASE: u32 = 0xE110;
const EXT_SEQ_BYTE_BASE: u32 = 0xE200;
const EXT_SEQ_MAX_LEN: u32 = 6;

/// Encode non-Unicode Emacs character codes as NeoVM internal sentinels.
///
/// Returns `None` for Unicode scalar values (which can be stored directly).
pub(crate) fn encode_nonunicode_char_for_storage(code: u32) -> Option<String> {
    if code <= 0x10FFFF {
        return None;
    }

    if (RAW_BYTE_CHAR_MIN..=RAW_BYTE_CHAR_MAX).contains(&code) {
        // Emacs raw-byte chars map 0x3FFF80..0x3FFFFF -> 0x80..0xFF.
        let raw = code - 0x3FFF00;
        let ch = char::from_u32(RAW_BYTE_SENTINEL_BASE + raw).expect("valid raw-byte sentinel");
        return Some(ch.to_string());
    }

    if code <= 0x3FFFFF {
        let bytes = encode_emacs_extended_utf8(code);
        return Some(encode_extended_sequence_for_storage(&bytes));
    }

    None
}

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
        [b0, b1] if (b0 & 0xE0) == 0xC0 && (b1 & 0xC0) == 0x80 => {
            Some((((b0 & 0x1F) as u32) << 6) | ((b1 & 0x3F) as u32))
        }
        [b0, b1, b2]
            if (b0 & 0xF0) == 0xE0 && (b1 & 0xC0) == 0x80 && (b2 & 0xC0) == 0x80 =>
        {
            Some(
                (((b0 & 0x0F) as u32) << 12)
                    | (((b1 & 0x3F) as u32) << 6)
                    | ((b2 & 0x3F) as u32),
            )
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

fn encode_extended_sequence_for_storage(bytes: &[u8]) -> String {
    let mut out = String::new();
    out.push(char::from_u32(EXT_SEQ_PREFIX).expect("valid extended prefix sentinel"));
    let len_char = char::from_u32(EXT_SEQ_LEN_BASE + bytes.len() as u32)
        .expect("valid extended length sentinel");
    out.push(len_char);
    for b in bytes {
        out.push(
            char::from_u32(EXT_SEQ_BYTE_BASE + (*b as u32))
                .expect("valid extended byte sentinel"),
        );
    }
    out
}

/// Losslessly encode potentially non-UTF-8 bytes into internal string storage.
pub(crate) fn bytes_to_storage_string(bytes: &[u8]) -> String {
    if let Ok(utf8) = String::from_utf8(bytes.to_vec()) {
        return utf8;
    }

    let mut out = String::new();
    let max_chunk = EXT_SEQ_MAX_LEN as usize;
    for chunk in bytes.chunks(max_chunk) {
        out.push_str(&encode_extended_sequence_for_storage(chunk));
    }
    out
}

fn decode_storage_units(s: &str) -> Vec<(u32, usize)> {
    let mut out = Vec::new();
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        let code = ch as u32;
        if (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&code) {
            let raw = (code - RAW_BYTE_SENTINEL_BASE) as u8;
            out.push((0x3FFF00 + raw as u32, 4));
            continue;
        }

        if code == EXT_SEQ_PREFIX {
            if let Some(bytes) = decode_extended_sequence(&mut chars) {
                if let Some(cp) = decode_emacs_extended_utf8(&bytes) {
                    out.push((cp, 1));
                    continue;
                }
            }
        }

        out.push((code, 1));
    }

    out
}

/// Decode NeoVM string storage into Emacs character codes.
pub(crate) fn decode_storage_char_codes(s: &str) -> Vec<u32> {
    decode_storage_units(s).into_iter().map(|(cp, _)| cp).collect()
}

/// Compute Emacs-like display width for NeoVM string storage.
pub(crate) fn storage_string_display_width(s: &str) -> usize {
    decode_storage_units(s).into_iter().map(|(_, width)| width).sum()
}

fn decode_extended_sequence(chars: &mut Peekable<Chars<'_>>) -> Option<Vec<u8>> {
    let len_char = chars.peek().copied()?;
    let len_code = len_char as u32;
    if !(EXT_SEQ_LEN_BASE + 1..=EXT_SEQ_LEN_BASE + EXT_SEQ_MAX_LEN).contains(&len_code) {
        return None;
    }
    chars.next();
    let len = (len_code - EXT_SEQ_LEN_BASE) as usize;

    let mut out = Vec::with_capacity(len);
    for _ in 0..len {
        let b_char = chars.peek().copied()?;
        let b_code = b_char as u32;
        if !(EXT_SEQ_BYTE_BASE..=EXT_SEQ_BYTE_BASE + 0xFF).contains(&b_code) {
            return None;
        }
        chars.next();
        out.push((b_code - EXT_SEQ_BYTE_BASE) as u8);
    }
    Some(out)
}

fn push_octal_escape(out: &mut Vec<u8>, byte: u8) {
    out.push(b'\\');
    out.extend_from_slice(format!("{:03o}", byte).as_bytes());
}

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

/// Format a Rust string as an Emacs Lisp string literal, preserving byte-level
/// sentinels via lossy UTF-8 conversion when invalid byte sequences occur.
pub(crate) fn format_lisp_string(s: &str) -> String {
    String::from_utf8_lossy(&format_lisp_string_bytes(s)).into_owned()
}

/// Format a Rust string as an Emacs Lisp string literal byte sequence.
pub(crate) fn format_lisp_string_bytes(s: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() + 2);
    out.push(b'"');

    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        let code = ch as u32;
        if (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&code) {
            let byte = (code - RAW_BYTE_SENTINEL_BASE) as u8;
            push_octal_escape(&mut out, byte);
            continue;
        }

        if code == EXT_SEQ_PREFIX {
            if let Some(bytes) = decode_extended_sequence(&mut chars) {
                for b in bytes {
                    push_escaped_literal_byte(&mut out, b);
                }
                continue;
            }
        }

        match ch {
            '"' => out.extend_from_slice(br#"\""#),
            '\\' => out.extend_from_slice(br#"\\"#),
            '\u{08}' => out.extend_from_slice(br#"\b"#),
            '\t' => out.extend_from_slice(br#"\t"#),
            '\n' => out.extend_from_slice(br#"\n"#),
            '\u{0b}' => out.extend_from_slice(br#"\v"#),
            '\u{0c}' => out.extend_from_slice(br#"\f"#),
            '\r' => out.extend_from_slice(br#"\r"#),
            '\u{07}' => out.extend_from_slice(br#"\a"#),
            '\u{1b}' => out.extend_from_slice(br#"\e"#),
            c if (c as u32) < 0x20 || c == '\u{7f}' => push_octal_escape(&mut out, c as u8),
            _ => {
                let mut tmp = [0u8; 4];
                let bytes = ch.encode_utf8(&mut tmp).as_bytes();
                out.extend_from_slice(bytes);
            }
        }
    }

    out.push(b'"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_control_chars() {
        assert_eq!(format_lisp_string("\n\t"), "\"\\n\\t\"");
        assert_eq!(format_lisp_string("\u{7f}"), "\"\\177\"");
    }

    #[test]
    fn keeps_non_bmp_visible() {
        assert_eq!(format_lisp_string("\u{10ffff}"), "\"\u{10ffff}\"");
    }

    #[test]
    fn escapes_raw_byte_sentinel_as_octal() {
        let raw_377 = char::from_u32(0xE0FF).expect("valid sentinel scalar");
        assert_eq!(
            format_lisp_string(&raw_377.to_string()),
            "\"\\377\""
        );
    }

    #[test]
    fn encode_nonunicode_char_uses_obsolete_utf8_bytes() {
        let encoded = encode_nonunicode_char_for_storage(0x110000)
            .expect("non-unicode char should be encoded");
        assert_eq!(
            format_lisp_string_bytes(&encoded),
            vec![b'"', 0xF4, 0x90, 0x80, 0x80, b'"']
        );
    }

    #[test]
    fn encode_nonunicode_char_uses_five_byte_sequence() {
        let encoded = encode_nonunicode_char_for_storage(0x200000)
            .expect("non-unicode char should be encoded");
        assert_eq!(
            format_lisp_string_bytes(&encoded),
            vec![b'"', 0xF8, 0x88, 0x80, 0x80, 0x80, b'"']
        );
    }

    #[test]
    fn bytes_to_storage_round_trips_non_utf8() {
        let raw = vec![0xF4, 0x90, 0x80, 0x80, 0x41];
        let encoded = bytes_to_storage_string(&raw);
        assert_eq!(
            format_lisp_string_bytes(&encoded),
            vec![b'"', 0xF4, 0x90, 0x80, 0x80, b'A', b'"']
        );
    }

    #[test]
    fn decode_storage_char_codes_handles_nonunicode_and_raw_byte() {
        let encoded = format!(
            "{}{}",
            encode_nonunicode_char_for_storage(0x110000).expect("should encode"),
            encode_nonunicode_char_for_storage(0x3FFFFF).expect("raw byte should encode")
        );
        assert_eq!(decode_storage_char_codes(&encoded), vec![0x110000, 0x3FFFFF]);
        assert_eq!(storage_string_display_width(&encoded), 5);
    }
}
