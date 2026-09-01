use super::*;

// ===================================================================
// 1. bidi_class() for ASCII letters (should be L)
// ===================================================================

#[test]
fn bidi_class_ascii_uppercase_letters_are_l() {
    for ch in 'A'..='Z' {
        assert_eq!(
            bidi_class(ch),
            BidiClass::L,
            "Expected L for uppercase letter '{}'",
            ch
        );
    }
}

#[test]
fn bidi_class_ascii_lowercase_letters_are_l() {
    for ch in 'a'..='z' {
        assert_eq!(
            bidi_class(ch),
            BidiClass::L,
            "Expected L for lowercase letter '{}'",
            ch
        );
    }
}

// ===================================================================
// 2. bidi_class() for ASCII digits (should be EN)
// ===================================================================

#[test]
fn bidi_class_ascii_digits_are_en() {
    for ch in '0'..='9' {
        assert_eq!(
            bidi_class(ch),
            BidiClass::EN,
            "Expected EN for digit '{}'",
            ch
        );
    }
}

// ===================================================================
// 3. bidi_class() for common punctuation
// ===================================================================

#[test]
fn bidi_class_common_separators() {
    // CS: comma, period, slash, colon
    assert_eq!(bidi_class(','), BidiClass::CS);
    assert_eq!(bidi_class('.'), BidiClass::CS);
    assert_eq!(bidi_class('/'), BidiClass::CS);
    assert_eq!(bidi_class(':'), BidiClass::CS);
}

#[test]
fn bidi_class_european_separators() {
    // ES: plus, minus/hyphen
    assert_eq!(bidi_class('+'), BidiClass::ES);
    assert_eq!(bidi_class('-'), BidiClass::ES);
}

#[test]
fn bidi_class_european_terminators() {
    // ET: #, $, %
    assert_eq!(bidi_class('#'), BidiClass::ET);
    assert_eq!(bidi_class('$'), BidiClass::ET);
    assert_eq!(bidi_class('%'), BidiClass::ET);
}

#[test]
fn bidi_class_other_neutrals_punctuation() {
    // ON: various punctuation and symbols
    assert_eq!(bidi_class('!'), BidiClass::ON);
    assert_eq!(bidi_class('"'), BidiClass::ON);
    assert_eq!(bidi_class('&'), BidiClass::ON);
    assert_eq!(bidi_class('\''), BidiClass::ON);
    assert_eq!(bidi_class('('), BidiClass::ON);
    assert_eq!(bidi_class(')'), BidiClass::ON);
    assert_eq!(bidi_class('*'), BidiClass::ON);
    assert_eq!(bidi_class(';'), BidiClass::ON);
    assert_eq!(bidi_class('?'), BidiClass::ON);
    assert_eq!(bidi_class('@'), BidiClass::ON);
    assert_eq!(bidi_class('['), BidiClass::ON);
    assert_eq!(bidi_class('\\'), BidiClass::ON);
    assert_eq!(bidi_class(']'), BidiClass::ON);
    assert_eq!(bidi_class('^'), BidiClass::ON);
    assert_eq!(bidi_class('_'), BidiClass::ON);
    assert_eq!(bidi_class('`'), BidiClass::ON);
    assert_eq!(bidi_class('{'), BidiClass::ON);
    assert_eq!(bidi_class('|'), BidiClass::ON);
    assert_eq!(bidi_class('}'), BidiClass::ON);
    assert_eq!(bidi_class('~'), BidiClass::ON);
}

// ===================================================================
// 4. bidi_class() for Arabic/Hebrew characters (should be R or AL)
// ===================================================================

#[test]
fn bidi_class_hebrew_letters_are_r() {
    // Hebrew Alef through Tav (U+05D0..U+05EA)
    assert_eq!(bidi_class('\u{05D0}'), BidiClass::R); // Alef
    assert_eq!(bidi_class('\u{05D1}'), BidiClass::R); // Bet
    assert_eq!(bidi_class('\u{05DA}'), BidiClass::R); // Final Kaf
    assert_eq!(bidi_class('\u{05EA}'), BidiClass::R); // Tav
}

