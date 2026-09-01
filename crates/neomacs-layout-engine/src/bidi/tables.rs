//! Unicode character property tables for the bidi algorithm.
//!
//! Provides Bidi_Class, Bidi_Mirroring_Glyph, and Bidi_Paired_Bracket lookups.
//! Data derived from Unicode 16.0.

use super::types::{BidiClass, BracketType};

/// Look up the Bidi_Class for a Unicode code point.
pub fn bidi_class(ch: char) -> BidiClass {
    let cp = ch as u32;

    // Fast path: ASCII
    if cp < 0x80 {
        return ascii_bidi_class(cp as u8);
    }

    // Search range tables
    bidi_class_from_ranges(cp)
}

/// Bidi class for ASCII characters.
fn ascii_bidi_class(b: u8) -> BidiClass {
    match b {
        // Paragraph separator
        0x0A | 0x0D => BidiClass::B,
        // Segment separators
        0x09 => BidiClass::S,
        // Whitespace
        0x0C | 0x20 => BidiClass::WS,
        // Other controls (BN)
        0x00..=0x08 | 0x0E..=0x1B => BidiClass::BN,
        0x7F => BidiClass::BN,
        // European digits
        0x30..=0x39 => BidiClass::EN,
        // European number separators
        0x2B | 0x2D => BidiClass::ES,
        // European number terminators
        0x23..=0x25 => BidiClass::ET,
        // Common separators
        0x2C | 0x2E | 0x2F | 0x3A => BidiClass::CS,
        // ON: other neutrals (punctuation, symbols)
        0x21 | 0x22 | 0x26..=0x2A | 0x3B..=0x40 | 0x5B..=0x60 | 0x7B..=0x7E => BidiClass::ON,
        // Strong L: Latin letters
        0x41..=0x5A | 0x61..=0x7A => BidiClass::L,
        // Everything else defaults to L for ASCII
        _ => BidiClass::L,
    }
}

/// Range-based Bidi_Class lookup for non-ASCII code points.
///
/// Each entry is (start, end_inclusive, class).
/// Ranges are sorted and non-overlapping. Default is L.
fn bidi_class_from_ranges(cp: u32) -> BidiClass {
    // Binary search through ranges
    let idx = BIDI_CLASS_RANGES.partition_point(|&(start, _, _)| start <= cp);
    if idx > 0 {
        let (start, end, class) = BIDI_CLASS_RANGES[idx - 1];
        if cp >= start && cp <= end {
            return class;
        }
    }
    // Default: most unassigned code points are L
    BidiClass::L
}

/// Look up the bidi mirroring glyph for a character.
/// Returns `None` if the character has no mirror.
pub fn bidi_mirror(ch: char) -> Option<char> {
    let cp = ch as u32;
    // Binary search through mirror pairs
    let idx = MIRROR_PAIRS.partition_point(|&(from, _)| from < cp);
    if idx < MIRROR_PAIRS.len() {
        let (from, to) = MIRROR_PAIRS[idx];
        if from == cp {
            return char::from_u32(to);
        }
    }
    None
}

/// Look up bracket type for the Paired Bracket Algorithm (BPA).
pub fn bracket_type(ch: char) -> BracketType {
    let cp = ch as u32;
    // Binary search through bracket pairs
    let idx = BRACKET_PAIRS.partition_point(|&(open, _)| (open as u32) < cp);
    if idx < BRACKET_PAIRS.len() {
        let (open, close) = BRACKET_PAIRS[idx];
        if open == ch {
            return BracketType::Open(close);
        }
    }
    // Check if it's a closing bracket
    for &(open, close) in BRACKET_PAIRS {
        if close == ch {
            return BracketType::Close(open);
        }
    }
    BracketType::None
}

