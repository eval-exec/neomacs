//! Character encoding, multibyte support, and character utilities.
//!
//! Neomacs uses UTF-8 internally.  This module provides Emacs-compatible
//! character classification, width calculation, and encoding conversion
//! APIs.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_min_args};
use crate::emacs_core::intern::{SymId, intern, resolve_sym};
// encoding.rs: sentinel imports removed; using emacs_char + LispString directly
use crate::buffer::{EmacsBytePos, EmacsByteRange, LispCharPos1, TextPositionAnchor};
use crate::emacs_core::value::{StringTextPropertyRun, Value, ValueKind};
use encoding_rs::{BIG5, GBK};

const MAX_CHAR_CODE: i64 = 0x3F_FFFF;
const RAW_BYTE_SENTINEL_BASE: u32 = 0xE000;
const RAW_BYTE_SENTINEL_MIN: u32 = 0xE080;
const RAW_BYTE_SENTINEL_MAX: u32 = 0xE0FF;
const UNIBYTE_BYTE_SENTINEL_MIN: u32 = 0xE300;
const UNIBYTE_BYTE_SENTINEL_BASE: u32 = 0xE300;
const UNIBYTE_BYTE_SENTINEL_MAX: u32 = 0xE3FF;

// GNU Emacs seeds the default `char-width-table` from
// `lisp/international/characters.el`.  Keep the built-in fallback aligned with
// that default table rather than maintaining an ad hoc set of wide ranges.
const GNU_DEFAULT_WIDE_RANGES: &[(u32, u32)] = &[
    (0x1100, 0x115F),
    (0x231A, 0x231B),
    (0x2329, 0x232A),
    (0x23E9, 0x23EC),
    (0x23F0, 0x23F0),
    (0x23F3, 0x23F3),
    (0x25FD, 0x25FE),
    (0x2614, 0x2615),
    (0x2630, 0x2637),
    (0x2648, 0x2653),
    (0x267F, 0x267F),
    (0x268A, 0x268F),
    (0x2690, 0x2693),
    (0x26A1, 0x26A1),
    (0x26AA, 0x26AB),
    (0x26BD, 0x26BE),
    (0x26C4, 0x26C5),
    (0x26CE, 0x26CE),
    (0x26D4, 0x26D4),
    (0x26EA, 0x26EA),
    (0x26F2, 0x26F3),
    (0x26F5, 0x26F5),
    (0x26FA, 0x26FA),
    (0x26FD, 0x26FD),
    (0x2705, 0x2705),
    (0x270A, 0x270B),
    (0x2728, 0x2728),
    (0x274C, 0x274C),
    (0x274E, 0x274E),
    (0x2753, 0x2755),
    (0x2757, 0x2757),
    (0x2795, 0x2797),
    (0x27B0, 0x27B0),
    (0x27BF, 0x27BF),
    (0x2B1B, 0x2B1C),
    (0x2B50, 0x2B50),
    (0x2B55, 0x2B55),
    (0x2E80, 0x2E99),
    (0x2E9B, 0x2EF3),
    (0x2F00, 0x2FD5),
    (0x2FF0, 0x2FFF),
    (0x3000, 0x303E),
    (0x3041, 0x3096),
    (0x3099, 0x30FF),
    (0x3105, 0x312F),
    (0x3131, 0x31E5),
    (0x31EF, 0x31EF),
    (0x31F0, 0x3247),
    (0x3250, 0x4DBF),
    (0x4DC0, 0x4DFF),
    (0x4E00, 0xA48C),
    (0xA490, 0xA4C6),
    (0xA960, 0xA97C),
    (0xAC00, 0xD7A3),
    (0xF900, 0xFAFF),
    (0xFE10, 0xFE19),
    (0xFE30, 0xFE6B),
    (0xFF01, 0xFF60),
    (0xFFE0, 0xFFE6),
    (0x16FE0, 0x16FE4),
    (0x16FF0, 0x16FF6),
    (0x17000, 0x187F7),
    (0x18800, 0x18AFF),
    (0x18B00, 0x18CD5),
    (0x18CFF, 0x18CFF),
    (0x18D00, 0x18D1E),
    (0x18D80, 0x18DF2),
    (0x1AFF0, 0x1AFF3),
    (0x1AFF5, 0x1AFFB),
    (0x1AFFD, 0x1AFFE),
    (0x1B000, 0x1B122),
    (0x1B132, 0x1B132),
    (0x1B150, 0x1B152),
    (0x1B155, 0x1B155),
    (0x1B164, 0x1B167),
    (0x1B170, 0x1B2FB),
    (0x1D300, 0x1D356),
    (0x1D360, 0x1D376),
    (0x1F004, 0x1F004),
    (0x1F0CF, 0x1F0CF),
    (0x1F18E, 0x1F18E),
    (0x1F191, 0x1F19A),
    (0x1F1AD, 0x1F1AD),
    (0x1F200, 0x1F202),
    (0x1F210, 0x1F23B),
    (0x1F240, 0x1F248),
    (0x1F250, 0x1F251),
    (0x1F260, 0x1F265),
    (0x1F300, 0x1F320),
    (0x1F32D, 0x1F335),
    (0x1F337, 0x1F37C),
    (0x1F37E, 0x1F393),
    (0x1F3A0, 0x1F3CA),
    (0x1F3CF, 0x1F3D3),
    (0x1F3E0, 0x1F3F0),
    (0x1F3F4, 0x1F3F4),
    (0x1F3F8, 0x1F3FA),
    (0x1F3FB, 0x1F3FF),
    (0x1F400, 0x1F43E),
    (0x1F440, 0x1F440),
    (0x1F442, 0x1F4FC),
    (0x1F4FF, 0x1F53D),
    (0x1F54B, 0x1F54E),
    (0x1F550, 0x1F567),
    (0x1F57A, 0x1F57A),
    (0x1F595, 0x1F596),
    (0x1F5A4, 0x1F5A4),
    (0x1F5FB, 0x1F5FF),
    (0x1F600, 0x1F64F),
    (0x1F680, 0x1F6C5),
    (0x1F6CC, 0x1F6CC),
    (0x1F6D0, 0x1F6D2),
    (0x1F6D5, 0x1F6D8),
    (0x1F6DC, 0x1F6DF),
    (0x1F6EB, 0x1F6EC),
    (0x1F6F4, 0x1F6FC),
    (0x1F7E0, 0x1F7EB),
    (0x1F7F0, 0x1F7F0),
    (0x1F90C, 0x1F93A),
    (0x1F93C, 0x1F945),
    (0x1F947, 0x1F9FF),
    (0x1FA00, 0x1FA53),
    (0x1FA60, 0x1FA6D),
    (0x1FA70, 0x1FA7C),
    (0x1FA80, 0x1FA8A),
    (0x1FA8E, 0x1FAC6),
    (0x1FAC8, 0x1FAC8),
    (0x1FACD, 0x1FADC),
    (0x1FADF, 0x1FAEA),
    (0x1FAEF, 0x1FAF8),
    (0x1FB00, 0x1FB92),
    (0x20000, 0x2FFFF),
    (0x30000, 0x3FFFF),
];

// Zero-width characters — non-spacing marks, enclosing combining
// marks, formatting controls, Hangul Jamo medial/final, and ZWJ/VS
// sequences. Transcribed from `lisp/international/characters.el`
// (the `;; 0: non-spacing, enclosing combining, …` block). This is
// the authoritative default char-width-table entry=0 set used by
// GNU Emacs. Must stay sorted ascending by `start` for the binary
// search in `codepoint_in_sorted_ranges` to work.
const ZERO_WIDTH_RANGES: &[(u32, u32)] = &[
    (0x0300, 0x036F),
    (0x0483, 0x0489),
    (0x0591, 0x05BD),
    (0x05BF, 0x05BF),
    (0x05C1, 0x05C2),
    (0x05C4, 0x05C5),
    (0x05C7, 0x05C7),
    (0x0600, 0x0605),
    (0x0610, 0x061C),
    (0x064B, 0x065F),
    (0x0670, 0x0670),
    (0x06D6, 0x06E4),
    (0x06E7, 0x06E8),
    (0x06EA, 0x06ED),
    (0x070F, 0x070F),
    (0x0711, 0x0711),
    (0x0730, 0x074A),
    (0x07A6, 0x07B0),
    (0x07EB, 0x07F3),
    (0x0816, 0x0823),
    (0x0825, 0x082D),
    (0x0859, 0x085B),
    (0x08D4, 0x0902),
    (0x093A, 0x093A),
    (0x093C, 0x093C),
    (0x0941, 0x0948),
    (0x094D, 0x094D),
    (0x0951, 0x0957),
    (0x0962, 0x0963),
    (0x0981, 0x0981),
    (0x09BC, 0x09BC),
    (0x09C1, 0x09C4),
    (0x09CD, 0x09CD),
    (0x09E2, 0x09E3),
    (0x0A01, 0x0A02),
    (0x0A3C, 0x0A3C),
    (0x0A41, 0x0A4D),
    (0x0A51, 0x0A51),
    (0x0A70, 0x0A71),
    (0x0A75, 0x0A75),
    (0x0A81, 0x0A82),
    (0x0ABC, 0x0ABC),
    (0x0AC1, 0x0AC8),
    (0x0ACD, 0x0ACD),
    (0x0AE2, 0x0AE3),
    (0x0B01, 0x0B01),
    (0x0B3C, 0x0B3C),
    (0x0B3F, 0x0B3F),
    (0x0B41, 0x0B44),
    (0x0B4D, 0x0B56),
    (0x0B62, 0x0B63),
    (0x0B82, 0x0B82),
    (0x0BC0, 0x0BC0),
    (0x0BCD, 0x0BCD),
    (0x0C00, 0x0C00),
    (0x0C3E, 0x0C40),
    (0x0C46, 0x0C56),
    (0x0C62, 0x0C63),
    (0x0C81, 0x0C81),
    (0x0CBC, 0x0CBC),
    (0x0CCC, 0x0CCD),
    (0x0CE2, 0x0CE3),
    (0x0D01, 0x0D01),
    (0x0D41, 0x0D44),
    (0x0D4D, 0x0D4D),
    (0x0D62, 0x0D63),
    (0x0D81, 0x0D81),
    (0x0DCA, 0x0DCA),
    (0x0DD2, 0x0DD6),
    (0x0E31, 0x0E31),
    (0x0E34, 0x0E3A),
    (0x0E47, 0x0E4E),
    (0x0EB1, 0x0EB1),
    (0x0EB4, 0x0EBC),
    (0x0EC8, 0x0ECD),
    (0x0F18, 0x0F19),
    (0x0F35, 0x0F35),
    (0x0F37, 0x0F37),
    (0x0F39, 0x0F39),
    (0x0F71, 0x0F7E),
    (0x0F80, 0x0F84),
    (0x0F86, 0x0F87),
    (0x0F8D, 0x0FBC),
    (0x0FC6, 0x0FC6),
    (0x102D, 0x1030),
    (0x1032, 0x1037),
    (0x1039, 0x103A),
    (0x103D, 0x103E),
    (0x1058, 0x1059),
    (0x105E, 0x1060),
    (0x1071, 0x1074),
    (0x1082, 0x1082),
    (0x1085, 0x1086),
    (0x108D, 0x108D),
    (0x109D, 0x109D),
    (0x1160, 0x11FF),
    (0x135D, 0x135F),
    (0x1712, 0x1714),
    (0x1732, 0x1734),
    (0x1752, 0x1753),
    (0x1772, 0x1773),
    (0x17B4, 0x17B5),
    (0x17B7, 0x17BD),
    (0x17C6, 0x17C6),
    (0x17C9, 0x17D3),
    (0x17DD, 0x17DD),
    (0x180B, 0x180E),
    (0x1885, 0x1886),
    (0x18A9, 0x18A9),
    (0x1920, 0x1922),
    (0x1927, 0x1928),
    (0x1932, 0x1932),
    (0x1939, 0x193B),
    (0x1A17, 0x1A18),
    (0x1A1B, 0x1A1B),
    (0x1A56, 0x1A56),
    (0x1A58, 0x1A5E),
    (0x1A60, 0x1A60),
    (0x1A62, 0x1A62),
    (0x1A65, 0x1A6C),
    (0x1A73, 0x1A7C),
    (0x1A7F, 0x1A7F),
    (0x1AB0, 0x1AC0),
    (0x1B00, 0x1B03),
    (0x1B34, 0x1B34),
    (0x1B36, 0x1B3A),
    (0x1B3C, 0x1B3C),
    (0x1B42, 0x1B42),
    (0x1B6B, 0x1B73),
    (0x1B80, 0x1B81),
    (0x1BA2, 0x1BA5),
    (0x1BA8, 0x1BA9),
    (0x1BAB, 0x1BAD),
    (0x1BE6, 0x1BE6),
    (0x1BE8, 0x1BE9),
    (0x1BED, 0x1BED),
    (0x1BEF, 0x1BF1),
    (0x1C2C, 0x1C33),
    (0x1C36, 0x1C37),
    (0x1CD0, 0x1CD2),
    (0x1CD4, 0x1CE0),
    (0x1CE2, 0x1CE8),
    (0x1CED, 0x1CED),
    (0x1CF4, 0x1CF4),
    (0x1CF8, 0x1CF9),
    (0x1DC0, 0x1DFF),
    (0x200B, 0x200F),
    (0x202A, 0x202E),
    (0x2060, 0x206F),
    (0x20D0, 0x20F0),
    (0x2CEF, 0x2CF1),
    (0x2D7F, 0x2D7F),
    (0x2DE0, 0x2DFF),
    (0xA66F, 0xA672),
    (0xA674, 0xA69F),
    (0xA6F0, 0xA6F1),
    (0xA802, 0xA802),
    (0xA806, 0xA806),
    (0xA80B, 0xA80B),
    (0xA825, 0xA826),
    (0xA82C, 0xA82C),
    (0xA8C4, 0xA8C5),
    (0xA8E0, 0xA8F1),
    (0xA926, 0xA92D),
    (0xA947, 0xA951),
    (0xA980, 0xA9B3),
    (0xA9B6, 0xA9B9),
    (0xA9BC, 0xA9BC),
    (0xA9E5, 0xA9E5),
    (0xAA29, 0xAA2E),
    (0xAA31, 0xAA32),
    (0xAA35, 0xAA36),
    (0xAA43, 0xAA43),
    (0xAA4C, 0xAA4C),
    (0xAA7C, 0xAA7C),
    (0xAAB0, 0xAAB0),
    (0xAAB2, 0xAAB4),
    (0xAAB7, 0xAAB8),
    (0xAABE, 0xAABF),
    (0xAAC1, 0xAAC1),
    (0xAAEC, 0xAAED),
    (0xAAF6, 0xAAF6),
    (0xABE5, 0xABE5),
    (0xABE8, 0xABE8),
    (0xABED, 0xABED),
    (0xD7B0, 0xD7FB),
    (0xFB1E, 0xFB1E),
    (0xFE00, 0xFE0F),
    (0xFE20, 0xFE2F),
    (0xFEFF, 0xFEFF),
    (0xFFF9, 0xFFFB),
    (0x101FD, 0x101FD),
    (0x102E0, 0x102E0),
    (0x10376, 0x1037A),
    (0x10A01, 0x10A0F),
    (0x10A38, 0x10A3F),
    (0x10AE5, 0x10AE6),
    (0x10D69, 0x10D6D),
    (0x10EAB, 0x10EAC),
    (0x10EFC, 0x10EFF),
    (0x11001, 0x11001),
    (0x11038, 0x11046),
    (0x1107F, 0x11081),
    (0x110B3, 0x110B6),
    (0x110B9, 0x110BA),
    (0x110BD, 0x110BD),
    (0x11100, 0x11102),
    (0x11127, 0x1112B),
    (0x1112D, 0x11134),
    (0x11173, 0x11173),
    (0x11180, 0x11181),
    (0x111B6, 0x111BE),
    (0x111CA, 0x111CC),
    (0x111CF, 0x111CF),
    (0x1122F, 0x11231),
    (0x11234, 0x11234),
    (0x11236, 0x11237),
    (0x1123E, 0x1123E),
    (0x112DF, 0x112DF),
    (0x112E3, 0x112EA),
    (0x11300, 0x11301),
    (0x1133C, 0x1133C),
    (0x11340, 0x11340),
    (0x11366, 0x1136C),
    (0x11370, 0x11374),
    (0x113BB, 0x113C0),
    (0x113CE, 0x113CE),
    (0x113D0, 0x113D0),
    (0x113D2, 0x113D2),
    (0x113E1, 0x113E2),
    (0x11438, 0x1143F),
    (0x11442, 0x11444),
    (0x11446, 0x11446),
    (0x114B3, 0x114B8),
    (0x114BA, 0x114C0),
    (0x114C2, 0x114C3),
    (0x115B2, 0x115B5),
    (0x115BC, 0x115BD),
    (0x115BF, 0x115C0),
    (0x115DC, 0x115DD),
    (0x11633, 0x1163A),
    (0x1163D, 0x1163D),
    (0x1163F, 0x11640),
    (0x116AB, 0x116AB),
    (0x116AD, 0x116AD),
    (0x116B0, 0x116B5),
    (0x116B7, 0x116B7),
    (0x1171D, 0x1171F),
    (0x11722, 0x11725),
    (0x11727, 0x1172B),
    (0x1193B, 0x1193C),
    (0x1193E, 0x1193E),
    (0x11943, 0x11943),
    (0x11C30, 0x11C36),
    (0x11C38, 0x11C3D),
    (0x11C92, 0x11CA7),
    (0x11CAA, 0x11CB0),
    (0x11CB2, 0x11CB3),
    (0x11CB5, 0x11CB6),
    (0x11F5A, 0x11F5A),
    (0x13430, 0x13440),
    (0x13447, 0x13455),
    (0x1611E, 0x16129),
    (0x1612D, 0x1612F),
    (0x16AF0, 0x16AF4),
    (0x16B30, 0x16B36),
    (0x16F8F, 0x16F92),
    (0x16FE4, 0x16FE4),
    (0x1BC9D, 0x1BC9E),
    (0x1BCA0, 0x1BCA3),
    (0x1CF00, 0x1CF02),
    (0x1D167, 0x1D169),
    (0x1D173, 0x1D182),
    (0x1D185, 0x1D18B),
    (0x1D1AA, 0x1D1AD),
    (0x1D242, 0x1D244),
    (0x1DA00, 0x1DA36),
    (0x1DA3B, 0x1DA6C),
    (0x1DA75, 0x1DA75),
    (0x1DA84, 0x1DA84),
    (0x1DA9B, 0x1DA9F),
    (0x1DAA1, 0x1DAAF),
    (0x1E000, 0x1E006),
    (0x1E008, 0x1E018),
    (0x1E01B, 0x1E021),
    (0x1E023, 0x1E024),
    (0x1E026, 0x1E02A),
    (0x1E5EE, 0x1E5EF),
    (0x1E8D0, 0x1E8D6),
    (0x1E944, 0x1E94A),
    (0xE0001, 0xE01EF),
];

