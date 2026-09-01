//! Emacs internal character encoding.
//!
//! Implements the encoding described in GNU Emacs `character.h` / `character.c`.
//! This is a UTF-8 superset: standard Unicode code points use normal UTF-8,
//! while raw bytes 0x80..0xFF are represented as "eight-bit" characters in the
//! range 0x3FFF00..0x3FFFFF, encoded with overlong 2-byte sequences (C0/C1
//! lead bytes) that are illegal in standard UTF-8.
//!
//! Encoding table:
//!
//! | Character Range         | Bytes | Encoding                                        |
//! |-------------------------|-------|-------------------------------------------------|
//! | U+0000 .. U+007F        | 1     | 0xxxxxxx  (ASCII)                               |
//! | U+0080 .. U+07FF        | 2     | 110xxxxx 10xxxxxx  (standard UTF-8)              |
//! | U+0800 .. U+FFFF        | 3     | 1110xxxx 10xxxxxx 10xxxxxx                       |
//! | U+10000 .. U+1FFFFF     | 4     | 11110xxx 10xxxxxx 10xxxxxx 10xxxxxx              |
//! | U+200000 .. U+3FFF7F    | 5     | 11111000 10xxxxxx 10xxxxxx 10xxxxxx 10xxxxxx     |
//! | Raw byte 0x80..0xFF     | 2     | 1100000x 10xxxxxx  (overlong, NOT valid UTF-8)   |

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

/// Maximum valid Emacs character code (0x3FFFFF).
pub const MAX_CHAR: u32 = 0x3F_FFFF;

/// Maximum Unicode code point (0x10FFFF).
pub const MAX_UNICODE_CHAR: u32 = 0x10_FFFF;

/// Maximum 1-byte (ASCII) Emacs character code (0x7F).
pub const MAX_1_BYTE_CHAR: u32 = 0x7F;

/// Maximum 2-byte Emacs character code (0x7FF).
pub const MAX_2_BYTE_CHAR: u32 = 0x7FF;

/// Maximum 3-byte Emacs character code (0xFFFF).
pub const MAX_3_BYTE_CHAR: u32 = 0xFFFF;

/// Maximum 4-byte Emacs character code (0x1FFFFF).
pub const MAX_4_BYTE_CHAR: u32 = 0x1F_FFFF;

/// Maximum 5-byte encoded character (0x3FFF7F).
/// Characters above this (0x3FFF80..0x3FFFFF) are raw-byte ("eight-bit") characters.
pub const MAX_5_BYTE_CHAR: u32 = 0x3F_FF7F;

/// Maximum bytes needed to encode any Emacs character in multibyte form.
pub const MAX_MULTIBYTE_LENGTH: usize = 5;

/// Minimum leading byte of a multibyte Emacs character form (0xC0).
pub const MIN_MULTIBYTE_LEADING_CODE: u8 = 0xC0;

/// Maximum leading byte of a multibyte Emacs character form (0xF8).
/// Note: this must be updated if `MAX_CHAR` is ever increased.
pub const MAX_MULTIBYTE_LEADING_CODE: u8 = 0xF8;

/// A validated Emacs character code in the 22-bit range `0..=MAX_CHAR`.
///
/// Issue #131: the historical bug class was treating an Emacs character code as
/// a Rust [`char`] (e.g. `char::from_u32(code)`) or packing it into a Rust
/// `String`. That silently succeeds for codes in the in-Unicode Private-Use
/// area — colliding a real nerd-font glyph with an eight-bit raw byte — and
/// fails outright for eight-bit (`0x3FFF80..`) and non-Unicode (`>0x10FFFF`)
/// codes, which are NOT Unicode scalar values.
///
/// `EmacsChar` makes that mistake un-writable: it is a distinct type from both
/// `char` and a bare `u32`, and the only conversions to/from a Rust `char` or a
/// raw byte are the explicit, correct ones below. The canonical serialisation
/// is the internal multibyte byte form via [`EmacsChar::char_string`] /
/// [`EmacsChar::to_emacs_bytes`] (GNU `CHAR_STRING`), never an in-Unicode
/// sentinel.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct EmacsChar(u32);

impl EmacsChar {
    /// Largest valid code (`MAX_CHAR`, 22-bit).
    pub const MAX: u32 = MAX_CHAR;

    /// Wrap a code, returning `None` if it exceeds `MAX_CHAR`.
    #[inline]
    pub fn from_code(code: u32) -> Option<Self> {
        (code <= MAX_CHAR).then_some(Self(code))
    }

    /// Wrap a code without range checking (debug-asserts validity).
    #[inline]
    pub fn from_code_unchecked(code: u32) -> Self {
        debug_assert!(code <= MAX_CHAR, "EmacsChar out of range: {code:#x}");
        Self(code)
    }

    /// The eight-bit raw-byte character for `byte` (`BYTE8_TO_CHAR`).
    #[inline]
    pub fn from_byte8(byte: u8) -> Self {
        Self(byte8_to_char(byte))
    }

    /// A unibyte byte made multibyte: ASCII stays itself, high bytes become
    /// eight-bit raw-byte characters (`UNIBYTE_TO_CHAR`).
    #[inline]
    pub fn from_unibyte_byte(byte: u8) -> Self {
        Self(unibyte_to_char(byte))
    }

    /// A Rust scalar value as an Emacs character (always in range).
    #[inline]
    pub fn from_char(c: char) -> Self {
        Self(c as u32)
    }

    /// The raw 22-bit code.
    #[inline]
    pub fn code(self) -> u32 {
        self.0
    }

    /// Whether this is a Unicode scalar value (representable as a Rust `char`).
    /// False for eight-bit raw bytes and non-Unicode codes.
    #[inline]
    pub fn is_unicode_scalar(self) -> bool {
        char::from_u32(self.0).is_some()
    }

    /// Whether this is an eight-bit raw-byte character (`CHAR_BYTE8_P`).
    #[inline]
    pub fn is_byte8(self) -> bool {
        self.0 > MAX_5_BYTE_CHAR
    }

    /// The underlying byte for an eight-bit (or ASCII) character, else `None`
    /// (`CHAR_TO_BYTE_SAFE`).
    #[inline]
    pub fn to_byte8(self) -> Option<u8> {
        char_to_byte_safe(self.0)
    }

    /// As a Rust `char`, or `None` for eight-bit/non-Unicode codes.
    #[inline]
    pub fn as_rust_char(self) -> Option<char> {
        char::from_u32(self.0)
    }

    /// Encode into `buf` in the canonical internal multibyte form
    /// (`CHAR_STRING`); returns the number of bytes written.
    #[inline]
    pub fn char_string(self, buf: &mut [u8]) -> usize {
        char_string(self.0, buf)
    }

    /// The canonical internal multibyte bytes for this character.
    #[inline]
    pub fn to_emacs_bytes(self) -> Vec<u8> {
        let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
        let len = char_string(self.0, &mut buf);
        buf[..len].to_vec()
    }
}

/// Start of the raw-byte character range.
const BYTE8_OFFSET: u32 = 0x3F_FF00;

// ---------------------------------------------------------------------------
// Character modifier bits (mirrors GNU `lisp.h` `CHAR_ALT` ... `CHAR_META`).
//
// Neomacs already defines the same bit values under `KEY_CHAR_*` names in
// `crate::emacs_core::keyboard::pure`. The aliases below give callers in the
// character/encoding layer the GNU spelling without duplicating values, so
// `char_resolve_modifier_mask` and `char_string` can be ported verbatim.
// ---------------------------------------------------------------------------

