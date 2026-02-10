//! Pure Rust character and coding utilities.
//!
//! Implements core character utilities analogous to Emacs character.c and casetab.c.
//! Self-contained with no external crate dependencies beyond std.

// ---------------------------------------------------------------------------
// 1. UTF-8 Encoding / Decoding
// ---------------------------------------------------------------------------

/// Encode a single `char` into the provided UTF-8 byte buffer.
/// Returns the number of bytes written (1..=4).
///
/// # Panics
/// Panics if `buf` is too small for the encoded character.
#[inline]
pub fn encode_utf8(ch: char, buf: &mut [u8]) -> usize {
    let s = ch.encode_utf8(buf);
    s.len()
}

/// Decode a single UTF-8 character from the beginning of `bytes`.
/// Returns `(char, bytes_consumed)` or `None` on invalid / empty input.
pub fn decode_utf8(bytes: &[u8]) -> Option<(char, usize)> {
    if bytes.is_empty() {
        return None;
    }
    let width = utf8_char_width(bytes[0]);
    if width == 0 || bytes.len() < width {
        return None;
    }
    let s = std::str::from_utf8(&bytes[..width]).ok()?;
    let ch = s.chars().next()?;
    Some((ch, width))
}

/// Determine the UTF-8 byte length of a character from its first byte.
/// Returns 0 for invalid leading bytes (continuation bytes 0x80..=0xBF, or 0xF8+).
#[inline]
pub fn utf8_char_width(first_byte: u8) -> usize {
    match first_byte {
        0x00..=0x7F => 1,
        0xC0..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF7 => 4,
        _ => 0, // continuation byte or invalid
    }
}

/// Return the number of UTF-8 bytes required to encode `ch`.
#[inline]
pub fn char_bytes(ch: char) -> usize {
    ch.len_utf8()
}

// ---------------------------------------------------------------------------
// 2. Character Width (Display)
// ---------------------------------------------------------------------------

/// Return the display width in terminal columns of `ch`.
///
/// - Fullwidth / Wide (CJK, Hangul, etc.) -> 2
/// - Combining marks -> 0
/// - Everything else -> 1
/// - ASCII control characters -> 0
#[inline]
pub fn char_display_width(ch: char) -> usize {
    let cp = ch as u32;

    // ASCII fast path
    if cp < 0x80 {
        // Control characters have zero display width
        if cp < 0x20 || cp == 0x7F {
            return 0;
        }
        return 1;
    }

    // Combining marks -> 0
    if is_combining_mark(ch) {
        return 0;
    }

    // Soft hyphen
    if cp == 0x00AD {
        return 1;
    }

    // Zero-width characters
    if cp == 0x200B // ZERO WIDTH SPACE
        || cp == 0x200C // ZERO WIDTH NON-JOINER
        || cp == 0x200D // ZERO WIDTH JOINER
        || cp == 0x2060 // WORD JOINER
        || cp == 0xFEFF // BOM / ZWNBSP
    {
        return 0;
    }

    // Wide / Fullwidth ranges (East Asian Width W/F)
    if is_wide_char(cp) {
        return 2;
    }

    1
}