#[test]
fn bidi_class_hebrew_misc_r() {
    assert_eq!(bidi_class('\u{05BE}'), BidiClass::R); // Hebrew Maqaf
    assert_eq!(bidi_class('\u{05C0}'), BidiClass::R); // Hebrew Paseq
    assert_eq!(bidi_class('\u{05C3}'), BidiClass::R); // Hebrew Sof Pasuq
    assert_eq!(bidi_class('\u{05C6}'), BidiClass::R); // Hebrew Nun Hafukha
}

#[test]
fn bidi_class_arabic_letters_are_al() {
    // Arabic letters (U+061D..U+064A)
    assert_eq!(bidi_class('\u{0627}'), BidiClass::AL); // Arabic Alef
    assert_eq!(bidi_class('\u{0628}'), BidiClass::AL); // Arabic Ba
    assert_eq!(bidi_class('\u{062A}'), BidiClass::AL); // Arabic Ta
    assert_eq!(bidi_class('\u{0644}'), BidiClass::AL); // Arabic Lam
    assert_eq!(bidi_class('\u{0645}'), BidiClass::AL); // Arabic Meem
    assert_eq!(bidi_class('\u{064A}'), BidiClass::AL); // Arabic Ya
}

#[test]
fn bidi_class_arabic_indic_digits_are_an() {
    // Arabic-Indic digits (U+0660..U+0669)
    assert_eq!(bidi_class('\u{0660}'), BidiClass::AN); // Arabic-Indic zero
    assert_eq!(bidi_class('\u{0665}'), BidiClass::AN); // Arabic-Indic five
    assert_eq!(bidi_class('\u{0669}'), BidiClass::AN); // Arabic-Indic nine
}

#[test]
fn bidi_class_arabic_presentation_forms_al() {
    // Arabic Presentation Forms-B (U+FE70..U+FEFC)
    assert_eq!(bidi_class('\u{FE70}'), BidiClass::AL);
    assert_eq!(bidi_class('\u{FEFC}'), BidiClass::AL);
}

#[test]
fn bidi_class_hebrew_presentation_forms_r() {
    // Hebrew presentation forms (U+FB1D, U+FB1F..U+FB28, U+FB2A..U+FB4F)
    assert_eq!(bidi_class('\u{FB1D}'), BidiClass::R);
    assert_eq!(bidi_class('\u{FB1F}'), BidiClass::R);
    assert_eq!(bidi_class('\u{FB4F}'), BidiClass::R);
}

#[test]
fn bidi_class_supplementary_rtl_scripts() {
    // Cypriot, Aramaic, etc. (U+10800..U+10FFF)
    assert_eq!(bidi_class('\u{10800}'), BidiClass::R);
    assert_eq!(bidi_class('\u{10900}'), BidiClass::R);
    assert_eq!(bidi_class('\u{10FFF}'), BidiClass::R);
}

// ===================================================================
// 5. bidi_class() for whitespace (should be WS or S)
// ===================================================================

#[test]
fn bidi_class_space_is_ws() {
    assert_eq!(bidi_class(' '), BidiClass::WS);
}

#[test]
fn bidi_class_form_feed_is_ws() {
    assert_eq!(bidi_class('\u{000C}'), BidiClass::WS); // Form feed
}

#[test]
fn bidi_class_tab_is_s() {
    assert_eq!(bidi_class('\t'), BidiClass::S); // Segment separator
}

#[test]
fn bidi_class_newline_cr_are_b() {
    // Paragraph separators
    assert_eq!(bidi_class('\n'), BidiClass::B);
    assert_eq!(bidi_class('\r'), BidiClass::B);
}

#[test]
fn bidi_class_nel_is_b() {
    // NEL (U+0085) is paragraph separator
    assert_eq!(bidi_class('\u{0085}'), BidiClass::B);
}