/// `CHAR_ALT` modifier bit (0x0400000).
pub const CHAR_ALT: u32 = 0x0400000;

/// `CHAR_SUPER` modifier bit (0x0800000).
pub const CHAR_SUPER: u32 = 0x0800000;

/// `CHAR_HYPER` modifier bit (0x1000000).
pub const CHAR_HYPER: u32 = 0x1000000;

/// `CHAR_SHIFT` modifier bit (0x2000000).
pub const CHAR_SHIFT: u32 = 0x2000000;

/// `CHAR_CTL` modifier bit (0x4000000).
pub const CHAR_CTL: u32 = 0x4000000;

/// `CHAR_META` modifier bit (0x8000000).
pub const CHAR_META: u32 = 0x8000000;

/// Bitmask of all character modifier bits.
pub const CHAR_MODIFIER_MASK: u32 =
    CHAR_ALT | CHAR_SUPER | CHAR_HYPER | CHAR_SHIFT | CHAR_CTL | CHAR_META;

/// Resolve modifier bits on character code `c` in the same way GNU does.
///
/// Mirrors `char_resolve_modifier_mask` in GNU `src/character.c:51`:
///
/// * Non-ASCII base characters are returned unchanged.
/// * `S-A`..`S-Z` lose `CHAR_SHIFT`; `S-a`..`S-z` are converted to the
///   corresponding upper-case letter and lose `CHAR_SHIFT`; `S-` on a
///   control character or SPC is dropped.
/// * `C-SPC` becomes `C-@` (NUL) with `CHAR_CTL` cleared; `C-?` becomes
///   DEL (0177) with `CHAR_CTL` cleared; letters and `@`..`_` get masked
///   with 0o37 and lose `CHAR_CTL`.
/// * `CHAR_META` is intentionally left alone (GNU bug#4751).
pub fn char_resolve_modifier_mask(mut c: i64) -> i64 {
    let mask = CHAR_MODIFIER_MASK as i64;
    let base = c & !mask;
    if !(0..=0x7F).contains(&base) {
        return c;
    }

    if c & CHAR_SHIFT as i64 != 0 {
        let low = c & 0o377;
        if (b'A' as i64..=b'Z' as i64).contains(&low) {
            c &= !(CHAR_SHIFT as i64);
        } else if (b'a' as i64..=b'z' as i64).contains(&low) {
            c = (c & !(CHAR_SHIFT as i64)) - (b'a' as i64 - b'A' as i64);
        } else if (c & !mask) <= 0x20 {
            c &= !(CHAR_SHIFT as i64);
        }
    }
    if c & CHAR_CTL as i64 != 0 {
        let low = c & 0o377;
        if low == b' ' as i64 {
            c &= !0o177 & !(CHAR_CTL as i64);
        } else if low == b'?' as i64 {
            c = 0o177 | (c & !0o177 & !(CHAR_CTL as i64));
        } else if (c & 0o137) >= 0o101 && (c & 0o137) <= 0o132
            || (c & 0o177) >= 0o100 && (c & 0o177) <= 0o137
        {
            c &= 0o37 | (!0o177 & !(CHAR_CTL as i64));
        }
    }
    c
}

// ---------------------------------------------------------------------------
// Named Unicode character codes used throughout the codebase
// (mirrors GNU `character.h` `enum`).
// ---------------------------------------------------------------------------

pub const NO_BREAK_SPACE: u32 = 0x00A0;
pub const SOFT_HYPHEN: u32 = 0x00AD;
pub const ZERO_WIDTH_NON_JOINER: u32 = 0x200C;
pub const ZERO_WIDTH_JOINER: u32 = 0x200D;
pub const HYPHEN: u32 = 0x2010;
pub const NON_BREAKING_HYPHEN: u32 = 0x2011;
pub const LEFT_SINGLE_QUOTATION_MARK: u32 = 0x2018;
pub const RIGHT_SINGLE_QUOTATION_MARK: u32 = 0x2019;
pub const PARAGRAPH_SEPARATOR: u32 = 0x2029;
pub const LEFT_POINTING_ANGLE_BRACKET: u32 = 0x2329;
pub const RIGHT_POINTING_ANGLE_BRACKET: u32 = 0x232A;
pub const LEFT_ANGLE_BRACKET: u32 = 0x3008;
pub const RIGHT_ANGLE_BRACKET: u32 = 0x3009;
pub const OBJECT_REPLACEMENT_CHARACTER: u32 = 0xFFFC;
pub const TAG_SPACE: u32 = 0xE0020;
pub const CANCEL_TAG: u32 = 0xE007F;

// ---------------------------------------------------------------------------
// Unicode general category (mirrors GNU `character.h` `enum unicode_category`).
//
// The numeric values match GNU exactly so that lookups in
// `unicode-category-table` (whose values are populated from UnicodeData.txt
// during character.c init) yield comparable results.
// ---------------------------------------------------------------------------

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u8)]
pub enum UnicodeCategory {
    /// Lu - Uppercase Letter
    UppercaseLetter = 0,
    /// Ll - Lowercase Letter
    LowercaseLetter,
    /// Lt - Titlecase Letter
    TitlecaseLetter,
    /// Lm - Modifier Letter
    ModifierLetter,
    /// Lo - Other Letter
    OtherLetter,
    /// Mn - Nonspacing Mark
    NonspacingMark,
    /// Mc - Spacing Mark
    SpacingMark,
    /// Me - Enclosing Mark
    EnclosingMark,
    /// Nd - Decimal Number
    DecimalNumber,
    /// Nl - Letter Number
    LetterNumber,
    /// No - Other Number
    OtherNumber,
    /// Pc - Connector Punctuation
    ConnectorPunctuation,
    /// Pd - Dash Punctuation
    DashPunctuation,
    /// Ps - Open Punctuation
    OpenPunctuation,
    /// Pe - Close Punctuation
    ClosePunctuation,
    /// Pi - Initial Punctuation
    InitialPunctuation,
    /// Pf - Final Punctuation
    FinalPunctuation,
    /// Po - Other Punctuation
    OtherPunctuation,
    /// Sm - Math Symbol
    MathSymbol,
    /// Sc - Currency Symbol
    CurrencySymbol,
    /// Sk - Modifier Symbol
    ModifierSymbol,
    /// So - Other Symbol
    OtherSymbol,
    /// Zs - Space Separator
    SpaceSeparator,
    /// Zl - Line Separator
    LineSeparator,
    /// Zp - Paragraph Separator
    ParagraphSeparator,
    /// Cc - Control
    Control,
    /// Cf - Format
    Format,
    /// Cs - Surrogate
    Surrogate,
    /// Co - Private Use
    PrivateUse,
    /// Cn - Unassigned
    Unassigned,
}