/// Return `true` if the codepoint is in a Wide or Fullwidth East Asian Width range.
#[inline]
fn is_wide_char(cp: u32) -> bool {
    // Hangul Jamo
    (0x1100..=0x115F).contains(&cp)
    || (0x2329..=0x232A).contains(&cp)
    // CJK Miscellaneous
    || (0x2E80..=0x303E).contains(&cp)
    // Hiragana, Katakana, Bopomofo, CJK strokes, Katakana ext, CJK enclosed, CJK compat
    || (0x3040..=0x33FF).contains(&cp)
    // CJK Unified Ideographs Extension A
    || (0x3400..=0x4DBF).contains(&cp)
    // CJK Unified Ideographs
    || (0x4E00..=0x9FFF).contains(&cp)
    // Hangul Syllables
    || (0xAC00..=0xD7A3).contains(&cp)
    // CJK Compatibility Ideographs
    || (0xF900..=0xFAFF).contains(&cp)
    // CJK Compatibility Forms, Small Form Variants, CJK half/full forms
    || (0xFE10..=0xFE19).contains(&cp)
    || (0xFE30..=0xFE6F).contains(&cp)
    // Fullwidth forms (FF01..FF60 are fullwidth ASCII variants)
    || (0xFF01..=0xFF60).contains(&cp)
    // Fullwidth signs (cent, pound, etc.)
    || (0xFFE0..=0xFFE6).contains(&cp)
    // Supplementary CJK (Ext B through H+)
    || (0x20000..=0x2FFFF).contains(&cp)
    || (0x30000..=0x3FFFF).contains(&cp)
}

/// Total display width (in columns) of a string.
pub fn string_display_width(s: &str) -> usize {
    s.chars().map(char_display_width).sum()
}

// ---------------------------------------------------------------------------
// 3. Case Conversion
// ---------------------------------------------------------------------------

/// Convert a character to its uppercase equivalent.
/// For characters that map to multiple codepoints (e.g. German eszett),
/// only the first codepoint is returned.
#[inline]
pub fn char_upcase(ch: char) -> char {
    ch.to_uppercase().next().unwrap_or(ch)
}

/// Convert a character to its lowercase equivalent.
#[inline]
pub fn char_downcase(ch: char) -> char {
    ch.to_lowercase().next().unwrap_or(ch)
}

/// Convert a character to its titlecase equivalent.
/// For most characters this is the same as uppercase. A few Unicode codepoints
/// (e.g. U+01F3 'dz' -> U+01F2 'Dz') have distinct titlecase forms, but
/// Rust's standard library does not expose titlecase directly, so we fall back
/// to uppercase.
#[inline]
pub fn char_titlecase(ch: char) -> char {
    // Rust std does not have char::to_titlecase — use uppercase as proxy.
    char_upcase(ch)
}

/// Return `true` if the character is an uppercase letter.
#[inline]
pub fn is_upper(ch: char) -> bool {
    ch.is_uppercase()
}

/// Return `true` if the character is a lowercase letter.
#[inline]
pub fn is_lower(ch: char) -> bool {
    ch.is_lowercase()
}

/// Convert an entire string to uppercase.
pub fn string_upcase(s: &str) -> String {
    s.to_uppercase()
}

/// Convert an entire string to lowercase.
pub fn string_downcase(s: &str) -> String {
    s.to_lowercase()
}

// ---------------------------------------------------------------------------
// 4. Character Classification
// ---------------------------------------------------------------------------

/// Return `true` if `ch` is an ASCII decimal digit ('0'..='9').
#[inline]
pub fn is_ascii_digit(ch: char) -> bool {
    ch.is_ascii_digit()
}

/// Return `true` if `ch` is an ASCII letter ('a'..='z' | 'A'..='Z').
#[inline]
pub fn is_ascii_alpha(ch: char) -> bool {
    ch.is_ascii_alphabetic()
}

/// Return `true` if `ch` is an ASCII letter or digit.
#[inline]
pub fn is_ascii_alnum(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
}

/// Return `true` if `ch` is a whitespace character: space, tab, newline,
/// form-feed, or carriage return.
#[inline]
pub fn is_whitespace(ch: char) -> bool {
    matches!(ch, ' ' | '\t' | '\n' | '\x0C' | '\r')
}

/// Return `true` if `ch` is a "word constituent" character in the Emacs
/// default sense: alphanumeric (Unicode) or underscore.
#[inline]
pub fn is_word_char(ch: char) -> bool {
    ch.is_alphanumeric() || ch == '_'
}