#[test]
fn bidi_class_unicode_spaces_are_ws() {
    // Various Unicode spaces (U+2000..U+200A)
    assert_eq!(bidi_class('\u{2000}'), BidiClass::WS); // En quad
    assert_eq!(bidi_class('\u{2003}'), BidiClass::WS); // Em space
    assert_eq!(bidi_class('\u{2009}'), BidiClass::WS); // Thin space
    assert_eq!(bidi_class('\u{200A}'), BidiClass::WS); // Hair space
}

#[test]
fn bidi_class_line_separator_is_ws() {
    assert_eq!(bidi_class('\u{2028}'), BidiClass::WS);
}

#[test]
fn bidi_class_paragraph_separator_u2029_is_b() {
    assert_eq!(bidi_class('\u{2029}'), BidiClass::B);
}

#[test]
fn bidi_class_ideographic_space_is_ws() {
    assert_eq!(bidi_class('\u{3000}'), BidiClass::WS);
}

#[test]
fn bidi_class_medium_math_space_is_ws() {
    assert_eq!(bidi_class('\u{205F}'), BidiClass::WS);
}

// ===================================================================
// 6. bidi_mirror() for bracket pairs
// ===================================================================

#[test]
fn bidi_mirror_parentheses() {
    assert_eq!(bidi_mirror('('), Some(')'));
    assert_eq!(bidi_mirror(')'), Some('('));
}

#[test]
fn bidi_mirror_square_brackets() {
    assert_eq!(bidi_mirror('['), Some(']'));
    assert_eq!(bidi_mirror(']'), Some('['));
}

#[test]
fn bidi_mirror_curly_braces() {
    assert_eq!(bidi_mirror('{'), Some('}'));
    assert_eq!(bidi_mirror('}'), Some('{'));
}

#[test]
fn bidi_mirror_angle_brackets() {
    assert_eq!(bidi_mirror('<'), Some('>'));
    assert_eq!(bidi_mirror('>'), Some('<'));
}

#[test]
fn bidi_mirror_guillemets() {
    // U+00AB LAQUO, U+00BB RAQUO
    assert_eq!(bidi_mirror('\u{00AB}'), Some('\u{00BB}'));
    assert_eq!(bidi_mirror('\u{00BB}'), Some('\u{00AB}'));
}

#[test]
fn bidi_mirror_math_angle_brackets() {
    // U+27E8 MATHEMATICAL LEFT ANGLE BRACKET
    // U+27E9 MATHEMATICAL RIGHT ANGLE BRACKET
    assert_eq!(bidi_mirror('\u{27E8}'), Some('\u{27E9}'));
    assert_eq!(bidi_mirror('\u{27E9}'), Some('\u{27E8}'));
}

#[test]
fn bidi_mirror_ceiling_floor() {
    // Ceiling: U+2308 / U+2309
    assert_eq!(bidi_mirror('\u{2308}'), Some('\u{2309}'));
    assert_eq!(bidi_mirror('\u{2309}'), Some('\u{2308}'));
    // Floor: U+230A / U+230B
    assert_eq!(bidi_mirror('\u{230A}'), Some('\u{230B}'));
    assert_eq!(bidi_mirror('\u{230B}'), Some('\u{230A}'));
}

#[test]
fn bidi_mirror_fullwidth_brackets() {
    // Fullwidth parentheses U+FF08/U+FF09
    assert_eq!(bidi_mirror('\u{FF08}'), Some('\u{FF09}'));
    assert_eq!(bidi_mirror('\u{FF09}'), Some('\u{FF08}'));
    // Fullwidth square brackets U+FF3B/U+FF3D
    assert_eq!(bidi_mirror('\u{FF3B}'), Some('\u{FF3D}'));
    assert_eq!(bidi_mirror('\u{FF3D}'), Some('\u{FF3B}'));
    // Fullwidth curly braces U+FF5B/U+FF5D
    assert_eq!(bidi_mirror('\u{FF5B}'), Some('\u{FF5D}'));
    assert_eq!(bidi_mirror('\u{FF5D}'), Some('\u{FF5B}'));
}