/// Condensed Bidi_Class range table.
/// Covers the most important Unicode ranges. Default for unlisted code points is L.
/// Format: (start, end_inclusive, BidiClass)
static BIDI_CLASS_RANGES: &[(u32, u32, BidiClass)] = &[
    // Latin-1 supplement controls
    (0x0080, 0x0084, BidiClass::BN),
    (0x0085, 0x0085, BidiClass::B), // NEL
    (0x0086, 0x009F, BidiClass::BN),
    (0x00A0, 0x00A0, BidiClass::CS), // NBSP
    (0x00A1, 0x00A1, BidiClass::ON),
    (0x00A2, 0x00A5, BidiClass::ET), // Currency symbols
    (0x00A6, 0x00A9, BidiClass::ON),
    (0x00AB, 0x00AB, BidiClass::ON),
    (0x00AC, 0x00AC, BidiClass::ON),
    (0x00AD, 0x00AD, BidiClass::BN), // Soft hyphen
    (0x00AE, 0x00AF, BidiClass::ON),
    (0x00B0, 0x00B1, BidiClass::ET),
    (0x00B2, 0x00B3, BidiClass::EN), // Superscript digits
    (0x00B4, 0x00B4, BidiClass::ON),
    (0x00B6, 0x00B8, BidiClass::ON),
    (0x00B9, 0x00B9, BidiClass::EN), // Superscript 1
    (0x00BB, 0x00BB, BidiClass::ON),
    (0x00BC, 0x00BE, BidiClass::ON), // Vulgar fractions
    (0x00BF, 0x00BF, BidiClass::ON),
    (0x00D7, 0x00D7, BidiClass::ON), // Multiplication sign
    (0x00F7, 0x00F7, BidiClass::ON), // Division sign
    // Combining marks (NSM) — general ranges
    (0x0300, 0x036F, BidiClass::NSM), // Combining Diacritical Marks
    // Hebrew
    (0x0590, 0x0590, BidiClass::R),
    (0x0591, 0x05BD, BidiClass::NSM), // Hebrew combining marks
    (0x05BE, 0x05BE, BidiClass::R),
    (0x05BF, 0x05BF, BidiClass::NSM),
    (0x05C0, 0x05C0, BidiClass::R),
    (0x05C1, 0x05C2, BidiClass::NSM),
    (0x05C3, 0x05C3, BidiClass::R),
    (0x05C4, 0x05C5, BidiClass::NSM),
    (0x05C6, 0x05C6, BidiClass::R),
    (0x05C7, 0x05C7, BidiClass::NSM),
    (0x05D0, 0x05EA, BidiClass::R), // Hebrew letters
    (0x05EF, 0x05F4, BidiClass::R),
    // Arabic
    (0x0600, 0x0605, BidiClass::AN), // Arabic number signs
    (0x0608, 0x0608, BidiClass::AL),
    (0x0609, 0x060A, BidiClass::ET), // Arabic-Indic per-mille/per-ten-thousand
    (0x060B, 0x060B, BidiClass::AL),
    (0x060C, 0x060C, BidiClass::CS), // Arabic comma
    (0x060D, 0x060D, BidiClass::AL),
    (0x0610, 0x061A, BidiClass::NSM), // Arabic combining marks
    (0x061B, 0x061B, BidiClass::AL),
    (0x061C, 0x061C, BidiClass::BN),  // Arabic letter mark
    (0x061D, 0x064A, BidiClass::AL),  // Arabic letters
    (0x064B, 0x065F, BidiClass::NSM), // Arabic combining marks
    (0x0660, 0x0669, BidiClass::AN),  // Arabic-Indic digits
    (0x066A, 0x066A, BidiClass::ET),  // Arabic percent
    (0x066B, 0x066C, BidiClass::AN),  // Arabic decimal/thousands
    (0x066D, 0x066F, BidiClass::AL),
    (0x0670, 0x0670, BidiClass::NSM),
    (0x0671, 0x06D5, BidiClass::AL), // Arabic letters continued
    (0x06D6, 0x06DC, BidiClass::NSM),
    (0x06DD, 0x06DD, BidiClass::AN),
    (0x06DE, 0x06DE, BidiClass::ON),
    (0x06DF, 0x06E4, BidiClass::NSM),
    (0x06E5, 0x06E6, BidiClass::AL),
    (0x06E7, 0x06E8, BidiClass::NSM),
    (0x06E9, 0x06E9, BidiClass::ON),
    (0x06EA, 0x06ED, BidiClass::NSM),
    (0x06EE, 0x06EF, BidiClass::AL),
    (0x06F0, 0x06F9, BidiClass::EN), // Extended Arabic-Indic digits
    (0x06FA, 0x070D, BidiClass::AL),
    (0x070F, 0x070F, BidiClass::AL),
    (0x0710, 0x0710, BidiClass::AL), // Syriac
    (0x0711, 0x0711, BidiClass::NSM),
    (0x0712, 0x072F, BidiClass::AL),
    (0x0730, 0x074A, BidiClass::NSM),
    (0x074D, 0x07A5, BidiClass::AL),
    (0x07A6, 0x07B0, BidiClass::NSM),
    (0x07B1, 0x07B1, BidiClass::AL),
    (0x07C0, 0x07EA, BidiClass::R), // NKo
    (0x07EB, 0x07F3, BidiClass::NSM),
    (0x07F4, 0x07F5, BidiClass::R),
    (0x07FA, 0x07FA, BidiClass::R),
    (0x07FD, 0x07FD, BidiClass::NSM),
    (0x07FE, 0x07FF, BidiClass::ET),
    // Samaritan
    (0x0800, 0x0815, BidiClass::R),
    (0x0816, 0x0819, BidiClass::NSM),
    (0x081A, 0x081A, BidiClass::R),
    (0x081B, 0x0823, BidiClass::NSM),
    (0x0824, 0x0824, BidiClass::R),
    (0x0825, 0x0827, BidiClass::NSM),
    (0x0828, 0x0828, BidiClass::R),
    (0x0829, 0x082D, BidiClass::NSM),
    (0x0830, 0x083E, BidiClass::R),
    // Mandaic
    (0x0840, 0x0858, BidiClass::R),
    (0x0859, 0x085B, BidiClass::NSM),
    (0x085E, 0x085E, BidiClass::R),
    // Arabic Extended ranges
    (0x0860, 0x086A, BidiClass::AL),
    (0x0870, 0x089F, BidiClass::AL),
    (0x08A0, 0x08C9, BidiClass::AL),
    (0x08CA, 0x08E1, BidiClass::NSM),
    (0x08E2, 0x08E2, BidiClass::AN),
    (0x08E3, 0x0902, BidiClass::NSM),
    // Devanagari and other Indic NSM ranges
    (0x093A, 0x093A, BidiClass::NSM),
    (0x093C, 0x093C, BidiClass::NSM),
    (0x0941, 0x0948, BidiClass::NSM),
    (0x094D, 0x094D, BidiClass::NSM),
    (0x0951, 0x0957, BidiClass::NSM),
    (0x0962, 0x0963, BidiClass::NSM),
    // General punctuation
    (0x2000, 0x200A, BidiClass::WS), // Various spaces
    (0x200B, 0x200D, BidiClass::BN), // ZWSP, ZWNJ, ZWJ
    (0x200E, 0x200E, BidiClass::L),  // LRM
    (0x200F, 0x200F, BidiClass::R),  // RLM
    (0x2010, 0x2027, BidiClass::ON), // Dashes, quotation marks, etc.
    (0x2028, 0x2028, BidiClass::WS), // Line separator
    (0x2029, 0x2029, BidiClass::B),  // Paragraph separator
    (0x202A, 0x202A, BidiClass::LRE),
    (0x202B, 0x202B, BidiClass::RLE),
    (0x202C, 0x202C, BidiClass::PDF),
    (0x202D, 0x202D, BidiClass::LRO),
    (0x202E, 0x202E, BidiClass::RLO),
    (0x202F, 0x202F, BidiClass::CS), // NNBSP
    (0x2030, 0x2034, BidiClass::ET), // Per-mille, per-ten-thousand, etc.
    (0x2035, 0x2043, BidiClass::ON),
    (0x2044, 0x2044, BidiClass::CS), // Fraction slash
    (0x2045, 0x205E, BidiClass::ON),
    (0x205F, 0x205F, BidiClass::WS), // Medium mathematical space
    (0x2060, 0x2064, BidiClass::BN), // Word joiner etc.
    (0x2066, 0x2066, BidiClass::LRI),
    (0x2067, 0x2067, BidiClass::RLI),
    (0x2068, 0x2068, BidiClass::FSI),
    (0x2069, 0x2069, BidiClass::PDI),
    (0x206A, 0x206F, BidiClass::BN), // Deprecated formatting
    // Superscripts and subscripts
    (0x2070, 0x2070, BidiClass::EN),
    (0x2074, 0x2079, BidiClass::EN),
    (0x207A, 0x207B, BidiClass::ES),
    (0x207C, 0x207E, BidiClass::ON),
    (0x2080, 0x2089, BidiClass::EN),
    (0x208A, 0x208B, BidiClass::ES),
    (0x208C, 0x208E, BidiClass::ON),
    // Currency symbols
    (0x20A0, 0x20C0, BidiClass::ET),
    // Combining marks for symbols
    (0x20D0, 0x20F0, BidiClass::NSM),
    // Letterlike symbols (misc)
    (0x2100, 0x2101, BidiClass::ON),
    (0x2103, 0x2106, BidiClass::ON),
    (0x2108, 0x2109, BidiClass::ON),
    (0x2114, 0x2114, BidiClass::ON),
    (0x2116, 0x2118, BidiClass::ON),
    (0x211E, 0x2123, BidiClass::ON),
    (0x2125, 0x2125, BidiClass::ON),
    (0x2127, 0x2127, BidiClass::ON),
    (0x2129, 0x2129, BidiClass::ON),
    (0x212E, 0x212E, BidiClass::ET), // Estimated symbol
    (0x213A, 0x213B, BidiClass::ON),
    // Number forms
    (0x2150, 0x215F, BidiClass::ON), // Vulgar fractions → ON (not EN)
    (0x2160, 0x2188, BidiClass::L),  // Roman numerals → L
    (0x2189, 0x218B, BidiClass::ON),
    // Arrows, math operators
    (0x2190, 0x2426, BidiClass::ON),
    (0x2440, 0x244A, BidiClass::ON),
    // Box drawing, block elements, geometric shapes
    (0x2500, 0x2775, BidiClass::ON),
    // Miscellaneous symbols
    (0x2776, 0x2793, BidiClass::ON), // Dingbat numbers (ON, not EN)
    (0x2794, 0x27BF, BidiClass::ON),
    (0x27C0, 0x27FF, BidiClass::ON),
    (0x2800, 0x28FF, BidiClass::L),  // Braille → L
    (0x2900, 0x2BFF, BidiClass::ON), // Arrows, math symbols
    (0x2E00, 0x2E5D, BidiClass::ON), // Supplemental punctuation
    // CJK (strong L)
    (0x2E80, 0x2FFF, BidiClass::ON), // CJK radicals, kangxi
    (0x3000, 0x3000, BidiClass::WS), // Ideographic space
    (0x3001, 0x3003, BidiClass::ON),
    (0x3008, 0x3011, BidiClass::ON), // CJK brackets
    (0x3012, 0x3013, BidiClass::ON),
    (0x3014, 0x301F, BidiClass::ON),
    (0x3030, 0x3030, BidiClass::ON),
    (0x3031, 0x3035, BidiClass::L),
    (0x303D, 0x303D, BidiClass::ON),
    (0x3040, 0x309F, BidiClass::L), // Hiragana
    (0x30A0, 0x30FF, BidiClass::L), // Katakana
    (0x3100, 0x312F, BidiClass::L), // Bopomofo
    (0x3130, 0x318F, BidiClass::L), // Hangul compatibility
    (0x3190, 0x319F, BidiClass::L),
    (0x31A0, 0x31BF, BidiClass::L),
    (0x31F0, 0x31FF, BidiClass::L),
    (0x3200, 0x321E, BidiClass::L),
    (0x3220, 0x3247, BidiClass::L),
    (0x3248, 0x324F, BidiClass::L), // CJK A enclosed ideographs (L)
    (0x3250, 0x32FE, BidiClass::L),
    (0x3300, 0x33FF, BidiClass::L),  // CJK compatibility
    (0x3400, 0x4DBF, BidiClass::L),  // CJK Unified Ideographs Extension A
    (0x4DC0, 0x4DFF, BidiClass::ON), // Yijing hexagram symbols
    (0x4E00, 0x9FFF, BidiClass::L),  // CJK Unified Ideographs
    (0xA000, 0xA4CF, BidiClass::L),  // Yi
    // Hangul syllables
    (0xAC00, 0xD7A3, BidiClass::L),
    (0xD7B0, 0xD7C6, BidiClass::L),
    (0xD7CB, 0xD7FB, BidiClass::L),
    // Arabic Presentation Forms-A
    (0xFB1D, 0xFB1D, BidiClass::R), // Hebrew YOD WITH HIRIQ
    (0xFB1E, 0xFB1E, BidiClass::NSM),
    (0xFB1F, 0xFB28, BidiClass::R),  // Hebrew ligatures
    (0xFB29, 0xFB29, BidiClass::ES), // Hebrew plus
    (0xFB2A, 0xFB4F, BidiClass::R),  // Hebrew presentation
    (0xFB50, 0xFD3D, BidiClass::AL), // Arabic presentation forms
    (0xFD3E, 0xFD3F, BidiClass::ON), // Ornate brackets
    (0xFD40, 0xFDCF, BidiClass::AL),
    (0xFDF0, 0xFDFC, BidiClass::AL),
    (0xFDFD, 0xFDFF, BidiClass::ON),
    // Combining half marks
    (0xFE20, 0xFE2F, BidiClass::NSM),
    // Small form variants
    (0xFE50, 0xFE50, BidiClass::CS),
    (0xFE51, 0xFE51, BidiClass::ON),
    (0xFE52, 0xFE52, BidiClass::CS),
    (0xFE54, 0xFE54, BidiClass::ON),
    (0xFE55, 0xFE55, BidiClass::CS),
    (0xFE56, 0xFE5E, BidiClass::ON),
    (0xFE5F, 0xFE5F, BidiClass::ET),
    (0xFE60, 0xFE61, BidiClass::ON),
    (0xFE62, 0xFE63, BidiClass::ES),
    (0xFE64, 0xFE66, BidiClass::ON),
    (0xFE68, 0xFE68, BidiClass::ON),
    (0xFE69, 0xFE6A, BidiClass::ET),
    (0xFE6B, 0xFE6B, BidiClass::ON),
    // Arabic Presentation Forms-B
    (0xFE70, 0xFEFC, BidiClass::AL),
    (0xFEFF, 0xFEFF, BidiClass::BN), // BOM/ZWNBSP
    // Fullwidth forms
    (0xFF01, 0xFF02, BidiClass::ON),
    (0xFF03, 0xFF05, BidiClass::ET),
    (0xFF06, 0xFF0A, BidiClass::ON),
    (0xFF0B, 0xFF0B, BidiClass::ES),
    (0xFF0C, 0xFF0C, BidiClass::CS),
    (0xFF0D, 0xFF0D, BidiClass::ES),
    (0xFF0E, 0xFF0E, BidiClass::CS),
    (0xFF0F, 0xFF0F, BidiClass::CS),
    (0xFF10, 0xFF19, BidiClass::EN), // Fullwidth digits
    (0xFF1A, 0xFF1A, BidiClass::CS),
    (0xFF1B, 0xFF20, BidiClass::ON),
    (0xFF21, 0xFF3A, BidiClass::L), // Fullwidth Latin
    (0xFF3B, 0xFF40, BidiClass::ON),
    (0xFF41, 0xFF5A, BidiClass::L),
    (0xFF5B, 0xFF65, BidiClass::ON),
    (0xFF66, 0xFFBE, BidiClass::L), // Halfwidth katakana
    (0xFFC2, 0xFFCF, BidiClass::L), // Halfwidth Hangul
    (0xFFD2, 0xFFD7, BidiClass::L),
    (0xFFDA, 0xFFDC, BidiClass::L),
    (0xFFE0, 0xFFE1, BidiClass::ET),
    (0xFFE2, 0xFFE4, BidiClass::ON),
    (0xFFE5, 0xFFE6, BidiClass::ET),
    (0xFFE8, 0xFFEE, BidiClass::ON),
    (0xFFF0, 0xFFF8, BidiClass::BN),
    (0xFFF9, 0xFFFB, BidiClass::ON),
    (0xFFFC, 0xFFFC, BidiClass::ON), // Object replacement
    (0xFFFD, 0xFFFD, BidiClass::ON), // Replacement character
    // Supplementary planes — key ranges
    // SMP: various scripts (mostly L), some R
    (0x10800, 0x10FFF, BidiClass::R),  // Cypriot, Aramaic, etc.
    (0x1E800, 0x1EDFF, BidiClass::R),  // Mende Kikakui, Adlam, etc.
    (0x1EE00, 0x1EEFF, BidiClass::AL), // Arabic Mathematical Alphabetic Symbols
    (0x1EF00, 0x1EFFF, BidiClass::R),
    // Tags, variation selectors
    (0xE0001, 0xE0001, BidiClass::BN),
    (0xE0020, 0xE007F, BidiClass::BN),
    (0xE0100, 0xE01EF, BidiClass::NSM), // Variation selectors supplement
    // Noncharacters
    (0xFDD0, 0xFDEF, BidiClass::BN),
];