/// Return `true` if `ch` is a printable character (has a visible glyph).
/// Excludes control characters, surrogates, and noncharacters.
#[inline]
pub fn is_printable(ch: char) -> bool {
    let cp = ch as u32;
    if cp < 0x20 || cp == 0x7F {
        return false;
    }
    // C1 control characters
    if (0x80..=0x9F).contains(&cp) {
        return false;
    }
    // Noncharacters
    if (0xFDD0..=0xFDEF).contains(&cp) {
        return false;
    }
    if cp & 0xFFFE == 0xFFFE && cp <= 0x10FFFF {
        return false;
    }
    true
}

/// Return `true` if `ch` is a Unicode combining mark (general categories Mn, Mc, Me).
#[inline]
pub fn is_combining_mark(ch: char) -> bool {
    let cp = ch as u32;

    // Combining Diacritical Marks (Mn)
    (0x0300..=0x036F).contains(&cp)
    // Combining Diacritical Marks Extended (Mn)
    || (0x1AB0..=0x1AFF).contains(&cp)
    // Combining Diacritical Marks Supplement (Mn)
    || (0x1DC0..=0x1DFF).contains(&cp)
    // Combining Diacritical Marks for Symbols (Mn)
    || (0x20D0..=0x20FF).contains(&cp)
    // Combining Half Marks (Mn)
    || (0xFE20..=0xFE2F).contains(&cp)
    // Hebrew combining marks (selected Mn ranges)
    || (0x0591..=0x05BD).contains(&cp)
    || cp == 0x05BF
    || (0x05C1..=0x05C2).contains(&cp)
    || (0x05C4..=0x05C5).contains(&cp)
    || cp == 0x05C7
    // Arabic combining marks
    || (0x0610..=0x061A).contains(&cp)
    || (0x064B..=0x065F).contains(&cp)
    || cp == 0x0670
    || (0x06D6..=0x06DC).contains(&cp)
    || (0x06DF..=0x06E4).contains(&cp)
    || (0x06E7..=0x06E8).contains(&cp)
    || (0x06EA..=0x06ED).contains(&cp)
    // Devanagari combining marks (Mn range)
    || (0x0900..=0x0903).contains(&cp)
    || (0x093A..=0x094F).contains(&cp)
    || (0x0951..=0x0957).contains(&cp)
    || (0x0962..=0x0963).contains(&cp)
    // Thai combining marks
    || (0x0E31..=0x0E31).contains(&cp)
    || (0x0E34..=0x0E3A).contains(&cp)
    || (0x0E47..=0x0E4E).contains(&cp)
    // Hangul Jamo combining (trailing consonants / vowels for syllable blocks)
    || (0x1160..=0x11FF).contains(&cp)
    // Variation Selectors
    || (0xFE00..=0xFE0F).contains(&cp)
    // Variation Selectors Supplement
    || (0xE0100..=0xE01EF).contains(&cp)
}

/// Return `true` if `ch` is a control character (C0, DEL, or C1).
#[inline]
pub fn is_control(ch: char) -> bool {
    let cp = ch as u32;
    cp < 0x20 || cp == 0x7F || (0x80..=0x9F).contains(&cp)
}

// ---------------------------------------------------------------------------
// 5. Character Properties — GeneralCategory
// ---------------------------------------------------------------------------

/// Simplified Unicode General Category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GeneralCategory {
    /// Letters (Lu, Ll, Lt, Lm, Lo)
    Letter,
    /// Marks (Mn, Mc, Me)
    Mark,
    /// Numbers (Nd, Nl, No)
    Number,
    /// Punctuation (Pc, Pd, Ps, Pe, Pi, Pf, Po)
    Punct,
    /// Symbols (Sm, Sc, Sk, So)
    Symbol,
    /// Separators (Zs, Zl, Zp)
    Separator,
    /// Other (Cc, Cf, Cs, Co, Cn)
    Other,
}