#[test]
fn bidi_mirror_math_operators_symmetric() {
    // Less-than or equal / Greater-than or equal
    assert_eq!(bidi_mirror('\u{2264}'), Some('\u{2265}'));
    assert_eq!(bidi_mirror('\u{2265}'), Some('\u{2264}'));
    // Much less-than / Much greater-than
    assert_eq!(bidi_mirror('\u{226A}'), Some('\u{226B}'));
    assert_eq!(bidi_mirror('\u{226B}'), Some('\u{226A}'));
    // Subset / Superset
    assert_eq!(bidi_mirror('\u{2282}'), Some('\u{2283}'));
    assert_eq!(bidi_mirror('\u{2283}'), Some('\u{2282}'));
}

// ===================================================================
// 7. bidi_mirror() for non-bracket chars (should return None)
// ===================================================================

#[test]
fn bidi_mirror_returns_none_for_ascii_letters() {
    assert_eq!(bidi_mirror('A'), None);
    assert_eq!(bidi_mirror('z'), None);
    assert_eq!(bidi_mirror('M'), None);
}

#[test]
fn bidi_mirror_returns_none_for_digits() {
    assert_eq!(bidi_mirror('0'), None);
    assert_eq!(bidi_mirror('5'), None);
    assert_eq!(bidi_mirror('9'), None);
}

#[test]
fn bidi_mirror_returns_none_for_space_and_newline() {
    assert_eq!(bidi_mirror(' '), None);
    assert_eq!(bidi_mirror('\n'), None);
    assert_eq!(bidi_mirror('\t'), None);
}

#[test]
fn bidi_mirror_returns_none_for_non_mirrored_punctuation() {
    assert_eq!(bidi_mirror('!'), None);
    assert_eq!(bidi_mirror(','), None);
    assert_eq!(bidi_mirror('.'), None);
    assert_eq!(bidi_mirror(';'), None);
    assert_eq!(bidi_mirror('?'), None);
    assert_eq!(bidi_mirror('@'), None);
    assert_eq!(bidi_mirror('#'), None);
}

#[test]
fn bidi_mirror_returns_none_for_cjk() {
    assert_eq!(bidi_mirror('\u{4E00}'), None); // CJK unified ideograph
}

// ===================================================================
// 8. bracket_type() for opening brackets
// ===================================================================

#[test]
fn bracket_type_open_parenthesis() {
    assert_eq!(bracket_type('('), BracketType::Open(')'));
}

#[test]
fn bracket_type_open_square_bracket() {
    assert_eq!(bracket_type('['), BracketType::Open(']'));
}

#[test]
fn bracket_type_open_curly_brace() {
    assert_eq!(bracket_type('{'), BracketType::Open('}'));
}

#[test]
fn bracket_type_open_unicode_brackets() {
    // Tibetan: U+0F3A opens U+0F3B
    assert_eq!(bracket_type('\u{0F3A}'), BracketType::Open('\u{0F3B}'));
    // Ogham: U+169B opens U+169C
    assert_eq!(bracket_type('\u{169B}'), BracketType::Open('\u{169C}'));
    // Superscript paren: U+207D opens U+207E
    assert_eq!(bracket_type('\u{207D}'), BracketType::Open('\u{207E}'));
    // Subscript paren: U+208D opens U+208E
    assert_eq!(bracket_type('\u{208D}'), BracketType::Open('\u{208E}'));
    // Math left angle: U+27E8 opens U+27E9
    assert_eq!(bracket_type('\u{27E8}'), BracketType::Open('\u{27E9}'));
    // CJK angle: U+3008 opens U+3009
    assert_eq!(bracket_type('\u{3008}'), BracketType::Open('\u{3009}'));
    // Fullwidth paren: U+FF08 opens U+FF09
    assert_eq!(bracket_type('\u{FF08}'), BracketType::Open('\u{FF09}'));
}

