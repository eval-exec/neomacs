use super::*;

#[test]
fn escape_octal_char_in_complex_block_measures_naturally_not_complex() {
    // U+FDD0 is a noncharacter shown as the ASCII octal escape `\176720`,
    // but it sits in the Arabic Presentation Forms block so it needs complex
    // shaping. The substitute is ASCII and must be measured naturally, else
    // every escape digit inherits FDD0's wide Arabic-shaped advance.
    assert!(crate::composition::needs_complex_shaping('\u{fdd0}'));
    assert!(crate::display_source::is_escape_glyph_octal('\u{fdd0}'));
    assert_eq!(
        DisplaySourceAppendMeasurementKind::for_char('\u{fdd0}'),
        DisplaySourceAppendMeasurementKind::NaturalRenderedSource
    );
    // A real Arabic letter (printable, drawn as itself) keeps complex shaping.
    assert_eq!(
        DisplaySourceAppendMeasurementKind::for_char('\u{0645}'), // MEEM
        DisplaySourceAppendMeasurementKind::ResolvedComplexRun
    );
    // Plain ASCII stays natural.
    assert_eq!(
        DisplaySourceAppendMeasurementKind::for_char('a'),
        DisplaySourceAppendMeasurementKind::NaturalRenderedSource
    );
}