impl From<unicode_general_category::GeneralCategory> for UnicodeCategory {
    fn from(gc: unicode_general_category::GeneralCategory) -> Self {
        use unicode_general_category::GeneralCategory as G;
        match gc {
            G::UppercaseLetter => UnicodeCategory::UppercaseLetter,
            G::LowercaseLetter => UnicodeCategory::LowercaseLetter,
            G::TitlecaseLetter => UnicodeCategory::TitlecaseLetter,
            G::ModifierLetter => UnicodeCategory::ModifierLetter,
            G::OtherLetter => UnicodeCategory::OtherLetter,
            G::NonspacingMark => UnicodeCategory::NonspacingMark,
            G::SpacingMark => UnicodeCategory::SpacingMark,
            G::EnclosingMark => UnicodeCategory::EnclosingMark,
            G::DecimalNumber => UnicodeCategory::DecimalNumber,
            G::LetterNumber => UnicodeCategory::LetterNumber,
            G::OtherNumber => UnicodeCategory::OtherNumber,
            G::ConnectorPunctuation => UnicodeCategory::ConnectorPunctuation,
            G::DashPunctuation => UnicodeCategory::DashPunctuation,
            G::OpenPunctuation => UnicodeCategory::OpenPunctuation,
            G::ClosePunctuation => UnicodeCategory::ClosePunctuation,
            G::InitialPunctuation => UnicodeCategory::InitialPunctuation,
            G::FinalPunctuation => UnicodeCategory::FinalPunctuation,
            G::OtherPunctuation => UnicodeCategory::OtherPunctuation,
            G::MathSymbol => UnicodeCategory::MathSymbol,
            G::CurrencySymbol => UnicodeCategory::CurrencySymbol,
            G::ModifierSymbol => UnicodeCategory::ModifierSymbol,
            G::OtherSymbol => UnicodeCategory::OtherSymbol,
            G::SpaceSeparator => UnicodeCategory::SpaceSeparator,
            G::LineSeparator => UnicodeCategory::LineSeparator,
            G::ParagraphSeparator => UnicodeCategory::ParagraphSeparator,
            G::Control => UnicodeCategory::Control,
            G::Format => UnicodeCategory::Format,
            G::Surrogate => UnicodeCategory::Surrogate,
            G::PrivateUse => UnicodeCategory::PrivateUse,
            G::Unassigned => UnicodeCategory::Unassigned,
            // `GeneralCategory` is marked `#[non_exhaustive]`; treat any
            // future category as Unassigned (the most conservative choice,
            // matching GNU's "not graphic/printable" fallback).
            _ => UnicodeCategory::Unassigned,
        }
    }
}

/// Return the Unicode general category code for codepoint `c`, matching the
/// integer values GNU stores in `unicode-category-table`.
///
/// Returns `None` for codepoints outside the Unicode scalar range (e.g. the
/// eight-bit raw-byte range), mirroring GNU's `CHAR_TABLE_REF` returning nil
/// (a non-fixnum) for which the graphic/printable predicates yield `false`.
pub fn char_general_category(c: u32) -> Option<i64> {
    let ch = char::from_u32(c)?;
    Some(UnicodeCategory::from(unicode_general_category::get_general_category(ch)) as i64)
}

// ---------------------------------------------------------------------------
// Raw-byte predicates and converters
// ---------------------------------------------------------------------------

/// Return `true` if `c` is a raw-byte ("eight-bit") character,
/// i.e. in the range 0x3FFF00..0x3FFFFF.
#[inline]
pub fn char_byte8_p(c: u32) -> bool {
    c > MAX_5_BYTE_CHAR
}

/// Strict conversion of a raw byte to its eight-bit Emacs character code.
///
/// Always returns `byte + 0x3FFF00`, even for ASCII bytes 0x00..0x7F (the
/// result is then in the eight-bit range, *not* an ASCII char). This mirrors
/// GNU `BYTE8_TO_CHAR` exactly.
///
/// For "make this byte a character, treating ASCII as ASCII", use
/// [`unibyte_to_char`] instead.
#[inline]
pub fn byte8_to_char(byte: u8) -> u32 {
    byte as u32 + BYTE8_OFFSET
}

/// Convert a unibyte byte (0x00..0xFF) to its multibyte character code.
///
/// For ASCII bytes (0x00..0x7F) the result is the byte itself; for high
/// bytes (0x80..0xFF) the result is the corresponding eight-bit raw-byte
/// character (`byte + 0x3FFF00`). This mirrors GNU `UNIBYTE_TO_CHAR`.
#[inline]
pub fn unibyte_to_char(byte: u8) -> u32 {
    if byte < 0x80 {
        byte as u32
    } else {
        byte8_to_char(byte)
    }
}

/// If `c` is not ASCII, make it multibyte. Assumes `c < 256`.
///
/// Mirrors GNU `make_char_multibyte` (`character.h`). This is identical to
/// [`unibyte_to_char`] but documents the intent (turning a unibyte char
/// stored in a multibyte buffer into its raw-byte representation).
#[inline]
pub fn make_char_multibyte(c: i32) -> u32 {
    debug_assert!((0..0x100).contains(&c));
    unibyte_to_char(c as u8)
}

/// Convert a raw-byte character code back to its byte value.
///
/// The caller must ensure `c` is a raw-byte character (i.e. `char_byte8_p(c)`
/// is true). For ASCII-range characters this also works (returns low byte).
/// In debug builds, asserts that `c` is either ASCII or a byte8 character to
/// catch silent low-byte truncation of unrelated multibyte characters.
#[inline]
pub fn char_to_byte8(c: u32) -> u8 {
    if char_byte8_p(c) {
        (c - BYTE8_OFFSET) as u8
    } else {
        debug_assert!(
            c < 0x80,
            "char_to_byte8 called on non-byte8 non-ASCII char {:#x}",
            c
        );
        c as u8
    }
}

/// Return the raw 8-bit byte for character `c`, or `None` if `c` doesn't
/// correspond to a single byte.
///
/// Mirrors GNU `CHAR_TO_BYTE_SAFE`. ASCII chars (0..0x7F) return themselves;
/// raw-byte chars (0x3FFF80..0x3FFFFF) return the underlying byte; anything
/// else returns `None`.
#[inline]
pub fn char_to_byte_safe(c: u32) -> Option<u8> {
    if c < 0x80 {
        Some(c as u8)
    } else if char_byte8_p(c) {
        Some((c - BYTE8_OFFSET) as u8)
    } else {
        None
    }
}

// ---------------------------------------------------------------------------
// Cheap byte-level / character predicates (mirrors GNU `character.h`).
// All of these are pure value tests, no allocations, no table lookups.
// ---------------------------------------------------------------------------

/// True iff `c` is a valid character code (0 ≤ c ≤ MAX_CHAR).
///
/// Mirrors GNU `CHAR_VALID_P`.
#[inline]
pub fn char_valid_p(c: i64) -> bool {
    0 <= c && c <= MAX_CHAR as i64
}

/// True iff `c` is a single-byte (< 0x100) character.
///
/// Mirrors GNU `SINGLE_BYTE_CHAR_P`.
#[inline]
pub fn single_byte_char_p(c: i64) -> bool {
    (0..0x100).contains(&c)
}

/// True iff `byte` starts a non-ASCII multibyte form (`(byte & 0xC0) == 0xC0`).
///
/// Mirrors GNU `LEADING_CODE_P`.
#[inline]
pub fn leading_code_p(byte: u8) -> bool {
    (byte & 0xC0) == 0xC0
}

/// True iff `byte` is a continuation byte (`(byte & 0xC0) == 0x80`).
///
/// Mirrors GNU `TRAILING_CODE_P`.
#[inline]
pub fn trailing_code_p(byte: u8) -> bool {
    (byte & 0xC0) == 0x80
}