// ===================================================================
// 9. bracket_type() for closing brackets
// ===================================================================

#[test]
fn bracket_type_close_parenthesis() {
    assert_eq!(bracket_type(')'), BracketType::Close('('));
}

#[test]
fn bracket_type_close_square_bracket() {
    assert_eq!(bracket_type(']'), BracketType::Close('['));
}

#[test]
fn bracket_type_close_curly_brace() {
    assert_eq!(bracket_type('}'), BracketType::Close('{'));
}

#[test]
fn bracket_type_close_unicode_brackets() {
    // Tibetan: U+0F3B closes U+0F3A
    assert_eq!(bracket_type('\u{0F3B}'), BracketType::Close('\u{0F3A}'));
    // Ogham: U+169C closes U+169B
    assert_eq!(bracket_type('\u{169C}'), BracketType::Close('\u{169B}'));
    // Superscript paren: U+207E closes U+207D
    assert_eq!(bracket_type('\u{207E}'), BracketType::Close('\u{207D}'));
    // CJK angle: U+3009 closes U+3008
    assert_eq!(bracket_type('\u{3009}'), BracketType::Close('\u{3008}'));
    // Fullwidth paren: U+FF09 closes U+FF08
    assert_eq!(bracket_type('\u{FF09}'), BracketType::Close('\u{FF08}'));
}

// ===================================================================
// 10. bracket_type() for non-brackets (should return None)
// ===================================================================

#[test]
fn bracket_type_none_for_letters() {
    assert_eq!(bracket_type('A'), BracketType::None);
    assert_eq!(bracket_type('z'), BracketType::None);
    assert_eq!(bracket_type('M'), BracketType::None);
}

#[test]
fn bracket_type_none_for_digits() {
    assert_eq!(bracket_type('0'), BracketType::None);
    assert_eq!(bracket_type('5'), BracketType::None);
    assert_eq!(bracket_type('9'), BracketType::None);
}

#[test]
fn bracket_type_none_for_non_bracket_punctuation() {
    assert_eq!(bracket_type('!'), BracketType::None);
    assert_eq!(bracket_type(','), BracketType::None);
    assert_eq!(bracket_type('.'), BracketType::None);
    assert_eq!(bracket_type(';'), BracketType::None);
    assert_eq!(bracket_type(':'), BracketType::None);
    assert_eq!(bracket_type('+'), BracketType::None);
    assert_eq!(bracket_type('-'), BracketType::None);
    assert_eq!(bracket_type('='), BracketType::None);
    assert_eq!(bracket_type('*'), BracketType::None);
}

#[test]
fn bracket_type_none_for_angle_brackets() {
    // < and > are mirrored but NOT in the bracket_pairs table
    assert_eq!(bracket_type('<'), BracketType::None);
    assert_eq!(bracket_type('>'), BracketType::None);
}

#[test]
fn bracket_type_none_for_spaces_and_controls() {
    assert_eq!(bracket_type(' '), BracketType::None);
    assert_eq!(bracket_type('\n'), BracketType::None);
    assert_eq!(bracket_type('\t'), BracketType::None);
}

#[test]
fn bracket_type_none_for_cjk_ideograph() {
    assert_eq!(bracket_type('\u{4E00}'), BracketType::None);
}

// ===================================================================
// 11. Edge cases
// ===================================================================

#[test]
fn bidi_class_null_char() {
    // U+0000 is in BN range (0x00..=0x08)
    assert_eq!(bidi_class('\0'), BidiClass::BN);
}

#[test]
fn bidi_class_max_ascii_del() {
    // U+007F DEL is BN
    assert_eq!(bidi_class('\u{007F}'), BidiClass::BN);
}