#[inline]
fn codepoint_in_sorted_ranges(cp: u32, ranges: &[(u32, u32)]) -> bool {
    let mut low = 0usize;
    let mut high = ranges.len();
    while low < high {
        let mid = (low + high) / 2;
        let (start, end) = ranges[mid];
        if cp < start {
            high = mid;
        } else if cp > end {
            low = mid + 1;
        } else {
            return true;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// Character classification
// ---------------------------------------------------------------------------

/// Character width for display purposes (East Asian width).
pub fn char_width(c: char) -> usize {
    let cp = c as u32;
    // Control characters with dedicated rendering widths.
    if cp == 0x09 {
        return 8; // TAB advances to tab stop
    }
    if cp == 0x0a {
        return 0; // NEWLINE has zero display width
    }
    if cp < 0x20 || cp == 0x7f {
        return 2; // ^X notation
    }
    if (0x80..=0x9f).contains(&cp) {
        return 4; // octal escaped control bytes
    }
    // GNU `CHARACTER_WIDTH` returns 1 for printable ASCII before the
    // char-width-table lookup.
    if cp < 0x80 {
        return 1;
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

fn char_width_for_code_without_display_table(code: i64) -> usize {
    if let Ok(cp) = u32::try_from(code)
        && crate::emacs_core::emacs_char::char_byte8_p(cp)
    {
        return 4;
    }
    if code > 0x10_FFFF {
        return 1;
    }
    char::from_u32(code as u32).map(char_width).unwrap_or(1)
}

fn display_table_replacement_width(disp: Value) -> Option<usize> {
    let items = disp.as_vector_data()?;
    let mut width = 0usize;
    for item in items {
        if let ValueKind::Fixnum(code) = item.kind()
            && (0..=MAX_CHAR_CODE).contains(&code)
        {
            width = width.saturating_add(char_width_for_code_without_display_table(code));
        }
    }
    Some(width)
}

pub(crate) fn active_display_table(ctx: &crate::emacs_core::eval::Context) -> Option<Value> {
    if let Some(table) = ctx
        .buffers
        .current_buffer()
        .and_then(|buffer| buffer.buffer_local_value("buffer-display-table"))
        .filter(crate::emacs_core::chartable::is_char_table)
    {
        return Some(table);
    }
    ctx.eval_symbol_by_id(crate::emacs_core::intern::intern("standard-display-table"))
        .ok()
        .filter(crate::emacs_core::chartable::is_char_table)
}

pub(crate) fn char_width_for_code_with_display_table(
    code: i64,
    display_table: Option<Value>,
) -> usize {
    let default_width = char_width_for_code_without_display_table(code);
    let Some(table) = display_table else {
        return default_width;
    };
    crate::emacs_core::chartable::ct_lookup(&table, code)
        .ok()
        .and_then(display_table_replacement_width)
        .unwrap_or(default_width)
}

/// Whether the character is zero-width (combining mark, etc.).
fn is_zero_width(c: char) -> bool {
    codepoint_in_sorted_ranges(c as u32, ZERO_WIDTH_RANGES)
}

/// Whether the character is full-width (East Asian wide).
fn is_wide_char(c: char) -> bool {
    codepoint_in_sorted_ranges(c as u32, GNU_DEFAULT_WIDE_RANGES)
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
    s.chars().any(|ch| {
        let cp = ch as u32;
        cp > 0x7f && !(UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&cp)
    })
}

/// Resolve the EOL conversion type of a coding-system NAME.
///
/// GNU keys EOL conversion off `CODING_ID_EOL_TYPE` (the resolved coding
/// system's eol_type slot — `setup_coding_system`/`consume_chars`/`decode_eol`,
/// src/coding.c), not off the spelling of the name.  Neomacs reaches the EOL
/// step with the coding system already spelled as a name string, so the eol
/// type is recovered here:
///   * an explicit `-unix`/`-dos`/`-mac` suffix maps to the matching type, and
///   * the three bare built-in EOL aliases `unix`/`dos`/`mac` (defined by GNU
///     `mule-conf.el` as `undecided-unix`/`-dos`/`-mac`) map to their type too.
///
/// Every other name (no suffix) yields `Undecided`, GNU's VECTOR eol_type: no
/// EOL conversion on encode (`consume_chars` forces `Qunix` for a vector,
/// src/coding.c:7623) and DETECTION on decode (`decode_eol` scans the decoded
/// text for a vector, src/coding.c:6785).  Centralizing this here means
/// encode-coding-string, decode-coding-string, write-region, call-process-region
/// and process send/filter all share one resolution and the bare aliases stop
/// being silently dropped.
///
/// The three exceptions are the C-defined byte-faithful systems.  GNU gives
/// `no-conversion` (and its `binary` alias, and `no-conversion-multibyte`)
/// `:eol-type unix` outright, so they are the only shipped coding systems whose
/// eol type is concrete while their NAME carries no suffix -- verified by
/// listing `coding-system-eol-type` over all 269 systems in GNU 31.0.90, where
/// exactly `binary`, `no-conversion` and `no-conversion-multibyte` come back
/// concrete without a suffix.  Reading them as undecided was harmless for as
/// long as undecided meant "do nothing"; it stops being harmless the moment it
/// means "detect", which is why entry 131 recorded the conflation in advance.
pub(crate) fn coding_name_eol(coding_system: &str) -> crate::emacs_core::coding::EolType {
    use crate::emacs_core::coding::EolType;
    match coding_system {
        "unix" => EolType::Unix,
        "dos" => EolType::Dos,
        "mac" => EolType::Mac,
        "binary" | "no-conversion" | "no-conversion-multibyte" => EolType::Unix,
        _ => EolType::from_suffix(coding_system).unwrap_or(EolType::Undecided),
    }
}

/// The `&str` twin of `expand_source_eol`: the single EOL pass for the
/// `encode_string` entry point.  Same rule, same GNU reference
/// (`consume_chars`, src/coding.c:7607) -- convert the SOURCE once, above the
/// codec match, so no codec branch is ever offered the decision.
fn expand_source_eol_text(
    s: &str,
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> String {
    use crate::emacs_core::coding::ResolvedEol;
    match coding_name_eol(coding_system).for_encode(eol_conversion) {
        ResolvedEol::Dos => {
            let mut out = String::with_capacity(s.len() + s.matches('\n').count());
            for ch in s.chars() {
                if ch == '\n' {
                    out.push('\r');
                }
                out.push(ch);
            }
            out
        }
        ResolvedEol::Mac => s.replace('\n', "\r"),
        ResolvedEol::Unix => s.to_string(),
    }
}

/// The coding-system NAME with its EOL leg spent.
///
/// Everything below the single EOL pass must be handed this name rather than
/// the caller's: `coding_name_eol` resolves it to `Unix`, so a second EOL pass
/// anywhere downstream is a no-op *by construction* and CR CR LF cannot be
/// produced however the encoders are later rearranged.  This is the naming
/// counterpart of GNU spending `CODING_ID_EOL_TYPE` inside `consume_chars`
/// (src/coding.c:7607): once the charbuf is filled, no `encode_coding_*` ever
/// looks at the eol type again.
fn coding_name_with_eol_spent(coding_system: &str) -> std::borrow::Cow<'_, str> {
    use crate::emacs_core::coding::EolType;
    use std::borrow::Cow;
    match coding_name_eol(coding_system) {
        EolType::Unix => Cow::Borrowed(coding_system),
        // The three bare built-in EOL aliases (`mule-conf.el`) are
        // `undecided-unix`/`-dos`/`-mac`; spending the leg leaves `unix`.
        // An UNDECIDED leg is spent the same way, because on the decode side it
        // is not a no-op any more: it detects, and a detection that ran above
        // must not run again below.  The `-unix` names this produces are the
        // same ones the dos/mac case has always produced (`utf-8-dos` spends to
        // `utf-8-unix`), so every family/codec lookup below already handles them.
        EolType::Dos | EolType::Mac | EolType::Undecided => match coding_system {
            "dos" | "mac" => Cow::Borrowed("unix"),
            named => Cow::Owned(format!("{}-unix", coding_system_base(named))),
        },
    }
}

/// Apply EOL conversion to the SOURCE text, once, before any encoder runs.
///
/// GNU applies EOL conversion in no encoder at all.  `consume_chars`
/// (src/coding.c:7607) expands the newline while filling the character buffer
/// that every encoder then reads (src/coding.c:7683):
///
/// ```c
/// if (! EQ (eol_type, Qunix))
///   { if (c == '\n') { if (EQ (eol_type, Qdos)) *buf++ = '\r'; else c = '\r'; } }
/// ```
///
/// so `encode_coding_utf_8`, `encode_coding_utf_16`, `encode_coding_iso_2022`
/// and the rest are structurally incapable of skipping it -- they never see a
/// bare newline.  (The buffer headroom comment at src/coding.c:7646,
/// "Compensate for CRLF and conversion", is there for this expansion.)
///
/// Returns `None` when there is nothing to do, so the caller keeps the original
/// string and its identity.  Newline and carriage return are ASCII, and no
/// Emacs-internal multibyte sequence contains an ASCII byte, so substituting at
/// the byte level is exactly a substitution at the character level; the
/// multibyteness of the source is preserved.
fn expand_source_eol(
    s: &crate::heap_types::LispString,
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Option<crate::heap_types::LispString> {
    use crate::emacs_core::coding::ResolvedEol;
    let eol = coding_name_eol(coding_system).for_encode(eol_conversion);
    if matches!(eol, ResolvedEol::Unix) {
        return None;
    }
    let bytes = s.as_bytes();
    if !bytes.contains(&b'\n') {
        return None;
    }
    let expanded: Vec<u8> = match eol {
        ResolvedEol::Dos => {
            let mut out =
                Vec::with_capacity(bytes.len() + bytes.iter().filter(|b| **b == b'\n').count());
            for &byte in bytes {
                if byte == b'\n' {
                    out.push(b'\r');
                }
                out.push(byte);
            }
            out
        }
        ResolvedEol::Mac => bytes
            .iter()
            .map(|&byte| if byte == b'\n' { b'\r' } else { byte })
            .collect(),
        ResolvedEol::Unix => unreachable!("early-returned above"),
    };
    Some(if s.is_multibyte() {
        crate::heap_types::LispString::from_emacs_bytes(expanded)
    } else {
        crate::heap_types::LispString::from_unibyte(expanded)
    })
}

/// GNU's whole `decode_eol` (src/coding.c:6760) for a coding-system NAME:
/// resolve the eol type against the text -- detecting when the name leaves it
/// undecided -- and then convert.
pub(crate) fn decode_eol_text(
    bytes: &[u8],
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Vec<u8> {
    decode_eol_bytes(
        bytes,
        coding_name_eol(coding_system).for_decode(bytes, eol_conversion),
    )
}

/// GNU `CODING_REQUIRE_DETECTION` (src/coding.h:553-554) for a coding-system
/// NAME.
///
/// `setup_coding_system` raises the mask for exactly three kinds of coding
/// system, and for nothing else:
///
/// * the `undecided` TYPE (src/coding.c:5713), which `prefer-utf-8` and the
///   bare `unix`/`dos`/`mac` aliases share, and which the `-unix`/`-dos`/`-mac`
///   subsidiaries of `undecided` share too -- `undecided-dos` is still type
///   `Qundecided`, so a chunk that has settled the END OF LINE has not settled
///   the character code;
/// * `utf-8` whose `:bom` is a CONS, i.e. `utf-8-auto` (:5786);
/// * `utf-16` whose `:bom` is a CONS, i.e. the bare `utf-16` (:5804).
///
/// `raw-text` is deliberately absent, for the same reason it is absent from
/// `process_coding_name_converts_nothing`: the half of it that is undecided is
/// the end of line, not the character code.  Measured under GNU 31.0.90 rather
/// than recalled --
///
/// ```text
/// (coding-system-type 'raw-text)      => raw-text
/// (coding-system-type 'undecided)     => undecided
/// (coding-system-type 'undecided-dos) => undecided
/// (coding-system-type 'prefer-utf-8)  => undecided
/// (coding-system-get 'utf-8-auto :bom) => (utf-8-with-signature . utf-8)
/// (coding-system-get 'utf-16 :bom)     => (utf-16le-with-signature . utf-16be-with-signature)
/// ```
///
/// -- and visible in behaviour too: a subprocess whose buffer is unibyte, i.e.
/// the `raw_text_coding_system` downgrade, reports `raw-text-dos` for
/// `caf <c3> <a9> CR LF`, never `utf-8-dos`.
pub(crate) fn coding_name_requires_detection(coding_system: &str) -> bool {
    matches!(
        coding_system_base(coding_system),
        "undecided" | "prefer-utf-8" | "utf-8-auto" | "utf-16"
    )
}

/// GNU's `detect_coding` (src/coding.c:6503-6759) reduced to its one lasting
/// effect: the coding system a decode of BYTES will actually run with.
///
/// `detect_coding` does not merely answer a question -- it REPLACES the coding
/// system, `setup_coding_system (found, coding)` (:6751), and then re-applies
/// the end-of-line type the caller had specified (:6752-6753).  So a decode
/// that started at `undecided` ends at `utf-8` with its eol still to be
/// resolved by `decode_eol`, and one that started at `undecided-dos` ends at
/// `utf-8-dos` outright.
///
/// `None` is GNU's nil `found`, which leaves the name alone.  That is not an
/// error case: it is the normal outcome for pure-ASCII bytes, because
/// `detect_coding`'s whole body is guarded by `null_byte_found ||
/// eight_bit_found || coding->head_ascii < coding->src_bytes ||
/// detect_info.found` (:6596-6599).  Measured under GNU 31.0.90, a subprocess
/// whose first chunk is `a CRLF b CRLF` reports `undecided-dos` -- the eol half
/// moved and the character half did not -- and a later non-ASCII chunk still
/// moves it on to `utf-8-dos`.
pub(crate) fn detected_coding_name(
    coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    coding_system: &str,
    bytes: &[u8],
    block: crate::emacs_core::coding::SourceBlock,
) -> Option<&'static str> {
    let base = coding_system_base(coding_system);
    let found = match base {
        "undecided" | "prefer-utf-8" => {
            let detected = resolve_sym(detect_undecided_coding(
                coding_systems,
                bytes,
                base == "prefer-utf-8",
                block,
            )?);
            // A detection result of `undecided` is this engine's spelling of
            // GNU's nil `found`.
            if coding_system_base(detected) == "undecided" {
                return None;
            }
            detected
        }
        // The two arms that are keyed on a byte-order mark rather than on the
        // category priority list (:6702-6742).
        _ => detect_bom_auto_coding(base, bytes, block)?,
    };
    let name = apply_explicit_eol_suffix(found, coding_name_eol(coding_system));
    Some(resolve_sym(intern(&name)))
}

/// One run of a subprocess's output, decoded, together with the coding system
/// the decode ENDED on.
///
/// This is GNU's `decode_coding` (src/coding.c:7398-7484) for the one caller
/// that has to REPORT what it decoded with: the decoder runs first, then
/// `decode_eol` scans the text the decoder produced (:7481), and
/// `read_process_output_set_last_coding_system` (src/process.c:6417-6425) reads
/// the resulting `coding->id` back out into `Vlast_coding_system_used` and into
/// the process's own decode slot.
///
/// Returning that name rather than only the text is the whole point.  A
/// subprocess is decoded through ONE `struct coding_system` in GNU, so once
/// `detect_coding` or `adjust_coding_eol_type` has rewritten `coding->id` the
/// choice is sticky for that process; the only way to be sticky here -- where
/// each read is a separate call -- is for the call to hand its answer back to
/// the process record.
#[derive(Debug)]
pub(crate) struct DecodedProcessRun {
    /// The text to insert, after both conversions.
    pub(crate) text: crate::heap_types::LispString,
    /// GNU `CODING_ID_NAME (coding->id)` once both rewrites have had their
    /// turn: the detected base carrying the resolved end-of-line type.
    pub(crate) used: &'static str,
    /// GNU `coding->carryover` (src/coding.c:7466-7474): the trailing bytes
    /// the DECODER could not consume, which
    /// `read_process_output_set_last_coding_system` copies onto the process
    /// after the decode (src/process.c:6448-6457) and the next read prepends.
    ///
    /// It comes back with the text and not with the read, because only the
    /// decoder knows it -- which is the whole point of [`SourceConsumed`].  The
    /// read that produced these bytes has already given the process record up,
    /// and would have had to guess.
    pub(crate) carryover: Vec<u8>,
}

/// GNU `CODING_ID_NAME (coding->id)` after a decode: the coding system the
/// decode reports having used, read out of the resolution that ran it.
///
/// This is the name behind `last-coding-system-used` (src/coding.c:9497 for a
/// region, :9644 for a string) and
/// behind `(process-coding-system P)` (src/process.c:6421) alike, because in
/// GNU they are one field of one object.  When `adjust_coding_eol_type` fired,
/// it is the SUBSIDIARY that call rewrote the id to -- and the subsidiary comes
/// out of the coding system's own eol vector, which holds canonical names, so
/// an alias reports its base's subsidiary (measured under GNU 31.0.90:
/// `coding-system-for-read` `latin-1` over a CR-only child answers
/// `iso-latin-1-mac`).  When it did not fire -- a concrete eol type, no
/// terminator in the text, or `inhibit-eol-conversion` -- the name is the one
/// the coding system was set up with, verbatim.
///
/// Taking the [`DecodeEolResolution`](crate::emacs_core::coding::DecodeEolResolution)
/// rather than re-scanning the text is what keeps the reported name and the
/// conversion the same decision: GNU makes them one call, and every caller here
/// has to make them one value.
pub(crate) fn adjusted_coding_name(
    coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    coding_system: &str,
    eol: crate::emacs_core::coding::DecodeEolResolution,
) -> String {
    match eol.adjusted() {
        Some(resolved) => coding_systems
            .canonical_name_for_detected_eol(coding_system, resolved.suffix())
            .unwrap_or_else(|| coding_system.to_owned()),
        None => coding_system.to_owned(),
    }
}

/// GNU `coding_inherit_eol_type (CODING_SYSTEM, Qnil)` (src/coding.c:5972-6008).
///
/// A coding system whose eol type is already concrete is returned unchanged; a
/// VECTOR one takes the SYSTEM's end-of-line type, which is `Qunix` everywhere
/// but DOS/Windows, i.e. `AREF (eol_type, 0)`.  `read_process_output_set_last_coding_system`
/// uses it to complete a process's ENCODE coding system from the coding its
/// output was just decoded with (src/process.c:6442-6444).
pub(crate) fn coding_inherit_unix_eol_type(
    coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    coding_system: &str,
) -> String {
    if !matches!(
        coding_name_eol(coding_system),
        crate::emacs_core::coding::EolType::Undecided
    ) {
        return coding_system.to_owned();
    }
    coding_systems
        .canonical_name_for_detected_eol(coding_system, "-unix")
        .unwrap_or_else(|| coding_system.to_owned())
}

/// The decode-side mirror of `expand_source_eol`, keyed on the RESOLVED EOL type
/// rather than on a coding-system name.
///
/// GNU is symmetric here: `decode_coding` (src/coding.c:7481) calls
/// `decode_eol` AFTER the decoder has run, for every coding system, so no
/// `decode_coding_*` decides this any more than an encoder does.  Taking a
/// `ResolvedEol` is what makes GNU's other half unskippable: an undecided eol
/// type has no representation here, so it cannot arrive and be quietly copied
/// through -- it has to be resolved by `EolType::for_decode` first.
fn decode_eol_bytes(bytes: &[u8], eol: crate::emacs_core::coding::ResolvedEol) -> Vec<u8> {
    use crate::emacs_core::coding::ResolvedEol;
    match eol {
        ResolvedEol::Dos => {
            let mut out = Vec::with_capacity(bytes.len());
            let mut i = 0usize;
            while i < bytes.len() {
                if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
                    out.push(b'\n');
                    i += 2;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            out
        }
        ResolvedEol::Mac => bytes
            .iter()
            .map(|byte| if *byte == b'\r' { b'\n' } else { *byte })
            .collect(),
        ResolvedEol::Unix => bytes.to_vec(),
    }
}

pub(crate) fn coding_system_family(coding_system: &str) -> &str {
    match coding_system_base(coding_system) {
        "utf-8-with-signature" => "utf-8",
        "latin-1" | "iso-8859-1" | "iso-latin-1" => "iso-latin-1",
        "latin-5" | "iso-8859-9" | "iso-latin-5" => "iso-latin-5",
        "latin-0" | "latin-9" | "iso-8859-15" | "iso-latin-9" => "iso-latin-9",
        "cn-gb-2312" | "euc-china" | "euc-cn" | "cn-gb" | "gb2312" | "chinese-iso-8bit" => {
            "chinese-iso-8bit"
        }
        "big5" | "cn-big5" | "cp950" | "chinese-big5" => "chinese-big5",
        "big5-hkscs" | "cn-big5-hkscs" | "chinese-big5-hkscs" => "chinese-big5-hkscs",
        "emacs-internal" => "utf-8-emacs",
        family => family,
    }
}

fn coding_system_base(coding_system: &str) -> &str {
    // The three bare built-in EOL aliases (`unix`/`dos`/`mac`) are GNU
    // `mule-conf.el` aliases of `undecided-{unix,dos,mac}`, so their base coding
    // system is `undecided` (raw-text-style byte pass-through plus EOL
    // conversion).  Without this the bare names fall through to the per-charset
    // encoders with an unknown family and drop every character (the `dos`
    // alias would encode to the empty string).
    match coding_system {
        "unix" | "dos" | "mac" => "undecided",
        _ => coding_system
            .strip_suffix("-unix")
            .or_else(|| coding_system.strip_suffix("-dos"))
            .or_else(|| coding_system.strip_suffix("-mac"))
            .unwrap_or(coding_system),
    }
}

fn coding_system_consumes_utf8_signature(coding_system: &str) -> bool {
    coding_system_base(coding_system) == "utf-8-with-signature"
}

/// Whether encoding through `coding_system` must prepend a UTF-8 BOM
/// (`EF BB BF`).  `utf-8-with-signature` always does; `utf-8-auto` does on
/// encode (GNU `encode_coding_utf_8`, BOM type `utf_with_bom`).
fn coding_system_prepends_utf8_signature(coding_system: &str) -> bool {
    matches!(
        coding_system_base(coding_system),
        "utf-8-with-signature" | "utf-8-auto"
    )
}

#[derive(Clone, Copy)]
enum Utf16Endian {
    Big,
    Little,
}

/// GNU's `enum utf_bom_type` (src/coding.c) for the UTF-16 family, which has
/// all THREE of its values in play -- measured under GNU 31.0.90 as
/// `(coding-system-get CS :bom)` over the six shipped systems.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Utf16Bom {
    /// `:bom nil` -- `utf-16be`, `utf-16le`.  A leading `FE FF` is DATA.
    Without,
    /// `:bom t` -- `utf-16be-with-signature`, `utf-16le-with-signature` and
    /// their `utf-16-be`/`utf-16-le` aliases.  `decode_coding_utf_16` reads two
    /// bytes and consumes them ONLY if they are the BOM for the system's own
    /// endianness, rewinding otherwise (src/coding.c:1594-1609).
    Declared,
    /// `:bom (LE . BE)` -- `utf-16`, the auto system.  The decoder consumes
    /// nothing: "We have already tried to detect BOM and failed in
    /// detect_coding" (src/coding.c:1612-1617).  A BOM is consumed only because
    /// `detect_coding` re-based the coding system to one of the two
    /// `-with-signature` systems first, and those are [`Utf16Bom::Declared`].
    Detect,
}

fn utf16_coding_variant(coding_system: &str) -> Option<(Utf16Endian, Utf16Bom)> {
    let base = coding_system
        .strip_suffix("-unix")
        .or_else(|| coding_system.strip_suffix("-dos"))
        .or_else(|| coding_system.strip_suffix("-mac"))
        .unwrap_or(coding_system);
    match base {
        "utf-16" => Some((Utf16Endian::Big, Utf16Bom::Detect)),
        "utf-16-be" | "utf-16be-with-signature" => Some((Utf16Endian::Big, Utf16Bom::Declared)),
        "utf-16-le" | "utf-16le-with-signature" => Some((Utf16Endian::Little, Utf16Bom::Declared)),
        "utf-16be" => Some((Utf16Endian::Big, Utf16Bom::Without)),
        "utf-16le" => Some((Utf16Endian::Little, Utf16Bom::Without)),
        _ => None,
    }
}

fn push_utf16_unit(out: &mut Vec<u8>, endian: Utf16Endian, unit: u16) {
    match endian {
        Utf16Endian::Big => out.extend_from_slice(&unit.to_be_bytes()),
        Utf16Endian::Little => out.extend_from_slice(&unit.to_le_bytes()),
    }
}

fn push_utf16_codepoint(out: &mut Vec<u8>, endian: Utf16Endian, code: u32) {
    let code = if crate::emacs_core::emacs_char::char_byte8_p(code) {
        crate::emacs_core::emacs_char::char_to_byte8(code) as u32
    } else if (0xD800..=0xDFFF).contains(&code) || code > 0x10FFFF {
        0xFFFD
    } else {
        code
    };

    if code <= 0xFFFF {
        push_utf16_unit(out, endian, code as u16);
    } else {
        let scalar = code - 0x1_0000;
        let high = 0xD800 | ((scalar >> 10) as u16);
        let low = 0xDC00 | ((scalar & 0x3FF) as u16);
        push_utf16_unit(out, endian, high);
        push_utf16_unit(out, endian, low);
    }
}

fn encode_utf16_lisp_string(
    s: &crate::heap_types::LispString,
    endian: Utf16Endian,
    bom: bool,
) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.sbytes() * 2 + 2);
    if bom {
        match endian {
            Utf16Endian::Big => out.extend_from_slice(&[0xFE, 0xFF]),
            Utf16Endian::Little => out.extend_from_slice(&[0xFF, 0xFE]),
        }
    }

    for code in coding_source_codepoints(s) {
        push_utf16_codepoint(&mut out, endian, code);
    }

    out
}

fn encode_utf16_text(s: &str, endian: Utf16Endian, bom: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len() * 2 + 2);
    if bom {
        match endian {
            Utf16Endian::Big => out.extend_from_slice(&[0xFE, 0xFF]),
            Utf16Endian::Little => out.extend_from_slice(&[0xFF, 0xFE]),
        }
    }
    for code in s.chars().map(|ch| ch as u32) {
        push_utf16_codepoint(&mut out, endian, code);
    }
    out
}

/// GNU `decode_coding_utf_16` (src/coding.c:1580-1690).
///
/// The BOM rule is the coding system's, not the bytes': only a
/// [`Utf16Bom::Declared`] system reads a signature, and only when it is the
/// signature for that system's OWN endianness -- otherwise the two bytes are
/// rewound and decoded as data (:1601-1609).  Measured under GNU 31.0.90:
///
/// ```text
/// (decode-coding-string "\xff\xfe a\0 \r\0 \n\0 b\0" 'utf-16le)  => (65279 97 10 98)
/// (decode-coding-string "\xff\xfe a\0 b\0"           'utf-16-be) => (65534 24832 25088)
/// ```
///
/// The first keeps U+FEFF because `utf-16le`'s `:bom` is nil; the second reads
/// the little-endian signature big-endianly, as U+FFFE, because `utf-16-be`
/// declares a BIG-endian one and this is not it.
fn decode_utf16_bytes(bytes: &[u8], default_endian: Utf16Endian, bom: Utf16Bom) -> String {
    let (endian, body) = match (bom, bytes) {
        (Utf16Bom::Declared, [0xFE, 0xFF, rest @ ..])
            if matches!(default_endian, Utf16Endian::Big) =>
        {
            (Utf16Endian::Big, rest)
        }
        (Utf16Bom::Declared, [0xFF, 0xFE, rest @ ..])
            if matches!(default_endian, Utf16Endian::Little) =>
        {
            (Utf16Endian::Little, rest)
        }
        _ => (default_endian, bytes),
    };

    let mut units = Vec::with_capacity(body.len() / 2);
    let mut chunks = body.chunks_exact(2);
    for chunk in &mut chunks {
        let pair = [chunk[0], chunk[1]];
        units.push(match endian {
            Utf16Endian::Big => u16::from_be_bytes(pair),
            Utf16Endian::Little => u16::from_le_bytes(pair),
        });
    }
    if let &[trailing] = chunks.remainder() {
        // An odd trailing byte is not a unit.  GNU runs out of source mid-pair,
        // and the last block flushes the byte as a character of its own
        // (src/coding.c:7433-7458): `ASCII_CHAR_P (c) ? c : BYTE8_TO_CHAR (c)`.
        // Measured, `(decode-coding-string "\xfe\xff\0" 'utf-16)` is
        // `(65279 0)`.  A trailing byte >= 0x80 becomes an eight-bit character
        // in GNU, which this `String`-returning decoder cannot represent; that
        // one case still answers U+FFFD.
        units.push(if trailing < 0x80 {
            u16::from(trailing)
        } else {
            0xFFFD
        });
    }

    std::char::decode_utf16(units)
        .map(|item| item.unwrap_or(char::REPLACEMENT_CHARACTER))
        .collect()
}

/// A Latin single-byte coding family and the GNU charset that supplies its
/// characters.  Keeping the codec and source-charset identity in one type
/// prevents a family from being decoded correctly while silently losing the
/// `(charset NAME)` property produced by GNU's `decode_coding_charset`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SingleByteCharset {
    Iso8859_1,
    Iso8859_9,
    Iso8859_15,
}

impl SingleByteCharset {
    fn for_coding_system(coding_system: &str) -> Option<Self> {
        match coding_system_family(coding_system) {
            "latin-1" | "iso-8859-1" | "iso-latin-1" => Some(Self::Iso8859_1),
            "iso-latin-5" => Some(Self::Iso8859_9),
            "iso-latin-9" => Some(Self::Iso8859_15),
            _ => None,
        }
    }

    fn source_charset_name(self) -> &'static str {
        match self {
            Self::Iso8859_1 => "iso-8859-1",
            Self::Iso8859_9 => "iso-8859-9",
            Self::Iso8859_15 => "iso-8859-15",
        }
    }

    fn decode_byte(self, byte: u8) -> char {
        match self {
            Self::Iso8859_1 => byte as char,
            Self::Iso8859_9 => match byte {
                0xD0 => '\u{011E}',
                0xDD => '\u{0130}',
                0xDE => '\u{015E}',
                0xF0 => '\u{011F}',
                0xFD => '\u{0131}',
                0xFE => '\u{015F}',
                _ => byte as char,
            },
            Self::Iso8859_15 => match byte {
                0xA4 => '\u{20AC}',
                0xA6 => '\u{0160}',
                0xA8 => '\u{0161}',
                0xB4 => '\u{017D}',
                0xB8 => '\u{017E}',
                0xBC => '\u{0152}',
                0xBD => '\u{0153}',
                0xBE => '\u{0178}',
                _ => byte as char,
            },
        }
    }

    fn encode_char(self, code: u32) -> Option<u8> {
        match self {
            Self::Iso8859_1 => (code <= 0xFF).then_some(code as u8),
            Self::Iso8859_9 => match code {
                0x011E => Some(0xD0),
                0x0130 => Some(0xDD),
                0x015E => Some(0xDE),
                0x011F => Some(0xF0),
                0x0131 => Some(0xFD),
                0x015F => Some(0xFE),
                _ if code <= 0xFF => Some(code as u8),
                _ => None,
            },
            Self::Iso8859_15 => match code {
                0x20AC => Some(0xA4),
                0x0160 => Some(0xA6),
                0x0161 => Some(0xA8),
                0x017D => Some(0xB4),
                0x017E => Some(0xB8),
                0x0152 => Some(0xBC),
                0x0153 => Some(0xBD),
                0x0178 => Some(0xBE),
                _ if code <= 0xFF => Some(code as u8),
                _ => None,
            },
        }
    }
}

/// GNU `decode_coding_utf_8` (src/coding.c:1238-1425), as one character per
/// call into [`decode_units`].
///
/// Every continuation byte is a `ONE_MORE_BYTE` (:1358, :1368, :1381, :1396),
/// so a sequence cut by the end of a read leaves `coding->consumed` at its
/// leading byte -- which is what makes the next read complete the character
/// instead of seeing four eight-bit ones.  A leading byte whose continuation
/// bytes are present but wrong is `invalid_code:` and produces exactly one
/// eight-bit character (:1334-1338).
fn decode_via_utf8(bytes: &[u8], eol: DosEolLookahead) -> DecodedSource {
    decode_units(bytes, eol, |unit, sink| {
        let b0 = unit.byte()?;
        if b0 < 0x80 {
            sink.push(u32::from(b0), None);
            return Ok(());
        }
        // `(payload bits of the leading byte, how many continuation bytes,
        //   the largest character the form may encode)`.
        let form = match b0 {
            0xC2..=0xDF => Some((u32::from(b0 & 0x1F), 1, u32::MAX)),
            0xE0..=0xEF => Some((u32::from(b0 & 0x0F), 2, u32::MAX)),
            0xF0..=0xF7 => Some((u32::from(b0 & 0x07), 3, u32::MAX)),
            0xF8..=0xFB => Some((
                u32::from(b0 & 0x03),
                4,
                crate::emacs_core::emacs_char::MAX_5_BYTE_CHAR,
            )),
            _ => None,
        };
        let Some((mut code, continuations, max)) = form else {
            sink.push_raw(b0);
            return Ok(());
        };
        let mut valid = true;
        for _ in 0..continuations {
            let byte = unit.byte()?;
            if (byte & 0xC0) != 0x80 {
                valid = false;
                break;
            }
            code = (code << 6) | u32::from(byte & 0x3F);
        }
        if valid && code <= max {
            sink.push(code, None);
            return Ok(());
        }
        invalid_code(unit, sink)
    })
}

/// Decode UTF-8(-emacs) `bytes` directly into Emacs-internal storage bytes
/// (the representation `LispString::from_emacs_bytes` expects).
///
/// This never goes through the Rust-`String` "storage string" with its
/// in-Unicode PUA sentinels, so eight-bit raw bytes
/// become the extended `0x3FFF00+byte` sequence while genuine Private-Use-Area
/// characters (nerd-font glyphs in U+E000..F8FF) keep their real code points —
/// the two can never be confused (issue #131).
///
/// The source is a COMPLETE one here, so a truncated trailing sequence is
/// flushed as eight-bit characters the way GNU's `CODING_MODE_LAST_BLOCK` arm
/// does (src/coding.c:7434-7462); [`decode_via_utf8`] is the door for a source
/// that may still be continued.
fn decode_utf8_to_emacs_bytes(bytes: &[u8]) -> Vec<u8> {
    let decoded = decode_via_utf8(bytes, DosEolLookahead::NotRequired);
    let mut out = decoded.bytes;
    flush_last_block_into(&mut out, &bytes[decoded.consumed.bytes()..]);
    out
}

/// GNU's `CODING_MODE_LAST_BLOCK` arm of `decode_coding`
/// (src/coding.c:7434-7462): the bytes no decoder could consume are appended
/// as characters, an ASCII byte as itself and anything else as the eight-bit
/// character `BYTE8_TO_CHAR (c)`.
fn flush_last_block_into(out: &mut Vec<u8>, tail: &[u8]) {
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    for &byte in tail {
        let code = if byte < 0x80 {
            u32::from(byte)
        } else {
            crate::emacs_core::emacs_char::unibyte_to_char(byte)
        };
        let len = crate::emacs_core::emacs_char::char_string(code, &mut buf);
        out.extend_from_slice(&buf[..len]);
    }
}

/// Prepare raw bytes for utf-8(-emacs) decoding (EOL conversion + optional BOM
/// strip) and decode straight to Emacs storage bytes — the sentinel-free
/// counterpart of the utf-8 arm of [`decode_bytes`].
fn decode_utf8_coding_to_emacs_bytes(
    bytes: &[u8],
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Vec<u8> {
    let mut prepared = decode_eol_text(bytes, coding_system, eol_conversion);
    if coding_system_consumes_utf8_signature(coding_system)
        && prepared.starts_with(&[0xEF, 0xBB, 0xBF])
    {
        prepared.drain(..3);
    }
    decode_utf8_to_emacs_bytes(&prepared)
}

fn encode_emacs_utf8_codepoint(code: u32, out: &mut Vec<u8>) {
    if code <= 0x7F {
        out.push(code as u8);
    } else if code <= 0x7FF {
        out.push(0xC0 | ((code >> 6) as u8));
        out.push(0x80 | ((code & 0x3F) as u8));
    } else if code <= 0xFFFF {
        out.push(0xE0 | ((code >> 12) as u8));
        out.push(0x80 | (((code >> 6) & 0x3F) as u8));
        out.push(0x80 | ((code & 0x3F) as u8));
    } else if code <= 0x1F_FFFF {
        out.push(0xF0 | (((code >> 18) & 0x07) as u8));
        out.push(0x80 | (((code >> 12) & 0x3F) as u8));
        out.push(0x80 | (((code >> 6) & 0x3F) as u8));
        out.push(0x80 | ((code & 0x3F) as u8));
    } else if code <= 0x3F_FFFF {
        out.push(0xF8 | (((code >> 24) & 0x03) as u8));
        out.push(0x80 | (((code >> 18) & 0x3F) as u8));
        out.push(0x80 | (((code >> 12) & 0x3F) as u8));
        out.push(0x80 | (((code >> 6) & 0x3F) as u8));
        out.push(0x80 | ((code & 0x3F) as u8));
    } else if code <= 0x7FFF_FFFF {
        out.push(0xFC | (((code >> 30) & 0x01) as u8));
        out.push(0x80 | (((code >> 24) & 0x3F) as u8));
        out.push(0x80 | (((code >> 18) & 0x3F) as u8));
        out.push(0x80 | (((code >> 12) & 0x3F) as u8));
        out.push(0x80 | (((code >> 6) & 0x3F) as u8));
        out.push(0x80 | ((code & 0x3F) as u8));
    }
}

fn encode_utf8_emacs_text(s: &str) -> Vec<u8> {
    // For standard UTF-8 strings (which is what we get from as_str()),
    // the Emacs internal encoding for Unicode chars IS UTF-8.
    // Sentinel codepoints are translated back to their raw byte values
    // before encoding.
    let mut out = Vec::with_capacity(s.len());
    for ch in s.chars() {
        let cp = ch as u32;
        // Translate sentinel codepoints back to raw byte values
        if (RAW_BYTE_SENTINEL_MIN..=RAW_BYTE_SENTINEL_MAX).contains(&cp) {
            let byte = (cp - RAW_BYTE_SENTINEL_BASE) as u8;
            out.push(byte);
        } else if (UNIBYTE_BYTE_SENTINEL_MIN..=UNIBYTE_BYTE_SENTINEL_MAX).contains(&cp) {
            let byte = (cp - UNIBYTE_BYTE_SENTINEL_BASE) as u8;
            out.push(byte);
        } else {
            encode_emacs_utf8_codepoint(cp, &mut out);
        }
    }
    out
}

pub fn encode_lisp_string(
    s: &crate::heap_types::LispString,
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Vec<u8> {
    // The single EOL pass, mirroring GNU `consume_chars` (src/coding.c:7607):
    // convert the SOURCE once, here, above every codec branch, and hand the
    // branches a coding name whose EOL leg is spent.  No branch below can skip
    // EOL conversion because no branch is given the chance to perform it.
    let expanded = expand_source_eol(s, coding_system, eol_conversion);
    encode_lisp_string_eol_spent(
        expanded.as_ref().unwrap_or(s),
        &coding_name_with_eol_spent(coding_system),
        eol_conversion,
    )
}

/// The codec dispatch proper.  Its `coding_system` has already had its EOL leg
/// spent by `encode_lisp_string`, and its source already carries the CR, so
/// nothing here may perform EOL conversion.
fn encode_lisp_string_eol_spent(
    s: &crate::heap_types::LispString,
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Vec<u8> {
    if let Some((endian, bom)) = utf16_coding_variant(coding_system) {
        // GNU `encode_coding_utf_16` writes a signature whenever the coding
        // system's `:bom` is not nil (src/coding.c), so the auto `utf-16` -- whose
        // `:bom` is a cons -- writes one too, in its own big-endian order.
        return encode_utf16_lisp_string(s, endian, bom != Utf16Bom::Without);
    }

    let family = coding_system_family(coding_system);
    if matches!(
        family,
        "utf-8" | "utf-8-emacs" | "undecided" | "prefer-utf-8"
    ) || is_byte_preserving_coding_system(coding_system)
    {
        let mut out = lisp_string_coding_source_bytes(s);
        // utf-8-with-signature / utf-8-auto prepend a BOM on encode (GNU
        // `encode_coding_utf_8`).  Applied here so every caller (write-region,
        // encode-coding-region, ...) gets it, not just the string codec.
        if coding_system_prepends_utf8_signature(coding_system)
            && !out.starts_with(&[0xEF, 0xBB, 0xBF])
        {
            let mut with_bom = vec![0xEF, 0xBB, 0xBF];
            with_bom.append(&mut out);
            out = with_bom;
        }
        return out;
    }

    if matches!(
        family,
        "chinese-iso-8bit" | "chinese-big5" | "chinese-big5-hkscs"
    ) {
        // The Big5/GBK encoders operate on real Unicode text; chinese text is
        // all real scalar values, so decode the LispString's Emacs bytes to a
        // display string rather than the legacy in-Unicode storage form.
        let text = crate::emacs_core::emacs_char::to_utf8_lossy(s.as_bytes());
        return encode_string(&text, coding_system, eol_conversion);
    }

    let single_byte_charset = SingleByteCharset::for_coding_system(coding_system);
    let mut out = Vec::with_capacity(s.sbytes());
    let mut push_encoded = |code: u32| {
        if let Some(charset) = single_byte_charset {
            if let Some(byte) = charset.encode_char(code) {
                out.push(byte);
            } else if crate::emacs_core::emacs_char::char_byte8_p(code) {
                out.push(crate::emacs_core::emacs_char::char_to_byte8(code));
            } else {
                // GNU substitutes an unencodable character with a space (0x20)
                // for the Latin charset coding systems (the ASCII coding uses
                // `?`).
                out.push(b' ');
            }
            return;
        }
        match family {
            "ascii" | "us-ascii" => {
                if code <= 0x7F {
                    out.push(code as u8);
                } else {
                    out.push(b'?');
                }
            }
            // Single-byte / codepage coding systems we do not fully model -- notably
            // the Windows ANSI codepages (windows-1252/cp1252/cp437), which
            // `default-file-name-coding-system` is set to on Windows. GNU round-trips
            // ASCII unchanged. The previous empty arm dropped the WHOLE string, so a
            // pure-ASCII input encoded to "" (e.g. make-temp-file-internal returned a
            // bare nonce because its encoded PREFIX came back empty, leaving
            // `org-persist-directory' separator-less and crashing org's load on
            // Windows). Pass ASCII and raw byte8 through; substitute `?` for
            // characters this codec does not model (matching the us-ascii fallback).
            _ => {
                if code <= 0x7F {
                    out.push(code as u8);
                } else if crate::emacs_core::emacs_char::char_byte8_p(code) {
                    out.push(crate::emacs_core::emacs_char::char_to_byte8(code));
                } else {
                    out.push(b'?');
                }
            }
        }
    };

    for code in coding_source_codepoints(s) {
        push_encoded(code);
    }

    // Release the closure's mutable borrow of `out` so it can be returned.
    drop(push_encoded);
    out
}

// ---------------------------------------------------------------------------
// Encoding conversion
// ---------------------------------------------------------------------------

/// Encode a string to bytes using the specified coding system.
/// Currently only UTF-8 is supported.
pub fn encode_string(
    s: &str,
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Vec<u8> {
    let eol_text = expand_source_eol_text(s, coding_system, eol_conversion);
    if let Some((endian, bom)) = utf16_coding_variant(coding_system) {
        return encode_utf16_text(&eol_text, endian, bom != Utf16Bom::Without);
    }

    if let Some(charset) = SingleByteCharset::for_coding_system(coding_system) {
        return eol_text
            .chars()
            .map(|ch| {
                // GNU substitutes a space (0x20) for unencodable characters in
                // Latin charset coding systems.
                charset.encode_char(ch as u32).unwrap_or(b' ')
            })
            .collect();
    }

    match coding_system_family(coding_system) {
        "utf-8" | "utf-8-unix" | "utf-8-dos" | "utf-8-mac" | "utf-8-emacs" => {
            encode_utf8_emacs_text(&eol_text)
        }
        "chinese-big5" | "chinese-big5-hkscs" => {
            let (encoded, _, _) = BIG5.encode(&eol_text);
            encoded.into_owned()
        }
        "chinese-iso-8bit" => {
            let (encoded, _, _) = GBK.encode(&eol_text);
            encoded.into_owned()
        }
        "ascii" | "us-ascii" => eol_text
            .chars()
            .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
            .collect(),
        _ => eol_text.as_bytes().to_vec(), // default to UTF-8
    }
}

/// Decode bytes to a string using the specified coding system.
/// Currently only UTF-8 is supported.
pub fn decode_bytes(
    bytes: &[u8],
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> String {
    if let Some((endian, bom)) = utf16_coding_variant(coding_system) {
        return decode_utf16_bytes(bytes, endian, bom);
    }

    let mut bytes = decode_eol_text(bytes, coding_system, eol_conversion);
    if coding_system_consumes_utf8_signature(coding_system)
        && bytes.starts_with(&[0xEF, 0xBB, 0xBF])
    {
        bytes.drain(..3);
    }
    if let Some(charset) = SingleByteCharset::for_coding_system(coding_system) {
        return bytes
            .iter()
            .map(|&byte| charset.decode_byte(byte))
            .collect();
    }

    match coding_system_family(coding_system) {
        "utf-8" | "utf-8-emacs" => {
            // `decode_bytes` is the lossy `String` boundary; the faithful,
            // byte-exact path is `decode_bytes_to_lisp_string` /
            // `decode_bytes_emacs`, which never touch this arm (issue #131).
            // Non-Unicode Emacs bytes collapse to U+FFFD here, so do NOT route
            // any reader/source path through this — they use the LispString
            // reader instead.
            crate::emacs_core::emacs_char::to_utf8_lossy(&decode_utf8_to_emacs_bytes(&bytes))
        }
        "chinese-big5" | "chinese-big5-hkscs" => {
            let (decoded, _, _) = BIG5.decode(&bytes);
            decoded.into_owned()
        }
        "chinese-iso-8bit" => {
            let (decoded, _, _) = GBK.decode(&bytes);
            decoded.into_owned()
        }
        "ascii" | "us-ascii" => bytes
            .iter()
            .map(|&b| if b < 128 { b as char } else { '?' })
            .collect(),
        _ => String::from_utf8_lossy(&bytes).into_owned(),
    }
}

/// Decode `bytes` under `coding_system` directly to canonical Emacs internal
/// bytes (issue #131) — never through the in-Unicode "storage string". UTF-8
/// uses the sentinel-free decoder (eight-bit -> 0x3FFF00+ extended, real PUA
/// glyphs keep their code points); the other families decode to real Unicode
/// text whose UTF-8 already IS the Emacs internal encoding.
pub(crate) fn decode_bytes_emacs(
    bytes: &[u8],
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Vec<u8> {
    if matches!(coding_system_family(coding_system), "utf-8" | "utf-8-emacs") {
        return decode_utf8_coding_to_emacs_bytes(bytes, coding_system, eol_conversion);
    }
    if is_byte_preserving_coding_system(coding_system) {
        // `binary`/`no-conversion`/`raw-text*` perform NO character-code
        // conversion: GNU's `setup_coding_system` gives them
        // `decode_coding_raw_text`, which copies the source bytes and (with
        // `dst_multibyte`) turns every byte >= 0x80 into an eight-bit
        // character (src/coding.c, `decode_coding_raw_text`).  Routing them
        // through `decode_bytes` instead would hand the bytes to
        // `String::from_utf8_lossy`, which silently *decodes* well-formed
        // UTF-8 — the very conversion the caller asked us not to do.  The EOL
        // half of the coding system still applies, because GNU runs
        // `decode_eol` after every decoder.
        return crate::emacs_core::emacs_char::str_to_multibyte(&decode_eol_text(
            bytes,
            coding_system,
            eol_conversion,
        ));
    }
    decode_bytes(bytes, coding_system, eol_conversion).into_bytes()
}

pub(crate) fn decode_bytes_to_lisp_string(
    bytes: &[u8],
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> crate::heap_types::LispString {
    crate::heap_types::LispString::from_emacs_bytes(decode_bytes_emacs(
        bytes,
        coding_system,
        eol_conversion,
    ))
}

fn is_byte_preserving_coding_system(coding_system: &str) -> bool {
    matches!(
        coding_system,
        "binary" | "no-conversion" | "raw-text" | "raw-text-unix" | "raw-text-dos" | "raw-text-mac"
    )
}

pub(crate) fn lisp_string_coding_source_bytes(s: &crate::heap_types::LispString) -> Vec<u8> {
    if s.is_multibyte() {
        crate::emacs_core::emacs_char::str_as_unibyte(s.as_bytes())
    } else {
        s.as_bytes().to_vec()
    }
}

fn charset_property_runs(
    text: &str,
    charset: &str,
    source_charset_decodes_ascii: bool,
) -> Vec<StringTextPropertyRun> {
    let mut char_count = 0usize;
    let mut first_non_ascii = None;
    for (idx, ch) in text.chars().enumerate() {
        if first_non_ascii.is_none() && (ch as u32) > 0x7f {
            first_non_ascii = Some(idx);
        }
        char_count = idx + 1;
    }

    let Some(first_non_ascii) = first_non_ascii else {
        return Vec::new();
    };
    // GNU records the charset selected by the decoder, not a property inferred
    // from the resulting Unicode scalar.  Every byte in an ISO-8859 coding,
    // including an ASCII byte, is decoded by that ISO charset, so once the
    // non-ASCII fast path is left its property run begins at zero.
    let start = if source_charset_decodes_ascii {
        0
    } else {
        first_non_ascii
    };

    vec![StringTextPropertyRun {
        start,
        end: char_count,
        plist: Value::list(vec![Value::symbol("charset"), Value::symbol(charset)]),
    }]
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

use crate::emacs_core::error::{EvalResult, signal};

fn known_coding_system(name: &str) -> bool {
    crate::emacs_core::coding::CodingSystemManager::new().is_known_or_derived(name)
}

fn validate_coding_system(
    name: &str,
    arg: Value,
    known: impl FnOnce(&str) -> bool,
) -> Result<(), crate::emacs_core::error::Flow> {
    if known(name) {
        Ok(())
    } else {
        Err(signal(LispCondition::CodingSystemError, vec![arg]))
    }
}

fn coding_string_nocopy(args: &[Value]) -> bool {
    args.get(2).is_some_and(|value| value.is_truthy())
}

fn coding_string_trivial_ascii_nocopy(bytes: &[u8], coding: &str, encode: bool) -> bool {
    if !bytes.iter().all(u8::is_ascii) {
        return false;
    }
    if encode && bytes.contains(&b'\n') {
        return false;
    }
    if !encode && bytes.contains(&b'\r') {
        return false;
    }
    is_byte_preserving_coding_system(coding)
        || matches!(
            coding_system_family(coding),
            "utf-8"
                | "utf-8-emacs"
                | "iso-latin-1"
                | "iso-latin-5"
                | "iso-latin-9"
                | "ascii"
                | "us-ascii"
                | "undecided"
                | "prefer-utf-8"
        )
}

fn copy_lisp_string_value(value: Value) -> Result<Value, crate::emacs_core::error::Flow> {
    let string = value.as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), value],
        )
    })?;
    Ok(Value::heap_string(string.clone()))
}

fn context_coding_name(
    ctx: &crate::emacs_core::eval::Context,
    coding_arg: Value,
) -> Result<String, crate::emacs_core::error::Flow> {
    let name = match coding_arg.kind() {
        ValueKind::Nil => "no-conversion".to_owned(),
        ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), coding_arg],
            ));
        }
    };
    validate_coding_system(&name, coding_arg, |candidate| {
        ctx.coding_systems.is_known_or_derived(candidate)
    })?;
    Ok(name)
}