/// True iff `byte` starts a character (ASCII or multibyte lead).
///
/// Mirrors GNU `CHAR_HEAD_P`.
#[inline]
pub fn char_head_p(byte: u8) -> bool {
    (byte & 0xC0) != 0x80
}

/// True iff `byte` is the lead byte of a raw-byte ("eight-bit") form
/// (i.e. `0xC0` or `0xC1`).
///
/// Mirrors GNU `CHAR_BYTE8_HEAD_P`.
#[inline]
pub fn char_byte8_head_p(byte: u8) -> bool {
    byte == 0xC0 || byte == 0xC1
}

/// Number of bytes a character whose lead byte is `byte` occupies in a
/// multibyte form.
///
/// Unlike [`multibyte_length`], this does **not** validate the multibyte form;
/// it only inspects the first byte. For a valid lead byte the result is in
/// `1..=5`. For invalid lead bytes (continuation bytes, 0xF9..0xFF) the
/// result follows GNU's table mechanically:
/// - bit 7 clear → 1
/// - bit 5 clear → 2
/// - bit 4 clear → 3
/// - bit 3 clear → 4
/// - else        → 5
///
/// Mirrors GNU `BYTES_BY_CHAR_HEAD`.
#[inline]
pub fn bytes_by_char_head(byte: u8) -> usize {
    if byte & 0x80 == 0 {
        1
    } else if byte & 0x20 == 0 {
        2
    } else if byte & 0x10 == 0 {
        3
    } else if byte & 0x08 == 0 {
        4
    } else {
        5
    }
}

/// Return the leading code of the multibyte form of `c`.
///
/// Mirrors GNU `CHAR_LEADING_CODE`.
#[inline]
pub fn char_leading_code(c: u32) -> u8 {
    if c <= MAX_1_BYTE_CHAR {
        c as u8
    } else if c <= MAX_2_BYTE_CHAR {
        0xC0 | (c >> 6) as u8
    } else if c <= MAX_3_BYTE_CHAR {
        0xE0 | (c >> 12) as u8
    } else if c <= MAX_4_BYTE_CHAR {
        0xF0 | (c >> 18) as u8
    } else if c <= MAX_5_BYTE_CHAR {
        0xF8
    } else {
        // Raw byte: overlong 2-byte lead (0xC0 or 0xC1).
        0xC0 | ((c >> 6) & 0x01) as u8
    }
}

/// True iff `c` is a Unicode variation selector (U+FE00..U+FE0F or
/// U+E0100..U+E01EF).
///
/// Mirrors GNU `CHAR_VARIATION_SELECTOR_P`.
#[inline]
pub fn char_variation_selector_p(c: u32) -> bool {
    (0xFE00..=0xFE0F).contains(&c) || (0xE0100..=0xE01EF).contains(&c)
}

/// True iff `c` is a UTF-16 surrogate code point (U+D800..U+DFFF).
///
/// Mirrors GNU `char_surrogate_p`.
#[inline]
pub fn char_surrogate_p(c: u32) -> bool {
    (0xD800..=0xDFFF).contains(&c)
}

/// If `c` is an ASCII hex digit, return its numeric value (0..15);
/// otherwise return -1.
///
/// Mirrors GNU `char_hexdigit`.
#[inline]
pub fn char_hexdigit(c: u32) -> i32 {
    match c {
        0x30..=0x39 => (c - 0x30) as i32,
        0x41..=0x46 => (c - 0x41 + 10) as i32,
        0x61..=0x66 => (c - 0x61 + 10) as i32,
        _ => -1,
    }
}

// ---------------------------------------------------------------------------
// Encoding helpers
// ---------------------------------------------------------------------------

/// Return the number of bytes needed to encode character `c` in Emacs
/// multibyte form.
#[inline]
pub fn char_bytes(c: u32) -> usize {
    if c < 0x80 {
        1
    } else if c < 0x800 {
        2
    } else if c < 0x1_0000 {
        3
    } else if c < 0x20_0000 {
        4
    } else if c <= MAX_5_BYTE_CHAR {
        5
    } else {
        // Raw byte: encoded as overlong 2-byte sequence.
        2
    }
}

/// Encode character `c` into `buf` and return the number of bytes written.
///
/// `buf` must be at least [`MAX_MULTIBYTE_LENGTH`] bytes long.
/// This mirrors GNU `char_string` (character.c:101): if `c` carries any
/// modifier bits they are first resolved by [`char_resolve_modifier_mask`]
/// and then stripped before encoding. `c` (after modifier stripping) must
/// be in `0..=MAX_CHAR`; out-of-range values panic in debug builds.
pub fn char_string(mut c: u32, buf: &mut [u8]) -> usize {
    if c & CHAR_MODIFIER_MASK != 0 {
        let resolved = char_resolve_modifier_mask(c as i64);
        c = (resolved as u32) & !CHAR_MODIFIER_MASK;
    }
    debug_assert!(c <= MAX_CHAR, "char_string: invalid character 0x{:X}", c);
    if c < 0x80 {
        // ASCII
        buf[0] = c as u8;
        1
    } else if c < 0x800 {
        // 2-byte standard UTF-8
        buf[0] = 0xC0 | (c >> 6) as u8;
        buf[1] = 0x80 | (c & 0x3F) as u8;
        2
    } else if c < 0x1_0000 {
        // 3-byte
        buf[0] = 0xE0 | (c >> 12) as u8;
        buf[1] = 0x80 | ((c >> 6) & 0x3F) as u8;
        buf[2] = 0x80 | (c & 0x3F) as u8;
        3
    } else if c < 0x20_0000 {
        // 4-byte
        buf[0] = 0xF0 | (c >> 18) as u8;
        buf[1] = 0x80 | ((c >> 12) & 0x3F) as u8;
        buf[2] = 0x80 | ((c >> 6) & 0x3F) as u8;
        buf[3] = 0x80 | (c & 0x3F) as u8;
        4
    } else if c <= MAX_5_BYTE_CHAR {
        // 5-byte (extended Emacs range, not raw byte)
        buf[0] = 0xF8;
        buf[1] = 0x80 | ((c >> 18) & 0x3F) as u8;
        buf[2] = 0x80 | ((c >> 12) & 0x3F) as u8;
        buf[3] = 0x80 | ((c >> 6) & 0x3F) as u8;
        buf[4] = 0x80 | (c & 0x3F) as u8;
        5
    } else {
        // Raw byte (0x3FFF80..0x3FFFFF) → overlong 2-byte.
        // Raw bytes 0x80..0xFF are encoded as if they were 0x00..0x7F in
        // overlong form:  byte 0x80 → [C0,80], byte 0xBF → [C0,BF],
        //                 byte 0xC0 → [C1,80], byte 0xFF → [C1,BF].
        let val = char_to_byte8(c).wrapping_sub(0x80);
        buf[0] = 0xC0 | (val >> 6);
        buf[1] = 0x80 | (val & 0x3F);
        2
    }
}

// ---------------------------------------------------------------------------
// Decoding helpers
// ---------------------------------------------------------------------------

/// Return `true` if `lead` is the start of a raw-byte overlong sequence.
///
/// Raw bytes are encoded as `[0xC0, 0x80..0xBF]` or `[0xC1, 0x80..0xBF]`,
/// which are the overlong representations of U+0000..U+007F and U+0040..U+007F
/// respectively. In Emacs encoding these represent raw bytes 0x80..0xFF.
#[inline]
fn is_raw_byte_lead(lead: u8) -> bool {
    lead == 0xC0 || lead == 0xC1
}