#[test]
fn bidi_class_last_printable_ascii() {
    // U+007E ~ is ON
    assert_eq!(bidi_class('~'), BidiClass::ON);
}

#[test]
fn bidi_class_first_non_ascii() {
    // U+0080 is BN (Latin-1 supplement control)
    assert_eq!(bidi_class('\u{0080}'), BidiClass::BN);
}

#[test]
fn bidi_class_nbsp() {
    // U+00A0 NBSP is CS
    assert_eq!(bidi_class('\u{00A0}'), BidiClass::CS);
}

#[test]
fn bidi_class_soft_hyphen() {
    // U+00AD Soft Hyphen is BN
    assert_eq!(bidi_class('\u{00AD}'), BidiClass::BN);
}

#[test]
fn bidi_class_bom() {
    // U+FEFF BOM/ZWNBSP is BN
    assert_eq!(bidi_class('\u{FEFF}'), BidiClass::BN);
}

#[test]
fn bidi_class_replacement_char() {
    // U+FFFD Replacement Character is ON
    assert_eq!(bidi_class('\u{FFFD}'), BidiClass::ON);
}

#[test]
fn bidi_class_zero_width_chars() {
    // U+200B ZWSP, U+200C ZWNJ, U+200D ZWJ are BN
    assert_eq!(bidi_class('\u{200B}'), BidiClass::BN);
    assert_eq!(bidi_class('\u{200C}'), BidiClass::BN);
    assert_eq!(bidi_class('\u{200D}'), BidiClass::BN);
}

#[test]
fn bidi_class_explicit_direction_marks() {
    // U+200E LRM is L, U+200F RLM is R
    assert_eq!(bidi_class('\u{200E}'), BidiClass::L);
    assert_eq!(bidi_class('\u{200F}'), BidiClass::R);
}

#[test]
fn bidi_class_explicit_formatting_characters() {
    assert_eq!(bidi_class('\u{202A}'), BidiClass::LRE);
    assert_eq!(bidi_class('\u{202B}'), BidiClass::RLE);
    assert_eq!(bidi_class('\u{202C}'), BidiClass::PDF);
    assert_eq!(bidi_class('\u{202D}'), BidiClass::LRO);
    assert_eq!(bidi_class('\u{202E}'), BidiClass::RLO);
    assert_eq!(bidi_class('\u{2066}'), BidiClass::LRI);
    assert_eq!(bidi_class('\u{2067}'), BidiClass::RLI);
    assert_eq!(bidi_class('\u{2068}'), BidiClass::FSI);
    assert_eq!(bidi_class('\u{2069}'), BidiClass::PDI);
}

#[test]
fn bidi_class_combining_diacritical_marks_are_nsm() {
    // U+0300..U+036F Combining Diacritical Marks
    assert_eq!(bidi_class('\u{0300}'), BidiClass::NSM);
    assert_eq!(bidi_class('\u{0301}'), BidiClass::NSM); // Combining acute
    assert_eq!(bidi_class('\u{036F}'), BidiClass::NSM);
}

#[test]
fn bidi_class_currency_symbols_are_et() {
    // U+00A2..U+00A5 (cent, pound, currency, yen)
    assert_eq!(bidi_class('\u{00A2}'), BidiClass::ET);
    assert_eq!(bidi_class('\u{00A3}'), BidiClass::ET);
    assert_eq!(bidi_class('\u{00A5}'), BidiClass::ET);
    // U+20AC Euro sign
    assert_eq!(bidi_class('\u{20AC}'), BidiClass::ET);
}

#[test]
fn bidi_class_cjk_unified_ideographs_are_l() {
    assert_eq!(bidi_class('\u{4E00}'), BidiClass::L); // First CJK unified
    assert_eq!(bidi_class('\u{9FFF}'), BidiClass::L); // Last CJK unified
}

#[test]
fn bidi_class_hiragana_katakana_are_l() {
    assert_eq!(bidi_class('\u{3042}'), BidiClass::L); // Hiragana A
    assert_eq!(bidi_class('\u{30A2}'), BidiClass::L); // Katakana A
}

