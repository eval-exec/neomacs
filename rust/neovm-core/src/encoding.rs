//! Character encoding, multibyte support, and character utilities.
//!
//! Neomacs uses UTF-8 internally.  This module provides Emacs-compatible
//! character classification, width calculation, and encoding conversion
//! APIs.

use crate::elisp::value::Value;
use crate::elisp::string_escape::storage_byte_len;

// ---------------------------------------------------------------------------
// Character classification
// ---------------------------------------------------------------------------

/// Character width for display purposes (East Asian width).
pub fn char_width(c: char) -> usize {
    let cp = c as u32;
    if cp == 0 {
        return 0;
    }
    // Control characters
    if cp < 0x20 || (0x7f..=0x9f).contains(&cp) {
        return if cp == 0x09 { 8 } else { 2 }; // ^X notation = 2 cols
    }
    // Non-spacing marks
    if is_zero_width(c) {
        return 0;
    }
    // Wide characters (CJK, etc.)
    if is_wide_char(c) {
        return 2;
    }
    1
}

/// Whether the character is zero-width (combining mark, etc.).
fn is_zero_width(c: char) -> bool {
    let cp = c as u32;
    // Common combining mark ranges
    (0x0300..=0x036f).contains(&cp) // Combining Diacriticals
        || (0x0483..=0x0489).contains(&cp) // Cyrillic combining
        || (0x0591..=0x05bd).contains(&cp) // Hebrew
        || (0x0610..=0x061a).contains(&cp) // Arabic
        || (0x064b..=0x065f).contains(&cp)
        || (0x0670..=0x0670).contains(&cp)
        || (0x06d6..=0x06dc).contains(&cp)
        || (0x0730..=0x074a).contains(&cp) // Syriac
        || (0x0900..=0x0903).contains(&cp) // Devanagari
        || (0x093a..=0x094f).contains(&cp)
        || (0x0e31..=0x0e3a).contains(&cp) // Thai
        || (0x0e47..=0x0e4e).contains(&cp)
        || (0x1160..=0x11ff).contains(&cp) // Hangul jungseong/jongseong
        || (0x200b..=0x200f).contains(&cp) // Zero-width space, ZWNJ, ZWJ
        || (0x202a..=0x202e).contains(&cp) // Bidi control
        || (0x2060..=0x2064).contains(&cp) // Invisible operators
        || (0xfe00..=0xfe0f).contains(&cp) // Variation selectors
        || (0xfe20..=0xfe2f).contains(&cp) // Combining half marks
        || (0xfeff..=0xfeff).contains(&cp) // BOM
        || (0x1d167..=0x1d169).contains(&cp) // Musical combining
        || (0x1d173..=0x1d182).contains(&cp)
        || (0xe0020..=0xe007f).contains(&cp) // Tags
        || (0xe0100..=0xe01ef).contains(&cp) // Variation selectors supplement
}

/// Whether the character is full-width (East Asian wide).
fn is_wide_char(c: char) -> bool {
    let cp = c as u32;
    // CJK Unified Ideographs
    (0x1100..=0x115f).contains(&cp) // Hangul Jamo
        || (0x2e80..=0x303e).contains(&cp) // CJK Radicals, Kangxi, etc.
        || (0x3040..=0x33bf).contains(&cp) // Hiragana, Katakana, CJK compat
        || (0x3400..=0x4dbf).contains(&cp) // CJK Extension A
        || (0x4e00..=0x9fff).contains(&cp) // CJK Unified Ideographs
        || (0xa000..=0xa4cf).contains(&cp) // Yi
        || (0xac00..=0xd7a3).contains(&cp) // Hangul Syllables
        || (0xf900..=0xfaff).contains(&cp) // CJK Compatibility Ideographs
        || (0xfe10..=0xfe19).contains(&cp) // Vertical forms
        || (0xfe30..=0xfe6b).contains(&cp) // CJK Compatibility Forms
        || (0xff01..=0xff60).contains(&cp) // Fullwidth forms
        || (0xffe0..=0xffe6).contains(&cp) // Fullwidth signs
        || (0x1f200..=0x1f2ff).contains(&cp) // Enclosed ideographic
        || (0x1f300..=0x1f9ff).contains(&cp) // Emoji (most are wide)
        || (0x20000..=0x2ffff).contains(&cp) // CJK Extension B-F
        || (0x30000..=0x3ffff).contains(&cp) // CJK Extension G+
}

/// String display width (sum of char widths).
pub fn string_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// Whether a character is printable (not a control char).
pub fn is_printable(c: char) -> bool {
    let cp = c as u32;
    cp >= 0x20 && cp != 0x7f && !(0x80..=0x9f).contains(&cp)
}