/// The coding-system name to store in `last-coding-system-used`.
///
/// GNU sets `Vlast_coding_system_used = CODING_ID_NAME (coding.id)` (coding.c
/// `code_convert_string` / `code_convert_region`), which is the name the coding
/// system was *set up* with — i.e. the alias exactly as the caller passed it
/// (`euc-jp`, `shift_jis`, `cp932`, `euc-jp-dos`), NOT its resolved base
/// (`japanese-iso-8bit`, …).  The only time the stored name differs from the
/// argument is when the argument was "not fully specified" (`undecided` /
/// `prefer-utf-8`), in which case the caller has already rewritten `name` to the
/// detected concrete coding system before reaching here.  So the precise name is
/// just `name` verbatim; resolving aliases to their base here was the bug.
fn canonical_context_coding_name(_ctx: &crate::emacs_core::eval::Context, name: &str) -> String {
    name.to_owned()
}

fn coding_region_destination(
    arg: Option<Value>,
) -> Result<Option<Option<crate::buffer::BufferId>>, crate::emacs_core::error::Flow> {
    let Some(value) = arg else {
        return Ok(Some(None));
    };
    if value.is_nil() {
        return Ok(Some(None));
    }
    if value.is_t() {
        return Ok(None);
    }
    if let Some(buffer_id) = value.as_buffer_id() {
        Ok(Some(Some(buffer_id)))
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), value],
        ))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CodingDirection {
    Encode,
    Decode,
}

/// Which of GNU's two entries into the conversion engine a call arrives by.
///
/// `code_convert_string` (src/coding.c:9578) -- the one behind
/// `encode-coding-string` and `decode-coding-string` -- opens with an identity
/// fast path (src/coding.c:9609-9628) that returns the string WITHOUT running
/// `decode_coding_object` at all, and records the ARGUMENT's coding system in
/// `last-coding-system-used` rather than the one the conversion would have
/// resolved to.  File I/O never reaches that function: `insert-file-contents`
/// calls `decode_coding_gap` / `decode_coding_object` itself (src/fileio.c), so
/// it always converts and always reports the resolved subsidiary.
///
/// The difference is measurable and is not about the text.  Under GNU 31.0.90,
/// decoding the ASCII text `"a\nb\n"` as `undecided` leaves
/// `last-coding-system-used' at `undecided' through `decode-coding-string' and
/// at `undecided-unix' through `insert-file-contents' or a BUFFER destination.
/// Naming the entry makes that a match arm rather than something a shared
/// helper has to guess from its arguments.
/// Which of GNU's conversion entry points this call IS.
///
/// It answers two questions that are decided by the entry and not by the
/// coding system: whether the identity fast path exists, and WHERE
/// `detect_coding` runs -- at this entry, and if so with what
/// `CODING_MODE_LAST_BLOCK`, or before the call arrived at all.  The four
/// variants are four named C functions, because GNU gives them different
/// answers and every difference is observable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CodingEntry {
    /// GNU `code_convert_string` (src/coding.c:9580): the identity fast path is
    /// available, and the flag is raised at :9606 before the conversion.
    CodeConvertString,
    /// GNU `code_convert_region` (src/coding.c:9455): no identity fast path,
    /// flag raised at :9480 before the conversion.
    CodeConvertRegion,
    /// GNU `decode_coding_gap` (src/coding.c:7905) and its encode twin, which
    /// file I/O reaches.  No identity fast path, and -- the part that is easy to
    /// miss -- detection runs at :7927-7928 BEFORE `coding->mode |=
    /// CODING_MODE_LAST_BLOCK` at :8009.
    FileGap,
    /// GNU `decode_coding_object` reached through the `decode_coding_c_string`
    /// macro (src/coding.h:750-755) from the three subprocess doors:
    /// `read_and_insert_process_output` (src/process.c:6502),
    /// `read_and_dispose_of_process_output`'s filter branch (:6562) and
    /// `Fcall_process` (src/callproc.c:856).  All three are literally the same
    /// call, which is why there is one arm here and not three.
    ///
    /// No identity fast path: that belongs to `code_convert_string` and to
    /// nobody else (:9609-9628), so a `vietnamese-viqr` process read runs its
    /// `:post-read-conversion` where the same bytes through
    /// `decode-coding-string` do not.
    ///
    /// Detection has ALREADY run when a call arrives here, and that is the one
    /// place this port's shape differs from GNU's rather than mirrors it.  GNU
    /// decodes with the decoder deciding for itself how many trailing bytes it
    /// could not consume (`coding->carryover_bytes`); here the read boundary is
    /// computed from the coding system's NAME before the bytes are handed over,
    /// so the name has to be settled first.  Detection therefore happens at the
    /// read, through the same `detected_coding_name` this function would have
    /// called, and running it a SECOND time here -- on the prefix rather than on
    /// the whole read, which are different bytes -- is exactly the class of
    /// second-copy bug this chain keeps finding.
    ProcessRun,
}

/// Where `detect_coding` runs for a call, and with which
/// `CODING_MODE_LAST_BLOCK` if it runs here.
///
/// GNU asks `if (CODING_REQUIRE_DETECTION (coding)) detect_coding (coding);`
/// exactly once per conversion (src/coding.c:8129-8130).  Making the answer an
/// enum rather than a bare `SourceBlock` is what stops a door from detecting
/// twice: `AlreadyRun` has no `SourceBlock` to offer, so there is no flag to
/// pass to a second detector call.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EntryDetection {
    /// `CODING_REQUIRE_DETECTION` still holds when this entry reaches
    /// `decode_coding_object`, so the detector runs here, with the flag in the
    /// state this entry left it.
    Here(crate::emacs_core::coding::SourceBlock),
    /// The caller has already run `detect_coding` for this conversion and the
    /// answer is in the coding system's name.
    AlreadyRun,
}

impl CodingEntry {
    /// GNU's identity fast path is `code_convert_string`'s alone
    /// (src/coding.c:9609-9628); nothing else has one.
    fn has_identity_fast_path(self) -> bool {
        matches!(self, Self::CodeConvertString)
    }

    /// `CODING_MODE_LAST_BLOCK` as `detect_coding` sees it at this entry.
    ///
    /// A file is a complete source in every sense that matters to a reader, and
    /// yet GNU detects it as though more bytes were coming, because
    /// `decode_coding_gap` simply has not raised the flag yet when it calls
    /// `detect_coding`.  That is not a subtlety without consequences -- it is
    /// the difference between `utf-8` and `iso-latin-1` for any file whose last
    /// character is a truncated multibyte sequence.  Measured under GNU Emacs
    /// 31.0.90 on the four bytes `c a f <c3>`:
    ///
    /// ```elisp
    /// (decode-coding-string "caf\303" 'undecided)   ; => iso-latin-1, four bytes kept
    /// ;; the same four bytes in a file, insert-file-contents
    /// ;; => utf-8, and the orphan <c3> lands as the eight-bit character 4194243
    /// ```
    ///
    /// A subprocess read answers `AlreadyRun`: its flag was spent at the read,
    /// where GNU raises it only at EOF (src/process.c:6321), and it was spent
    /// on the carryover PLUS this read rather than on the prefix that survives
    /// the read boundary.
    fn detection(self) -> EntryDetection {
        match self {
            Self::CodeConvertString | Self::CodeConvertRegion => {
                EntryDetection::Here(crate::emacs_core::coding::SourceBlock::Last)
            }
            Self::FileGap => EntryDetection::Here(crate::emacs_core::coding::SourceBlock::More),
            Self::ProcessRun => EntryDetection::AlreadyRun,
        }
    }
}

/// Whether an encoding operation owns the end of the input stream.
///
/// GNU sets `CODING_MODE_LAST_BLOCK` for string and region conversion, but
/// deliberately leaves it clear while `write-region` feeds text to `e_write`.
/// Stateful encoders such as ISO-2022 use that distinction to decide whether
/// to emit a final reset sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EncodingBoundary {
    CompleteText,
    FileRegion,
}

impl EncodingBoundary {
    fn owns_end_of_stream(self) -> bool {
        matches!(self, Self::CompleteText)
    }
}

impl CodingDirection {
    fn string_function_name(self) -> &'static str {
        match self {
            Self::Encode => "encode-coding-string",
            Self::Decode => "decode-coding-string",
        }
    }

    fn region_function_name(self) -> &'static str {
        match self {
            Self::Encode => "encode-coding-region",
            Self::Decode => "decode-coding-region",
        }
    }

    fn is_encode(self) -> bool {
        matches!(self, Self::Encode)
    }
}

/// Convert the text of a buffer region through the same context-aware codec
/// the string functions use, so `encode-coding-region` / `decode-coding-region`
/// support every coding system `encode-coding-string` / `decode-coding-string`
/// do (euc-jp/shift_jis/iso-2022-jp and the other dedicated codecs).  GNU's
/// `code_convert_region` and `code_convert_string` share the same
/// `encode_coding_object`/`decode_coding_object` engine; this restores that
/// shared path.  The standalone `encode_lisp_string` / `decode` fallbacks below
/// only know the UTF-8 / single-byte / Big5 families and silently drop CJK text
/// for the charset/ISO-2022 codings.
fn transformed_region_string_in_context(
    ctx: &mut crate::emacs_core::eval::Context,
    source: crate::heap_types::LispString,
    coding: &str,
    direction: CodingDirection,
) -> Result<Value, crate::emacs_core::error::Flow> {
    // Destination nil => the codec returns the converted string, which
    // `builtin_coding_region` carries back into the region.  The ENTRY is still
    // `code_convert_region`, not `code_convert_string`: GNU's region function
    // (src/coding.c:9455-9502) has no identity fast path and always records the
    // resolved `CODING_ID_NAME (coding.id)` (:9497), which is why
    // `(decode-coding-region ... 'undecided)` over `"a\nb\n"` reports
    // `undecided-unix' where `decode-coding-string' reports `undecided'.
    builtin_coding_string_in_context(
        ctx,
        vec![
            Value::heap_string(source),
            Value::symbol(coding),
            Value::NIL,
            Value::NIL,
        ],
        direction,
        EncodingBoundary::CompleteText,
        CodingEntry::CodeConvertRegion,
        &mut CodingRun::complete_source(),
    )
}

fn insert_coding_result(
    ctx: &mut crate::emacs_core::eval::Context,
    buffer_id: crate::buffer::BufferId,
    text: &crate::heap_types::LispString,
    restore_point: Option<TextPositionAnchor>,
) -> Result<(), crate::emacs_core::error::Flow> {
    ctx.buffers
        .insert_lisp_string_into_buffer(buffer_id, text)
        .ok_or_else(|| signal("error", vec![Value::string("Selecting deleted buffer")]))?;
    if let Some(point) = restore_point {
        let _ = ctx.buffers.set_buffer_point_anchor(buffer_id, point);
    }
    Ok(())
}

fn coding_string_destination(
    arg: Option<Value>,
) -> Result<Option<crate::buffer::BufferId>, crate::emacs_core::error::Flow> {
    let Some(value) = arg else {
        return Ok(None);
    };
    if value.is_nil() || value.is_t() {
        return Ok(None);
    }
    value.as_buffer_id().map(Some).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), value],
        )
    })
}

/// If `coding` is a plain charset-type coding system that the family branches
/// of `encode_lisp_string` / `decode_bytes` do not already handle, return its
/// ordered `:charset-list` so the caller can encode/decode each character
/// through the charset registry. Returns `None` for utf-8, the single-byte /
/// Big5 / GBK families handled elsewhere, and for non-charset coding types
/// (iso-2022/ccl/shift-jis) that need a state machine rather than a flat
/// charset lookup.
fn general_charset_coding_list(
    ctx: &crate::emacs_core::eval::Context,
    coding: &str,
    for_encode: bool,
) -> Option<Vec<SymId>> {
    let family = coding_system_family(coding);
    if matches!(
        family,
        "utf-8"
            | "utf-8-emacs"
            | "undecided"
            | "prefer-utf-8"
            | "latin-1"
            | "iso-8859-1"
            | "iso-latin-1"
            | "iso-latin-5"
            | "iso-latin-9"
            | "chinese-iso-8bit"
    ) {
        return None;
    }
    // Big5: the existing `encoding_rs` decoder already matches GNU, but its
    // encoder substitutes HTML entity references for unencodable characters;
    // route only *encode* through the charset codec, which substitutes a space.
    if matches!(family, "chinese-big5" | "chinese-big5-hkscs") && !for_encode {
        return None;
    }
    // The ASCII coding systems substitute `?` for unencodable characters on
    // *encode* (GNU), so keep the existing encoder; on *decode* they emit
    // eight-bit raw characters, which the general charset decoder produces.
    if for_encode && matches!(family, "ascii" | "us-ascii") {
        return None;
    }
    let info = ctx.coding_systems.get(coding_system_base(coding))?;
    if !matches!(resolve_sym(info.coding_type), "charset" | "big5") || info.charset_list.is_empty()
    {
        return None;
    }
    // GNU's Big5 encoder emits only base `big5` code points (HKSCS-only code
    // points are recognized on decode but not produced on encode), so encode
    // every Big5 coding through `(ascii big5)` regardless of whether its charset
    // list names `big5-hkscs`. (neomacs types chinese-big5-hkscs as `charset`,
    // not `big5`, so gate on the family rather than the coding type.)
    if matches!(family, "chinese-big5" | "chinese-big5-hkscs") {
        return Some(vec![
            crate::emacs_core::intern::intern("ascii"),
            crate::emacs_core::intern::intern("big5"),
        ]);
    }
    Some(info.charset_list.clone())
}