/// Strict byte length of the multibyte form starting at `bytes`.
///
/// Returns the number of bytes (1..=5) that form a valid Emacs multibyte
/// character at `bytes[0..]`, or `None` if the bytes do not form a valid
/// multibyte sequence (e.g. a stray continuation byte, an incomplete
/// trailing sequence, or — when `allow_8bit` is false — a raw eight-bit
/// overlong form).
///
/// Mirrors GNU `multibyte_length` (`character.h`) with `check = true`. The
/// `check = false` GNU variant skips bounds checks because the caller knows
/// the underlying buffer extends past `pend`; in Rust the slice length is
/// authoritative, so this function always validates lengths.
///
/// Use this when you need to *validate* a byte sequence; use
/// [`string_char`] / [`string_char_unchecked`] when you just need to
/// decode a character whose validity has already been established.
pub fn multibyte_length(bytes: &[u8], allow_8bit: bool) -> Option<usize> {
    if bytes.is_empty() {
        return None;
    }
    let c = bytes[0];
    if c < 0x80 {
        return Some(1);
    }
    if bytes.len() < 2 {
        return None;
    }
    let d = bytes[1];
    let w: i32 = (((d as i32) & 0xC0) << 2) + c as i32;
    let two_lo: i32 = if allow_8bit { 0x2C0 } else { 0x2C2 };
    if (two_lo..=0x2DF).contains(&w) {
        return Some(2);
    }
    if bytes.len() < 3 {
        return None;
    }
    let e = bytes[2];
    let w = w + (((e as i32) & 0xC0) << 4);
    let w1 = w | (((d as i32) & 0x20) >> 2);
    if (0xAE1..=0xAEF).contains(&w1) {
        return Some(3);
    }
    if bytes.len() < 4 {
        return None;
    }
    let f = bytes[3];
    let w = w + (((f as i32) & 0xC0) << 6);
    let w2 = w | (((d as i32) & 0x30) >> 3);
    if (0x2AF1..=0x2AF7).contains(&w2) {
        return Some(4);
    }
    if bytes.len() < 5 {
        return None;
    }
    let lw: i64 = w as i64 + (((bytes[4] as i64) & 0xC0) << 8);
    let w3: i64 = (lw << 24) + ((d as i64) << 16) + ((e as i64) << 8) + f as i64;
    if (0xAAF8888080i64..=0xAAF88FBFBDi64).contains(&w3) {
        return Some(5);
    }
    None
}

/// Decode one character from `bytes`, returning `(char_code, bytes_consumed)`.
///
/// This mirrors GNU `STRING_CHAR_AND_LENGTH`. The input must be a valid Emacs
/// multibyte sequence (no bounds checking on continuation bytes beyond what the
/// lead byte promises).
pub fn string_char(bytes: &[u8]) -> (u32, usize) {
    debug_assert!(!bytes.is_empty());
    let b0 = bytes[0];

    if b0 < 0x80 {
        // ASCII
        (b0 as u32, 1)
    } else if is_raw_byte_lead(b0) && bytes.len() >= 2 && (bytes[1] & 0xC0) == 0x80 {
        // Raw-byte overlong: 2 bytes → eight-bit character.
        let raw = ((b0 & 0x01) << 6) | (bytes[1] & 0x3F);
        (byte8_to_char(raw | 0x80), 2)
    } else if b0 < 0xE0 && bytes.len() >= 2 && (bytes[1] & 0xC0) == 0x80 {
        // 2-byte standard UTF-8
        let c = ((b0 as u32 & 0x1F) << 6) | (bytes[1] as u32 & 0x3F);
        (c, 2)
    } else if b0 < 0xF0
        && bytes.len() >= 3
        && (bytes[1] & 0xC0) == 0x80
        && (bytes[2] & 0xC0) == 0x80
    {
        // 3-byte
        let c =
            ((b0 as u32 & 0x0F) << 12) | ((bytes[1] as u32 & 0x3F) << 6) | (bytes[2] as u32 & 0x3F);
        (c, 3)
    } else if b0 < 0xF8
        && bytes.len() >= 4
        && (bytes[1] & 0xC0) == 0x80
        && (bytes[2] & 0xC0) == 0x80
        && (bytes[3] & 0xC0) == 0x80
    {
        // 4-byte
        let c = ((b0 as u32 & 0x07) << 18)
            | ((bytes[1] as u32 & 0x3F) << 12)
            | ((bytes[2] as u32 & 0x3F) << 6)
            | (bytes[3] as u32 & 0x3F);
        (c, 4)
    } else if b0 == 0xF8
        && bytes.len() >= 5
        && (bytes[1] & 0xC0) == 0x80
        && (bytes[2] & 0xC0) == 0x80
        && (bytes[3] & 0xC0) == 0x80
        && (bytes[4] & 0xC0) == 0x80
    {
        // 5-byte (F8 lead, Emacs extension)
        let c = ((bytes[1] as u32 & 0x3F) << 18)
            | ((bytes[2] as u32 & 0x3F) << 12)
            | ((bytes[3] as u32 & 0x3F) << 6)
            | (bytes[4] as u32 & 0x3F);
        (c, 5)
    } else {
        // Invalid or truncated sequence — treat lead byte as raw byte
        (byte8_to_char(b0), 1)
    }
}

/// Decode one character from a valid Emacs multibyte byte sequence.
///
/// This mirrors GNU Emacs' inline `string_char_and_length` fast path in
/// `src/character.h`: callers that already own valid internal Lisp string
/// bytes can skip continuation-byte and truncation checks in tight loops.
/// Use [`string_char`] instead when malformed or incomplete input is possible.
#[inline]
pub fn string_char_unchecked(bytes: &[u8]) -> (u32, usize) {
    debug_assert!(!bytes.is_empty());
    let c = bytes[0] as i32;
    if (c & 0x80) == 0 {
        return (c as u32, 1);
    }

    let mut d = (c << 6) + bytes[1] as i32 - ((0xC0 << 6) + 0x80);
    if (c & 0x20) == 0 {
        let raw_offset = if c < 0xC2 { 0x3F_FF80 } else { 0 };
        return ((d + raw_offset) as u32, 2);
    }

    d = (d << 6) + bytes[2] as i32 - ((0x20 << 12) + 0x80);
    if (c & 0x10) == 0 {
        return (d as u32, 3);
    }

    d = (d << 6) + bytes[3] as i32 - ((0x10 << 18) + 0x80);
    if (c & 0x08) == 0 {
        return (d as u32, 4);
    }

    d = (d << 6) + bytes[4] as i32 - ((0x08 << 24) + 0x80);
    (d as u32, 5)
}

/// Number of bytes in the multibyte character ending at `bytes[..end]`.
///
/// Mirrors GNU `raw_prev_char_len` (character.h:359). Caller must guarantee
/// that `bytes[..end]` ends on a character boundary and that there is at
/// least one preceding char head somewhere in `bytes[..end]`.
pub fn raw_prev_char_len(bytes: &[u8], end: usize) -> usize {
    debug_assert!(end > 0 && end <= bytes.len());
    let mut len = 1;
    loop {
        if char_head_p(bytes[end - len]) {
            return len;
        }
        len += 1;
    }
}