/// Whether a character is a whitespace character.
pub fn is_whitespace(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0b' | '\x0c')
}

/// Whether a character is a word constituent (alphanumeric + underscore).
pub fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// Whether a string is all ASCII.
pub fn is_ascii_string(s: &str) -> bool {
    s.bytes().all(|b| b < 128)
}

/// Whether a string is multibyte (contains non-ASCII).
pub fn is_multibyte_string(s: &str) -> bool {
    !is_ascii_string(s)
}

// ---------------------------------------------------------------------------
// Encoding conversion
// ---------------------------------------------------------------------------

/// Encode a string to bytes using the specified coding system.
/// Currently only UTF-8 is supported.
pub fn encode_string(s: &str, coding_system: &str) -> Vec<u8> {
    match coding_system {
        "utf-8" | "utf-8-unix" | "utf-8-dos" | "utf-8-mac" => s.as_bytes().to_vec(),
        "latin-1" | "iso-8859-1" | "iso-latin-1" => s
            .chars()
            .map(|c| if (c as u32) <= 0xff { c as u8 } else { b'?' })
            .collect(),
        "ascii" | "us-ascii" => s
            .chars()
            .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
            .collect(),
        _ => s.as_bytes().to_vec(), // default to UTF-8
    }
}

/// Decode bytes to a string using the specified coding system.
/// Currently only UTF-8 is supported.
pub fn decode_bytes(bytes: &[u8], coding_system: &str) -> String {
    match coding_system {
        "utf-8" | "utf-8-unix" | "utf-8-dos" | "utf-8-mac" => {
            String::from_utf8_lossy(bytes).into_owned()
        }
        "latin-1" | "iso-8859-1" | "iso-latin-1" => bytes.iter().map(|&b| b as char).collect(),
        "ascii" | "us-ascii" => bytes
            .iter()
            .map(|&b| if b < 128 { b as char } else { '?' })
            .collect(),
        _ => String::from_utf8_lossy(bytes).into_owned(),
    }
}

// ---------------------------------------------------------------------------
// Byte/char position conversion
// ---------------------------------------------------------------------------

/// Convert character position to byte position in a UTF-8 string.
pub fn char_to_byte_pos(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(byte_pos, _)| byte_pos)
        .unwrap_or(s.len())
}

/// Convert byte position to character position in a UTF-8 string.
pub fn byte_to_char_pos(s: &str, byte_pos: usize) -> usize {
    s[..byte_pos.min(s.len())].chars().count()
}

// ---------------------------------------------------------------------------
// Glyphless character representation
// ---------------------------------------------------------------------------

/// How to display a glyphless (control/non-printable) character.
pub fn glyphless_char_display(c: char) -> String {
    let cp = c as u32;
    if cp < 0x20 {
        format!("^{}", (cp + 0x40) as u8 as char)
    } else if cp == 0x7f {
        "^?".to_string()
    } else if cp < 0x100 {
        format!("\\{:03o}", cp)
    } else if cp < 0x10000 {
        format!("\\u{:04X}", cp)
    } else {
        format!("\\U{:08X}", cp)
    }
}

// ---------------------------------------------------------------------------
// Builtins
// ---------------------------------------------------------------------------

use crate::elisp::error::{signal, EvalResult};

fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), crate::elisp::error::Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_min_args(
    name: &str,
    args: &[Value],
    min: usize,
) -> Result<(), crate::elisp::error::Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_string(val: &Value) -> Result<String, crate::elisp::error::Flow> {
    match val {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

/// `(char-width CHAR)` -> integer
pub(crate) fn builtin_char_width(args: Vec<Value>) -> EvalResult {
    expect_args("char-width", &args, 1)?;
    let c = match &args[0] {
        Value::Char(c) => *c,
        Value::Int(n) => char::from_u32(*n as u32).unwrap_or('?'),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("characterp"), other.clone()],
            ))
        }
    };
    Ok(Value::Int(char_width(c) as i64))
}

/// `(string-width STRING)` — already exists, but this provides the backing impl.
pub(crate) fn builtin_string_width(args: Vec<Value>) -> EvalResult {
    expect_args("string-width", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::Int(string_width(&s) as i64))
}

/// `(string-bytes STRING)` -> integer byte length of STRING.
pub(crate) fn builtin_string_bytes(args: Vec<Value>) -> EvalResult {
    expect_args("string-bytes", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::Int(storage_byte_len(&s) as i64))
}

/// `(multibyte-string-p STRING)` -> t or nil
pub(crate) fn builtin_multibyte_string_p(args: Vec<Value>) -> EvalResult {
    expect_args("multibyte-string-p", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::bool(is_multibyte_string(&s)))
}