/// Encode `s` through an explicit ordered charset list (a charset-type coding
/// system): each character is emitted as the bytes of the first charset that
/// can represent it, and characters that no charset can encode are replaced by
/// a space (0x20), matching GNU `encode-coding-string`.
fn encode_via_charset_list(s: &crate::heap_types::LispString, charset_list: &[SymId]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.sbytes());
    let emit = |code: u32, out: &mut Vec<u8>| {
        for &charset in charset_list {
            if let Some(bytes) =
                crate::emacs_core::charset::charset_encode_char_bytes(charset, i64::from(code))
            {
                out.extend(bytes);
                return;
            }
        }
        out.push(b' ');
    };
    for code in coding_source_codepoints(s) {
        emit(code, &mut out);
    }
    out
}

/// Decode `bytes` through an explicit ordered charset list (a charset-type
/// coding system): at each position, consume the bytes of the first charset in
/// the list that yields an assigned code point and emit that character; a byte
/// that no charset can decode becomes an eight-bit raw character, matching GNU.
/// The result is Emacs internal multibyte bytes plus the `(charset NAME)`
/// text-property runs GNU's `decode_coding_charset` attaches (`produce_charset`,
/// src/coding.c).
/// Accumulates `(charset NAME)` text-property runs while a decoder emits
/// characters, mirroring GNU's `decode_coding_*` charset annotation.  GNU only
/// touches its `last_id`/`last_offset` pair for a non-ASCII charset:
///
/// ```c
/// if (charset->id != charset_ascii && last_id != charset->id)
///   {
///     if (last_id != charset_ascii)
///       ADD_CHARSET_DATA (charbuf, char_offset - last_offset, last_id);
///     last_id = charset->id;
///     last_offset = char_offset;
///   }
/// ```
///
/// so an ASCII character (or an eight-bit raw byte taken through the
/// `invalid_code` path) never terminates the run being accumulated: it is
/// absorbed into it.  A run therefore starts at the first character of a
/// non-ASCII charset and ends only where the *next different* non-ASCII charset
/// starts, or at the end of the decoded text — trailing ASCII included.
struct CharsetRunBuilder {
    runs: Vec<StringTextPropertyRun>,
    /// The charset of the run currently being accumulated, with its start char
    /// index.  `None` means no non-ASCII charset has been seen yet, or the last
    /// one was already flushed.
    current: Option<(SymId, usize)>,
    /// GNU's `charset_ascii`: a decoder may hand us this charset explicitly
    /// (an ISO 2022 register designated to `ascii`), and it must behave like an
    /// unannotated ASCII character.
    ascii: SymId,
}

impl CharsetRunBuilder {
    fn new() -> Self {
        Self {
            runs: Vec::new(),
            current: None,
            ascii: intern("ascii"),
        }
    }

    /// Record the source charset (`None` for ASCII / eight-bit) of the character
    /// at output position `char_index`.
    fn push(&mut self, char_index: usize, charset: Option<SymId>) {
        // GNU's guard is `charset->id != charset_ascii && last_id !=
        // charset->id`: ASCII characters and eight-bit raw bytes leave
        // `last_id`/`last_offset` alone, so they extend the current run instead
        // of ending it.
        let Some(cs) = charset else { return };
        if cs == self.ascii || self.current.map(|(c, _)| c) == Some(cs) {
            return;
        }
        self.flush(char_index);
        self.current = Some((cs, char_index));
    }

    fn flush(&mut self, end: usize) {
        if let Some((charset, start)) = self.current.take() {
            self.runs.push(StringTextPropertyRun {
                start,
                end,
                plist: Value::list(vec![
                    Value::symbol("charset"),
                    Value::symbol(resolve_sym(charset)),
                ]),
            });
        }
    }

    fn finish(mut self, end: usize) -> Vec<StringTextPropertyRun> {
        self.flush(end);
        self.runs
    }
}

/// The part of GNU's `coding->spec.iso_2022` that outlives one call into the
/// decoder: the four graphic registers' designations, which of them is invoked
/// into GL, and a single shift that has been seen but not yet spent.
///
/// GNU never has to think about this, because a subprocess decodes through the
/// process's own `struct coding_system` for the process's whole life
/// (`proc_decode_coding_system[channel]`, src/process.c:6242) and
/// `setup_coding_system` seeds the registers once
/// (`CODING_ISO_INITIAL`, src/coding.c:5760-5775).  This port decodes each run
/// as a separate call, so the state has to be an object with the same
/// lifetime, and [`CodingDecoderState`] is where a process keeps it.
///
/// A designation is not carryover.  `ESC $ B` at the end of a read is a
/// COMPLETE escape sequence: GNU consumes it and records it, and the next
/// read's `$ "` is then a JIS X 0208 character rather than two ASCII ones.
/// Holding the escape back instead would give the same answer for that one
/// case and the wrong answer as soon as a character follows the designation in
/// the same read.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Iso2022DecodeState {
    /// GNU `CODING_ISO_DESIGNATION (coding, reg)`.  `None` in every register
    /// means the state has not been seeded from the coding system yet.
    designation: [Option<SymId>; 4],
    seeded: bool,
    /// GNU `CODING_ISO_INVOCATION (coding, 0)`.
    gl: usize,
    /// GNU `CODING_ISO_SINGLE_SHIFTING (coding)`, as the register it selects.
    single_shift: Option<usize>,
}

impl Iso2022DecodeState {
    /// GNU `setup_coding_system`'s seeding of the registers from the coding
    /// system's `:initial` designations (src/coding.c:5760-5775), which
    /// happens once per `struct coding_system` and not once per decode.
    fn designate_initially(&mut self, spec: &crate::emacs_core::coding::Iso2022Spec) {
        if self.seeded {
            return;
        }
        self.designation = spec.initial;
        self.seeded = true;
    }
}

/// GNU `coding->spec`: the decoder state a `struct coding_system` carries
/// between calls.
///
/// Only the ISO-2022 arm is here, and the reason is a measurement rather than
/// an omission: it is the only decoder in this port whose state is observable
/// across a read boundary.  Shift-JIS, EUC, the charset codings, `emacs-mule`
/// and UTF-8 are stateless between characters -- everything they carry across a
/// boundary is the incomplete character itself, which is `coding->carryover`
/// and is handled by [`SourceConsumed`].  UTF-16's endianness and `emacs-mule`'s
/// composition status are the two GNU also keeps here and this port does not;
/// see the ledger entry.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CodingDecoderState {
    iso_2022: Iso2022DecodeState,
}

impl CodingDecoderState {
    fn iso_2022_mut(&mut self) -> &mut Iso2022DecodeState {
        &mut self.iso_2022
    }
}

/// The mutable half of GNU's `struct coding_system` for ONE conversion: what a
/// decoder may carry IN from the previous run, and what it must carry OUT of
/// this one.
///
/// GNU has no such type because it does not need one: `decode_coding_object`
/// takes a `struct coding_system *`, and for a subprocess that pointer is the
/// process's own struct with its `spec`, its `mode` and its `consumed` already
/// in it (src/process.c:6242).  This port decodes each run as a separate call
/// through a door that answers a Lisp string, so the three have to travel
/// alongside the call.
///
/// [`Self::complete_source`] is what every door but a subprocess read passes,
/// and it is a statement rather than a convenience: a string, a region and a
/// file are `CODING_MODE_LAST_BLOCK` sources with no previous run to continue
/// from and no next run to continue into.
pub(crate) struct CodingRun<'a> {
    /// GNU `coding->mode & CODING_MODE_LAST_BLOCK`.  With it set, bytes the
    /// decoder could not consume are flushed as eight-bit characters
    /// (src/coding.c:7434-7462); with it clear they become
    /// `coding->carryover` (:7466-7474).
    block: crate::emacs_core::coding::SourceBlock,
    /// GNU `coding->spec`.  `None` at a door whose decoder has no previous
    /// state to continue from, which is every door but a subprocess read.
    state: Option<&'a mut CodingDecoderState>,
    /// GNU `coding->consumed`, written by whichever decoder ran.
    consumed: Option<SourceConsumed>,
}

impl<'a> CodingRun<'a> {
    /// A source that is complete in every sense: the flag is set, so nothing
    /// is ever held back, and there is no decoder state on either side of the
    /// call.
    pub(crate) fn complete_source() -> Self {
        Self {
            block: crate::emacs_core::coding::SourceBlock::Last,
            state: None,
            consumed: None,
        }
    }

    /// One read of a subprocess: GNU's per-process `struct coding_system`,
    /// with the `CODING_MODE_LAST_BLOCK` the read raised (or did not).
    pub(crate) fn process_read(
        state: &'a mut CodingDecoderState,
        block: crate::emacs_core::coding::SourceBlock,
    ) -> Self {
        Self {
            block,
            state: Some(state),
            consumed: None,
        }
    }

    fn block(&self) -> crate::emacs_core::coding::SourceBlock {
        self.block
    }

    /// Whether the tail no decoder consumed is flushed here or waits for the
    /// next read.
    fn flushes_last_block(&self) -> bool {
        matches!(self.block, crate::emacs_core::coding::SourceBlock::Last)
    }

    fn record(&mut self, consumed: SourceConsumed) {
        self.consumed = Some(consumed);
    }

    /// The decoder state to continue from, borrowed for one decode.  A door
    /// with none gets a scratch one, which is the same thing GNU's
    /// `setup_coding_system` hands a fresh `struct coding_system`.
    fn decoder_state<'s>(
        &'s mut self,
        scratch: &'s mut CodingDecoderState,
    ) -> &'s mut CodingDecoderState {
        match self.state {
            Some(ref mut state) => state,
            None => scratch,
        }
    }

    /// GNU `coding->carryover_bytes` (src/coding.c:7473): the bytes of the
    /// character the decoder stopped in the middle of.
    ///
    /// A `Last` block has none by construction -- its tail was flushed as
    /// characters -- and a run in which no decoder that CAN stop short ever
    /// ran reports none either, which is the honest answer for a decoder in
    /// which every byte is a character.
    pub(crate) fn carryover_bytes(&self, source_len: usize) -> usize {
        if self.flushes_last_block() {
            return 0;
        }
        match self.consumed {
            Some(consumed) => source_len.saturating_sub(consumed.bytes()),
            None => 0,
        }
    }
}

// ---------------------------------------------------------------------------
// GNU's decoder loop: `src_base`, `ONE_MORE_BYTE` and `coding->consumed`
// ---------------------------------------------------------------------------

/// GNU `coding->consumed` (src/coding.c:7477): how many of a source's bytes a
/// decoder turned into characters.
///
/// It is a newtype rather than a bare `usize` because there is exactly one
/// correct way to produce one -- take it off the cursor that drove the decode
/// ([`decode_units`]) -- and a hand-computed count is the byte-length table
/// this port keeps deleting.  Everything downstream is `src_bytes - consumed`:
/// a process read's `coding->carryover` when `CODING_MODE_LAST_BLOCK` is clear
/// (src/coding.c:7466-7474), and the binary flush when it is set (:7434-7462).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct SourceConsumed(usize);

impl SourceConsumed {
    /// Every byte of the source.
    ///
    /// This is a DECODER's answer and not a default: a decoder in which every
    /// byte is a character (`raw-text`, the ISO-8859 charsets) genuinely
    /// cannot stop short, and neither can one GNU implements in Lisp through
    /// `:post-read-conversion` over `raw-text` (`utf-7`, `chinese-hz`) --
    /// measured, GNU decodes each chunk of those independently, because the C
    /// decoder underneath consumed the run before the hook ever saw it.
    fn all(bytes: &[u8]) -> Self {
        Self(bytes.len())
    }

    pub(crate) fn bytes(self) -> usize {
        self.0
    }
}

/// The source ran out in the middle of a character: GNU's
/// `goto no_more_source` (src/coding.c:184).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct NoMoreSource;

/// One character's worth of source, as GNU's `ONE_MORE_BYTE` reads it.
///
/// Its position is private: a decoder can take bytes and it can put back the
/// character it is on, but it cannot say where that character ended, because
/// where it ended is what the cursor reports and not what the decoder reports.
struct UnitReader<'a> {
    bytes: &'a [u8],
    /// GNU `src_base - coding->source`.
    start: usize,
    /// GNU `src - coding->source`.
    pos: usize,
}

impl<'a> UnitReader<'a> {
    /// GNU `ONE_MORE_BYTE (c)` (src/coding.c:169-190).
    fn byte(&mut self) -> Result<u8, NoMoreSource> {
        let byte = *self.bytes.get(self.pos).ok_or(NoMoreSource)?;
        self.pos += 1;
        Ok(byte)
    }

    /// N more bytes at once, all or nothing -- the shape of a charset whose
    /// dimension says how many position bytes follow its leading code.
    fn take(&mut self, n: usize) -> Result<&'a [u8], NoMoreSource> {
        let end = self.pos.checked_add(n).ok_or(NoMoreSource)?;
        let slice = self.bytes.get(self.pos..end).ok_or(NoMoreSource)?;
        self.pos = end;
        Ok(slice)
    }

    /// GNU's `src = src_base`: several decoders read a byte, decide it is not
    /// theirs, and put it back -- `decode_coding_utf_16`'s signature check
    /// (src/coding.c:1601-1609), `decode_coding_utf_8`'s BOM check
    /// (:1256-1272), and every `invalid_code:` label (:1334-1335).
    fn rewind(&mut self) {
        self.pos = self.start;
    }
}

/// Where a [`DecodeSink`] was before a unit ran, so the unit can be undone
/// whole.  GNU never needs this: its decoders write into `coding->charbuf`
/// only once `ONE_MORE_BYTE` has succeeded for every byte of the character.
#[derive(Clone, Copy, Debug)]
struct DecodeMark {
    out_len: usize,
    char_index: usize,
    last_char: Option<u32>,
}

/// The characters a decoder has produced, in Emacs's internal encoding, with
/// the `(charset NAME)` runs GNU's `decode_coding_*` annotate them with
/// (`ADD_CHARSET_DATA`, src/coding.c:7060-7078).
struct DecodeSink {
    out: Vec<u8>,
    runs: CharsetRunBuilder,
    char_index: usize,
    buf: [u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH],
    last_char: Option<u32>,
}

impl DecodeSink {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            out: Vec::with_capacity(capacity),
            runs: CharsetRunBuilder::new(),
            char_index: 0,
            buf: [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH],
            last_char: None,
        }
    }

    /// One decoded character and the charset that decoded it.  `None` is
    /// GNU's unannotated case: an ASCII character, or an eight-bit raw byte
    /// taken through `invalid_code:`.
    fn push(&mut self, code: u32, charset: Option<SymId>) {
        self.runs.push(self.char_index, charset);
        let len = crate::emacs_core::emacs_char::char_string(code, &mut self.buf);
        self.out.extend_from_slice(&self.buf[..len]);
        self.char_index += 1;
        self.last_char = Some(code);
    }

    /// GNU's `invalid_code:` arm: a byte no rule of this decoder accepts
    /// becomes the eight-bit character `BYTE8_TO_CHAR (c)`
    /// (src/coding.c:1334-1338).
    fn push_raw(&mut self, byte: u8) {
        self.push(crate::emacs_core::emacs_char::unibyte_to_char(byte), None);
    }

    fn mark(&self) -> DecodeMark {
        DecodeMark {
            out_len: self.out.len(),
            char_index: self.char_index,
            last_char: self.last_char,
        }
    }

    /// Undo everything a unit produced.  Only the byte buffer and the
    /// character count need undoing: [`CharsetRunBuilder::push`] is a no-op for
    /// the `None` charset, and a rolled-back unit either produced nothing (it
    /// ran out of source) or produced a carriage return, which is ASCII.
    fn rollback(&mut self, mark: DecodeMark) {
        self.out.truncate(mark.out_len);
        self.char_index = mark.char_index;
        self.last_char = mark.last_char;
    }

    fn finish(self) -> (Vec<u8>, Vec<StringTextPropertyRun>) {
        let char_index = self.char_index;
        (self.out, self.runs.finish(char_index))
    }
}

/// GNU's `eol_dos`, which every decoder computes for itself at the top rather
/// than leaving to `decode_eol`:
///
/// ```c
///   bool eol_dos
///     = !inhibit_eol_conversion && EQ (CODING_ID_EOL_TYPE (coding->id), Qdos);
/// ```
///
/// (src/coding.c:1250-1251 for UTF-8, and the same two lines in seven more
/// decoders.)  A decoder with it set does `ONE_MORE_BYTE (byte_after_cr)` after
/// producing a CR (:1348-1349), so a CR that ends the source is left
/// UNCONSUMED and becomes `coding->carryover`.
///
/// It is an argument and not something a decoder here can work out, because
/// this port spends the coding system's end-of-line leg BEFORE the decoder
/// runs -- `decode_coding` calls `decode_eol` after the decoder
/// (src/coding.c:7481), so the name the decoder is handed no longer carries the
/// subsidiary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DosEolLookahead {
    /// The coding system's eol type is `dos` and `inhibit-eol-conversion' is
    /// nil, so a trailing CR waits for the next read.
    Required,
    /// Anything else.  Note which case this is NOT: a VECTOR eol type -- one
    /// still waiting to be detected -- does not hold a CR back either, because
    /// `CODING_ID_EOL_TYPE` is compared against `Qdos` and a vector is not it
    /// (entry 134 measured this and got it wrong in the other direction).
    NotRequired,
}

/// GNU's decoder loop, with `coding->consumed` as its return value.
///
/// Every `decode_coding_*` in src/coding.c is a `while (1)` whose first
/// statement is `src_base = src;` and whose byte reads are `ONE_MORE_BYTE`, a
/// macro that jumps to `no_more_source:` when the source runs out
/// (src/coding.c:169-190).  The label's whole job is one line --
/// `coding->consumed = src_base - coding->source;` (:1423, :1696, :2541,
/// :3982, :4791, :4886, :5591) -- so the count is always the start of the
/// character the decoder was in the middle of, and never something the decoder
/// worked out for itself.
///
/// This is that shape, and it is the answer to the residual entry 159 handed
/// over: a decoder here cannot report the wrong `consumed`, because it never
/// reports one.  BODY decodes exactly one character (or one state change) per
/// call; a BODY that runs off the end says [`NoMoreSource`], and the bytes it
/// had taken are put back along with anything it had already pushed.
fn decode_units<F>(bytes: &[u8], eol: DosEolLookahead, mut body: F) -> DecodedSource
where
    F: FnMut(&mut UnitReader<'_>, &mut DecodeSink) -> Result<(), NoMoreSource>,
{
    let mut sink = DecodeSink::with_capacity(bytes.len());
    let mut consumed = 0usize;
    while consumed < bytes.len() {
        let mark = sink.mark();
        let mut unit = UnitReader {
            bytes,
            start: consumed,
            pos: consumed,
        };
        if body(&mut unit, &mut sink).is_err() {
            sink.rollback(mark);
            break;
        }
        if unit.pos <= consumed {
            // A body that took nothing would spin forever.  GNU cannot write
            // this down -- every path through a `decode_coding_*` loop passes
            // at least one `ONE_MORE_BYTE` -- so treat it as the source having
            // run out rather than as a reason to hang.
            debug_assert!(
                false,
                "a decoder unit must consume at least one byte or the loop cannot terminate"
            );
            sink.rollback(mark);
            break;
        }
        // GNU's `eol_dos` lookahead, in one place instead of the eight places
        // it has it: the CR is produced, `ONE_MORE_BYTE (byte_after_cr)` is
        // attempted, and an exhausted source unwinds the whole character.
        if eol == DosEolLookahead::Required
            && sink.last_char == Some(u32::from(b'\r'))
            && unit.pos == bytes.len()
        {
            sink.rollback(mark);
            break;
        }
        consumed = unit.pos;
    }
    let (out, runs) = sink.finish();
    DecodedSource {
        bytes: out,
        runs,
        consumed: SourceConsumed(consumed),
    }
}

/// GNU's `invalid_code:` label, which every byte-oriented decoder ends with
/// (src/coding.c:1334-1338, :4614-4618, :5577-5581):
///
/// ```c
///  invalid_code:
///   src = src_base;
///   consumed_chars = consumed_chars_base;
///   ONE_MORE_BYTE (c);
///   *charbuf++ = ASCII_CHAR_P (c) ? c : BYTE8_TO_CHAR (c);
/// ```
///
/// It rewinds to the start of the character, takes exactly ONE byte, and emits
/// it as itself -- so a two-byte sequence whose second byte is not valid is
/// re-examined from its second byte rather than skipped.
fn invalid_code(unit: &mut UnitReader<'_>, sink: &mut DecodeSink) -> Result<(), NoMoreSource> {
    unit.rewind();
    let byte = unit.byte()?;
    if byte < 0x80 {
        sink.push(u32::from(byte), None);
    } else {
        sink.push_raw(byte);
    }
    Ok(())
}

/// What one decoder produced: the characters, their `(charset NAME)` runs, and
/// GNU's `coding->consumed`.
pub(crate) struct DecodedSource {
    bytes: Vec<u8>,
    runs: Vec<StringTextPropertyRun>,
    consumed: SourceConsumed,
}

impl DecodedSource {
    /// A decoder that consumed its whole source, which is a statement about
    /// that decoder and not a default -- see [`SourceConsumed::all`].
    fn whole(bytes: Vec<u8>, source: &[u8]) -> Self {
        Self {
            bytes,
            runs: Vec::new(),
            consumed: SourceConsumed::all(source),
        }
    }
}

/// GNU `decode_coding_utf_16` (src/coding.c:1580-1700), for a source that may
/// still be continued.
///
/// Two bytes are two `ONE_MORE_BYTE`s (:1626-1627) and a surrogate pair is
/// four (:1661-1670), so a read that ends inside either leaves the whole unit
/// for the next one -- which is why a `utf-16le` subprocess picks up mid-pair
/// instead of turning half a unit into a character.  The signature is the
/// coding system's business and not the bytes': only a [`Utf16Bom::Declared`]
/// system reads one, and only for its OWN endianness (:1594-1609).
fn decode_via_utf16(
    bytes: &[u8],
    default_endian: Utf16Endian,
    bom: Utf16Bom,
    eol: DosEolLookahead,
) -> DecodedSource {
    let (endian, signature) = match (bom, bytes) {
        (Utf16Bom::Declared, [0xFE, 0xFF, ..]) if matches!(default_endian, Utf16Endian::Big) => {
            (Utf16Endian::Big, 2)
        }
        (Utf16Bom::Declared, [0xFF, 0xFE, ..]) if matches!(default_endian, Utf16Endian::Little) => {
            (Utf16Endian::Little, 2)
        }
        _ => (default_endian, 0),
    };
    let body = &bytes[signature..];
    let unit_at = move |pair: &[u8]| -> u16 {
        let pair = [pair[0], pair[1]];
        match endian {
            Utf16Endian::Big => u16::from_be_bytes(pair),
            Utf16Endian::Little => u16::from_le_bytes(pair),
        }
    };
    let mut decoded = decode_units(body, eol, |unit, sink| {
        let first = unit_at(unit.take(2)?);
        if (0xD800..0xDC00).contains(&first) {
            // A high surrogate needs its low half, and GNU reads it with two
            // more `ONE_MORE_BYTE`s before producing anything.
            let second = unit_at(unit.take(2)?);
            if (0xDC00..0xE000).contains(&second) {
                let code =
                    0x1_0000 + ((u32::from(first) - 0xD800) << 10) + (u32::from(second) - 0xDC00);
                sink.push(code, None);
                return Ok(());
            }
            unit.rewind();
            unit.take(2)?;
        }
        sink.push(
            if (0xD800..0xE000).contains(&first) {
                u32::from(char::REPLACEMENT_CHARACTER)
            } else {
                u32::from(first)
            },
            None,
        );
        Ok(())
    });
    decoded.consumed = SourceConsumed(signature + decoded.consumed.bytes());
    decoded
}

/// GNU `decode_coding_big5` (src/coding.c:4700-4790).
///
/// The characters come from `encoding_rs`, which is what every other door in
/// this port has always used for Big5; the read boundary comes from GNU's own
/// loop, which is one `ONE_MORE_BYTE` for the leading byte and a second one
/// for the trailing byte of anything at or above 0x80 (:4762-4766).  They are
/// the same rule, and keeping them in one function is what stops them from
/// drifting apart.
fn decode_via_big5(bytes: &[u8], eol: DosEolLookahead) -> DecodedSource {
    // The boundary first: `decode_units` is only being asked where the last
    // complete character ends, so its sink is discarded and the text is
    // decoded from the prefix.
    let measured = decode_units(bytes, eol, |unit, sink| {
        let b = unit.byte()?;
        if b >= 0x80 {
            unit.byte()?;
        }
        sink.push(u32::from(b), None);
        Ok(())
    });
    let consumed = measured.consumed;
    // `chinese-big5-hkscs` shares the table here, as it always has at this
    // port's other doors: `encoding_rs` has no separate HKSCS decoder.
    let (decoded, _, _) = BIG5.decode(&bytes[..consumed.bytes()]);
    let text = decoded.into_owned();
    let runs = charset_property_runs(&text, "big5", false);
    DecodedSource {
        bytes: text.into_bytes(),
        runs,
        consumed,
    }
}

/// The two multi-byte decoders that this port reaches through
/// [`decode_bytes`] rather than through a `decode_via_*` arm of its own, given
/// a `coding->consumed` of their own.
///
/// `None` means the fall-through really is a decoder in which every byte is a
/// character, or a source in which every byte is ASCII -- either way one that
/// cannot stop short, so the caller may say `SourceConsumed::all` and mean it.
fn decode_fallthrough_source(
    bytes: &[u8],
    coding: &str,
    eol: DosEolLookahead,
) -> Option<DecodedSource> {
    // A pure-ASCII source under an ASCII-compatible coding takes
    // `builtin_decode_coding_string_with_known`'s NOCOPY path, which may hand
    // back the caller's own string object; nothing here may pre-empt that, and
    // nothing needs to, because pure ASCII cannot stop short.
    if coding_string_trivial_ascii_nocopy(bytes, coding, false) {
        return None;
    }
    if let Some((endian, bom)) = utf16_coding_variant(coding) {
        return Some(decode_via_utf16(bytes, endian, bom, eol));
    }
    match coding_system_family(coding) {
        "chinese-big5" | "chinese-big5-hkscs" => Some(decode_via_big5(bytes, eol)),
        "utf-8" | "utf-8-emacs" => {
            let mut src = bytes;
            let signature = if coding_system_consumes_utf8_signature(coding)
                && src.starts_with(&[0xEF, 0xBB, 0xBF])
            {
                src = &src[3..];
                3
            } else {
                0
            };
            let mut decoded = decode_via_utf8(src, eol);
            decoded.consumed = SourceConsumed(signature + decoded.consumed.bytes());
            Some(decoded)
        }
        _ => None,
    }
}

/// Build a decoded multibyte Lisp string from Emacs internal `bytes`, attaching
/// the `(charset NAME)` text-property `runs` GNU's `decode_coding_*` produce.
fn decoded_string_with_charset_runs(bytes: Vec<u8>, runs: Vec<StringTextPropertyRun>) -> Value {
    let value = Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(bytes));
    if !runs.is_empty() {
        crate::emacs_core::value::set_string_text_properties_for_value(value, runs);
    }
    value
}