/// Like [`string_char`] but advance `pos` past the decoded character.
///
/// Returns the decoded character code. Mirrors GNU `string_char_advance`.
#[inline]
pub fn string_char_advance(bytes: &[u8], pos: &mut usize) -> u32 {
    let (c, len) = string_char(&bytes[*pos..]);
    *pos += len;
    c
}

/// Count how many characters are in `bytes`, treating it as a valid
/// Emacs multibyte sequence.
///
/// Mirrors GNU `multibyte_chars_in_text` (character.c:519). Panics in
/// debug builds if a byte position would yield a zero-length char.
pub fn multibyte_chars_in_text(bytes: &[u8]) -> usize {
    let mut chars = 0usize;
    let mut i = 0usize;
    while i < bytes.len() {
        let len = match multibyte_length(&bytes[i..], true) {
            Some(n) => n,
            None => {
                debug_assert!(false, "multibyte_chars_in_text: invalid sequence at {}", i);
                1
            }
        };
        i += len;
        chars += 1;
    }
    chars
}

/// Number of characters in `bytes`, given the multibyte mode flag.
///
/// Mirrors GNU `chars_in_text` (character.c:503). When `multibyte` is
/// false this returns `bytes.len()`; otherwise it delegates to
/// [`multibyte_chars_in_text`].
#[inline]
pub fn chars_in_text(bytes: &[u8], multibyte: bool) -> usize {
    if !multibyte {
        bytes.len()
    } else {
        multibyte_chars_in_text(bytes)
    }
}

// ---------------------------------------------------------------------------
// Unibyte ↔ multibyte conversion (mirrors GNU character.c:543..740).
// ---------------------------------------------------------------------------

/// Number of bytes a unibyte buffer would occupy when converted to
/// multibyte by [`str_to_multibyte`].
///
/// Mirrors GNU `count_size_as_multibyte` (character.c:668). Each non-ASCII
/// byte expands to two bytes in the multibyte form.
#[inline]
pub fn count_size_as_multibyte(src: &[u8]) -> usize {
    src.len() + src.iter().filter(|&&b| b >= 0x80).count()
}

/// Convert unibyte text to multibyte text, preserving each byte as a
/// single character (high bytes become raw-byte characters).
///
/// Mirrors GNU `str_to_multibyte` (character.c:686).
pub fn str_to_multibyte(src: &[u8]) -> Vec<u8> {
    // ASCII promotes to itself: one block copy instead of a per-byte loop.
    // (One spare slot: the result often becomes a LispString payload, whose
    // constructor appends a trailing NUL.)
    if src.is_ascii() {
        let mut out = Vec::with_capacity(src.len() + 1);
        out.extend_from_slice(src);
        return out;
    }
    let mut out = Vec::with_capacity(count_size_as_multibyte(src));
    for &c in src {
        if c <= 0x7F {
            out.push(c);
        } else {
            out.push(0xC0 | ((c >> 6) & 1));
            out.push(0x80 | (c & 0x3F));
        }
    }
    out
}

/// Convert multibyte text to unibyte text, one output byte per character.
///
/// Mirrors GNU `copy_text` with `from_multibyte=1, to_multibyte=0`
/// (insdel.c:632-648): each character is decoded, and if it is not ASCII its
/// low byte (`c & 0xFF`) is emitted. A regular multibyte char like é
/// (U+00E9) collapses to the single byte 0xE9, and an eight-bit raw-byte
/// character (`byte + 0x3FFF00`) collapses to `byte`. This is the narrowing
/// GNU's plain-string search uses when a multibyte pattern is searched in a
/// unibyte buffer (search.c:1335-1343 `search_buffer_non_re`); it is NOT
/// `string-as-unibyte` (which reinterprets the internal bytes).
pub fn str_to_unibyte(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut p = 0usize;
    while p < src.len() {
        let (c, len) = string_char(&src[p..]);
        out.push((c & 0xFF) as u8);
        p += len.max(1);
    }
    out
}

/// Parse unibyte text as a multibyte sequence: count characters and
/// final byte size, treating valid multibyte sequences as already
/// multibyte and lone high bytes as raw-byte chars (2 bytes each).
///
/// Mirrors GNU `parse_str_as_multibyte` (character.c:543).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MultibyteParseMetrics {
    pub chars: usize,
    pub nbytes: usize,
}

pub fn parse_str_as_multibyte(src: &[u8]) -> MultibyteParseMetrics {
    let mut chars = 0usize;
    let mut nbytes = 0usize;
    let mut p = 0usize;
    while p < src.len() {
        match multibyte_length(&src[p..], true) {
            Some(n) => {
                p += n;
                nbytes += n;
            }
            None => {
                p += 1;
                nbytes += 2;
            }
        }
        chars += 1;
    }
    MultibyteParseMetrics { chars, nbytes }
}

/// Reinterpret unibyte text as multibyte, preserving valid multibyte
/// sequences and converting lone high bytes to raw-byte characters.
///
/// Mirrors GNU `str_as_multibyte` (character.c:586). The GNU version
/// edits in place using a worst-case-sized buffer; here we return a
/// freshly allocated `Vec<u8>` for safety.
pub fn str_as_multibyte(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(count_size_as_multibyte(src));
    let mut p = 0usize;
    while p < src.len() {
        match multibyte_length(&src[p..], true) {
            Some(n) => {
                out.extend_from_slice(&src[p..p + n]);
                p += n;
            }
            None => {
                let b = src[p];
                p += 1;
                let c = byte8_to_char(b);
                let mut buf = [0u8; MAX_MULTIBYTE_LENGTH];
                let n = char_string(c, &mut buf);
                out.extend_from_slice(&buf[..n]);
            }
        }
    }
    out
}

/// Convert multibyte text to unibyte by extracting raw-byte chars
/// to single bytes; non-byte8 multibyte chars keep their multibyte form.
///
/// Mirrors GNU `str_as_unibyte` (character.c:709). The GNU version is
/// in-place (the result is shorter); here we return a new `Vec<u8>`.
pub fn str_as_unibyte(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len());
    let mut p = 0usize;
    while p < src.len() {
        let lead = src[p];
        let len = bytes_by_char_head(lead);
        if char_byte8_head_p(lead) && p + len <= src.len() {
            let (c, _) = string_char_unchecked(&src[p..]);
            out.push(char_to_byte8(c));
            p += len;
        } else {
            let take = (p + len).min(src.len());
            out.extend_from_slice(&src[p..take]);
            p = take;
        }
    }
    out
}

/// Downcase an Emacs char code for case-folded comparison. Unicode scalar
/// values fold via their single-char lowercase; codes outside the scalar range
/// (eight-bit raw bytes, extended codes) are caseless and returned verbatim —
/// matching GNU, where raw bytes have no case.
pub(crate) fn emacs_downcase_code(code: u32) -> u32 {
    match char::from_u32(code) {
        Some(c) => {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(first), None) => first as u32,
                _ => code,
            }
        }
        None => code,
    }
}