/// `(unibyte-string-p STRING)` -> t or nil
pub(crate) fn builtin_unibyte_string_p(args: Vec<Value>) -> EvalResult {
    expect_args("unibyte-string-p", &args, 1)?;
    let s = expect_string(&args[0])?;
    Ok(Value::bool(is_ascii_string(&s)))
}

/// `(encode-coding-string STRING CODING-SYSTEM)` -> string
pub(crate) fn builtin_encode_coding_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("encode-coding-string", &args, 2)?;
    let s = expect_string(&args[0])?;
    let coding = match &args[1] {
        Value::Symbol(s) => s.clone(),
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), other.clone()],
            ))
        }
    };
    let bytes = encode_string(&s, &coding);
    // Return as unibyte string (for now, just convert back)
    let result: String = bytes.iter().map(|&b| b as char).collect();
    Ok(Value::string(result))
}

/// `(decode-coding-string STRING CODING-SYSTEM)` -> string
pub(crate) fn builtin_decode_coding_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("decode-coding-string", &args, 2)?;
    let s = expect_string(&args[0])?;
    let coding = match &args[1] {
        Value::Symbol(s) => s.clone(),
        Value::Str(s) => (**s).clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), other.clone()],
            ))
        }
    };
    let bytes: Vec<u8> = s.chars().map(|c| c as u8).collect();
    let result = decode_bytes(&bytes, &coding);
    Ok(Value::string(result))
}

/// `(char-or-string-p OBJ)` -> t or nil
pub(crate) fn builtin_char_or_string_p(args: Vec<Value>) -> EvalResult {
    expect_args("char-or-string-p", &args, 1)?;
    Ok(Value::bool(matches!(
        &args[0],
        Value::Char(_) | Value::Str(_) | Value::Int(_)
    )))
}

/// `(max-char)` -> integer
pub(crate) fn builtin_max_char(args: Vec<Value>) -> EvalResult {
    let _ = args; // 0 args
    Ok(Value::Int(0x10FFFF))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_width() {
        assert_eq!(char_width('a'), 1);
        assert_eq!(char_width(' '), 1);
        assert_eq!(char_width('Z'), 1);
    }

    #[test]
    fn cjk_width() {
        assert_eq!(char_width('中'), 2);
        assert_eq!(char_width('日'), 2);
        assert_eq!(char_width('あ'), 2);
        assert_eq!(char_width('ア'), 2);
    }

    #[test]
    fn control_char_width() {
        assert_eq!(char_width('\0'), 0);
        assert_eq!(char_width('\x01'), 2); // ^A
        assert_eq!(char_width('\x7f'), 2); // ^?
    }

    #[test]
    fn string_width_mixed() {
        assert_eq!(string_width("hello"), 5);
        assert_eq!(string_width("中文"), 4);
        assert_eq!(string_width("hi中"), 4);
    }

    #[test]
    fn builtin_string_bytes_counts_utf8_length() {
        let result = builtin_string_bytes(vec![Value::string("Aé中")]).unwrap();
        assert_eq!(result, Value::Int(6));
    }

    #[test]
    fn char_byte_conversion() {
        let s = "hello中文";
        assert_eq!(char_to_byte_pos(s, 5), 5);
        assert_eq!(char_to_byte_pos(s, 6), 8); // '中' is 3 bytes
        assert_eq!(byte_to_char_pos(s, 5), 5);
        assert_eq!(byte_to_char_pos(s, 8), 6);
    }

    #[test]
    fn encoding_utf8() {
        let bytes = encode_string("hello", "utf-8");
        assert_eq!(bytes, b"hello");
        let decoded = decode_bytes(b"hello", "utf-8");
        assert_eq!(decoded, "hello");
    }

    #[test]
    fn encoding_latin1() {
        let bytes = encode_string("café", "latin-1");
        assert_eq!(bytes.len(), 4); // é maps to 0xe9
        let decoded = decode_bytes(&[0x63, 0x61, 0x66, 0xe9], "latin-1");
        assert_eq!(decoded, "café");
    }

    #[test]
    fn glyphless_display() {
        assert_eq!(glyphless_char_display('\x01'), "^A");
        assert_eq!(glyphless_char_display('\x7f'), "^?");
        assert_eq!(glyphless_char_display('\u{FEFF}'), "\\uFEFF");
    }

    #[test]
    fn multibyte_detection() {
        assert!(!is_multibyte_string("hello"));
        assert!(is_multibyte_string("héllo"));
        assert!(is_multibyte_string("中文"));
    }

    #[test]
    fn printable_check() {
        assert!(is_printable('a'));
        assert!(is_printable('中'));
        assert!(!is_printable('\x00'));
        assert!(!is_printable('\x7f'));
    }
}