/// GNU `decode_coding_charset` (src/coding.c:5470-5590).
///
/// Its loop reads the leading byte with `ONE_MORE_BYTE`, LOOKS THAT BYTE UP,
/// and only then reads the `dim - 1` bytes the charset it found needs
/// (:5518-5545).  The order is the whole rule:
///
/// ```c
///       ONE_MORE_BYTE (c);
///       ...
///       val = AREF (valids, c);
///       if (! FIXNUMP (val) && ! CONSP (val))
///         goto invalid_code;
/// ```
///
/// so a byte no charset in the list can START becomes an eight-bit character
/// in the read it arrived in, even when a longer charset in the same list
/// would have needed bytes the source does not have yet.  A byte that CAN
/// start a charset and then runs out of source does stop the decoder, and
/// `coding->consumed` is the start of that character -- which is what makes a
/// `chinese-gbk` or `cp936` subprocess pick up in the next read where the
/// previous one left off.
///
/// `valids` is a 256-entry vector GNU builds once per coding system, each
/// element the charsets that byte can begin, sorted by dimension with the
/// smaller first (src/coding.c:11122-11165).  Here it is the same data asked
/// per charset instead of per byte -- [`CharsetLeadingByte`] is built from the
/// charset's own `code_space`, which is what GNU builds the vector FROM -- so
/// nothing below is a second copy of a decoding rule.
fn decode_via_charset_list(
    bytes: &[u8],
    charset_list: &[SymId],
    eol: DosEolLookahead,
) -> DecodedSource {
    let mut leads: Vec<(SymId, crate::emacs_core::charset::CharsetLeadingByte)> = charset_list
        .iter()
        .filter_map(|&charset| {
            Some((
                charset,
                crate::emacs_core::charset::charset_leading_byte(charset)?,
            ))
        })
        .collect();
    // GNU inserts each charset ahead of the first entry with a LARGER
    // dimension, so a byte's candidate list is ascending by dimension and ties
    // keep `:charset-list` order.  `sort_by_key` is stable, so this is that.
    leads.sort_by_key(|(_, lead)| lead.dimension);

    decode_units(bytes, eol, |unit, sink| {
        let start = unit.pos;
        let rest = &unit.bytes[start..];
        let Some(&leading) = rest.first() else {
            return Err(NoMoreSource);
        };
        let mut short = false;
        let decoded = leads
            .iter()
            .filter(|(_, lead)| lead.accepts(leading))
            .find_map(|&(charset, lead)| {
                if rest.len() < lead.dimension {
                    // `ONE_MORE_BYTE` would have jumped to `no_more_source`
                    // here -- but only for a charset this byte can begin,
                    // which is why the filter above comes first.
                    short = true;
                    return None;
                }
                crate::emacs_core::charset::charset_decode_char_from_bytes(charset, rest)
                    .map(|(ch, consumed)| (charset, ch, consumed))
            });
        match decoded {
            // GNU annotates a run with the charset that decoded it, but ASCII
            // characters (`charset->id == charset_ascii`) carry no property.
            Some((charset, ch, consumed)) => {
                let annotation = if ch < 0x80 { None } else { Some(charset) };
                unit.take(consumed)?;
                sink.push(ch as u32, annotation);
                Ok(())
            }
            // A byte no charset can decode becomes an eight-bit raw character
            // with no `charset` annotation (GNU's `invalid_code` path) -- but
            // only once the source is known to be complete, because a charset
            // that ran out of bytes has not been refuted yet.
            None if short => Err(NoMoreSource),
            None => {
                let byte = unit.byte()?;
                sink.push_raw(byte);
                Ok(())
            }
        }
    })
}

/// If `coding` is an EUC-profile ISO-2022 coding system, return its designation
/// state and charset list. The EUC profile is 8-bit with fixed designations
/// (graphic registers loaded once, characters selected by the high bit and the
/// SS2/SS3 single shifts) — euc-jp, euc-kr, … The full ISO-2022 escape-sequence
/// machine (iso-2022-jp/cn/kr, 7-bit, escape designations) is not handled here.
fn euc_iso2022_spec(
    ctx: &crate::emacs_core::eval::Context,
    coding: &str,
) -> Option<(crate::emacs_core::coding::Iso2022Spec, Vec<SymId>)> {
    let info = ctx.coding_systems.get(coding_system_base(coding))?;
    if resolve_sym(info.coding_type) != "iso-2022" {
        return None;
    }
    let spec = crate::emacs_core::coding::iso2022_spec(info)?;
    use crate::emacs_core::coding::IsoFlag;
    if spec.flags.contains(IsoFlag::SevenBits) || spec.flags.contains(IsoFlag::Designation) {
        return None;
    }
    Some((spec, info.charset_list.clone()))
}

/// Encode `s` as 8-bit EUC: ASCII stays in G0; a character of the charset
/// designated to G1 is emitted with the high bit set, and a character of the G2
/// or G3 charset is prefixed by SS2 (`0x8E`) or SS3 (`0x8F`).
fn encode_via_euc(
    s: &crate::heap_types::LispString,
    spec: &crate::emacs_core::coding::Iso2022Spec,
    charset_list: &[SymId],
) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.sbytes());
    let emit = |code: u32, out: &mut Vec<u8>| {
        if code < 0x80 {
            out.push(code as u8);
            return;
        }
        for &charset in charset_list {
            // Prefer the register this charset is the *initial* designation of
            // (GNU's ISO-2022 engine designates by initial/reg-usage, not by the
            // request alist): e.g. euc-tw's cns11643-1 is G1, so it must use the
            // GR high-bit form rather than the SS2 (0x8E) the request would pick.
            let reg = (0..4)
                .find(|&g| spec.initial[g] == Some(charset))
                .map(|g| g as u8)
                .or_else(|| spec.register_of(charset));
            let Some(reg) = reg else {
                continue;
            };
            let Some(bytes) =
                crate::emacs_core::charset::charset_encode_char_bytes(charset, i64::from(code))
            else {
                continue;
            };
            match reg {
                0 => out.extend(bytes),
                1 => out.extend(bytes.iter().map(|b| b | 0x80)),
                2 => {
                    out.push(0x8E);
                    out.extend(bytes.iter().map(|b| b | 0x80));
                }
                3 => {
                    out.push(0x8F);
                    out.extend(bytes.iter().map(|b| b | 0x80));
                }
                _ => out.push(b' '),
            }
            return;
        }
        out.push(b' ');
    };
    for code in coding_source_codepoints(s) {
        emit(code, &mut out);
    }
    out
}

/// Decode the GR bytes of a single-shifted / G1 EUC sequence: strip the high
/// bit to recover the GL code, then look the character up in the charset.
fn decode_euc_register(charset: Option<SymId>, bytes: &[u8]) -> Option<(u32, usize)> {
    let charset = charset?;
    let stripped: Vec<u8> = bytes.iter().take(3).map(|byte| byte & 0x7F).collect();
    let (ch, consumed) =
        crate::emacs_core::charset::charset_decode_char_from_bytes(charset, &stripped)?;
    Some((ch as u32, consumed))
}

/// Decode 8-bit EUC bytes through the coding system's G0-G3 designations.
fn decode_via_euc(
    bytes: &[u8],
    spec: &crate::emacs_core::coding::Iso2022Spec,
    eol: DosEolLookahead,
) -> DecodedSource {
    decode_units(bytes, eol, |unit, sink| {
        let b = unit.byte()?;
        if b < 0x80 {
            sink.push(u32::from(b), None);
            return Ok(());
        }
        // `(register, does the single shift eat a byte of its own)`.  A
        // successful register decode annotates the run with that register's
        // designated charset; ASCII and raw bytes carry no `charset` property
        // (GNU's `decode_coding_iso_2022`).
        // SS2/SS3 spend a byte of their own; a plain GR byte is itself the
        // first byte of the register's code point.
        let (register, shift_bytes) = match b {
            0x8E => (spec.initial[2], 1),
            0x8F => (spec.initial[3], 1),
            _ => (spec.initial[1], 0),
        };
        let width = register
            .and_then(crate::emacs_core::charset::charset_dimension_by_sym)
            .unwrap_or(1) as usize;
        let from = unit.start + shift_bytes;
        let rest = &unit.bytes[from.min(unit.bytes.len())..];
        // `ONE_MORE_BYTE` has to succeed for every position byte of the
        // register's charset (src/coding.c:5533-5538); a source that ends
        // inside one leaves the whole character for the next read.
        if rest.len() < width {
            return Err(NoMoreSource);
        }
        match decode_euc_register(register, rest) {
            Some((ch, consumed)) => {
                unit.rewind();
                unit.take(shift_bytes + consumed)?;
                sink.push(ch, register);
                Ok(())
            }
            None => invalid_code(unit, sink),
        }
    })
}

/// JIS code point (jisx0208 row/cell) -> the two Shift-JIS bytes. Mirrors
/// `JIS_TO_SJIS` (coding.h).
fn jis_to_sjis(code: u32) -> (u8, u8) {
    let j1 = (code >> 8) as i32;
    let j2 = (code & 0xFF) as i32;
    let (s1, s2) = if j1 & 1 != 0 {
        (
            j1 / 2 + if j1 < 0x5F { 0x71 } else { 0xB1 },
            j2 + if j2 >= 0x60 { 0x20 } else { 0x1F },
        )
    } else {
        (j1 / 2 + if j1 < 0x5F { 0x70 } else { 0xB0 }, j2 + 0x7E)
    };
    (s1 as u8, s2 as u8)
}

/// Two Shift-JIS bytes -> the JIS code point. Mirrors `SJIS_TO_JIS` (coding.h).
fn sjis_to_jis(s1: u8, s2: u8) -> u32 {
    let s1 = s1 as i32;
    let s2 = s2 as i32;
    let (j1, j2) = if s2 >= 0x9F {
        (s1 * 2 - if s1 >= 0xE0 { 0x160 } else { 0xE0 }, s2 - 0x7E)
    } else {
        (
            s1 * 2 - if s1 >= 0xE0 { 0x161 } else { 0xE1 },
            s2 - if s2 >= 0x7F { 0x20 } else { 0x1F },
        )
    };
    ((j1 << 8) | j2) as u32
}

/// Charset list of a Shift-JIS coding system (`(ascii katakana-jisx0201
/// japanese-jisx0208 …)`), or `None` if `coding` is not a shift-jis type.
fn sjis_charsets(ctx: &crate::emacs_core::eval::Context, coding: &str) -> Option<Vec<SymId>> {
    let info = ctx.coding_systems.get(coding_system_base(coding))?;
    if resolve_sym(info.coding_type) != "shift-jis" || info.charset_list.len() < 3 {
        return None;
    }
    Some(info.charset_list.clone())
}

/// Encode `s` as Shift-JIS: ASCII as-is, half-width katakana as `code | 0x80`,
/// and JISX0208 kanji through the `JIS_TO_SJIS` shift into two bytes.
fn encode_via_sjis(s: &crate::heap_types::LispString, charsets: &[SymId]) -> Vec<u8> {
    use crate::emacs_core::charset::charset_encode_char;
    let kana = charsets[1];
    let kanji = charsets[2];
    let mut out = Vec::with_capacity(s.sbytes());
    let emit = |code: u32, out: &mut Vec<u8>| {
        if code < 0x80 {
            out.push(code as u8);
            return;
        }
        if crate::emacs_core::emacs_char::char_byte8_p(code) {
            out.push(crate::emacs_core::emacs_char::char_to_byte8(code));
            return;
        }
        if let Some(k) = charset_encode_char(kana, i64::from(code)) {
            out.push((k as u8) | 0x80);
        } else if let Some(j) = charset_encode_char(kanji, i64::from(code)) {
            let (s1, s2) = jis_to_sjis(j as u32);
            out.push(s1);
            out.push(s2);
        } else {
            out.push(b' ');
        }
    };
    for code in coding_source_codepoints(s) {
        emit(code, &mut out);
    }
    out
}

/// Decode Shift-JIS bytes: ASCII, half-width katakana (0xA1-0xDF), and the
/// two-byte JISX0208 sequences (`SJIS_TO_JIS`); other bytes become eight-bit raw.
/// GNU `decode_coding_sjis` (src/coding.c:4560-4625).
///
/// A Shift-JIS lead byte is followed by `ONE_MORE_BYTE (c1)` (:4586), so a
/// lead byte that ends the source stops the decoder at `src_base` -- which is
/// the whole reason this function reports what it consumed.
fn decode_via_sjis(bytes: &[u8], charsets: &[SymId], eol: DosEolLookahead) -> DecodedSource {
    let kana = charsets[1];
    let kanji = charsets[2];
    decode_units(bytes, eol, |unit, sink| {
        let b = unit.byte()?;
        // Half-width katakana annotates the `katakana-jisx0201` charset,
        // two-byte sequences annotate the JISX0208 charset; ASCII and raw
        // bytes carry no `charset` property.
        if b < 0x80 {
            sink.push(u32::from(b), None);
            return Ok(());
        }
        if (0xA1..=0xDF).contains(&b) {
            return match crate::emacs_core::charset::charset_decode_char(kana, i64::from(b & 0x7F))
            {
                Some(ch) => {
                    sink.push(ch as u32, Some(kana));
                    Ok(())
                }
                None => invalid_code(unit, sink),
            };
        }
        if (0x81..=0x9F).contains(&b) || (0xE0..=0xFC).contains(&b) {
            let trail = unit.byte()?;
            let jis = sjis_to_jis(b, trail);
            return match crate::emacs_core::charset::charset_decode_char(kanji, i64::from(jis)) {
                Some(ch) => {
                    sink.push(ch as u32, Some(kanji));
                    Ok(())
                }
                None => invalid_code(unit, sink),
            };
        }
        invalid_code(unit, sink)
    })
}

/// Returns `Some(imap)` if `coding` is utf-7 (`imap=false`) or utf-7-imap.
fn utf7_variant(coding: &str) -> Option<bool> {
    match coding_system_base(coding) {
        "utf-7" => Some(false),
        "utf-7-imap" => Some(true),
        _ => None,
    }
}

/// Whether a byte is in UTF-7's directly-encoded set (passed through literally).
fn utf7_direct(c: u8, imap: bool) -> bool {
    matches!(c, b'\t' | b'\n' | b'\r')
        || if imap {
            (0x20..=0x25).contains(&c) || (0x27..=0x7E).contains(&c)
        } else {
            (0x20..=0x2A).contains(&c) || (0x2C..=0x5B).contains(&c) || (0x5D..=0x7D).contains(&c)
        }
}

/// Encode `s` as UTF-7 (RFC 2152) / modified UTF-7 (IMAP, RFC 2060): direct
/// characters pass through; runs of other characters are emitted as modified
/// Base64 of their UTF-16BE units between the shift char (`+`/`&`) and `-`.
fn encode_via_utf7(s: &crate::heap_types::LispString, imap: bool) -> Vec<u8> {
    let esc = if imap { b'&' } else { b'+' };
    let chars: Vec<u32> = coding_source_codepoints(s).collect();
    let mut out = Vec::with_capacity(chars.len());
    let mut i = 0usize;
    while i < chars.len() {
        let c = chars[i];
        if c < 0x80 && utf7_direct(c as u8, imap) {
            out.push(c as u8);
            i += 1;
        } else if c == u32::from(esc) {
            out.push(esc);
            out.push(b'-');
            i += 1;
        } else {
            out.push(esc);
            let mut units = Vec::new();
            while i < chars.len() {
                let cc = chars[i];
                if (cc < 0x80 && utf7_direct(cc as u8, imap)) || cc == u32::from(esc) {
                    break;
                }
                push_utf16_codepoint(&mut units, Utf16Endian::Big, cc);
                i += 1;
            }
            let mut b64 = crate::emacs_core::fns::base64_standard_encode_unpadded(&units);
            if imap {
                b64 = b64.replace('/', ",");
            }
            out.extend_from_slice(b64.as_bytes());
            if imap || i < chars.len() {
                out.push(b'-');
            }
        }
    }
    out
}

/// Decode UTF-7 / modified UTF-7 to Emacs internal bytes.
fn decode_via_utf7(bytes: &[u8], imap: bool) -> Vec<u8> {
    let esc = if imap { b'&' } else { b'+' };
    let is_b64 = |c: u8| {
        c.is_ascii_alphanumeric() || c == b'+' || (if imap { c == b',' } else { c == b'/' })
    };
    let mut out = Vec::with_capacity(bytes.len());
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let mut emit = |code: u32, out: &mut Vec<u8>| {
        let n = crate::emacs_core::emacs_char::char_string(code, &mut buf);
        out.extend_from_slice(&buf[..n]);
    };
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b != esc {
            // Direct byte; high bytes are not valid UTF-7 and become eight-bit raw.
            let code = if b < 0x80 {
                u32::from(b)
            } else {
                crate::emacs_core::emacs_char::unibyte_to_char(b)
            };
            emit(code, &mut out);
            i += 1;
            continue;
        }
        i += 1; // consume the shift char
        let run_start = i;
        while i < bytes.len() && is_b64(bytes[i]) {
            i += 1;
        }
        let mut run = bytes[run_start..i].to_vec();
        if i < bytes.len() && bytes[i] == b'-' {
            i += 1; // consume explicit terminator
        }
        if run.is_empty() {
            emit(u32::from(esc), &mut out); // `+-` / `&-` -> literal shift char
            continue;
        }
        if imap {
            for x in run.iter_mut() {
                if *x == b',' {
                    *x = b'/';
                }
            }
        }
        while !run.len().is_multiple_of(4) {
            run.push(b'=');
        }
        if let Some(decoded) = crate::emacs_core::fns::base64_standard_decode(&run) {
            let mut j = 0usize;
            while j + 1 < decoded.len() {
                let unit = u16::from_be_bytes([decoded[j], decoded[j + 1]]);
                j += 2;
                let code = if (0xD800..=0xDBFF).contains(&unit) && j + 1 < decoded.len() {
                    let lo = u16::from_be_bytes([decoded[j], decoded[j + 1]]);
                    if (0xDC00..=0xDFFF).contains(&lo) {
                        j += 2;
                        0x10000 + ((u32::from(unit) - 0xD800) << 10) + (u32::from(lo) - 0xDC00)
                    } else {
                        u32::from(unit)
                    }
                } else {
                    u32::from(unit)
                };
                emit(code, &mut out);
            }
        }
    }
    out
}

/// Charset list of the chinese-hz (HZ-GB-2312) coding system, or `None`.
fn chinese_hz_charset(ctx: &crate::emacs_core::eval::Context, coding: &str) -> Option<SymId> {
    if coding_system_base(coding) != "chinese-hz" {
        return None;
    }
    let info = ctx.coding_systems.get(coding_system_base(coding))?;
    // charset-list = (ascii chinese-gb2312)
    info.charset_list.get(1).copied()
}

/// Encode `s` as HZ-GB-2312 (RFC 1843): ASCII by default, `~{` switches to
/// GB2312 (7-bit GL bytes), `~}` back, `~` doubled to `~~`.
fn encode_via_hz(s: &crate::heap_types::LispString, gb2312: SymId) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.sbytes());
    let mut in_gb = false;
    for code in coding_source_codepoints(s) {
        if code < 0x80 {
            if in_gb {
                out.extend_from_slice(b"~}");
                in_gb = false;
            }
            if code == u32::from(b'~') {
                out.extend_from_slice(b"~~");
            } else {
                out.push(code as u8);
            }
        } else if let Some(gbcode) =
            crate::emacs_core::charset::charset_encode_char(gb2312, i64::from(code))
        {
            if !in_gb {
                out.extend_from_slice(b"~{");
                in_gb = true;
            }
            out.push(((gbcode >> 8) & 0x7F) as u8);
            out.push((gbcode & 0x7F) as u8);
        } else {
            // Non-ASCII, non-GB2312: GNU emits a \uXXXX literal (rare); leave a
            // space rather than mis-encode.
            if in_gb {
                out.extend_from_slice(b"~}");
                in_gb = false;
            }
            out.push(b' ');
        }
    }
    if in_gb {
        out.extend_from_slice(b"~}");
    }
    out
}

/// Decode HZ-GB-2312 to Emacs internal bytes.
fn decode_via_hz(bytes: &[u8], gb2312: SymId) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    let mut emit = |code: u32, out: &mut Vec<u8>| {
        let n = crate::emacs_core::emacs_char::char_string(code, &mut buf);
        out.extend_from_slice(&buf[..n]);
    };
    let mut in_gb = false;
    let mut i = 0usize;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'~' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b'{' => {
                    in_gb = true;
                    i += 2;
                    continue;
                }
                b'}' => {
                    in_gb = false;
                    i += 2;
                    continue;
                }
                b'~' => {
                    emit(u32::from(b'~'), &mut out);
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        if in_gb && (0x21..0x7F).contains(&b) && i + 1 < bytes.len() {
            let code = (i64::from(b) << 8) | i64::from(bytes[i + 1]);
            if let Some(ch) = crate::emacs_core::charset::charset_decode_char(gb2312, code) {
                emit(ch as u32, &mut out);
                i += 2;
                continue;
            }
        }
        let code = if b < 0x80 {
            u32::from(b)
        } else {
            crate::emacs_core::emacs_char::unibyte_to_char(b)
        };
        emit(code, &mut out);
        i += 1;
    }
    out
}

/// Characters consumed by GNU's non-CCL coding encoders.
///
/// `consume_chars` in GNU `src/coding.c` reads a multibyte source normally. For
/// a unibyte source, however, it recognizes any valid embedded Emacs multibyte
/// sequence as one character and promotes every other high octet to a byte8
/// character. Keeping that policy in one iterator prevents individual codecs
/// from accidentally treating unibyte octets as Latin-1 Unicode scalars.
struct CodingSourceCodepoints<'a> {
    bytes: &'a [u8],
    pos: usize,
    multibyte: bool,
}

impl Iterator for CodingSourceCodepoints<'_> {
    type Item = u32;

    fn next(&mut self) -> Option<Self::Item> {
        let source = self.bytes.get(self.pos..)?;
        if source.is_empty() {
            return None;
        }
        let (code, len) = if self.multibyte
            || crate::emacs_core::emacs_char::multibyte_length(source, true).is_some()
        {
            crate::emacs_core::emacs_char::string_char(source)
        } else {
            (crate::emacs_core::emacs_char::unibyte_to_char(source[0]), 1)
        };
        self.pos += len;
        Some(code)
    }
}

fn coding_source_codepoints(s: &crate::heap_types::LispString) -> CodingSourceCodepoints<'_> {
    CodingSourceCodepoints {
        bytes: s.as_bytes(),
        pos: 0,
        multibyte: s.is_multibyte(),
    }
}

/// Whether `coding` is the emacs-mule coding system (Emacs's old internal
/// multibyte representation as a coding system).
fn is_emacs_mule(ctx: &crate::emacs_core::eval::Context, coding: &str) -> bool {
    ctx.coding_systems
        .get(coding_system_base(coding))
        .is_some_and(|info| resolve_sym(info.coding_type) == "emacs-mule")
}

/// Return the `:pre-write-conversion` (when `encode`) or `:post-read-conversion`
/// (otherwise) hook function of `coding`, together with the coding system's
/// base `:coding-type` name, when the hook should be run by the generic
/// codec path.
///
/// GNU's coding pipeline runs `CODING_ATTR_PRE_WRITE` before encoding and
/// `CODING_ATTR_POST_READ` after decoding (src/coding.c
/// `encode_coding_object` / `decode_coding_object`).  The mnemonic VIQR coding
/// (`vietnamese-viqr`, base `:coding-type` utf-8) and a few others rely on
/// these elisp hooks for the actual character translation; without them the
/// generic codec is a pass-through (and, for VIQR specifically, would drop
/// every non-ASCII character because the family is unknown).
///
/// Codings that neomacs already routes through a dedicated codec (utf-7, hz,
/// emacs-mule, …) reimplement their conversion entirely in Rust and must NOT
/// re-run the hook, so this returns `None` for them; the caller only consults
/// it on the otherwise-generic branch.
fn coding_conversion_hook(
    ctx: &crate::emacs_core::eval::Context,
    coding: &str,
    encode: bool,
) -> Option<(SymId, String)> {
    let info = ctx.coding_systems.get(coding_system_base(coding))?;
    let hook = if encode {
        info.pre_write_conversion
    } else {
        info.post_read_conversion
    }?;
    Some((hook, resolve_sym(info.coding_type).to_string()))
}