/// Return the simplified Unicode General Category of `ch`.
///
/// This uses heuristic ranges rather than a full Unicode database,
/// which is sufficient for display-engine purposes.
pub fn general_category(ch: char) -> GeneralCategory {
    let cp = ch as u32;

    // Control characters (C0, DEL, C1)
    if cp < 0x20 || cp == 0x7F || (0x80..=0x9F).contains(&cp) {
        return GeneralCategory::Other;
    }

    // Combining marks (already have a function)
    if is_combining_mark(ch) {
        return GeneralCategory::Mark;
    }

    // ASCII fast paths
    if cp < 0x80 {
        return match ch {
            '0'..='9' => GeneralCategory::Number,
            'A'..='Z' | 'a'..='z' => GeneralCategory::Letter,
            ' ' => GeneralCategory::Separator,
            '!'..='/' | ':'..='@' | '['..='`' | '{'..='~' => {
                // Distinguish punctuation from symbols for ASCII
                match ch {
                    '+' | '<' | '=' | '>' | '|' | '~' | '^' => GeneralCategory::Symbol,
                    '$' => GeneralCategory::Symbol,
                    _ => GeneralCategory::Punct,
                }
            }
            _ => GeneralCategory::Other,
        };
    }

    // Letters — broad ranges covering Latin Extended, Greek, Cyrillic,
    // Armenian, Georgian, CJK, Hangul, etc.
    if ch.is_alphabetic() {
        return GeneralCategory::Letter;
    }

    // Digits beyond ASCII
    if ch.is_numeric() {
        return GeneralCategory::Number;
    }

    // Whitespace / separator
    if ch.is_whitespace() {
        return GeneralCategory::Separator;
    }

    // Common symbol/currency blocks
    if (0x00A2..=0x00A9).contains(&cp)  // currency, copyright, etc.
        || (0x00AC..=0x00AE).contains(&cp)
        || cp == 0x00B0 || cp == 0x00B1
        || (0x00D7..=0x00D7).contains(&cp) // multiplication sign
        || cp == 0x00F7 // division sign
        || (0x2100..=0x214F).contains(&cp) // Letterlike Symbols
        || (0x2190..=0x21FF).contains(&cp) // Arrows
        || (0x2200..=0x22FF).contains(&cp) // Mathematical Operators
        || (0x2300..=0x23FF).contains(&cp) // Misc Technical
        || (0x2500..=0x257F).contains(&cp) // Box Drawing
        || (0x2580..=0x259F).contains(&cp) // Block Elements
        || (0x25A0..=0x25FF).contains(&cp) // Geometric Shapes
        || (0x2600..=0x26FF).contains(&cp) // Misc Symbols
        || (0x2700..=0x27BF).contains(&cp) // Dingbats
        || (0x2900..=0x297F).contains(&cp) // Supplemental Arrows-B
        || (0x2980..=0x29FF).contains(&cp) // Misc Mathematical Symbols-B
        || (0x2A00..=0x2AFF).contains(&cp) // Supplemental Mathematical Operators
        || (0x1F000..=0x1FFFF).contains(&cp) // Emoji/Symbols
    {
        return GeneralCategory::Symbol;
    }

    // Punctuation blocks
    if (0x2000..=0x206F).contains(&cp)  // General Punctuation
        || (0x3000..=0x303F).contains(&cp) // CJK Symbols and Punctuation
        || (0xFE30..=0xFE4F).contains(&cp) // CJK Compatibility Forms
        || (0xFE50..=0xFE6F).contains(&cp) // Small Form Variants
        || (0xFF01..=0xFF0F).contains(&cp) // Fullwidth punctuation
        || (0xFF1A..=0xFF20).contains(&cp)
        || (0xFF3B..=0xFF40).contains(&cp)
        || (0xFF5B..=0xFF65).contains(&cp)
    {
        return GeneralCategory::Punct;
    }

    // Format characters (Cf)
    if cp == 0x00AD  // SOFT HYPHEN
        || cp == 0x200B // ZERO WIDTH SPACE
        || cp == 0x200C || cp == 0x200D
        || cp == 0x2060 // WORD JOINER
        || cp == 0xFEFF // BOM
    {
        return GeneralCategory::Other;
    }

    // Private Use Area
    if (0xE000..=0xF8FF).contains(&cp)
        || (0xF0000..=0xFFFFD).contains(&cp)
        || (0x100000..=0x10FFFD).contains(&cp)
    {
        return GeneralCategory::Other;
    }

    // Default: if nothing matched, call it Other
    GeneralCategory::Other
}