#[test]
fn bidi_class_hangul_syllables_are_l() {
    assert_eq!(bidi_class('\u{AC00}'), BidiClass::L); // First Hangul syllable
    assert_eq!(bidi_class('\u{D7A3}'), BidiClass::L); // Last Hangul syllable
}

#[test]
fn bidi_class_braille_is_l() {
    assert_eq!(bidi_class('\u{2800}'), BidiClass::L);
    assert_eq!(bidi_class('\u{28FF}'), BidiClass::L);
}

#[test]
fn bidi_class_ascii_controls_are_bn() {
    // U+0001..U+0008 are BN
    assert_eq!(bidi_class('\u{0001}'), BidiClass::BN);
    assert_eq!(bidi_class('\u{0008}'), BidiClass::BN);
    // U+000E..U+001B are BN
    assert_eq!(bidi_class('\u{000E}'), BidiClass::BN);
    assert_eq!(bidi_class('\u{001B}'), BidiClass::BN);
}

#[test]
fn bidi_class_default_for_unlisted_non_ascii_is_l() {
    // Unassigned or unlisted code points default to L
    // Latin Extended-A (not in range table explicitly)
    assert_eq!(bidi_class('\u{0100}'), BidiClass::L); // Latin A with macron
    assert_eq!(bidi_class('\u{0250}'), BidiClass::L); // IPA Extensions
}

#[test]
fn bidi_mirror_null_char() {
    assert_eq!(bidi_mirror('\0'), None);
}

#[test]
fn bidi_mirror_max_ascii() {
    assert_eq!(bidi_mirror('\u{007F}'), None);
}

#[test]
fn bracket_type_null_char() {
    assert_eq!(bracket_type('\0'), BracketType::None);
}

#[test]
fn bracket_type_max_ascii() {
    assert_eq!(bracket_type('\u{007F}'), BracketType::None);
}

// ===================================================================
// Consistency: all bracket pairs have correct open/close classification
// ===================================================================

#[test]
fn all_bracket_pairs_open_and_close_consistent() {
    // For every pair in BRACKET_PAIRS, the open char should be Open
    // and the close char should be Close.
    for &(open, close) in BRACKET_PAIRS {
        assert_eq!(
            bracket_type(open),
            BracketType::Open(close),
            "Expected Open({:?}) for {:?} (U+{:04X})",
            close,
            open,
            open as u32
        );
        assert_eq!(
            bracket_type(close),
            BracketType::Close(open),
            "Expected Close({:?}) for {:?} (U+{:04X})",
            open,
            close,
            close as u32
        );
    }
}

// ===================================================================
// Consistency: all mirror pairs are symmetric
// ===================================================================

#[test]
fn all_mirror_pairs_are_symmetric() {
    // For every (from, to) in MIRROR_PAIRS, bidi_mirror(from) == Some(to)
    for &(from, to) in MIRROR_PAIRS {
        let from_char = char::from_u32(from).unwrap();
        let to_char = char::from_u32(to).unwrap();
        assert_eq!(
            bidi_mirror(from_char),
            Some(to_char),
            "Mirror of U+{:04X} should be U+{:04X}",
            from,
            to
        );
    }
}

// ===================================================================
// canonical_bracket()
// ===================================================================

#[test]
fn canonical_bracket_maps_deprecated_angle_brackets() {
    assert_eq!(canonical_bracket('\u{2329}'), '\u{3008}');
    assert_eq!(canonical_bracket('\u{232A}'), '\u{3009}');
}

#[test]
fn canonical_bracket_passes_through_others() {
    assert_eq!(canonical_bracket('('), '(');
    assert_eq!(canonical_bracket(')'), ')');
    assert_eq!(canonical_bracket('A'), 'A');
    assert_eq!(canonical_bracket('\u{3008}'), '\u{3008}');
}