/// True when GNU's `code_convert_string` identity fast path applies: the coding
/// is `:ascii-compatible-p`, the input `args[0]` is pure ASCII, and no
/// end-of-line conversion is required (unix EOL, or no `\n`/`\r` to convert).
/// In that case GNU returns the string unchanged and never enters
/// encode/decode_coding_object, so the pre/post-conversion hooks do not run.
fn coding_ascii_identity_fast_path(
    ctx: &crate::emacs_core::eval::Context,
    coding: &str,
    args: &[Value],
    encode: bool,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> bool {
    if !ctx.coding_systems.is_ascii_compatible(coding) {
        return false;
    }
    if ctx.coding_systems.get(coding_system_base(coding)).is_none() {
        return false;
    }
    let Some(string) = args[0].as_lisp_string() else {
        return false;
    };
    let bytes = string.as_bytes();
    // For both unibyte and all-ASCII multibyte strings every storage byte is
    // < 0x80 (eight-bit and multibyte chars use >= 0x80 lead bytes).
    if !bytes.iter().all(u8::is_ascii) {
        return false;
    }
    // EOL conversion forces the slow path unless the coding is unix EOL or the
    // relevant newline byte is absent.  GNU tests `CODING_ID_EOL_TYPE
    // (coding.id)` (src/coding.c:9612), which is the eol type of the RESOLVED
    // system -- `raw-text-unix`'s own `Qunix`, not the undecided one its base
    // `raw-text` carries.  `coding_name_eol` is that resolution, and it is the
    // same function the conversion itself uses, so the fast path and the
    // conversion cannot disagree about whether EOL work is owed.
    //
    // `inhibit_eol_conversion` is the middle term of GNU's three-way escape
    // (src/coding.c:9613-9615), and it is observable through this path rather
    // than only through the text: taking the fast path returns a MULTIBYTE
    // string on decode where the slow `raw-text` decoder builds a unibyte one.
    // Measured under GNU 31.0.90, `(decode-coding-string "a\r\n" 'raw-text)` is
    // unibyte normally and multibyte under `inhibit-eol-conversion'.
    if eol_conversion == crate::emacs_core::coding::EolConversion::Inhibited {
        return true;
    }
    match coding_name_eol(coding) {
        crate::emacs_core::coding::EolType::Unix => true,
        _ => {
            let needle = if encode { b'\n' } else { b'\r' };
            !bytes.contains(&needle)
        }
    }
}

/// Build (and evaluate) the elisp form that mirrors GNU's
/// `encode_coding_object` pre-write protocol: insert `src` into a fresh
/// conversion work buffer, call `(FN (point-min) (point-max))`, and return the
/// text of whatever buffer is current afterwards (the hook is allowed to switch
/// to its own buffer, as `viqr-pre-write-conversion` does).  Both work buffers
/// are killed.  Returns the transformed multibyte string `Value`.
fn run_pre_write_conversion(
    ctx: &mut crate::emacs_core::eval::Context,
    hook: SymId,
    src: Value,
) -> EvalResult {
    // `src` is a heap Value that must survive the allocations performed while
    // building the form below (exact GC does not scan Rust locals).  Root it on
    // the specpdl for the duration of construction; `eval_sub` then roots the
    // finished form itself.
    let root_scope = ctx.save_specpdl_roots();
    ctx.push_specpdl_root(src);
    let form = build_pre_write_form(hook, src);
    ctx.restore_specpdl_roots(root_scope);
    ctx.eval_sub(form)
}

fn build_pre_write_form(hook: SymId, src: Value) -> Value {
    // (save-current-buffer
    //   (let ((src-buf (generate-new-buffer " *code-conversion-work*")))
    //     (unwind-protect
    //         (progn
    //           (with-current-buffer src-buf (insert SRC))
    //           (set-buffer src-buf)
    //           (funcall 'FN (point-min) (point-max))
    //           (prog1 (buffer-string)
    //             (unless (eq (current-buffer) src-buf)
    //               (kill-buffer (current-buffer)))))
    //       (when (buffer-live-p src-buf) (kill-buffer src-buf)))))
    // `save-current-buffer` restores the caller's buffer after the hook (which
    // may `set-buffer` to its own work buffer and which we then kill).
    let src_buf = Value::symbol("--neovm-cc-src-buf");
    let gen_buf = Value::list(vec![
        Value::symbol("generate-new-buffer"),
        Value::string(" *code-conversion-work*"),
    ]);
    let binding = Value::list(vec![Value::list(vec![src_buf, gen_buf])]);
    let insert_form = Value::list(vec![
        Value::symbol("with-current-buffer"),
        src_buf,
        Value::list(vec![Value::symbol("insert"), src]),
    ]);
    let set_buf = Value::list(vec![Value::symbol("set-buffer"), src_buf]);
    let call = Value::list(vec![
        Value::symbol("funcall"),
        quote_value(Value::symbol(resolve_sym(hook))),
        Value::list(vec![Value::symbol("point-min")]),
        Value::list(vec![Value::symbol("point-max")]),
    ]);
    let kill_current = Value::list(vec![
        Value::symbol("unless"),
        Value::list(vec![
            Value::symbol("eq"),
            Value::list(vec![Value::symbol("current-buffer")]),
            src_buf,
        ]),
        Value::list(vec![
            Value::symbol("kill-buffer"),
            Value::list(vec![Value::symbol("current-buffer")]),
        ]),
    ]);
    let result = Value::list(vec![
        Value::symbol("prog1"),
        Value::list(vec![Value::symbol("buffer-string")]),
        kill_current,
    ]);
    let body = Value::list(vec![
        Value::symbol("progn"),
        insert_form,
        set_buf,
        call,
        result,
    ]);
    let cleanup = Value::list(vec![
        Value::symbol("when"),
        Value::list(vec![Value::symbol("buffer-live-p"), src_buf]),
        Value::list(vec![Value::symbol("kill-buffer"), src_buf]),
    ]);
    let unwind = Value::list(vec![Value::symbol("unwind-protect"), body, cleanup]);
    let let_form = Value::list(vec![Value::symbol("let"), binding, unwind]);
    Value::list(vec![Value::symbol("save-current-buffer"), let_form])
}

/// Mirror GNU's `decode_coding_object` post-read protocol: insert the decoded
/// `text` into a temp buffer, move point to the start, call `(FN LEN)`, and
/// return the resulting buffer text.
fn run_post_read_conversion(
    ctx: &mut crate::emacs_core::eval::Context,
    hook: SymId,
    text: Value,
) -> EvalResult {
    // `text` is a heap Value that must survive form-construction allocations.
    let root_scope = ctx.save_specpdl_roots();
    ctx.push_specpdl_root(text);
    let form = build_post_read_form(hook, text);
    ctx.restore_specpdl_roots(root_scope);
    ctx.eval_sub(form)
}

fn build_post_read_form(hook: SymId, text: Value) -> Value {
    // (with-temp-buffer
    //   (insert TEXT)
    //   (goto-char (point-min))
    //   (funcall 'FN (- (point-max) (point-min)))
    //   (buffer-string))
    let len = Value::list(vec![
        Value::symbol("-"),
        Value::list(vec![Value::symbol("point-max")]),
        Value::list(vec![Value::symbol("point-min")]),
    ]);
    let call = Value::list(vec![
        Value::symbol("funcall"),
        quote_value(Value::symbol(resolve_sym(hook))),
        len,
    ]);
    Value::list(vec![
        Value::symbol("with-temp-buffer"),
        Value::list(vec![Value::symbol("insert"), text]),
        Value::list(vec![
            Value::symbol("goto-char"),
            Value::list(vec![Value::symbol("point-min")]),
        ]),
        call,
        Value::list(vec![Value::symbol("buffer-string")]),
    ])
}

/// `'value` -> `(quote value)`.
fn quote_value(value: Value) -> Value {
    Value::list(vec![Value::symbol("quote"), value])
}

/// Encode or decode `args[0]` through a coding system whose conversion is
/// implemented by an elisp `:pre-write-conversion` / `:post-read-conversion`
/// hook (e.g. `vietnamese-viqr`).  Mirrors GNU: encoding runs the pre-write
/// hook first and then encodes the transformed text with the base
/// `:coding-type`; decoding decodes with the base `:coding-type` first and then
/// runs the post-read hook on the decoded text.
fn run_coding_with_conversion_hook(
    ctx: &mut crate::emacs_core::eval::Context,
    args: &[Value],
    base_type: &str,
    hook: SymId,
    encode: bool,
    eol: crate::emacs_core::coding::EolType,
    run: &mut CodingRun<'_>,
) -> Result<
    (
        Value,
        Option<crate::emacs_core::coding::DecodeEolResolution>,
    ),
    crate::emacs_core::error::Flow,
> {
    // The base `:coding-type` symbol names the codec to apply to the text the
    // hook produces / consumes.  GNU's mnemonic codings are utf-8 based; map
    // any other base type by name and fall back to utf-8 (the universal
    // multibyte codec) when the type is not itself a usable coding-system name.
    let base_coding = if known_coding_system(base_type) {
        base_type
    } else {
        "utf-8"
    };
    // The EOL leg belongs to the coding system, not to the hook: GNU runs
    // `pre-write-conversion` and then encodes the RESULT through
    // `encode_coding_object`, whose `consume_chars` (src/coding.c:7607) still
    // expands the newline.  So `vietnamese-viqr-dos` gets CRLF even though its
    // conversion is an elisp hook.  Carry the leg onto the base codec name, or
    // it is dropped here exactly the way it used to be dropped in the codec
    // chain below.
    let base_coding = &apply_explicit_eol_suffix(base_coding, eol);
    if encode {
        let transformed = run_pre_write_conversion(ctx, hook, args[0])?;
        let transformed_str = ctx.lisp_string(transformed).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), transformed],
            )
        })?;
        let bytes = encode_lisp_string(transformed_str, base_coding, ctx.eol_conversion());
        Ok((
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes)),
            None,
        ))
    } else {
        // GNU's order is decoder, then `decode_eol` (src/coding.c:7481), then
        // the `:post-read-conversion` hook (:8180-8194) -- three steps, and the
        // middle one is what names the coding system the conversion reports.
        // Spending the EOL leg here rather than inside the base decode is what
        // makes the resolution available: an UNDECIDED eol type is resolved by
        // scanning the text the decoder produced, and a decode that had already
        // converted its own newlines has nothing left to scan.  Measured under
        // GNU Emacs 31.0.90, the pure-ASCII VIQR source through
        // `insert-file-contents` reports `vietnamese-viqr-unix', not
        // `vietnamese-viqr'.
        let body = coding_name_with_eol_spent(base_coding).into_owned();
        // GNU decodes through the SAME `struct coding_system` the hook then
        // runs off, so `coding->consumed` is already on it when
        // `decode_coding_object` reaches the hook (src/coding.c:8180).  Here
        // the base decode reaches a door that answers a Lisp string, so the
        // count is taken from the same decoder separately -- and only for a
        // source that may still be continued, which is a subprocess read.
        let source_bytes =
            lisp_string_coding_source_bytes(args[0].as_lisp_string().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), args[0]],
                )
            })?);
        let dos_eol = if matches!(
            coding_name_eol(&body),
            crate::emacs_core::coding::EolType::Dos
        ) && ctx.eol_conversion()
            == crate::emacs_core::coding::EolConversion::Enabled
        {
            DosEolLookahead::Required
        } else {
            DosEolLookahead::NotRequired
        };
        let consumed = match decode_fallthrough_source(&source_bytes, &body, dos_eol) {
            Some(decoded) => decoded.consumed,
            None => SourceConsumed::all(&source_bytes),
        };
        run.record(consumed);
        // A source that may still be continued is decoded only as far as the
        // base decoder got: the rest is `coding->carryover` and belongs to the
        // next read, and decoding it here would put it in the text AND in the
        // carryover.  A `CODING_MODE_LAST_BLOCK` source keeps every byte,
        // because its tail is flushed as characters rather than held back.
        let source_arg = if run.flushes_last_block() || consumed.bytes() == source_bytes.len() {
            args[0]
        } else {
            Value::heap_string(crate::heap_types::LispString::from_unibyte(
                source_bytes[..consumed.bytes()].to_vec(),
            ))
        };
        let decoded = builtin_decode_coding_string_with_known(
            vec![source_arg, Value::symbol(&body)],
            |_| true,
            ctx.eol_conversion(),
        )?;
        let decoded_str = ctx.lisp_string(decoded).ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), decoded],
            )
        })?;
        let resolution = eol.resolve_for_decode(decoded_str.as_bytes(), ctx.eol_conversion());
        let converted = match resolution.eol() {
            crate::emacs_core::coding::ResolvedEol::Unix => decoded,
            converting => {
                let bytes = decode_eol_bytes(decoded_str.as_bytes(), converting);
                let multibyte = decoded_str.is_multibyte();
                Value::heap_string(if multibyte {
                    crate::heap_types::LispString::from_emacs_bytes(bytes)
                } else {
                    crate::heap_types::LispString::from_unibyte(bytes)
                })
            }
        };
        Ok((
            run_post_read_conversion(ctx, hook, converted)?,
            Some(resolution),
        ))
    }
}

/// Standard UTF-8 encoding of a Lisp string (GNU `CHAR_STRING` per character):
/// eight-bit characters become their single raw byte, everything else its
/// plain UTF-8 bytes. This is what raw-text/no-conversion and the UTF-8 codec
/// emit for the character payload.
fn encode_utf8_plain(s: &crate::heap_types::LispString) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.sbytes());
    for cp in coding_source_codepoints(s) {
        if crate::emacs_core::emacs_char::char_byte8_p(cp) {
            out.push(crate::emacs_core::emacs_char::char_to_byte8(cp));
        } else if let Some(ch) = char::from_u32(cp) {
            let mut b = [0u8; 4];
            out.extend_from_slice(ch.encode_utf8(&mut b).as_bytes());
        } else {
            encode_emacs_utf8_codepoint(cp, &mut out);
        }
    }
    out
}

/// Decode raw-text / no-conversion: ASCII bytes stay ASCII, every other byte
/// becomes an eight-bit raw character.
fn decode_raw_text_multibyte(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut buf = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    for &b in bytes {
        let code = if b < 0x80 {
            u32::from(b)
        } else {
            crate::emacs_core::emacs_char::unibyte_to_char(b)
        };
        let n = crate::emacs_core::emacs_char::char_string(code, &mut buf);
        out.extend_from_slice(&buf[..n]);
    }
    out
}

/// The emacs-mule leading byte(s) for a charset's emacs-mule id
/// (`EMACS_MULE_LEADING_CODES`, coding.c): direct (`id`) for id < 0xA0, else a
/// private leading code 0x9A-0x9D followed by `id`.
fn emacs_mule_leading(id: i64) -> Vec<u8> {
    let id = id as u8;
    match id {
        _ if id < 0xA0 => vec![id],
        0xA0..=0xDF => vec![0x9A, id],
        0xE0..=0xEF => vec![0x9B, id],
        0xF0..=0xF4 => vec![0x9C, id],
        _ => vec![0x9D, id],
    }
}

/// Encode `s` as emacs-mule.
fn encode_via_emacs_mule(s: &crate::heap_types::LispString) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.sbytes());
    for code in coding_source_codepoints(s) {
        if code < 0x80 {
            out.push(code as u8);
        } else if crate::emacs_core::emacs_char::char_byte8_p(code) {
            out.push(crate::emacs_core::emacs_char::char_to_byte8(code));
        } else if let Some((id, dim, cs_code)) =
            crate::emacs_core::charset::emacs_mule_encode_char(i64::from(code))
        {
            out.extend(emacs_mule_leading(id));
            if dim >= 2 {
                let c = (cs_code | 0x8080) as u32;
                out.push((c >> 8) as u8);
                out.push((c & 0xFF) as u8);
            } else {
                out.push((cs_code | 0x80) as u8);
            }
        } else {
            out.push(b' ');
        }
    }
    out
}

/// Decode emacs-mule to Emacs internal bytes.
/// GNU `decode_coding_emacs_mule` (src/coding.c:2280-2545).
///
/// Its leading code is followed by `ONE_MORE_BYTE` per position byte
/// (`emacs_mule_char`, :1750-1830), so a source that stops inside one leaves
/// the character for the next read.
fn decode_via_emacs_mule(bytes: &[u8], eol: DosEolLookahead) -> DecodedSource {
    // Decode a charset character: read `dim` position bytes, strip the high
    // bit, look the code up in the charset.  `None` means the leading code is
    // not a charset this build knows; `Err` means the source ran out.
    fn read_charset(
        id: i64,
        unit: &mut UnitReader<'_>,
    ) -> Result<Option<(u32, SymId)>, NoMoreSource> {
        let Some((cs, dim)) = crate::emacs_core::charset::charset_by_emacs_mule_id(id) else {
            return Ok(None);
        };
        let dim = dim.clamp(1, 2) as usize;
        let position = unit.take(dim)?;
        let mut code = 0i64;
        for byte in position {
            code = (code << 8) | i64::from(byte & 0x7F);
        }
        Ok(crate::emacs_core::charset::charset_decode_char(cs, code).map(|ch| (ch as u32, cs)))
    }

    decode_units(bytes, eol, |unit, sink| {
        let b = unit.byte()?;
        if b < 0x80 {
            sink.push(u32::from(b), None);
            return Ok(());
        }
        if (0x9A..=0x9D).contains(&b) {
            // Private leading code: next byte is the charset's emacs-mule id.
            let id = unit.byte()?;
            return match read_charset(i64::from(id), unit)? {
                Some((ch, _)) => {
                    sink.push(ch, None);
                    Ok(())
                }
                None => invalid_code(unit, sink),
            };
        }
        if (0x81..=0x99).contains(&b) {
            // Direct leading code == the charset's emacs-mule id.
            return match read_charset(i64::from(b), unit)? {
                Some((ch, _)) => {
                    sink.push(ch, None);
                    Ok(())
                }
                None => invalid_code(unit, sink),
            };
        }
        invalid_code(unit, sink)
    })
}

/// If `coding` is an ISO-2022 coding system that uses escape-sequence
/// designations and/or 7-bit output (iso-2022-jp/cn/kr, the 7bit / 8bit-ss2
/// variants, compound-text/ctext), return its designation state + charset list.
/// The fixed-designation 8-bit EUC profile is handled by `euc_iso2022_spec`.
fn full_iso2022_spec(
    ctx: &crate::emacs_core::eval::Context,
    coding: &str,
) -> Option<(crate::emacs_core::coding::Iso2022Spec, Vec<SymId>)> {
    let info = ctx.coding_systems.get(coding_system_base(coding))?;
    if resolve_sym(info.coding_type) != "iso-2022" {
        return None;
    }
    let spec = crate::emacs_core::coding::iso2022_spec(info)?;
    use crate::emacs_core::coding::IsoFlag;
    if !(spec.flags.contains(IsoFlag::SevenBits) || spec.flags.contains(IsoFlag::Designation)) {
        return None;
    }
    Some((spec, info.charset_list.clone()))
}

fn runtime_ccl_spec(
    ctx: &crate::emacs_core::eval::Context,
    coding: &str,
) -> Option<crate::emacs_core::coding::CclCodingSpec> {
    let info = ctx.coding_systems.get(coding_system_base(coding))?;
    (resolve_sym(info.coding_type) == "ccl")
        .then(|| crate::emacs_core::coding::ccl_coding_spec(info))?
}

fn ccl_encoding_input(source: &crate::heap_types::LispString, coding: &str) -> Vec<i64> {
    use crate::emacs_core::coding::EolType;

    let mut input = Vec::with_capacity(source.schars());
    for code in coding_source_codepoints(source) {
        if code == u32::from(b'\n') {
            match coding_name_eol(coding) {
                EolType::Dos => input.extend([i64::from(b'\r'), i64::from(b'\n')]),
                EolType::Mac => input.push(i64::from(b'\r')),
                EolType::Unix | EolType::Undecided => input.push(i64::from(code)),
            }
        } else {
            input.push(i64::from(code));
        }
    }
    input
}

fn decode_ccl_eol(mut codes: Vec<i64>, coding: &str) -> Vec<i64> {
    use crate::emacs_core::coding::EolType;

    match coding_name_eol(coding) {
        EolType::Dos => {
            let mut decoded = Vec::with_capacity(codes.len());
            let mut index = 0usize;
            while index < codes.len() {
                if codes[index] == i64::from(b'\r')
                    && codes.get(index + 1) == Some(&i64::from(b'\n'))
                {
                    decoded.push(i64::from(b'\n'));
                    index += 2;
                } else {
                    decoded.push(codes[index]);
                    index += 1;
                }
            }
            decoded
        }
        EolType::Mac => {
            for code in &mut codes {
                if *code == i64::from(b'\r') {
                    *code = i64::from(b'\n');
                }
            }
            codes
        }
        EolType::Unix | EolType::Undecided => codes,
    }
}

fn encode_via_ccl(
    source: &crate::heap_types::LispString,
    spec: crate::emacs_core::coding::CclCodingSpec,
    coding: &str,
    boundary: EncodingBoundary,
) -> Result<Vec<u8>, crate::emacs_core::error::Flow> {
    let input = ccl_encoding_input(source, coding);
    let output = crate::emacs_core::ccl::execute_compiled_ccl(
        spec.encoder,
        &input,
        boundary.owns_end_of_stream(),
    )?;
    Ok(output.into_iter().map(|code| code as u8).collect())
}

fn decode_via_ccl(
    source: &[u8],
    spec: crate::emacs_core::coding::CclCodingSpec,
    coding: &str,
) -> Result<Vec<u8>, crate::emacs_core::error::Flow> {
    let input = source
        .iter()
        .map(|byte| i64::from(*byte))
        .collect::<Vec<_>>();
    let output = crate::emacs_core::ccl::execute_compiled_ccl(spec.decoder, &input, true)?;
    let output = decode_ccl_eol(output, coding);
    let mut bytes = Vec::with_capacity(output.len());
    let mut buffer = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    for code in output {
        let code = u32::try_from(code)
            .ok()
            .filter(|code| *code <= crate::emacs_core::emacs_char::MAX_CHAR)
            .unwrap_or(char::REPLACEMENT_CHARACTER as u32);
        let length = crate::emacs_core::emacs_char::char_string(code, &mut buffer);
        bytes.extend_from_slice(&buffer[..length]);
    }
    Ok(bytes)
}

/// The ESC designation sequence loading `charset` into graphic register `reg`
/// (`ENCODE_DESIGNATION`, coding.c): `ESC <I> F` for 94/96-sets, `ESC $ [I] F`
/// for the multi-byte sets (the intermediate `I` is omitted only for the
/// short-form dim-2 G0 case).
fn iso2022_designation_escape(
    reg: usize,
    final_char: i64,
    dim: i64,
    chars_96: bool,
    long_form: bool,
) -> Vec<u8> {
    const I94: [u8; 4] = [b'(', b')', b'*', b'+'];
    const I96: [u8; 4] = [b',', b'-', b'.', b'/'];
    let mut out = vec![0x1B];
    if dim <= 1 {
        out.push(if chars_96 { I96[reg] } else { I94[reg] });
    } else {
        out.push(b'$');
        if chars_96 {
            out.push(I96[reg]);
        } else if long_form || reg != 0 || !(0x40..=0x42).contains(&final_char) {
            out.push(I94[reg]);
        }
    }
    out.push(final_char as u8);
    out
}

/// Append a character's position code as ISO-2022 bytes: graphic-left
/// (`& 0x7F`) or graphic-right (`| 0x80`), one byte per charset dimension.
fn iso2022_emit_code(out: &mut Vec<u8>, code: i64, dim: i64, graphic_right: bool) {
    let mask = |b: i64| {
        if graphic_right {
            (b | 0x80) as u8
        } else {
            (b & 0x7F) as u8
        }
    };
    if dim >= 2 {
        out.push(mask((code >> 8) & 0xFF));
    }
    out.push(mask(code & 0xFF));
}

/// Encode `s` through the full ISO-2022 escape-sequence machine.
fn encode_via_iso2022(
    s: &crate::heap_types::LispString,
    spec: &crate::emacs_core::coding::Iso2022Spec,
    charset_list: &[SymId],
    boundary: EncodingBoundary,
) -> Vec<u8> {
    use crate::emacs_core::charset::{charset_encode_char, charset_iso2022_designation};
    use crate::emacs_core::coding::IsoFlag;
    let seven = spec.flags.contains(IsoFlag::SevenBits);
    let single_shift = spec.flags.contains(IsoFlag::SingleShift);
    let long_form = spec.flags.contains(IsoFlag::LongForm);
    let reset_eol = spec.flags.contains(IsoFlag::AsciiAtEol);
    let reset_cntl = spec.flags.contains(IsoFlag::AsciiAtCntl);
    let use_roman = spec.flags.contains(IsoFlag::UseRoman);
    let ascii = intern("ascii");
    // FULL_SUPPORT coding systems store `:charset-list` as the symbol
    // `iso-2022`; substitute the full ISO-2022 charset candidate set.
    let full_support = charset_list.len() == 1 && resolve_sym(charset_list[0]) == "iso-2022";
    let candidates: Vec<SymId> = if full_support {
        crate::emacs_core::charset::iso2022_full_charset_candidates()
    } else {
        charset_list.to_vec()
    };

    let initial = spec.initial;
    let mut desig: [Option<SymId>; 4] = initial;
    let mut gl: usize = 0; // register currently invoked to the GL plane
    let gr: i32 = if seven { -1 } else { 1 }; // G1 -> GR plane in 8-bit

    let mut out = Vec::with_capacity(s.sbytes());
    // Reset planes/registers to their initial designations (used at EOL and at
    // end of input when RESET_AT_EOL is set).
    let reset = |out: &mut Vec<u8>, desig: &mut [Option<SymId>; 4], gl: &mut usize| {
        if *gl != 0 {
            out.push(0x0F); // SI
            *gl = 0;
        }
        for r in 0..4 {
            if initial[r].is_some() && desig[r] != initial[r] {
                if let Some((fc, d, c96)) = initial[r].and_then(charset_iso2022_designation) {
                    out.extend(iso2022_designation_escape(r, fc, d, c96, long_form));
                }
                desig[r] = initial[r];
            }
        }
    };

    for c in coding_source_codepoints(s) {
        if c < 0x20 || c == 0x7F {
            if c == 0x0A {
                if reset_eol {
                    reset(&mut out, &mut desig, &mut gl);
                }
            } else if reset_cntl {
                reset(&mut out, &mut desig, &mut gl);
            }
            out.push(c as u8);
            continue;
        }
        if crate::emacs_core::emacs_char::char_byte8_p(c) {
            out.push(crate::emacs_core::emacs_char::char_to_byte8(c));
            continue;
        }
        // Select the charset (and its position code, dimension, set size).
        let (charset, code, dim, chars_96) = if c < 0x80 {
            // The use-roman flag (japanese-iso-7bit-1978-irv / old-jis) maps
            // ASCII through latin-jisx0201 — the G0 initial designation — so the
            // byte is emitted with no re-designation. GNU keeps the position
            // code = c (it does not re-encode through latin-jisx0201).
            if use_roman {
                (intern("latin-jisx0201"), i64::from(c), 1, false)
            } else {
                (ascii, i64::from(c), 1, false)
            }
        } else {
            // Iterate the coding's charset list in order (GNU char_charset
            // over `:charset-list`): the first charset that can represent the
            // character wins. The list order is what distinguishes, e.g.,
            // iso-2022-jp (jisx0208 before -1978) from japanese-iso-7bit-1978-irv
            // (-1978 first). For FULL_SUPPORT codings `candidates` is the broad
            // id-ordered set (with -1978 already dropped).
            let mut found = None;
            for &cs in &candidates {
                if let Some((_, dim, c96)) = charset_iso2022_designation(cs)
                    && let Some(code) = charset_encode_char(cs, i64::from(c))
                {
                    found = Some((cs, code, dim, c96));
                    break;
                }
            }
            found.unwrap_or((ascii, 0x20, 1, false)) // unencodable -> space
        };
        // Register the charset is designated to: an existing designation, else
        // the reg-usage rule for its set size (GNU `setup_iso_safe_charsets`).
        let reg = desig
            .iter()
            .position(|&d| d == Some(charset))
            .unwrap_or_else(|| spec.encode_register(chars_96));
        if desig[reg] != Some(charset) {
            if let Some((fc, d, c96)) = charset_iso2022_designation(charset) {
                out.extend(iso2022_designation_escape(reg, fc, d, c96, long_form));
            }
            desig[reg] = Some(charset);
        }
        if gl == reg {
            iso2022_emit_code(&mut out, code, dim, false);
        } else if gr == reg as i32 {
            iso2022_emit_code(&mut out, code, dim, true);
        } else {
            match reg {
                0 => {
                    out.push(0x0F); // SI
                    gl = 0;
                    iso2022_emit_code(&mut out, code, dim, false);
                }
                1 => {
                    out.push(0x0E); // SO
                    gl = 1;
                    iso2022_emit_code(&mut out, code, dim, false);
                }
                _ if single_shift => {
                    if seven {
                        out.push(0x1B);
                        out.push(if reg == 2 { 0x4E } else { 0x4F }); // ESC N / ESC O
                    } else {
                        out.push(if reg == 2 { 0x8E } else { 0x8F }); // SS2 / SS3
                    }
                    iso2022_emit_code(&mut out, code, dim, !seven);
                }
                _ => {
                    out.push(0x1B);
                    out.push(if reg == 2 { 0x6E } else { 0x6F }); // LS2 / LS3
                    gl = reg;
                    iso2022_emit_code(&mut out, code, dim, false);
                }
            }
        }
    }
    if reset_eol && boundary.owns_end_of_stream() {
        reset(&mut out, &mut desig, &mut gl);
    }
    out
}

/// Decode bytes through the ISO-2022 escape-sequence machine to Emacs internal
/// bytes (GNU `decode_coding_iso_2022`, src/coding.c:3200-3990), plus the
/// `(charset NAME)` text-property runs GNU attaches for each maximal run of
/// characters from the same non-ASCII charset.
///
/// STATE is the part of GNU's `coding->spec.iso_2022` that survives a read:
/// the four graphic registers' designations, which register is invoked into
/// GL, and a pending single shift.  GNU keeps it in the process's own
/// `struct coding_system`, so a designation set by one read is still in force
/// in the next; here the caller owns it and hands it in, which is the same
/// lifetime by a different route.
fn decode_via_iso2022(
    bytes: &[u8],
    spec: &crate::emacs_core::coding::Iso2022Spec,
    state: &mut Iso2022DecodeState,
    eol: DosEolLookahead,
) -> DecodedSource {
    use crate::emacs_core::charset::{
        charset_by_iso_final, charset_decode_char, charset_iso2022_designation,
    };
    use crate::emacs_core::coding::IsoFlag;
    let seven = spec.flags.contains(IsoFlag::SevenBits);
    let ascii = intern("ascii");
    let gr: i32 = if seven { -1 } else { 1 };
    state.designate_initially(spec);

    decode_units(bytes, eol, |unit, sink| {
        let b = unit.byte()?;
        // --- Escape sequences ---------------------------------------------
        if b == 0x1B {
            // Every byte of an escape sequence is a `ONE_MORE_BYTE`
            // (src/coding.c:3320-3460), so a sequence cut by the end of a read
            // is left whole for the next one.
            let e = unit.byte()?;
            match e {
                0x4E => {
                    state.single_shift = Some(2);
                    return Ok(());
                }
                0x4F => {
                    state.single_shift = Some(3);
                    return Ok(());
                }
                0x6E => {
                    state.gl = 2;
                    return Ok(());
                }
                0x6F => {
                    state.gl = 3;
                    return Ok(());
                }
                0x24 => {
                    // Multi-byte (dim-2) designations.
                    let i2 = unit.byte()?;
                    let (reg, chars_96, final_byte) = match i2 {
                        0x28..=0x2B => (usize::from(i2 - 0x28), false, unit.byte()?),
                        0x2C..=0x2F => (usize::from(i2 - 0x2C), true, unit.byte()?),
                        // Short form `ESC $ F` (F in @ A B) -> dim-2 94-set, G0.
                        0x40..=0x42 => (0, false, i2),
                        _ => {
                            unit.rewind();
                            unit.byte()?;
                            sink.push(0x1B, None);
                            return Ok(());
                        }
                    };
                    if let Some((cs, _)) = charset_by_iso_final(i64::from(final_byte), 2, chars_96)
                    {
                        state.designation[reg] = Some(cs);
                    }
                    return Ok(());
                }
                0x28..=0x2B => {
                    let reg = usize::from(e - 0x28);
                    let final_byte = unit.byte()?;
                    if let Some((cs, _)) = charset_by_iso_final(i64::from(final_byte), 1, false) {
                        state.designation[reg] = Some(cs);
                    }
                    return Ok(());
                }
                0x2C..=0x2F => {
                    let reg = usize::from(e - 0x2C);
                    let final_byte = unit.byte()?;
                    if let Some((cs, _)) = charset_by_iso_final(i64::from(final_byte), 1, true) {
                        state.designation[reg] = Some(cs);
                    }
                    return Ok(());
                }
                _ => {
                    unit.rewind();
                    unit.byte()?;
                    sink.push(0x1B, None);
                    return Ok(());
                }
            }
        }
        if b == 0x0E {
            state.gl = 1;
            return Ok(());
        }
        if b == 0x0F {
            state.gl = 0;
            return Ok(());
        }
        if !seven && b == 0x8E {
            state.single_shift = Some(2);
            return Ok(());
        }
        if !seven && b == 0x8F {
            state.single_shift = Some(3);
            return Ok(());
        }
        if b < 0x20 || b == 0x7F {
            sink.push(u32::from(b), None);
            return Ok(());
        }
        // --- Graphic character --------------------------------------------
        let reg = if let Some(r) = state.single_shift.take() {
            r
        } else if b < 0x80 {
            state.gl
        } else if gr >= 0 {
            gr as usize
        } else {
            // 7-bit codec: stray high byte -> eight-bit raw (no `charset`).
            sink.push_raw(b);
            return Ok(());
        };
        let charset = state.designation[reg];
        if charset == Some(ascii) {
            sink.push(u32::from(b & 0x7F), None);
            return Ok(());
        }
        if let Some(cs) = charset
            && let Some((_, dim, _)) = charset_iso2022_designation(cs)
        {
            let dim = dim.clamp(1, 2) as usize;
            unit.rewind();
            let position = unit.take(dim)?;
            let mut code = 0i64;
            for byte in position {
                code = (code << 8) | i64::from(byte & 0x7F);
            }
            if let Some(ch) = charset_decode_char(cs, code) {
                sink.push(ch as u32, Some(cs));
                return Ok(());
            }
            return invalid_code(unit, sink);
        }
        if b < 0x80 {
            sink.push(u32::from(b), None);
        } else {
            sink.push_raw(b);
        }
        Ok(())
    })
}