// ---------------------------------------------------------------------------
// 6. Multibyte Utilities
// ---------------------------------------------------------------------------

/// Count the number of Unicode scalar values (characters) in `s`.
#[inline]
pub fn string_char_count(s: &str) -> usize {
    s.chars().count()
}

/// Convert a byte index in `s` to a character (scalar value) index.
///
/// # Panics
/// Panics if `byte_pos` is not on a character boundary or exceeds `s.len()`.
pub fn byte_to_char_pos(s: &str, byte_pos: usize) -> usize {
    assert!(
        byte_pos <= s.len(),
        "byte_pos {} out of range for string of length {}",
        byte_pos,
        s.len()
    );
    assert!(
        s.is_char_boundary(byte_pos),
        "byte_pos {} is not on a character boundary",
        byte_pos
    );
    s[..byte_pos].chars().count()
}

/// Convert a character index to a byte index in `s`.
///
/// # Panics
/// Panics if `char_pos` exceeds the number of characters.
pub fn char_to_byte_pos(s: &str, char_pos: usize) -> usize {
    s.char_indices()
        .nth(char_pos)
        .map(|(i, _)| i)
        .unwrap_or_else(|| {
            // char_pos might equal char count (pointing past the last char)
            if char_pos == s.chars().count() {
                s.len()
            } else {
                panic!(
                    "char_pos {} out of range for string with {} characters",
                    char_pos,
                    s.chars().count()
                );
            }
        })
}

/// Return the character at byte position `byte_pos` in `s`, or `None` if
/// `byte_pos` is out of range or not on a character boundary.
pub fn char_at_byte(s: &str, byte_pos: usize) -> Option<char> {
    if byte_pos >= s.len() {
        return None;
    }
    if !s.is_char_boundary(byte_pos) {
        return None;
    }
    s[byte_pos..].chars().next()
}