/// Mirroring pairs (from BidiMirroring.txt).
/// Format: (code_point, mirrored_code_point).
static MIRROR_PAIRS: &[(u32, u32)] = &[
    (0x0028, 0x0029), // ( ↔ )
    (0x0029, 0x0028),
    (0x003C, 0x003E), // < ↔ >
    (0x003E, 0x003C),
    (0x005B, 0x005D), // [ ↔ ]
    (0x005D, 0x005B),
    (0x007B, 0x007D), // { ↔ }
    (0x007D, 0x007B),
    (0x00AB, 0x00BB), // « ↔ »
    (0x00BB, 0x00AB),
    (0x0F3A, 0x0F3B),
    (0x0F3B, 0x0F3A),
    (0x0F3C, 0x0F3D),
    (0x0F3D, 0x0F3C),
    (0x169B, 0x169C),
    (0x169C, 0x169B),
    (0x2039, 0x203A), // ‹ ↔ ›
    (0x203A, 0x2039),
    (0x2045, 0x2046),
    (0x2046, 0x2045),
    (0x207D, 0x207E),
    (0x207E, 0x207D),
    (0x208D, 0x208E),
    (0x208E, 0x208D),
    (0x2208, 0x220B), // ∈ ↔ ∋
    (0x2209, 0x220C),
    (0x220A, 0x220D),
    (0x220B, 0x2208),
    (0x220C, 0x2209),
    (0x220D, 0x220A),
    (0x2215, 0x29F5), // ∕ ↔ ⧵
    (0x221F, 0x2BFE),
    (0x2220, 0x29A3),
    (0x2221, 0x299B),
    (0x2222, 0x29A0),
    (0x2224, 0x2AEE),
    (0x223C, 0x223D), // ∼ ↔ ∽
    (0x223D, 0x223C),
    (0x2243, 0x22CD),
    (0x2245, 0x224C),
    (0x224C, 0x2245),
    (0x2252, 0x2253),
    (0x2253, 0x2252),
    (0x2254, 0x2255),
    (0x2255, 0x2254),
    (0x2264, 0x2265), // ≤ ↔ ≥
    (0x2265, 0x2264),
    (0x2266, 0x2267),
    (0x2267, 0x2266),
    (0x2268, 0x2269),
    (0x2269, 0x2268),
    (0x226A, 0x226B), // ≪ ↔ ≫
    (0x226B, 0x226A),
    (0x226E, 0x226F),
    (0x226F, 0x226E),
    (0x2270, 0x2271),
    (0x2271, 0x2270),
    (0x2272, 0x2273),
    (0x2273, 0x2272),
    (0x2274, 0x2275),
    (0x2275, 0x2274),
    (0x2276, 0x2277),
    (0x2277, 0x2276),
    (0x2278, 0x2279),
    (0x2279, 0x2278),
    (0x227A, 0x227B), // ≺ ↔ ≻
    (0x227B, 0x227A),
    (0x227C, 0x227D),
    (0x227D, 0x227C),
    (0x227E, 0x227F),
    (0x227F, 0x227E),
    (0x2280, 0x2281),
    (0x2281, 0x2280),
    (0x2282, 0x2283), // ⊂ ↔ ⊃
    (0x2283, 0x2282),
    (0x2284, 0x2285),
    (0x2285, 0x2284),
    (0x2286, 0x2287),
    (0x2287, 0x2286),
    (0x2288, 0x2289),
    (0x2289, 0x2288),
    (0x228A, 0x228B),
    (0x228B, 0x228A),
    (0x228F, 0x2290),
    (0x2290, 0x228F),
    (0x2291, 0x2292),
    (0x2292, 0x2291),
    (0x2298, 0x29B8),
    (0x22A2, 0x22A3), // ⊢ ↔ ⊣
    (0x22A3, 0x22A2),
    (0x22A6, 0x2ADE),
    (0x22A8, 0x2AE4),
    (0x22A9, 0x2AE3),
    (0x22AB, 0x2AE5),
    (0x22B0, 0x22B1),
    (0x22B1, 0x22B0),
    (0x22B2, 0x22B3),
    (0x22B3, 0x22B2),
    (0x22B4, 0x22B5),
    (0x22B5, 0x22B4),
    (0x22B6, 0x22B7),
    (0x22B7, 0x22B6),
    (0x22B8, 0x27DC),
    (0x22C9, 0x22CA),
    (0x22CA, 0x22C9),
    (0x22CB, 0x22CC),
    (0x22CC, 0x22CB),
    (0x22CD, 0x2243),
    (0x22D0, 0x22D1),
    (0x22D1, 0x22D0),
    (0x22D6, 0x22D7),
    (0x22D7, 0x22D6),
    (0x22D8, 0x22D9),
    (0x22D9, 0x22D8),
    (0x22DA, 0x22DB),
    (0x22DB, 0x22DA),
    (0x22DC, 0x22DD),
    (0x22DD, 0x22DC),
    (0x22DE, 0x22DF),
    (0x22DF, 0x22DE),
    (0x22E0, 0x22E1),
    (0x22E1, 0x22E0),
    (0x22E2, 0x22E3),
    (0x22E3, 0x22E2),
    (0x22E4, 0x22E5),
    (0x22E5, 0x22E4),
    (0x22E6, 0x22E7),
    (0x22E7, 0x22E6),
    (0x22E8, 0x22E9),
    (0x22E9, 0x22E8),
    (0x22EA, 0x22EB),
    (0x22EB, 0x22EA),
    (0x22EC, 0x22ED),
    (0x22ED, 0x22EC),
    (0x22F0, 0x22F1),
    (0x22F1, 0x22F0),
    (0x22F2, 0x22FA),
    (0x22F3, 0x22FB),
    (0x22F4, 0x22FC),
    (0x22F6, 0x22FD),
    (0x22F7, 0x22FE),
    (0x22FA, 0x22F2),
    (0x22FB, 0x22F3),
    (0x22FC, 0x22F4),
    (0x22FD, 0x22F6),
    (0x22FE, 0x22F7),
    (0x2308, 0x2309), // ⌈ ↔ ⌉
    (0x2309, 0x2308),
    (0x230A, 0x230B), // ⌊ ↔ ⌋
    (0x230B, 0x230A),
    (0x2329, 0x232A), // 〈 ↔ 〉
    (0x232A, 0x2329),
    (0x2768, 0x2769),
    (0x2769, 0x2768),
    (0x276A, 0x276B),
    (0x276B, 0x276A),
    (0x276C, 0x276D),
    (0x276D, 0x276C),
    (0x276E, 0x276F),
    (0x276F, 0x276E),
    (0x2770, 0x2771),
    (0x2771, 0x2770),
    (0x2772, 0x2773),
    (0x2773, 0x2772),
    (0x2774, 0x2775),
    (0x2775, 0x2774),
    (0x27C3, 0x27C4),
    (0x27C4, 0x27C3),
    (0x27C5, 0x27C6),
    (0x27C6, 0x27C5),
    (0x27C8, 0x27C9),
    (0x27C9, 0x27C8),
    (0x27CB, 0x27CD),
    (0x27CD, 0x27CB),
    (0x27D5, 0x27D6),
    (0x27D6, 0x27D5),
    (0x27DC, 0x22B8),
    (0x27DD, 0x27DE),
    (0x27DE, 0x27DD),
    (0x27E2, 0x27E3),
    (0x27E3, 0x27E2),
    (0x27E4, 0x27E5),
    (0x27E5, 0x27E4),
    (0x27E6, 0x27E7), // ⟦ ↔ ⟧
    (0x27E7, 0x27E6),
    (0x27E8, 0x27E9), // ⟨ ↔ ⟩
    (0x27E9, 0x27E8),
    (0x27EA, 0x27EB),
    (0x27EB, 0x27EA),
    (0x27EC, 0x27ED),
    (0x27ED, 0x27EC),
    (0x27EE, 0x27EF),
    (0x27EF, 0x27EE),
    (0x2983, 0x2984),
    (0x2984, 0x2983),
    (0x2985, 0x2986),
    (0x2986, 0x2985),
    (0x2987, 0x2988),
    (0x2988, 0x2987),
    (0x2989, 0x298A),
    (0x298A, 0x2989),
    (0x298B, 0x298C),
    (0x298C, 0x298B),
    (0x298D, 0x2990),
    (0x298E, 0x298F),
    (0x298F, 0x298E),
    (0x2990, 0x298D),
    (0x2991, 0x2992),
    (0x2992, 0x2991),
    (0x2993, 0x2994),
    (0x2994, 0x2993),
    (0x2995, 0x2996),
    (0x2996, 0x2995),
    (0x2997, 0x2998),
    (0x2998, 0x2997),
    (0x29B8, 0x2298),
    (0x29C0, 0x29C1),
    (0x29C1, 0x29C0),
    (0x29C4, 0x29C5),
    (0x29C5, 0x29C4),
    (0x29CF, 0x29D0),
    (0x29D0, 0x29CF),
    (0x29D1, 0x29D2),
    (0x29D2, 0x29D1),
    (0x29D4, 0x29D5),
    (0x29D5, 0x29D4),
    (0x29D8, 0x29D9),
    (0x29D9, 0x29D8),
    (0x29DA, 0x29DB),
    (0x29DB, 0x29DA),
    (0x29F5, 0x2215),
    (0x29F8, 0x29F9),
    (0x29F9, 0x29F8),
    (0x29FC, 0x29FD),
    (0x29FD, 0x29FC),
    (0x2A2B, 0x2A2C),
    (0x2A2C, 0x2A2B),
    (0x2A2D, 0x2A2E),
    (0x2A2E, 0x2A2D),
    (0x2A34, 0x2A35),
    (0x2A35, 0x2A34),
    (0x2A3C, 0x2A3D),
    (0x2A3D, 0x2A3C),
    (0x2A64, 0x2A65),
    (0x2A65, 0x2A64),
    (0x2A79, 0x2A7A),
    (0x2A7A, 0x2A79),
    (0x2A7B, 0x2A7C),
    (0x2A7C, 0x2A7B),
    (0x2A7D, 0x2A7E),
    (0x2A7E, 0x2A7D),
    (0x2A7F, 0x2A80),
    (0x2A80, 0x2A7F),
    (0x2A81, 0x2A82),
    (0x2A82, 0x2A81),
    (0x2A83, 0x2A84),
    (0x2A84, 0x2A83),
    (0x2A8B, 0x2A8C),
    (0x2A8C, 0x2A8B),
    (0x2A91, 0x2A92),
    (0x2A92, 0x2A91),
    (0x2A93, 0x2A94),
    (0x2A94, 0x2A93),
    (0x2A95, 0x2A96),
    (0x2A96, 0x2A95),
    (0x2A97, 0x2A98),
    (0x2A98, 0x2A97),
    (0x2A99, 0x2A9A),
    (0x2A9A, 0x2A99),
    (0x2A9B, 0x2A9C),
    (0x2A9C, 0x2A9B),
    (0x2AA1, 0x2AA2),
    (0x2AA2, 0x2AA1),
    (0x2AA6, 0x2AA7),
    (0x2AA7, 0x2AA6),
    (0x2AA8, 0x2AA9),
    (0x2AA9, 0x2AA8),
    (0x2AAA, 0x2AAB),
    (0x2AAB, 0x2AAA),
    (0x2AAC, 0x2AAD),
    (0x2AAD, 0x2AAC),
    (0x2AAF, 0x2AB0),
    (0x2AB0, 0x2AAF),
    (0x2AB3, 0x2AB4),
    (0x2AB4, 0x2AB3),
    (0x2ABB, 0x2ABC),
    (0x2ABC, 0x2ABB),
    (0x2ABD, 0x2ABE),
    (0x2ABE, 0x2ABD),
    (0x2ABF, 0x2AC0),
    (0x2AC0, 0x2ABF),
    (0x2AC1, 0x2AC2),
    (0x2AC2, 0x2AC1),
    (0x2AC3, 0x2AC4),
    (0x2AC4, 0x2AC3),
    (0x2AC5, 0x2AC6),
    (0x2AC6, 0x2AC5),
    (0x2ACD, 0x2ACE),
    (0x2ACE, 0x2ACD),
    (0x2ACF, 0x2AD0),
    (0x2AD0, 0x2ACF),
    (0x2AD1, 0x2AD2),
    (0x2AD2, 0x2AD1),
    (0x2AD3, 0x2AD4),
    (0x2AD4, 0x2AD3),
    (0x2AD5, 0x2AD6),
    (0x2AD6, 0x2AD5),
    (0x2ADE, 0x22A6),
    (0x2AE3, 0x22A9),
    (0x2AE4, 0x22A8),
    (0x2AE5, 0x22AB),
    (0x2AEC, 0x2AED),
    (0x2AED, 0x2AEC),
    (0x2AEE, 0x2224),
    (0x2AF7, 0x2AF8),
    (0x2AF8, 0x2AF7),
    (0x2AF9, 0x2AFA),
    (0x2AFA, 0x2AF9),
    (0x2BFE, 0x221F),
    (0x2E02, 0x2E03),
    (0x2E03, 0x2E02),
    (0x2E04, 0x2E05),
    (0x2E05, 0x2E04),
    (0x2E09, 0x2E0A),
    (0x2E0A, 0x2E09),
    (0x2E0C, 0x2E0D),
    (0x2E0D, 0x2E0C),
    (0x2E1C, 0x2E1D),
    (0x2E1D, 0x2E1C),
    (0x2E20, 0x2E21),
    (0x2E21, 0x2E20),
    (0x2E22, 0x2E23),
    (0x2E23, 0x2E22),
    (0x2E24, 0x2E25),
    (0x2E25, 0x2E24),
    (0x2E26, 0x2E27),
    (0x2E27, 0x2E26),
    (0x2E28, 0x2E29),
    (0x2E29, 0x2E28),
    (0x2E55, 0x2E56),
    (0x2E56, 0x2E55),
    (0x2E57, 0x2E58),
    (0x2E58, 0x2E57),
    (0x2E59, 0x2E5A),
    (0x2E5A, 0x2E59),
    (0x2E5B, 0x2E5C),
    (0x2E5C, 0x2E5B),
    (0x3008, 0x3009), // 〈 ↔ 〉
    (0x3009, 0x3008),
    (0x300A, 0x300B),
    (0x300B, 0x300A),
    (0x300C, 0x300D),
    (0x300D, 0x300C),
    (0x300E, 0x300F),
    (0x300F, 0x300E),
    (0x3010, 0x3011),
    (0x3011, 0x3010),
    (0x3014, 0x3015),
    (0x3015, 0x3014),
    (0x3016, 0x3017),
    (0x3017, 0x3016),
    (0x3018, 0x3019),
    (0x3019, 0x3018),
    (0x301A, 0x301B),
    (0x301B, 0x301A),
    (0xFE59, 0xFE5A),
    (0xFE5A, 0xFE59),
    (0xFE5B, 0xFE5C),
    (0xFE5C, 0xFE5B),
    (0xFE5D, 0xFE5E),
    (0xFE5E, 0xFE5D),
    (0xFF08, 0xFF09),
    (0xFF09, 0xFF08),
    (0xFF1C, 0xFF1E),
    (0xFF1E, 0xFF1C),
    (0xFF3B, 0xFF3D),
    (0xFF3D, 0xFF3B),
    (0xFF5B, 0xFF5D),
    (0xFF5D, 0xFF5B),
    (0xFF5F, 0xFF60),
    (0xFF60, 0xFF5F),
    (0xFF62, 0xFF63),
    (0xFF63, 0xFF62),
];