/// Resolve `undecided` / `prefer-utf-8` to a concrete coding system by
/// inspecting the byte pattern, mirroring GNU's `detect_coding`: walk the
/// configured detection categories in priority order and return the first
/// defined coding system whose category accepts the bytes.
fn detect_undecided_coding(
    coding_systems: &crate::emacs_core::coding::CodingSystemManager,
    bytes: &[u8],
    prefer_utf_8: bool,
    block: crate::emacs_core::coding::SourceBlock,
) -> Option<crate::emacs_core::intern::SymId> {
    // `prefer-utf-8` raises UTF-8 ahead of the configured category priority --
    // GNU's `prefer_utf_8 && detect_coding_utf_8 (coding, &detect_info)`
    // (src/coding.c:6619-6620), which is the same detector as everywhere else
    // and therefore tolerates a trailing partial character when this is not the
    // last block.
    if prefer_utf_8 && utf8_or_truncated_utf8(bytes, block) && bytes.iter().any(|&b| b >= 0x80) {
        return Some(intern("utf-8"));
    }

    crate::emacs_core::coding::detect_highest_coding_system_for_unibyte_bytes(
        coding_systems,
        bytes,
        block,
    )
}

/// Valid UTF-8, or valid UTF-8 that stops in the middle of its last character
/// when more bytes may still follow.
///
/// GNU has no separate predicate for this: `detect_coding_utf_8` walks the
/// source and its `no_more_source:` tail rejects a partial trailing character
/// only under `CODING_MODE_LAST_BLOCK` (src/coding.c:1215).  This is that one
/// rule spelled for the `prefer-utf-8` shortcut, which does not go through the
/// category walk.
fn utf8_or_truncated_utf8(bytes: &[u8], block: crate::emacs_core::coding::SourceBlock) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(_) => true,
        Err(error) => {
            !matches!(block, crate::emacs_core::coding::SourceBlock::Last)
                && error.error_len().is_none()
                && std::str::from_utf8(&bytes[..error.valid_up_to()]).is_ok()
        }
    }
}

/// The BOM-detecting arms of GNU's `detect_coding` (src/coding.c:6702-6742).
///
/// Two coding systems are "auto" in a way `undecided` is not: their `:bom`
/// property is a CONS of the two concrete systems the byte-order mark chooses
/// between, and their detection category is `coding_category_utf_8_auto` /
/// `coding_category_utf_16_auto` rather than `coding_category_undecided`.
/// `detect_coding` gives each its own arm, and each arm ends in
/// `setup_coding_system (found, coding)` -- so the coding system a `utf-16`
/// decode actually RUNS with, and reports, is one of the two concrete ones.
/// Measured under GNU 31.0.90:
///
/// ```text
/// (decode-coding-string "\xfe\xff\0a\0\r\0\n\0b" 'utf-16) => last-coding-system-used  utf-16be-with-signature-dos
/// (decode-coding-string "\xff\xfe" ... 'utf-16)           => last-coding-system-used  utf-16le-with-signature-dos
/// (decode-coding-string "\0a\0\r\0\n\0b" 'utf-16)         => last-coding-system-used  utf-16-dos
/// ```
///
/// The third row is the one that says what "found" means: with no BOM,
/// `detect_coding_utf_16` sets neither `CATEGORY_MASK_UTF_16_LE` nor `_BE` --
/// those two flags come ONLY from the two BOM branches (src/coding.c:1513-1526)
/// -- so `found` stays nil and the coding system keeps its own name and its own
/// default (big-endian) byte order.  The `_NOSIG` categories that the pattern
/// scan below those branches can set are a different pair of flags and the auto
/// arm does not look at them.
///
/// Returns `None` when GNU's `found` would be nil, i.e. when nothing re-bases.
fn detect_bom_auto_coding(
    base: &str,
    bytes: &[u8],
    block: crate::emacs_core::coding::SourceBlock,
) -> Option<&'static str> {
    match base {
        // src/coding.c:6724-6742.  `coding->head_ascii = 0` and then
        // `detect_coding_utf_16`, which REFUSES an odd-length last block
        // outright (src/coding.c:1501-1507) before it looks at a signature --
        // measured, `(decode-coding-string "\xfe\xff\0" 'utf-16)` answers
        // `utf-16` and keeps U+FEFF as a character.
        "utf-16"
            if bytes.len() % 2 == 1
                && matches!(block, crate::emacs_core::coding::SourceBlock::Last) =>
        {
            None
        }
        "utf-16" => match bytes {
            [0xFF, 0xFE, ..] => Some("utf-16le-with-signature"),
            [0xFE, 0xFF, ..] => Some("utf-16be-with-signature"),
            _ => None,
        },
        // src/coding.c:6702-6722.  All-ASCII takes `XCDR` without even running
        // the detector; otherwise `detect_coding_utf_8` decides, and a BOM
        // takes `XCAR`.  Either way `found` is non-nil whenever the text is
        // valid UTF-8, so `utf-8-auto` re-bases to a system that is not `auto`.
        "utf-8-auto" => {
            if bytes.iter().all(|byte| byte.is_ascii()) {
                return Some("utf-8");
            }
            if std::str::from_utf8(bytes).is_err() {
                return None;
            }
            if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
                Some("utf-8-with-signature")
            } else {
                Some("utf-8")
            }
        }
        _ => None,
    }
}

/// Re-attach an explicit (concrete) eol_type to a detected base coding system,
/// so a bare EOL alias (`dos`/`mac`/`unix`) still forces its EOL after the
/// character code has been detected (GNU forces the specified eol_type in
/// `decode_eol`).  An undecided eol leaves the name untouched.
fn apply_explicit_eol_suffix(base: &str, eol: crate::emacs_core::coding::EolType) -> String {
    use crate::emacs_core::coding::EolType;
    let base = coding_system_base(base);
    // `no-conversion` is byte-faithful (raw-text-unix); GNU keeps it whole and
    // never appends a DOS/MAC subsidiary, so leave it alone.
    if matches!(eol, EolType::Undecided) || base == "no-conversion" {
        return base.to_string();
    }
    format!("{base}{}", eol.suffix())
}

/// GNU `adjust_coding_eol_type` (src/coding.c:6471-6497) does two things at
/// once: it picks the eol type `decode_eol` converts with, and it replaces
/// `coding->id` with the subsidiary that carries it.  `code_convert_string`
/// then reports that id (`Vlast_coding_system_used = CODING_ID_NAME (coding.id)`,
/// src/coding.c:9622), so a detected end-of-line is observable by NAME as well
/// as by text -- measured under GNU 31.0.90, `(decode-coding-string "a\r\nb\r\n"
/// 'latin-1)` leaves `last-coding-system-used' as `iso-latin-1-dos'.
///
/// Only an UNDECIDED eol type moves the name, and only when the scan found a
/// line terminator at all: `decode_eol` guards the call with
/// `if (eol_seen != EOL_SEEN_NONE)` (src/coding.c:6805), which is why
/// `(decode-coding-string "abc" 'undecided)` still reports plain `undecided'.
fn builtin_coding_string_in_context(
    ctx: &mut crate::emacs_core::eval::Context,
    mut args: Vec<Value>,
    direction: CodingDirection,
    encoding_boundary: EncodingBoundary,
    entry: CodingEntry,
    run: &mut CodingRun<'_>,
) -> EvalResult {
    let name = direction.string_function_name();
    let encode = direction.is_encode();
    expect_min_args(name, &args, 2)?;
    if args.len() > 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol(name), Value::fixnum(args.len() as i64)],
        ));
    }
    let _ = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let mut coding = context_coding_name(ctx, args[1])?;
    // `last-coding-system-used' reports the name the caller passed, not the
    // system the codec resolved to, so keep it before any rewriting below.
    let mut reported_coding = coding.clone();
    // A coding system defined with `define-coding-system-alias' is, in GNU, a
    // KEY in the same hash table as its target pointing at the SAME spec
    // (`Fputhash (alias, spec, Vcoding_system_hash_table)`, src/coding.c), so
    // every lookup resolves it -- encoders included.  Here the codec is chosen
    // by matching the NAME against a static table of built-in systems, which a
    // user-defined alias falls straight through, leaving no codec and encoding
    // non-ASCII as `?`.
    //
    // Resolve only in that fall-through case: when the static table does not
    // recognise the name but the runtime manager does.  Built-in aliases that
    // the static table already handles keep their current path, and `coding`
    // itself is left alone because `last-coding-system-used' must report the
    // alias exactly as the caller wrote it (see
    // `canonical_context_coding_name`), which GNU also does.
    if coding_system_base(&coding) == coding.as_str()
        && let Some(canonical) = ctx.coding_systems.resolve(&coding)
    {
        let canonical = resolve_sym(canonical);
        if canonical != coding.as_str() {
            coding = canonical.to_owned();
            // Downstream helpers re-read `args[1]` rather than `coding` (see the
            // `undecided` rewrite below), so the resolved system has to be put
            // back there too or the codec still selects on the alias name.
            args[1] = Value::symbol(&coding);
        }
    }
    let destination = coding_string_destination(args.get(3).copied())?;
    // GNU `code_convert_string`'s identity fast path (src/coding.c:9609-9628).
    // It runs BEFORE detection and before any conversion, and only for a `Qt`
    // destination -- which is why a BUFFER destination reports the resolved
    // coding system where a returned string reports the argument's.  It also
    // decides the result's multibyteness: `make_multibyte_string` on decode,
    // `make_unibyte_string` on encode (src/coding.c:9625-9627).
    if entry.has_identity_fast_path()
        && destination.is_none()
        && coding_ascii_identity_fast_path(ctx, &coding, &args, encode, ctx.eol_conversion())
    {
        // Nothing was decoded, so nothing was left over: `code_convert_string`
        // returns the argument's own bytes (src/coding.c:9609-9628).
        run.record(SourceConsumed::all(&lisp_string_coding_source_bytes(
            args[0].as_lisp_string().expect("string validated above"),
        )));
        ctx.set_variable(
            "last-coding-system-used",
            Value::symbol(canonical_context_coding_name(ctx, &reported_coding)),
        );
        if coding_string_nocopy(&args) {
            return Ok(args[0]);
        }
        let bytes = lisp_string_coding_source_bytes(
            args[0].as_lisp_string().expect("string validated above"),
        );
        return Ok(Value::heap_string(if encode {
            crate::heap_types::LispString::from_unibyte(bytes)
        } else {
            crate::heap_types::LispString::from_emacs_bytes(bytes)
        }));
    }
    // `undecided` / `prefer-utf-8` decode by detecting the byte pattern and
    // decoding as the resolved concrete coding system (GNU detect_coding).  When
    // the alias carries a CONCRETE eol_type (the bare `dos`/`mac`/`unix`
    // aliases = undecided base + a fixed eol), GNU still detects the character
    // code but forces that eol in `decode_eol` rather than auto-detecting it, so
    // re-attach the explicit EOL suffix to the detected coding system.
    // `if (CODING_REQUIRE_DETECTION (coding)) detect_coding (coding);`
    // (src/coding.c:8129-8130).  A `ProcessRun` entry answers `AlreadyRun`, so
    // the whole block is skipped: its detector has run, at the read, on the
    // carryover plus the read and with the read's own `CODING_MODE_LAST_BLOCK`.
    if let (false, EntryDetection::Here(block)) = (encode, entry.detection()) {
        let base = coding_system_base(&coding).to_owned();
        if (base == "undecided" || base == "prefer-utf-8")
            && let Some(src) = ctx.lisp_string(args[0])
        {
            let bytes = lisp_string_coding_source_bytes(src);
            let detected = detect_undecided_coding(
                &ctx.coding_systems,
                &bytes,
                base == *"prefer-utf-8",
                // `CODING_MODE_LAST_BLOCK` is the entry's to state, not this
                // function's: `code_convert_string` and `code_convert_region`
                // raise it before converting, `decode_coding_gap` detects
                // before raising it.  See `CodingEntry::detection`.
                block,
            )
            .ok_or_else(|| {
                signal(
                    LispCondition::CodingSystemError,
                    vec![Value::symbol(&coding)],
                )
            })?;
            coding = apply_explicit_eol_suffix(resolve_sym(detected), coding_name_eol(&coding));
            // Rewrite the coding argument too, so the fallback decode path
            // (which re-reads args[1]) uses the detected system rather than the
            // original `undecided`/`prefer-utf-8`.
            args[1] = Value::symbol(&coding);
            // GNU stores the DETECTED system when the argument was not fully
            // specified (`undecided`/`prefer-utf-8`), so this rewrite does
            // reach `last-coding-system-used' -- but only when detection found
            // one.  `detect_coding` guards the rewrite with `if (! NILP
            // (found))` (src/coding.c:6743-6754), and leaving `found` nil is
            // the normal outcome for pure-ASCII text; the coding system then
            // keeps the caller's name, which is why GNU answers
            // `prefer-utf-8-dos' rather than `undecided-dos' for
            // `(decode-coding-string "a\r\nb\r\n" 'prefer-utf-8)'.  A detection
            // result of `undecided` is this engine's spelling of GNU's nil.
            if coding_system_base(&coding) != "undecided" {
                reported_coding = coding.clone();
            }
        }
        // The other two "not fully specified" categories: `utf-16` and
        // `utf-8-auto` pick a concrete BASE from the byte-order mark rather
        // than from the category priority list, and they end in the same
        // `setup_coding_system (found, coding)` (src/coding.c:6743-6754).
        // `detect_coding` runs exactly ONE of its three arms, so this is keyed
        // on the ORIGINAL base: a `undecided` decode that detected `utf-16`
        // above does not then get re-based a second time.
        if let Some(src) = ctx.lisp_string(args[0])
            && let Some(found) =
                detect_bom_auto_coding(&base, &lisp_string_coding_source_bytes(src), block)
        {
            coding = apply_explicit_eol_suffix(found, coding_name_eol(&coding));
            args[1] = Value::symbol(&coding);
            reported_coding = coding.clone();
        }
    }
    // Charset-type coding systems (cp437, koi8-r, chinese-gbk, …) encode and
    // decode each character through their charset list, and EUC-profile
    // ISO-2022 coding systems (euc-jp/kr/…) through their G0-G3 designations;
    // the legacy family-based paths leave them empty / undecoded.  Other
    // families keep their existing handling.
    let utf7_coding = utf7_variant(&coding);
    let hz_coding = chinese_hz_charset(ctx, &coding);
    let emacs_mule = is_emacs_mule(ctx, &coding);
    let no_conv_multibyte = coding_system_base(&coding) == "no-conversion-multibyte";
    // utf-8-with-signature and utf-8-auto both write the BOM on encode and
    // strip an optional BOM on decode.
    let utf8_signature = matches!(
        coding_system_base(&coding),
        "utf-8-with-signature" | "utf-8-auto"
    );
    let full_iso = full_iso2022_spec(ctx, &coding);
    let ccl_coding = runtime_ccl_spec(ctx, &coding);
    let euc_coding = euc_iso2022_spec(ctx, &coding);
    let sjis_coding = sjis_charsets(ctx, &coding);
    let charset_coding = general_charset_coding_list(ctx, &coding, encode);
    // GNU runs a coding system's :pre-write-conversion before encoding and its
    // :post-read-conversion after decoding (src/coding.c encode_coding_object /
    // decode_coding_object).  These elisp hooks do the real character
    // translation for mnemonic codings such as `vietnamese-viqr` (base
    // :coding-type utf-8).  The dedicated codec branches above (utf-7, hz,
    // emacs-mule, …) reimplement their conversion entirely in Rust and already
    // subsume any hook, so only run a hook when none of them claim this coding.
    let dedicated_codec = utf7_coding.is_some()
        || hz_coding.is_some()
        || emacs_mule
        || no_conv_multibyte
        || utf8_signature
        || ccl_coding.is_some()
        || full_iso.is_some()
        || euc_coding.is_some()
        || sjis_coding.is_some()
        || charset_coding.is_some();
    // A coding system whose conversion is implemented by an elisp
    // :pre-write-conversion / :post-read-conversion hook (e.g. vietnamese-viqr)
    // is handled entirely here; the generic family encoders below do not know
    // how to encode it (its family is "unknown") and would drop every
    // character.
    if !dedicated_codec
        && let Some((hook, base_type)) = coding_conversion_hook(ctx, &coding, encode)
    {
        // GNU's `code_convert_string` takes an identity fast path for an
        // ASCII-compatible coding when the input is pure ASCII and needs no EOL
        // conversion, returning the string unchanged WITHOUT running
        // encode/decode_coding_object — and therefore WITHOUT the pre/post
        // conversion hook.  So a pure-ASCII `(encode-coding-string "cafe'"
        // 'vietnamese-viqr)` is `"cafe'"`, not the VIQR-translated `café`.
        //
        // It is `code_convert_string`'s fast path and nobody else's
        // (src/coding.c:9609-9628), which is why the ENTRY has to be asked
        // rather than only the bytes.  `decode_coding_gap` refuses the
        // analogous ASCII optimization outright when a `:post-read-conversion`
        // exists (`NILP (CODING_ATTR_POST_READ (attrs))` at :7933), and
        // `code_convert_region` has no such path at all, so both of those run
        // the hook.  Measured under GNU Emacs 31.0.90 on the pure-ASCII VIQR
        // source `Vie^.t Nam a` e^``:
        //
        //   (decode-coding-string ... 'vietnamese-viqr) => the source, unchanged
        //   (decode-coding-region ... 'vietnamese-viqr) => Việt Nam à ề
        //   insert-file-contents with that coding       => Việt Nam à ề
        let result = if entry.has_identity_fast_path()
            && coding_ascii_identity_fast_path(ctx, &coding, &args, encode, ctx.eol_conversion())
        {
            // Identity: encode yields a unibyte string, decode a multibyte one
            // (GNU `make_unibyte_string` / `make_multibyte_string`).  The bytes
            // are pure ASCII, so the two storage forms coincide.  Nothing was
            // decoded, so `decode_eol` never ran and there is no resolution to
            // report -- which is why GNU's `decode-coding-string` answers plain
            // `vietnamese-viqr' where every other door answers
            // `vietnamese-viqr-unix'.
            let bytes = lisp_string_coding_source_bytes(
                args[0].as_lisp_string().expect("string validated above"),
            );
            run.record(SourceConsumed::all(&bytes));
            if encode {
                Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
            } else {
                Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(bytes))
            }
        } else {
            let (converted, resolution) = run_coding_with_conversion_hook(
                ctx,
                &args,
                &base_type,
                hook,
                encode,
                coding_name_eol(&coding),
                run,
            )?;
            if let Some(resolution) = resolution {
                // GNU `adjust_coding_eol_type` (src/coding.c:6471) rewrites
                // `coding->id`, and the id is what every door but the string
                // one reports.
                reported_coding =
                    adjusted_coding_name(&ctx.coding_systems, &reported_coding, resolution);
            }
            converted
        };
        let result_text = result
            .as_lisp_string()
            .ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), result],
                )
            })?
            .clone();
        ctx.set_variable(
            "last-coding-system-used",
            Value::symbol(canonical_context_coding_name(ctx, &reported_coding)),
        );
        let Some(buffer_id) = destination else {
            return Ok(result);
        };
        let restore_point = ctx.buffers.get(buffer_id).map(|buf| buf.point_anchor());
        if restore_point.is_none() {
            return Err(signal(
                "error",
                vec![Value::string("Selecting deleted buffer")],
            ));
        }
        insert_coding_result(ctx, buffer_id, &result_text, restore_point)?;
        return Ok(Value::fixnum(result_text.schars() as i64));
    }
    // The single EOL pass for the codec chain below, mirroring GNU
    // `consume_chars` (src/coding.c:7607, eol block at :7683): the newline is
    // expanded ONCE into the source that every codec then reads, and the coding
    // name handed to the codecs has its EOL leg spent.  None of the arms below
    // performs EOL conversion, and none of them can silently omit it either --
    // the choice is no longer theirs to make.
    if encode {
        let source = args[0]
            .as_lisp_string()
            .expect("string argument validated above");
        if let Some(expanded) = expand_source_eol(source, &coding, ctx.eol_conversion()) {
            args[0] = Value::heap_string(expanded);
            coding = coding_name_with_eol_spent(&coding).into_owned();
            // The fall-through arm re-reads `args[1]`, so the name has to be
            // spent there too or `encode_lisp_string` would expand a second
            // time and emit CR CR LF.
            args[1] = Value::symbol(&coding);
            // NOCOPY promises the caller's own string object back.  A source
            // that needed EOL expansion is a fresh object already, so the
            // promise no longer applies and the identity fast path downstream
            // must not fire on the expanded bytes.
            if let Some(nocopy) = args.get_mut(2) {
                *nocopy = Value::NIL;
            }
        }
    }
    // The decode-side mirror, and the same single-pass rule: GNU's
    // `decode_coding` (src/coding.c:7481) runs the decoder and then calls
    // `decode_eol` once, outside every `decode_coding_*`.  The pass has to be
    // AFTER the decoder, not before it: in UTF-16 a CR is the byte pair 0D 00,
    // so collapsing CR LF in the SOURCE bytes would never see it.  The name is
    // spent up front so the arms and the fall-through cannot collapse a second
    // time -- decoding is not idempotent ("\r\r\n\n" collapses twice).
    let decoded_eol = if encode {
        crate::emacs_core::coding::EolType::Unix
    } else {
        coding_name_eol(&coding)
    };
    if !encode && !matches!(decoded_eol, crate::emacs_core::coding::EolType::Unix) {
        let spent = coding_name_with_eol_spent(&coding);
        if spent != coding.as_str() {
            coding = spent.into_owned();
            args[1] = Value::symbol(&coding);
        }
    }
    let source_string = || {
        args[0]
            .as_lisp_string()
            .expect("string argument validated above")
            .clone()
    };
    let result = if encode {
        if let Some(imap) = utf7_coding {
            let bytes = encode_via_utf7(&source_string(), imap);
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else if let Some(gb2312) = hz_coding {
            let bytes = encode_via_hz(&source_string(), gb2312);
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else if emacs_mule {
            let bytes = encode_via_emacs_mule(&source_string());
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else if no_conv_multibyte {
            let bytes = encode_utf8_plain(&source_string());
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else if utf8_signature {
            let mut bytes = vec![0xEF, 0xBB, 0xBF];
            bytes.extend(encode_utf8_plain(&source_string()));
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else if let Some(spec) = ccl_coding {
            let bytes = encode_via_ccl(&source_string(), spec, &coding, encoding_boundary)?;
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else if let Some((spec, charsets)) = &full_iso {
            let bytes = encode_via_iso2022(&source_string(), spec, charsets, encoding_boundary);
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else if let Some((spec, charsets)) = &euc_coding {
            let bytes = encode_via_euc(&source_string(), spec, charsets);
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else if let Some(charsets) = &sjis_coding {
            let bytes = encode_via_sjis(&source_string(), charsets);
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else if let Some(charset_list) = &charset_coding {
            let bytes = encode_via_charset_list(&source_string(), charset_list);
            Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
        } else {
            builtin_encode_coding_string_with_known(args, |_| true, ctx.eol_conversion())?
        }
    } else {
        // GNU's decoders, each of which reports `coding->consumed` for itself;
        // see `decode_units`.  The end-of-line lookahead is passed in rather
        // than derived from the name because the name's EOL leg was spent
        // above -- GNU's decoders read it off `CODING_ID_EOL_TYPE (coding->id)`
        // before the same conversion happens (src/coding.c:1250-1251).
        let dos_eol = if matches!(decoded_eol, crate::emacs_core::coding::EolType::Dos)
            && ctx.eol_conversion() == crate::emacs_core::coding::EolConversion::Enabled
        {
            DosEolLookahead::Required
        } else {
            DosEolLookahead::NotRequired
        };
        let mut scratch = CodingDecoderState::default();
        let source_bytes = lisp_string_coding_source_bytes(&source_string());
        let decoded = if let Some(imap) = utf7_coding {
            // GNU implements utf-7 in Lisp, as a `:post-read-conversion` over
            // `raw-text` (lisp/international/utf-7.el), so the C decoder under
            // it consumes every byte and each chunk is converted on its own.
            // Measured: a child writing `a+MEI` and then `-b` gives GNU the
            // characters `a U+3042` and then the LITERAL `-b`.
            Some(DecodedSource::whole(
                decode_via_utf7(&source_bytes, imap),
                &source_bytes,
            ))
        } else if let Some(gb2312) = hz_coding {
            // The same, through lisp/language/china-util.el.
            Some(DecodedSource::whole(
                decode_via_hz(&source_bytes, gb2312),
                &source_bytes,
            ))
        } else if emacs_mule {
            Some(decode_via_emacs_mule(&source_bytes, dos_eol))
        } else if no_conv_multibyte {
            Some(DecodedSource::whole(
                decode_raw_text_multibyte(&source_bytes),
                &source_bytes,
            ))
        } else if utf8_signature {
            let mut src = source_bytes.clone();
            let signature = if src.starts_with(&[0xEF, 0xBB, 0xBF]) {
                src.drain(..3);
                3
            } else {
                0
            };
            let mut decoded = decode_via_utf8(&src, dos_eol);
            decoded.consumed = SourceConsumed(signature + decoded.consumed.bytes());
            Some(decoded)
        } else if let Some(spec) = ccl_coding {
            // A CCL program decides for itself how many bytes it reads, and
            // this port drives it to the end of the source; GNU's
            // `decode_coding_ccl` does report `coding->consumed`
            // (src/coding.c:5185), so a CCL coding system across a subprocess
            // read boundary is the one decoder still owed an answer.
            Some(DecodedSource::whole(
                decode_via_ccl(&source_bytes, spec, &coding)?,
                &source_bytes,
            ))
        } else if let Some((spec, _)) = &full_iso {
            Some(decode_via_iso2022(
                &source_bytes,
                spec,
                run.decoder_state(&mut scratch).iso_2022_mut(),
                dos_eol,
            ))
        } else if let Some((spec, _)) = &euc_coding {
            Some(decode_via_euc(&source_bytes, spec, dos_eol))
        } else if let Some(charsets) = &sjis_coding {
            Some(decode_via_sjis(&source_bytes, charsets, dos_eol))
        } else if let Some(charset_list) = &charset_coding {
            Some(decode_via_charset_list(
                &source_bytes,
                charset_list,
                dos_eol,
            ))
        } else if let Some(decoded) = decode_fallthrough_source(&source_bytes, &coding, dos_eol) {
            Some(decoded)
        } else {
            None
        };
        match decoded {
            Some(mut decoded) => {
                run.record(decoded.consumed);
                let tail = &source_bytes[decoded.consumed.bytes()..];
                if run.flushes_last_block() {
                    // GNU `decode_coding`'s `CODING_MODE_LAST_BLOCK` arm
                    // (src/coding.c:7434-7462): what no decoder could consume
                    // is produced as characters, ASCII as itself and anything
                    // else as an eight-bit character.
                    flush_last_block_into(&mut decoded.bytes, tail);
                }
                decoded_string_with_charset_runs(decoded.bytes, decoded.runs)
            }
            // Every remaining coding system is one in which each byte is a
            // character (the ISO-8859 charsets, `raw-text`, `binary`), so its
            // decoder cannot stop short and `coding->consumed` is the whole
            // source by construction rather than by default.
            None => {
                run.record(SourceConsumed::all(&source_bytes));
                builtin_decode_coding_string_with_known(args, |_| true, ctx.eol_conversion())?
            }
        }
    };
    // The single decode-side pass (GNU `decode_coding`, src/coding.c:7481).
    // `for_decode` is where GNU's `decode_eol` resolves a VECTOR eol_type by
    // scanning the text the decoder just produced (src/coding.c:6785-6806) --
    // which is why the scan happens here, on `decoded`, and not on the source
    // bytes: in UTF-16 a CR is the byte pair 0D 00.
    let result = match ctx.lisp_string(result) {
        Some(decoded) => {
            let resolution =
                decoded_eol.resolve_for_decode(decoded.as_bytes(), ctx.eol_conversion());
            // GNU's `adjust_coding_eol_type` (src/coding.c:6471) picks the eol
            // type the conversion runs with AND rewrites `coding->id` in ONE
            // call, and `code_convert_string` reports that id
            // (src/coding.c:9497 for a region, :9644 for a string) -- so an
            // undecided eol that resolved moves
            // `last-coding-system-used' even when it resolved to unix.  Both
            // halves come out of the single `resolution` here; deriving the
            // name from a second scan of the text is how the two can disagree,
            // which is exactly what `inhibit-eol-conversion' would have made
            // them do.
            reported_coding =
                adjusted_coding_name(&ctx.coding_systems, &reported_coding, resolution);
            // Only a dos/mac result rebuilds the string; a unix one is a no-op
            // and must keep the decoded object as it is, text properties (the
            // source-charset runs) included.
            if matches!(
                resolution.eol(),
                crate::emacs_core::coding::ResolvedEol::Unix
            ) {
                result
            } else {
                let bytes = decode_eol_bytes(decoded.as_bytes(), resolution.eol());
                let multibyte = decoded.is_multibyte();
                if multibyte {
                    Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(bytes))
                } else {
                    Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes))
                }
            }
        }
        None => result,
    };
    let result_text = result
        .as_lisp_string()
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), result],
            )
        })?
        .clone();
    ctx.set_variable(
        "last-coding-system-used",
        Value::symbol(canonical_context_coding_name(ctx, &reported_coding)),
    );

    let Some(buffer_id) = destination else {
        return Ok(result);
    };
    let restore_point = ctx.buffers.get(buffer_id).map(|buf| buf.point_anchor());
    if restore_point.is_none() {
        return Err(signal(
            "error",
            vec![Value::string("Selecting deleted buffer")],
        ));
    }
    insert_coding_result(ctx, buffer_id, &result_text, restore_point)?;
    Ok(Value::fixnum(result_text.schars() as i64))
}