// ---------------------------------------------------------------------------
// 7. Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- UTF-8 encode / decode --

    #[test]
    fn test_encode_utf8_ascii() {
        let mut buf = [0u8; 4];
        let n = encode_utf8('A', &mut buf);
        assert_eq!(n, 1);
        assert_eq!(buf[0], b'A');
    }

    #[test]
    fn test_encode_utf8_two_byte() {
        let mut buf = [0u8; 4];
        let n = encode_utf8('\u{00E9}', &mut buf); // e-acute
        assert_eq!(n, 2);
        assert_eq!(&buf[..2], "\u{00E9}".as_bytes());
    }

    #[test]
    fn test_encode_utf8_three_byte() {
        let mut buf = [0u8; 4];
        let n = encode_utf8('\u{4E16}', &mut buf); // CJK: "world"
        assert_eq!(n, 3);
        assert_eq!(&buf[..3], "\u{4E16}".as_bytes());
    }

    #[test]
    fn test_encode_utf8_four_byte() {
        let mut buf = [0u8; 4];
        let n = encode_utf8('\u{1F600}', &mut buf); // grinning face emoji
        assert_eq!(n, 4);
        assert_eq!(&buf[..4], "\u{1F600}".as_bytes());
    }

    #[test]
    fn test_decode_utf8_ascii() {
        let bytes = b"Hello";
        let (ch, consumed) = decode_utf8(bytes).unwrap();
        assert_eq!(ch, 'H');
        assert_eq!(consumed, 1);
    }

    #[test]
    fn test_decode_utf8_multibyte() {
        let bytes = "\u{4E16}\u{754C}".as_bytes(); // "世界"
        let (ch, consumed) = decode_utf8(bytes).unwrap();
        assert_eq!(ch, '\u{4E16}');
        assert_eq!(consumed, 3);
        let (ch2, consumed2) = decode_utf8(&bytes[consumed..]).unwrap();
        assert_eq!(ch2, '\u{754C}');
        assert_eq!(consumed2, 3);
    }

    #[test]
    fn test_decode_utf8_empty() {
        assert!(decode_utf8(b"").is_none());
    }

    #[test]
    fn test_decode_utf8_invalid_continuation() {
        // A continuation byte alone is invalid
        assert!(decode_utf8(&[0x80]).is_none());
    }

    #[test]
    fn test_utf8_char_width_values() {
        assert_eq!(utf8_char_width(b'A'), 1);
        assert_eq!(utf8_char_width(0xC3), 2); // start of 2-byte seq
        assert_eq!(utf8_char_width(0xE4), 3); // start of 3-byte seq
        assert_eq!(utf8_char_width(0xF0), 4); // start of 4-byte seq
        assert_eq!(utf8_char_width(0x80), 0); // continuation byte -> 0
        assert_eq!(utf8_char_width(0xFF), 0); // invalid -> 0
    }

    #[test]
    fn test_char_bytes() {
        assert_eq!(char_bytes('A'), 1);
        assert_eq!(char_bytes('\u{00E9}'), 2);
        assert_eq!(char_bytes('\u{4E16}'), 3);
        assert_eq!(char_bytes('\u{1F600}'), 4);
    }

    // -- Display width --

    #[test]
    fn test_char_display_width_ascii() {
        assert_eq!(char_display_width('A'), 1);
        assert_eq!(char_display_width(' '), 1);
        assert_eq!(char_display_width('~'), 1);
    }

    #[test]
    fn test_char_display_width_cjk() {
        assert_eq!(char_display_width('\u{4E16}'), 2); // CJK Unified
        assert_eq!(char_display_width('\u{AC00}'), 2); // Hangul syllable
        assert_eq!(char_display_width('\u{FF01}'), 2); // Fullwidth exclamation
    }

    #[test]
    fn test_char_display_width_combining() {
        assert_eq!(char_display_width('\u{0300}'), 0); // Combining grave accent
        assert_eq!(char_display_width('\u{0301}'), 0); // Combining acute accent
        assert_eq!(char_display_width('\u{0591}'), 0); // Hebrew combining
    }

    #[test]
    fn test_char_display_width_control() {
        assert_eq!(char_display_width('\0'), 0);
        assert_eq!(char_display_width('\n'), 0);
        assert_eq!(char_display_width('\x7F'), 0); // DEL
    }

    #[test]
    fn test_string_display_width_mixed() {
        // "Hello世界" => 5 * 1 + 2 * 2 = 9
        assert_eq!(string_display_width("Hello\u{4E16}\u{754C}"), 9);
    }

    #[test]
    fn test_string_display_width_with_combining() {
        // 'e' (1) + combining acute (0) = 1
        assert_eq!(string_display_width("e\u{0301}"), 1);
    }

    // -- Case conversion --

    #[test]
    fn test_char_upcase_ascii() {
        assert_eq!(char_upcase('a'), 'A');
        assert_eq!(char_upcase('z'), 'Z');
        assert_eq!(char_upcase('A'), 'A');
        assert_eq!(char_upcase('1'), '1'); // non-letter unchanged
    }

    #[test]
    fn test_char_downcase_ascii() {
        assert_eq!(char_downcase('A'), 'a');
        assert_eq!(char_downcase('z'), 'z');
    }

    #[test]
    fn test_case_unicode() {
        assert_eq!(char_upcase('\u{00E9}'), '\u{00C9}'); // e-acute -> E-acute
        assert_eq!(char_downcase('\u{00C9}'), '\u{00E9}');
    }

    #[test]
    fn test_turkish_i_uppercase() {
        // Standard Unicode: lowercase 'i' -> uppercase 'I'
        // (Locale-sensitive Turkish rules are NOT applied here.)
        assert_eq!(char_upcase('i'), 'I');
        // Turkish capital I with dot: U+0130 -> lowercase = U+0069 'i'
        assert_eq!(char_downcase('\u{0130}'), 'i');
    }

    #[test]
    fn test_is_upper_lower() {
        assert!(is_upper('A'));
        assert!(!is_upper('a'));
        assert!(is_lower('a'));
        assert!(!is_lower('A'));
        assert!(!is_upper('1'));
        assert!(!is_lower('1'));
    }

    #[test]
    fn test_string_upcase_downcase() {
        assert_eq!(string_upcase("hello"), "HELLO");
        assert_eq!(string_downcase("HELLO"), "hello");
        assert_eq!(string_upcase("caf\u{00E9}"), "CAF\u{00C9}");
        // German eszett expands: "ss" when uppercased (or "\u{1E9E}" depending on locale)
        // Rust's to_uppercase produces "SS"
        assert_eq!(string_upcase("\u{00DF}"), "SS");
    }

    // -- Classification --

    #[test]
    fn test_ascii_classification() {
        assert!(is_ascii_digit('0'));
        assert!(is_ascii_digit('9'));
        assert!(!is_ascii_digit('a'));

        assert!(is_ascii_alpha('a'));
        assert!(is_ascii_alpha('Z'));
        assert!(!is_ascii_alpha('0'));

        assert!(is_ascii_alnum('a'));
        assert!(is_ascii_alnum('5'));
        assert!(!is_ascii_alnum('!'));
    }

    #[test]
    fn test_whitespace() {
        assert!(is_whitespace(' '));
        assert!(is_whitespace('\t'));
        assert!(is_whitespace('\n'));
        assert!(is_whitespace('\r'));
        assert!(is_whitespace('\x0C'));
        assert!(!is_whitespace('a'));
        // Note: we only match the five Emacs whitespace chars, not Unicode Zs
        assert!(!is_whitespace('\u{00A0}')); // non-breaking space is NOT in our set
    }

    #[test]
    fn test_word_char() {
        assert!(is_word_char('a'));
        assert!(is_word_char('Z'));
        assert!(is_word_char('5'));
        assert!(is_word_char('_'));
        assert!(!is_word_char(' '));
        assert!(!is_word_char('-'));
        // Unicode letter counts as word char
        assert!(is_word_char('\u{00E9}'));
    }

    #[test]
    fn test_is_printable() {
        assert!(is_printable('A'));
        assert!(is_printable(' '));
        assert!(!is_printable('\0'));
        assert!(!is_printable('\x7F'));
        assert!(!is_printable('\u{0080}')); // C1 control
        assert!(is_printable('\u{00A0}')); // non-breaking space is printable
    }

    #[test]
    fn test_is_control() {
        assert!(is_control('\0'));
        assert!(is_control('\x1F'));
        assert!(is_control('\x7F'));
        assert!(is_control('\u{0080}'));
        assert!(is_control('\u{009F}'));
        assert!(!is_control(' '));
        assert!(!is_control('A'));
    }

    // -- General category --

    #[test]
    fn test_general_category() {
        assert_eq!(general_category('A'), GeneralCategory::Letter);
        assert_eq!(general_category('5'), GeneralCategory::Number);
        assert_eq!(general_category(' '), GeneralCategory::Separator);
        assert_eq!(general_category('\u{0300}'), GeneralCategory::Mark);
        assert_eq!(general_category('.'), GeneralCategory::Punct);
        assert_eq!(general_category('+'), GeneralCategory::Symbol);
        assert_eq!(general_category('\0'), GeneralCategory::Other);
    }

    // -- Multibyte utilities --

    #[test]
    fn test_string_char_count() {
        assert_eq!(string_char_count("hello"), 5);
        assert_eq!(string_char_count("\u{4E16}\u{754C}"), 2);
        assert_eq!(string_char_count(""), 0);
        assert_eq!(string_char_count("caf\u{00E9}"), 4);
    }

    #[test]
    fn test_byte_to_char_pos() {
        let s = "caf\u{00E9}"; // 'c'=1, 'a'=1, 'f'=1, e-acute=2 => 5 bytes
        assert_eq!(byte_to_char_pos(s, 0), 0);
        assert_eq!(byte_to_char_pos(s, 1), 1);
        assert_eq!(byte_to_char_pos(s, 3), 3); // start of e-acute
        assert_eq!(byte_to_char_pos(s, 5), 4); // past end = 4 chars
    }

    #[test]
    fn test_char_to_byte_pos() {
        let s = "caf\u{00E9}";
        assert_eq!(char_to_byte_pos(s, 0), 0);
        assert_eq!(char_to_byte_pos(s, 3), 3); // char 3 = e-acute
        assert_eq!(char_to_byte_pos(s, 4), 5); // past last char
    }

    #[test]
    fn test_char_at_byte() {
        let s = "caf\u{00E9}";
        assert_eq!(char_at_byte(s, 0), Some('c'));
        assert_eq!(char_at_byte(s, 3), Some('\u{00E9}'));
        assert_eq!(char_at_byte(s, 5), None); // past end
        assert_eq!(char_at_byte(s, 4), None); // mid-character (not a boundary)
    }

    #[test]
    fn test_roundtrip_encode_decode() {
        let chars = ['A', '\u{00E9}', '\u{4E16}', '\u{1F600}', '\u{0}'];
        for &ch in &chars {
            let mut buf = [0u8; 4];
            let n = encode_utf8(ch, &mut buf);
            let (decoded, consumed) = decode_utf8(&buf[..n]).unwrap();
            assert_eq!(decoded, ch);
            assert_eq!(consumed, n);
        }
    }

    #[test]
    fn test_cjk_extension_b_width() {
        // U+20000 is CJK Unified Ideographs Extension B
        assert_eq!(char_display_width('\u{20000}'), 2);
    }

    #[test]
    fn test_zero_width_chars() {
        assert_eq!(char_display_width('\u{200B}'), 0); // ZERO WIDTH SPACE
        assert_eq!(char_display_width('\u{200D}'), 0); // ZERO WIDTH JOINER
        assert_eq!(char_display_width('\u{FEFF}'), 0); // BOM
    }

    #[test]
    fn test_string_display_width_fullwidth_forms() {
        // U+FF21 = fullwidth 'A', should be width 2
        assert_eq!(char_display_width('\u{FF21}'), 2);
        // Three fullwidth chars = 6 columns
        assert_eq!(string_display_width("\u{FF21}\u{FF22}\u{FF23}"), 6);
    }

    #[test]
    fn test_char_titlecase() {
        // For basic Latin, titlecase == uppercase
        assert_eq!(char_titlecase('a'), 'A');
        assert_eq!(char_titlecase('A'), 'A');
        assert_eq!(char_titlecase('1'), '1');
    }

    #[test]
    fn test_is_combining_mark_variation_selectors() {
        // U+FE00..U+FE0F are variation selectors (Mn)
        assert!(is_combining_mark('\u{FE00}'));
        assert!(is_combining_mark('\u{FE0F}'));
        // U+E0100..U+E01EF are variation selectors supplement
        assert!(is_combining_mark('\u{E0100}'));
    }

    #[test]
    fn test_general_category_cjk_letter() {
        // CJK ideograph should be classified as Letter
        assert_eq!(general_category('\u{4E16}'), GeneralCategory::Letter);
    }

    #[test]
    fn test_byte_char_pos_empty_string() {
        assert_eq!(byte_to_char_pos("", 0), 0);
        assert_eq!(char_to_byte_pos("", 0), 0);
    }
}