/// Paired bracket data (from BidiBrackets.txt).
/// Format: (opening_bracket, closing_bracket).
static BRACKET_PAIRS: &[(char, char)] = &[
    ('(', ')'),
    ('[', ']'),
    ('{', '}'),
    ('\u{0F3A}', '\u{0F3B}'),
    ('\u{0F3C}', '\u{0F3D}'),
    ('\u{169B}', '\u{169C}'),
    ('\u{2045}', '\u{2046}'),
    ('\u{207D}', '\u{207E}'),
    ('\u{208D}', '\u{208E}'),
    ('\u{2308}', '\u{2309}'),
    ('\u{230A}', '\u{230B}'),
    ('\u{2329}', '\u{232A}'),
    ('\u{2768}', '\u{2769}'),
    ('\u{276A}', '\u{276B}'),
    ('\u{276C}', '\u{276D}'),
    ('\u{276E}', '\u{276F}'),
    ('\u{2770}', '\u{2771}'),
    ('\u{2772}', '\u{2773}'),
    ('\u{2774}', '\u{2775}'),
    ('\u{27C5}', '\u{27C6}'),
    ('\u{27E6}', '\u{27E7}'),
    ('\u{27E8}', '\u{27E9}'),
    ('\u{27EA}', '\u{27EB}'),
    ('\u{27EC}', '\u{27ED}'),
    ('\u{27EE}', '\u{27EF}'),
    ('\u{2983}', '\u{2984}'),
    ('\u{2985}', '\u{2986}'),
    ('\u{2987}', '\u{2988}'),
    ('\u{2989}', '\u{298A}'),
    ('\u{298B}', '\u{298C}'),
    ('\u{298D}', '\u{298E}'),
    ('\u{298F}', '\u{2990}'),
    ('\u{2991}', '\u{2992}'),
    ('\u{2993}', '\u{2994}'),
    ('\u{2995}', '\u{2996}'),
    ('\u{2997}', '\u{2998}'),
    ('\u{29D8}', '\u{29D9}'),
    ('\u{29DA}', '\u{29DB}'),
    ('\u{29FC}', '\u{29FD}'),
    ('\u{2E22}', '\u{2E23}'),
    ('\u{2E24}', '\u{2E25}'),
    ('\u{2E26}', '\u{2E27}'),
    ('\u{2E28}', '\u{2E29}'),
    ('\u{2E55}', '\u{2E56}'),
    ('\u{2E57}', '\u{2E58}'),
    ('\u{2E59}', '\u{2E5A}'),
    ('\u{2E5B}', '\u{2E5C}'),
    ('\u{3008}', '\u{3009}'),
    ('\u{300A}', '\u{300B}'),
    ('\u{300C}', '\u{300D}'),
    ('\u{300E}', '\u{300F}'),
    ('\u{3010}', '\u{3011}'),
    ('\u{3014}', '\u{3015}'),
    ('\u{3016}', '\u{3017}'),
    ('\u{3018}', '\u{3019}'),
    ('\u{301A}', '\u{301B}'),
    ('\u{FE59}', '\u{FE5A}'),
    ('\u{FE5B}', '\u{FE5C}'),
    ('\u{FE5D}', '\u{FE5E}'),
    ('\u{FF08}', '\u{FF09}'),
    ('\u{FF3B}', '\u{FF3D}'),
    ('\u{FF5B}', '\u{FF5D}'),
    ('\u{FF5F}', '\u{FF60}'),
    ('\u{FF62}', '\u{FF63}'),
];

/// Check if a character is a canonical equivalent bracket.
/// U+2329 ↔ U+3008, U+232A ↔ U+3009
pub fn canonical_bracket(ch: char) -> char {
    match ch {
        '\u{2329}' => '\u{3008}',
        '\u{232A}' => '\u{3009}',
        _ => ch,
    }
}

#[cfg(test)]
#[path = "tables_test.rs"]
mod tests;