pub(crate) fn builtin_encode_coding_string_in_context(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_coding_string_in_context(
        ctx,
        args,
        CodingDirection::Encode,
        EncodingBoundary::CompleteText,
        CodingEntry::CodeConvertString,
        &mut CodingRun::complete_source(),
    )
}

pub(crate) fn builtin_decode_coding_string_in_context(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_coding_string_in_context(
        ctx,
        args,
        CodingDirection::Decode,
        EncodingBoundary::CompleteText,
        CodingEntry::CodeConvertString,
        &mut CodingRun::complete_source(),
    )
}

/// An externally encoded byte stream together with the concrete coding system
/// that produced it.
pub(crate) struct EncodedTextBytes {
    pub(crate) bytes: Vec<u8>,
    pub(crate) coding_system: crate::emacs_core::intern::SymId,
}

/// A coding-system symbol that must be interpreted by the current evaluator.
///
/// The inner symbol stays private so external byte consumers cannot
/// accidentally turn a runtime coding protocol back into a name and feed it
/// to the context-free family encoder.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RuntimeCodingSystem(crate::emacs_core::intern::SymId);

impl RuntimeCodingSystem {
    pub(crate) fn from_symbol(symbol: crate::emacs_core::intern::SymId) -> Self {
        Self(symbol)
    }
}

/// Encode Lisp text through the complete runtime coding engine.
///
/// External byte consumers must use this seam instead of the context-free
/// `encode_lisp_string` family switch: ISO-2022, EUC, Shift-JIS, charset
/// codings, and conversion hooks depend on definitions stored in the current
/// evaluator.  `RuntimeCodingSystem` preserves that requirement across the
/// call boundary.  The result also carries GNU's concrete
/// `last-coding-system-used` value, so callers can report aliases and EOL
/// subsidiaries consistently.
fn encode_external_text_with_boundary(
    ctx: &mut crate::emacs_core::eval::Context,
    text: crate::heap_types::LispString,
    coding: RuntimeCodingSystem,
    boundary: EncodingBoundary,
) -> Result<EncodedTextBytes, crate::emacs_core::error::Flow> {
    let encoded = builtin_coding_string_in_context(
        ctx,
        vec![Value::heap_string(text), Value::symbol(coding.0)],
        CodingDirection::Encode,
        boundary,
        // File I/O is GNU's `encode_coding_object` entry, not
        // `code_convert_string`: no identity fast path.
        CodingEntry::FileGap,
        &mut CodingRun::complete_source(),
    )?;
    let bytes = encoded
        .as_lisp_string()
        .expect("encode-coding-string must return a Lisp string")
        .as_bytes()
        .to_vec();
    let coding_system = ctx
        .visible_variable_value_or_nil("last-coding-system-used")
        .as_symbol_id()
        .filter(|&symbol| symbol != intern("nil"))
        .expect("a successful context-aware encode records its concrete coding system");
    Ok(EncodedTextBytes {
        bytes,
        coding_system,
    })
}

pub(crate) fn encode_external_text_in_context(
    ctx: &mut crate::emacs_core::eval::Context,
    text: crate::heap_types::LispString,
    coding: RuntimeCodingSystem,
) -> Result<EncodedTextBytes, crate::emacs_core::error::Flow> {
    encode_external_text_with_boundary(ctx, text, coding, EncodingBoundary::CompleteText)
}

/// Encode the text passed to GNU's `write-region`/`e_write` path.
///
/// Unlike complete string or region conversion, this operation does not claim
/// the final coding block. Stateful encoders therefore preserve their ending
/// designation instead of appending a synthetic stream terminator.
pub(crate) fn encode_file_region_in_context(
    ctx: &mut crate::emacs_core::eval::Context,
    text: crate::heap_types::LispString,
    coding: RuntimeCodingSystem,
) -> Result<EncodedTextBytes, crate::emacs_core::error::Flow> {
    encode_external_text_with_boundary(ctx, text, coding, EncodingBoundary::FileRegion)
}

/// A file decode result whose coding-system selection remains an interned
/// protocol value until the file-I/O boundary needs its Lisp name.
pub(crate) struct DecodedFileBytes {
    pub(crate) text: crate::heap_types::LispString,
    pub(crate) coding_system: crate::emacs_core::intern::SymId,
}

/// Decode raw file bytes through the complete context-aware coding engine.
///
/// File I/O must use this seam rather than the context-free `decode_bytes`
/// family switch: ISO-2022, EUC, Shift-JIS, charset codings, and conversion
/// hooks depend on coding-system definitions stored in the current evaluator.
/// Encapsulate GNU's `last-coding-system-used` protocol here and return the
/// concrete interned coding-system symbol together with the decoded text, so
/// file-I/O callers cannot accidentally report the unresolved input alias.
pub(crate) fn decode_file_bytes_in_context(
    ctx: &mut crate::emacs_core::eval::Context,
    bytes: &[u8],
    coding: &str,
) -> Result<DecodedFileBytes, crate::emacs_core::error::Flow> {
    let source = Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes.to_vec()));
    let decoded = builtin_coding_string_in_context(
        ctx,
        vec![source, Value::symbol(coding)],
        CodingDirection::Decode,
        EncodingBoundary::CompleteText,
        // `insert-file-contents` reaches `decode_coding_gap` (src/coding.c:7905),
        // so it never takes `code_convert_string`'s identity fast path, always
        // reports the resolved coding system -- including the end-of-line
        // subsidiary `decode_eol` detected -- and detects with
        // `CODING_MODE_LAST_BLOCK` still CLEAR.
        CodingEntry::FileGap,
        &mut CodingRun::complete_source(),
    )?;
    let text = decoded
        .as_lisp_string()
        .expect("decode-coding-string must return a Lisp string")
        .clone();
    let coding_system = ctx
        .visible_variable_value_or_nil("last-coding-system-used")
        .as_symbol_id()
        .filter(|&symbol| symbol != intern("nil"))
        .expect("a successful context-aware decode records its concrete coding system");
    Ok(DecodedFileBytes {
        text,
        coding_system,
    })
}

/// Decode one run of a subprocess's output through the SAME engine
/// `decode-coding-string`, `decode-coding-region` and `insert-file-contents`
/// go through.
///
/// GNU has no restricted process decoder to mirror.  Its three subprocess doors
/// -- `read_and_insert_process_output` (src/process.c:6502), the filter branch
/// of `read_and_dispose_of_process_output` (:6562) and `Fcall_process`
/// (src/callproc.c:856) -- all decode by expanding the `decode_coding_c_string`
/// macro, whose body is `decode_coding_object (coding, Qnil, 0, 0, bytes,
/// bytes, dst_object)` (src/coding.h:750-755): the same C function
/// `code_convert_string` calls.  So a subprocess gets ISO-2022, `emacs-mule`,
/// Shift-JIS, GBK, CCL and the charset codings, and it runs the coding system's
/// `:post-read-conversion` (src/coding.c:8180-8194), because there is nowhere
/// else for those to live.
///
/// CODING has already been through [`detected_coding_name`]; see
/// [`CodingEntry::ProcessRun`] for why detection cannot be deferred to here.
///
/// This function takes a `&mut Context` and that is the whole point of the
/// [`crate::emacs_core::process::PendingProcessRun`] hand-off: the evaluator
/// owns the `ProcessManager`, so the read has to have let go of the process
/// before this can be called.
pub(crate) fn decode_process_run_in_context(
    ctx: &mut crate::emacs_core::eval::Context,
    bytes: &[u8],
    coding: &'static str,
    state: &mut CodingDecoderState,
    block: crate::emacs_core::coding::SourceBlock,
) -> Result<DecodedProcessRun, crate::emacs_core::error::Flow> {
    let source = Value::heap_string(crate::heap_types::LispString::from_unibyte(bytes.to_vec()));
    let mut run = CodingRun::process_read(state, block);
    let decoded = builtin_coding_string_in_context(
        ctx,
        vec![source, Value::symbol(coding)],
        CodingDirection::Decode,
        EncodingBoundary::CompleteText,
        CodingEntry::ProcessRun,
        &mut run,
    )?;
    let carryover_bytes = run.carryover_bytes(bytes.len());
    let text = decoded
        .as_lisp_string()
        .expect("decode-coding-string must return a Lisp string")
        .clone();
    // GNU never has to ask: the decode ran through the process's own
    // `struct coding_system`, so `CODING_ID_NAME (coding->id)` IS the answer
    // (src/process.c:6421).  Here each run is a separate call, so the name has
    // to be read back out of the variable the shared engine records it in --
    // the same variable `read_process_output_set_last_coding_system` writes.
    let used = ctx
        .visible_variable_value_or_nil("last-coding-system-used")
        .as_symbol_id()
        .filter(|&symbol| symbol != intern("nil"))
        .expect("a successful context-aware decode records its concrete coding system");
    Ok(DecodedProcessRun {
        text,
        used: resolve_sym(used),
        carryover: bytes[bytes.len() - carryover_bytes..].to_vec(),
    })
}

/// Reconcile a coding result string with the multibyteness of the buffer it is
/// about to be stored into.  GNU `decode_coding_object`/`encode_coding_object`
/// set `coding->dst_multibyte` from the destination buffer's
/// `enable-multibyte-characters` (coding.c:8153): decoding into a *unibyte*
/// buffer stores each decoded character's internal byte sequence
/// (`str_as_unibyte`), and encoding into a *multibyte* buffer turns raw bytes
/// into characters (`str_as_multibyte`).  Storing a multibyte string straight
/// into a unibyte buffer would otherwise truncate each character to one byte,
/// which is what breaks the `tit-dic-convert` LEIM idiom
/// (`set-buffer-multibyte nil` / `decode-coding-region` / `set-buffer-multibyte t`).
fn coding_result_for_buffer_multibyte(
    text: &crate::heap_types::LispString,
    target_multibyte: bool,
) -> crate::heap_types::LispString {
    if text.is_multibyte() == target_multibyte {
        return text.clone();
    }
    let mut converted = if target_multibyte {
        // Encoded bytes stored into a multibyte buffer become eight-bit
        // characters (GNU `string-to-multibyte` / BYTE8_STRING), NOT re-parsed
        // as a UTF-8 sequence (`string-as-multibyte`), which would wrongly turn
        // e.g. the UTF-8 encoding of "é" back into the character "é".
        crate::heap_types::LispString::from_emacs_bytes(
            crate::emacs_core::emacs_char::str_to_multibyte(text.as_bytes()),
        )
    } else {
        crate::heap_types::LispString::from_unibyte(crate::emacs_core::emacs_char::str_as_unibyte(
            text.as_bytes(),
        ))
    };
    // Changing the multibyteness only re-encodes the bytes; the character count
    // is unchanged, so the (char-indexed) text-property intervals — e.g. the
    // `charset` property a charset-type coding (latin-1) attaches at decode time
    // — remain valid and must ride along into the buffer.  This is the
    // `decode-coding-region`/`decode-coding-string` path; GNU keeps the charset
    // annotation across `code_convert_region`'s dst multibyteness adjustment.
    if text.has_intervals() {
        let intervals = text.intervals().clone();
        if !intervals.is_empty() {
            *converted.intervals_mut() = intervals;
        }
    }
    converted
}

fn builtin_coding_region(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
    direction: CodingDirection,
) -> EvalResult {
    let name = direction.region_function_name();
    expect_args_range(name, &args, 3, 4)?;

    let coding = context_coding_name(ctx, args[2])?;
    let mut destination = coding_region_destination(args.get(3).copied())?;
    // A DESTINATION that is the current buffer means in-place conversion
    // (GNU `code_convert_region` with dst_object == the source buffer), not an
    // insertion that would duplicate the text.
    if let Some(Some(dest_id)) = destination
        && Some(dest_id) == ctx.buffers.current_buffer_id()
    {
        destination = Some(None);
    }
    let Some(byte_range) =
        crate::emacs_core::editfns::current_buffer_accessible_char_region_in_buffers(
            &ctx.buffers,
            &args[0],
            &args[1],
        )?
    else {
        return Ok(Value::NIL);
    };
    let Some(current_id) = ctx.buffers.current_buffer_id() else {
        return Ok(Value::NIL);
    };
    let source = ctx
        .buffers
        .get(current_id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?
        .buffer_substring_lisp_string_range(byte_range);
    let start_byte = byte_range.start().get();
    let end_byte = byte_range.end().get();
    let result = transformed_region_string_in_context(ctx, source, &coding, direction)?;
    let result_text = result
        .as_lisp_string()
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), result],
            )
        })?
        .clone();
    // `last-coding-system-used' is NOT re-set here.  GNU's `code_convert_region`
    // records `CODING_ID_NAME (coding.id)` (src/coding.c:9497) -- the coding
    // system as the conversion left it, which for an undecided end-of-line type
    // is the subsidiary `decode_eol` detected (src/coding.c:6806).  The inner
    // conversion above already wrote that name; overwriting it with the
    // ARGUMENT's name here reported `undecided' where GNU reports
    // `undecided-dos'.
    match destination {
        None => Ok(result),
        Some(None) => {
            crate::emacs_core::editfns::ensure_current_buffer_writable_in_state(
                &ctx.obarray,
                &[],
                &ctx.buffers,
            )?;
            // GNU sets `coding->dst_multibyte` from the destination buffer, so
            // the stored text must match the buffer's multibyteness
            // (coding.c:8153).
            let target_multibyte = ctx
                .buffers
                .get(current_id)
                .map(|buf| buf.get_multibyte())
                .unwrap_or(true);
            let stored = coding_result_for_buffer_multibyte(&result_text, target_multibyte);
            let produced_chars = stored.schars();
            // Capture point geometry BEFORE the replacement so we can mirror
            // GNU's in-place point restoration (coding.c
            // `decode_coding_object`/`encode_coding_object`, the `saved_pt >= 0`
            // block).  All three are 1-based char positions.
            let (saved_pt, from_char, chars) = ctx
                .buffers
                .get(current_id)
                .map(|buf| {
                    let saved_pt = buf.point_lisp_char_pos().as_i64();
                    let from_char = buf
                        .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(start_byte))
                        .as_i64();
                    let to_char = buf
                        .emacs_byte_pos_to_lisp_char_pos(EmacsBytePos::new(end_byte))
                        .as_i64();
                    (saved_pt, from_char, to_char - from_char)
                })
                .unwrap_or((1, 1, 0));
            crate::emacs_core::fns::replace_buffer_emacs_byte_range_lisp_string(
                ctx,
                current_id,
                EmacsByteRange::new(EmacsBytePos::new(start_byte), EmacsBytePos::new(end_byte)),
                &stored,
            )?;
            // GNU restores PT after replacing the region in place:
            //   saved_pt < from               -> point unchanged (before region)
            //   from <= saved_pt < from+chars  -> point moves to the region START
            //   saved_pt >= from+chars         -> point shifts by the size delta
            // (coding.c, the `if (saved_pt >= 0)` branch).  Without this the
            // del+insert replacement leaves point at the region END.
            let new_pt = if saved_pt < from_char {
                saved_pt
            } else if saved_pt < from_char + chars {
                from_char
            } else {
                saved_pt + (produced_chars as i64 - chars)
            };
            if let Some(buf) = ctx.buffers.get(current_id) {
                let byte_pos = buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(new_pt));
                let _ = ctx.buffers.goto_buffer_emacs_byte_pos(current_id, byte_pos);
            }
            Ok(Value::fixnum(produced_chars as i64))
        }
        Some(Some(buffer_id)) => {
            let restore_point = ctx.buffers.get(buffer_id).map(|buf| buf.point_anchor());
            if restore_point.is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Selecting deleted buffer")],
                ));
            }
            let target_multibyte = ctx
                .buffers
                .get(buffer_id)
                .map(|buf| buf.get_multibyte())
                .unwrap_or(true);
            let stored = coding_result_for_buffer_multibyte(&result_text, target_multibyte);
            let produced_chars = stored.schars();
            insert_coding_result(ctx, buffer_id, &stored, restore_point)?;
            Ok(Value::fixnum(produced_chars as i64))
        }
    }
}

pub(crate) fn builtin_encode_coding_region(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_coding_region(ctx, args, CodingDirection::Encode)
}

pub(crate) fn builtin_decode_coding_region(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_coding_region(ctx, args, CodingDirection::Decode)
}

/// `(char-width CHAR)` -> integer without a current display table.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_char_width(args: Vec<Value>) -> EvalResult {
    builtin_char_width_with_display_table(None, args)
}

/// `(char-width CHAR)` -> integer in the current buffer.
pub(crate) fn builtin_char_width_in_context(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    // GNU `CHARACTER_WIDTH` (buffer.h) returns `SANE_TAB_WIDTH (current_buffer)'
    // for a TAB, i.e. the buffer-local `tab-width' (not a hardcoded constant).
    // `char-width' must reflect this so e.g. overwrite-mode's tab handling and
    // column math agree with GNU.  Only short-circuit when no display table
    // remaps TAB; otherwise fall through to the display-table-aware path.
    if matches!(
        args.first().map(|v| v.kind()),
        Some(ValueKind::Fixnum(0x09))
    ) && active_display_table(ctx).is_none()
    {
        let width = crate::emacs_core::indent::current_buffer_tab_width(ctx);
        return Ok(Value::fixnum(width as i64));
    }
    builtin_char_width_with_display_table(active_display_table(ctx), args)
}

fn builtin_char_width_with_display_table(
    display_table: Option<Value>,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("char-width", &args, 1)?;
    let code = match args[0].kind() {
        ValueKind::Fixnum(c) => c,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), args[0]],
            ));
        }
    };
    if !(0..=MAX_CHAR_CODE).contains(&code) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), Value::fixnum(code)],
        ));
    }
    let width = char_width_for_code_with_display_table(code, display_table);
    Ok(Value::fixnum(width as i64))
}

/// `(string-bytes STRING)` -> integer byte length of STRING.
pub(crate) fn builtin_string_bytes(args: Vec<Value>) -> EvalResult {
    expect_args("string-bytes", &args, 1)?;
    let string = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    Ok(Value::fixnum(string.sbytes() as i64))
}

/// `(multibyte-string-p STRING)` -> t or nil
pub(crate) fn builtin_multibyte_string_p(args: Vec<Value>) -> EvalResult {
    expect_args("multibyte-string-p", &args, 1)?;
    match args[0].kind() {
        ValueKind::String => Ok(Value::bool_val(args[0].string_is_multibyte())),
        _ => Ok(Value::NIL),
    }
}

/// `(unibyte-string-p STRING)` -> t or nil
#[cfg(test)]
pub(crate) fn builtin_unibyte_string_p(args: Vec<Value>) -> EvalResult {
    expect_args("unibyte-string-p", &args, 1)?;
    match args[0].kind() {
        ValueKind::String => Ok(Value::bool_val(!args[0].string_is_multibyte())),
        _ => Ok(Value::NIL),
    }
}

/// `(encode-coding-string STRING CODING-SYSTEM)` -> string, with end-of-line
/// conversion ENABLED.
///
/// Test-only, and `#[cfg(test)]` for the reason entry 143 records: an
/// un-parameterised spelling of a coding conversion is a spelling that has
/// decided what `inhibit-eol-conversion' holds without asking.  Production code
/// reaches the codec through `builtin_encode_coding_string_with_known`, which
/// cannot be called without saying.
#[cfg(test)]
pub(crate) fn builtin_encode_coding_string(args: Vec<Value>) -> EvalResult {
    builtin_encode_coding_string_with_known(
        args,
        known_coding_system,
        crate::emacs_core::coding::EolConversion::Enabled,
    )
}

pub(crate) fn builtin_encode_coding_string_with_known(
    args: Vec<Value>,
    known: impl FnOnce(&str) -> bool,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> EvalResult {
    expect_min_args("encode-coding-string", &args, 2)?;
    if args.len() > 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("encode-coding-string"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let string = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let coding = match args[1].kind() {
        ValueKind::Nil => {
            return if coding_string_nocopy(&args) {
                Ok(args[0])
            } else {
                copy_lisp_string_value(args[0])
            };
        }
        ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[1]],
            ));
        }
    };
    validate_coding_system(&coding, args[1], known)?;
    if coding_string_nocopy(&args)
        && coding_string_trivial_ascii_nocopy(string.as_bytes(), &coding, true)
    {
        return Ok(args[0]);
    }
    let bytes = encode_lisp_string(string, &coding, eol_conversion);
    Ok(Value::heap_string(
        crate::heap_types::LispString::from_unibyte(bytes),
    ))
}

/// `(decode-coding-string STRING CODING-SYSTEM)` -> string, with end-of-line
/// conversion ENABLED.  Test-only; see
/// [`builtin_encode_coding_string`] for why it is `#[cfg(test)]`.
#[cfg(test)]
pub(crate) fn builtin_decode_coding_string(args: Vec<Value>) -> EvalResult {
    builtin_decode_coding_string_with_known(
        args,
        known_coding_system,
        crate::emacs_core::coding::EolConversion::Enabled,
    )
}

pub(crate) fn builtin_decode_coding_string_with_known(
    args: Vec<Value>,
    known: impl FnOnce(&str) -> bool,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> EvalResult {
    expect_min_args("decode-coding-string", &args, 2)?;
    if args.len() > 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("decode-coding-string"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    let s = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let coding = match args[1].kind() {
        ValueKind::Nil => {
            return if coding_string_nocopy(&args) {
                Ok(args[0])
            } else {
                copy_lisp_string_value(args[0])
            };
        }
        ValueKind::Symbol(id) => resolve_sym(id).to_owned(),
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), args[1]],
            ));
        }
    };
    validate_coding_system(&coding, args[1], known)?;
    let bytes = lisp_string_coding_source_bytes(s);
    if coding_string_nocopy(&args) && coding_string_trivial_ascii_nocopy(&bytes, &coding, false) {
        return Ok(args[0]);
    }
    if coding_string_trivial_ascii_nocopy(&bytes, &coding, false) {
        return Ok(Value::multibyte_string(
            String::from_utf8(bytes).expect("ASCII bytes are valid UTF-8"),
        ));
    }
    if is_byte_preserving_coding_system(&coding) {
        let bytes = if coding.starts_with("raw-text") {
            decode_eol_text(&bytes, &coding, eol_conversion)
        } else {
            bytes
        };
        return Ok(Value::heap_string(
            crate::heap_types::LispString::from_unibyte(bytes),
        ));
    }
    if matches!(coding_system_family(&coding), "utf-8" | "utf-8-emacs") {
        // Decode straight to Emacs storage bytes so eight-bit raw bytes use the
        // 0x3FFF00+ extended form and real Private-Use-Area glyphs keep their
        // code points — never the colliding in-Unicode sentinels (issue #131).
        return Ok(Value::heap_string(
            crate::heap_types::LispString::from_emacs_bytes(decode_utf8_coding_to_emacs_bytes(
                &bytes,
                &coding,
                eol_conversion,
            )),
        ));
    }
    let decoded = decode_bytes(&bytes, &coding, eol_conversion);
    let charset = match SingleByteCharset::for_coding_system(&coding) {
        Some(single_byte) => Some((single_byte.source_charset_name(), true)),
        None => match coding_system_family(&coding) {
            "chinese-iso-8bit" => Some(("chinese-gb2312", false)),
            "chinese-big5" | "chinese-big5-hkscs" => Some(("big5", false)),
            _ => None,
        },
    };
    if let Some((charset, source_charset_decodes_ascii)) = charset {
        let runs = charset_property_runs(&decoded, charset, source_charset_decodes_ascii);
        if !runs.is_empty() {
            return Ok(Value::multibyte_string_with_text_properties(decoded, runs));
        }
    }
    Ok(Value::multibyte_string(decoded))
}

/// `(char-or-string-p OBJ)` -> t or nil
pub(crate) fn builtin_char_or_string_p(args: Vec<Value>) -> EvalResult {
    expect_args("char-or-string-p", &args, 1)?;
    // GNU `Fchar_or_string_p` (`src/data.c`) only accepts fixnums in
    // the valid character code range [0, MAX_CHAR_CODE = 0x3FFFFF].
    // Negative or out-of-range integers must return nil.
    let is_char_or_string = match args[0].kind() {
        ValueKind::Fixnum(n) => (0..=MAX_CHAR_CODE).contains(&n),
        ValueKind::String => true,
        _ => false,
    };
    Ok(Value::bool_val(is_char_or_string))
}

/// `(char-displayable-p CHAR)` -> t, nil, or `unicode`
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_char_displayable_p(args: Vec<Value>) -> EvalResult {
    expect_args("char-displayable-p", &args, 1)?;
    let code = match args[0].kind() {
        ValueKind::Fixnum(c) => c,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("number-or-marker-p"), args[0]],
            ));
        }
    };
    if !(0..=MAX_CHAR_CODE).contains(&code) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), Value::fixnum(code)],
        ));
    }
    if code <= 0x7F {
        return Ok(Value::T);
    }
    if code <= 0x10_FFFF {
        return Ok(Value::symbol("unicode"));
    }
    Ok(Value::NIL)
}

/// `(max-char)` -> integer
pub(crate) fn builtin_max_char(args: Vec<Value>) -> EvalResult {
    if args.len() > 1 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("max-char"), Value::fixnum(args.len() as i64)],
        ));
    }
    let unicode_only = args.first().is_some_and(|v| !v.is_nil());
    Ok(Value::fixnum(if unicode_only {
        0x10_FFFF
    } else {
        MAX_CHAR_CODE
    }))
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "encoding_test.rs"]
mod tests;