/// End offset into `hay` of a case-folded match of `needle` starting at byte
/// `at`, comparing decoded Emacs char codes in place so offsets stay in `hay`'s
/// own byte space (correct for unibyte and multibyte; raw bytes compare
/// verbatim). Returns `None` if `needle` does not match at `at`.
pub(crate) fn case_fold_match_len(hay: &[u8], at: usize, needle: &[u8]) -> Option<usize> {
    let mut h = at;
    let mut n = 0;
    while n < needle.len() {
        if h >= hay.len() {
            return None;
        }
        let (hc, hl) = string_char(&hay[h..]);
        let (nc, nl) = string_char(&needle[n..]);
        if emacs_downcase_code(hc) != emacs_downcase_code(nc) {
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
pub(crate) fn unibyte_case_fold_match_len(hay: &[u8], at: usize, needle: &[u8]) -> Option<usize> {
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

// ---------------------------------------------------------------------------
// Eight-bit byte counting and escaping (mirrors GNU character.c:742..839).
// ---------------------------------------------------------------------------

/// Count eight-bit raw-byte characters in `bytes`.
///
/// Mirrors GNU `string_count_byte8` (character.c:742). `multibyte`
/// indicates whether the buffer is multibyte; in unibyte mode, every
/// byte ≥ 0x80 counts; in multibyte mode, only `byte8`-leading code
/// units count.
pub fn string_count_byte8(bytes: &[u8], multibyte: bool) -> usize {
    if !multibyte {
        return bytes.iter().filter(|&&b| b >= 0x80).count();
    }
    let mut count = 0usize;
    let mut p = 0usize;
    while p < bytes.len() {
        let lead = bytes[p];
        let len = bytes_by_char_head(lead);
        if char_byte8_head_p(lead) {
            count += 1;
        }
        p += len.min(bytes.len() - p);
    }
    count
}

/// Replace eight-bit raw-byte characters in `bytes` with `\NNN` octal
/// escapes. ASCII and other multibyte characters are passed through.
///
/// Mirrors GNU `string_escape_byte8` (character.c:772) for the byte
/// payload only — the GNU version returns a new Lisp string with the
/// updated `nchars`/`nbytes`. Callers wanting char/byte counts can
/// recompute via [`chars_in_text`] on the result.
pub fn string_escape_byte8(bytes: &[u8], multibyte: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    if !multibyte {
        for &b in bytes {
            if b >= 0x80 {
                let _ = write_octal(&mut out, b as u32);
            } else {
                out.push(b);
            }
        }
        return out;
    }
    let mut p = 0usize;
    while p < bytes.len() {
        let lead = bytes[p];
        let len = bytes_by_char_head(lead).min(bytes.len() - p);
        if char_byte8_head_p(lead) {
            let (c, n) = string_char_unchecked(&bytes[p..]);
            let _ = write_octal(&mut out, char_to_byte8(c) as u32);
            p += n;
        } else {
            out.extend_from_slice(&bytes[p..p + len]);
            p += len;
        }
    }
    out
}

fn write_octal(out: &mut Vec<u8>, b: u32) -> usize {
    let s = format!("\\{:03o}", b & 0xFF);
    let n = s.len();
    out.extend_from_slice(s.as_bytes());
    n
}

/// Display width of a buffer of multibyte (or unibyte) text.
///
/// Mirrors GNU `strwidth` / `c_string_width` (character.c:290) — the
/// simplified form without precision cap or display-table consultation.
/// Sums [`crate::encoding::char_width`] over decoded code points.
pub fn strwidth(bytes: &[u8], multibyte: bool) -> usize {
    let mut total = 0usize;
    if !multibyte {
        for &b in bytes {
            total += crate::encoding::char_width(b as char);
        }
        return total;
    }
    let mut p = 0usize;
    while p < bytes.len() {
        let (c, len) = string_char(&bytes[p..]);
        if let Some(ch) = char::from_u32(c) {
            total += crate::encoding::char_width(ch);
        } else {
            // Raw-byte / extended char: width 4 (octal display).
            total += 4;
        }
        p += len.max(1);
    }
    total
}

// ---------------------------------------------------------------------------
// Unicode general-category predicates (mirrors GNU character.c:956..1063).
//
// These take a category code (the integer value stored in
// `unicode-category-table`) and return whether it falls into the given UTS
// #18 set. The lookup is performed by the caller; if the lookup yields
// nil (or anything non-fixnum), the predicate must return `false` — to
// match GNU semantics, callers can pass `None` to the `*_opt` variants.
// ---------------------------------------------------------------------------

#[inline]
fn cat_eq(cat: i64, v: UnicodeCategory) -> bool {
    cat == v as i64
}

/// True if `cat` denotes an alphabetic character (UTS #18).
///
/// Mirrors GNU `alphabeticp` (character.c:956): accepts Lu, Ll, Lt,
/// Lm, Lo, Mn, Mc, Me, Nl.
pub fn alphabeticp(cat: i64) -> bool {
    use UnicodeCategory::*;
    cat_eq(cat, UppercaseLetter)
        || cat_eq(cat, LowercaseLetter)
        || cat_eq(cat, TitlecaseLetter)
        || cat_eq(cat, ModifierLetter)
        || cat_eq(cat, OtherLetter)
        || cat_eq(cat, NonspacingMark)
        || cat_eq(cat, SpacingMark)
        || cat_eq(cat, EnclosingMark)
        || cat_eq(cat, LetterNumber)
}

/// True if `cat` denotes an alphabetic-or-decimal character.
///
/// Mirrors GNU `alphanumericp` (character.c:979): adds Nd to the set.
pub fn alphanumericp(cat: i64) -> bool {
    alphabeticp(cat) || cat_eq(cat, UnicodeCategory::DecimalNumber)
}

/// True if `cat` denotes a graphic character (UTS #18).
///
/// Mirrors GNU `graphicp` (character.c:1001): excludes Zs, Zl, Zp,
/// Cc, Cs, Cn.
pub fn graphicp(cat: i64) -> bool {
    use UnicodeCategory::*;
    !(cat_eq(cat, SpaceSeparator)
        || cat_eq(cat, LineSeparator)
        || cat_eq(cat, ParagraphSeparator)
        || cat_eq(cat, Control)
        || cat_eq(cat, Surrogate)
        || cat_eq(cat, Unassigned))
}

/// True if `cat` denotes a printable character.
///
/// Mirrors GNU `printablep` (character.c:1019): excludes Cc, Cs, Cn.
pub fn printablep(cat: i64) -> bool {
    use UnicodeCategory::*;
    !(cat_eq(cat, Control) || cat_eq(cat, Surrogate) || cat_eq(cat, Unassigned))
}

/// True if `cat` denotes a graphic base (printable, non-mark).
///
/// Mirrors GNU `graphic_base_p` (character.c:1034): excludes marks,
/// separators, and Cc/Cs/Cf/Cn.
pub fn graphic_base_p(cat: i64) -> bool {
    use UnicodeCategory::*;
    !(cat_eq(cat, NonspacingMark)
        || cat_eq(cat, SpacingMark)
        || cat_eq(cat, EnclosingMark)
        || cat_eq(cat, SpaceSeparator)
        || cat_eq(cat, LineSeparator)
        || cat_eq(cat, ParagraphSeparator)
        || cat_eq(cat, Control)
        || cat_eq(cat, Surrogate)
        || cat_eq(cat, Format)
        || cat_eq(cat, Unassigned))
}

/// True if `cat` is the space-separator category (Zs).
///
/// Mirrors GNU `blankp` (character.c:1056).
pub fn blankp(cat: i64) -> bool {
    cat_eq(cat, UnicodeCategory::SpaceSeparator)
}

// ---------------------------------------------------------------------------
// Higher-level utilities
// ---------------------------------------------------------------------------

/// Count the number of characters in a multibyte byte sequence.
///
/// Mirrors GNU `multibyte_chars_in_text` (character.c:519). Each step uses
/// `multibyte_length(allow_8bit=true)` and a 0-length result triggers
/// `emacs_abort` in GNU. We assert in debug builds and fall back to a 1-byte
/// advance in release builds so a malformed buffer does not crash production.
pub fn chars_in_multibyte(bytes: &[u8]) -> usize {
    let count = count_lead_bytes(bytes);
    #[cfg(debug_assertions)]
    assert_eq!(
        count,
        chars_in_multibyte_by_decode(bytes),
        "lead-byte count diverged from per-char decode (malformed content?)"
    );
    count
}

/// Number of non-continuation bytes in `bytes`.
///
/// Every character in the internal encoding is exactly one lead byte
/// (anything outside 0x80..=0xBF) followed by its continuation bytes — for
/// ASCII, the C0/C1 raw-byte encoding, and 2..5-byte sequences alike (see
/// `multibyte_length`). Counting leads therefore counts characters, and this
/// bytewise form auto-vectorizes to a few bytes per cycle where the previous
/// per-char `multibyte_length` loop paid a validation ladder per character —
/// position-conversion scans were 10 percent of an elisp editing profile.
/// GNU walks `BYTES_BY_CHAR_HEAD` per char (character.h); on well-formed
/// content this computes the same count without the walk.
#[inline]
fn count_lead_bytes(bytes: &[u8]) -> usize {
    bytes.iter().filter(|&&b| (b & 0xC0) != 0x80).count()
}

/// The pre-vectorization per-char counting loop, kept as the debug oracle
/// for `chars_in_multibyte`: the two agree on every well-formed buffer, and
/// disagreement means the buffer invariant (well-formed internal encoding
/// everywhere) was broken upstream.
#[cfg(debug_assertions)]
fn chars_in_multibyte_by_decode(bytes: &[u8]) -> usize {
    let mut count = 0usize;
    let mut pos = 0usize;
    while pos < bytes.len() {
        let len = match multibyte_length(&bytes[pos..], true) {
            Some(n) if n > 0 => n,
            _ => 1,
        };
        pos += len;
        count += 1;
    }
    count
}

#[cfg(test)]
thread_local! {
    static POSITION_CONVERSION_SCAN_STEPS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn record_position_conversion_scan_step() {
    POSITION_CONVERSION_SCAN_STEPS.with(|steps| steps.set(steps.get() + 1));
}

#[cfg(test)]
pub(crate) fn reset_position_conversion_scan_steps_for_test() {
    POSITION_CONVERSION_SCAN_STEPS.with(|steps| steps.set(0));
}

#[cfg(test)]
pub(crate) fn position_conversion_scan_steps_for_test() -> usize {
    POSITION_CONVERSION_SCAN_STEPS.with(std::cell::Cell::get)
}

/// Convert a character index to a byte offset.
///
/// Returns the byte position of the `char_idx`-th character (0-based).
/// If `char_idx` is beyond the end of the string, returns `bytes.len()`.
pub fn char_to_byte_pos(bytes: &[u8], char_idx: usize) -> usize {
    // Byte offset of the char_idx-th lead byte (see count_lead_bytes for
    // the invariant): count leads a block at a time (vectorized), then walk
    // the block containing the target. Past-the-end char_idx clamps to
    // bytes.len(), matching the previous scan loop.
    if char_idx == 0 {
        return 0;
    }
    #[cfg(test)]
    record_position_conversion_scan_step();
    let mut remaining = char_idx;
    let mut base = 0usize;
    const BLOCK: usize = 64;
    while base + BLOCK <= bytes.len() {
        let leads = count_lead_bytes(&bytes[base..base + BLOCK]);
        if leads >= remaining {
            break;
        }
        remaining -= leads;
        base += BLOCK;
    }
    for (i, &b) in bytes[base..].iter().enumerate() {
        if (b & 0xC0) != 0x80 {
            remaining -= 1;
            if remaining == 0 {
                // The target char STARTS at this lead; its end is the next
                // lead (or the end of the slice).
                let start = base + i;
                return start
                    + bytes[start + 1..]
                        .iter()
                        .position(|&b| (b & 0xC0) != 0x80)
                        .map_or(bytes.len() - start, |n| n + 1);
            }
        }
    }
    bytes.len()
}

/// Convert a byte offset to a character index.
///
/// `byte_pos` should fall on a character boundary. Returns the number of
/// characters before that byte position.
pub fn byte_to_char_pos(bytes: &[u8], byte_pos: usize) -> usize {
    // Characters before byte_pos = lead bytes before it (a char started
    // before byte_pos iff its lead byte is before byte_pos), identical to
    // the previous per-char scan even for a byte_pos inside a character.
    #[cfg(test)]
    if byte_pos > 0 {
        record_position_conversion_scan_step();
    }
    count_lead_bytes(&bytes[..byte_pos.min(bytes.len())])
}

/// If the byte sequence is valid UTF-8 (i.e. contains no raw-byte overlong
/// sequences), return it as a `&str`. Otherwise return `None`.
pub fn try_as_utf8(bytes: &[u8]) -> Option<&str> {
    // A quick check: if std UTF-8 validation passes AND there are no C0/C1
    // lead bytes, it is plain UTF-8. However, C0/C1 leads are always invalid
    // UTF-8 anyway, so `std::str::from_utf8` already rejects them.
    std::str::from_utf8(bytes).ok()
}

/// Convert an Emacs-encoded byte sequence to a UTF-8 `String`, replacing any
/// raw-byte characters with U+FFFD (REPLACEMENT CHARACTER).
pub fn to_utf8_lossy(bytes: &[u8]) -> String {
    let mut out = String::new();
    let mut pos = 0;
    while pos < bytes.len() {
        let (c, len) = string_char(&bytes[pos..]);
        if char_byte8_p(c) {
            out.push('\u{FFFD}');
        } else if let Some(ch) = char::from_u32(c) {
            out.push(ch);
        } else {
            out.push('\u{FFFD}');
        }
        pos += len;
    }
    out
}

/// Decode Emacs-bytes into a lossy Rust `String` for measurement/layout/Debug
/// consumers (line/char counting, motion, display estimation).
///
/// Valid Unicode (including real PUA glyphs) is preserved; raw eight-bit bytes
/// that have no Unicode scalar become `U+FFFD` (which still counts as one
/// char). This is NOT a faithful round-trip and must never be used on any
/// subprocess/content path — those use the byte-faithful APIs instead.
pub fn emacs_bytes_to_lossy_string(bytes: &[u8], multibyte: bool) -> String {
    if multibyte {
        to_utf8_lossy(bytes)
    } else {
        bytes
            .iter()
            .map(|&b| if b < 0x80 { b as char } else { '\u{FFFD}' })
            .collect()
    }
}

/// Convert a UTF-8 string to Emacs internal encoding.
///
/// For standard Unicode this is a no-op (UTF-8 is a subset of Emacs encoding).
/// The returned `Vec<u8>` is byte-for-byte identical to the input's UTF-8
/// representation.
pub fn utf8_to_emacs(s: &str) -> Vec<u8> {
    // Standard UTF-8 is already valid Emacs encoding. No transformation needed.
    s.as_bytes().to_vec()
}

// ---------------------------------------------------------------------------
// Tests (in separate file per project convention)
// ---------------------------------------------------------------------------

#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
