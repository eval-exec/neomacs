use super::*;

#[test]
fn glyph_plan_only_allows_independent_plain_scalars_to_use_simple_copy() {
    assert_eq!(
        composition_glyph_plan("A©"),
        CompositionGlyphPlan::SimpleCopy
    );
    assert_eq!(
        composition_glyph_plan("e\u{301}"),
        CompositionGlyphPlan::Shape
    );
    assert_eq!(composition_glyph_plan("سلام"), CompositionGlyphPlan::Shape);
    assert_eq!(
        composition_glyph_plan("a\u{200d}b"),
        CompositionGlyphPlan::Shape
    );
}

// --- base_width_cols ---

#[test]
fn ascii_is_one_column() {
    assert_eq!(base_width_cols('a'), 1);
    assert_eq!(base_width_cols('Z'), 1);
}

#[test]
fn cjk_is_two_columns() {
    // U+4E2D (中) is a wide CJK ideograph.
    assert_eq!(base_width_cols('\u{4E2D}'), 2);
}

#[test]
fn emoji_base_is_two_columns() {
    // U+1F468 (👨) is wide via the char-width table.
    assert_eq!(base_width_cols('\u{1F468}'), 2);
}

#[test]
fn regional_indicator_is_forced_two_columns() {
    // Regional indicators are NOT in the wide char-width ranges, but must
    // reserve a 2-column cell so a composed flag does not overlap.
    assert_eq!(base_width_cols('\u{1F1EF}'), 2);
}

// --- continues_cluster ---

#[test]
fn extender_always_continues() {
    // Combining acute accent (U+0301) is a cluster extender regardless of
    // the preceding glyph.
    assert!(continues_cluster('\u{0301}', None));
    assert!(continues_cluster('\u{0301}', Some(('e', false))));
}

#[test]
fn char_after_zwj_continues() {
    // An emoji following a ZWJ continues the ZWJ sequence.
    assert!(continues_cluster('\u{1F469}', Some(('\u{200D}', false))));
}

#[test]
fn second_regional_indicator_after_lone_one_continues() {
    // Flag pair: a regional indicator after a lone regional indicator.
    assert!(continues_cluster('\u{1F1F5}', Some(('\u{1F1EF}', true))));
}

#[test]
fn regional_indicator_after_completed_flag_does_not_continue() {
    // The tail is a Composite (is_lone_regional_indicator == false), so a
    // third regional indicator starts a fresh flag instead of merging.
    assert!(!continues_cluster('\u{1F1F5}', Some(('\u{1F1EF}', false))));
}

#[test]
fn ordinary_char_does_not_continue() {
    assert!(!continues_cluster('b', Some(('a', false))));
    assert!(!continues_cluster('a', None));
}

#[test]
fn zwj_itself_continues_as_extender() {
    // The ZWJ joins the base it follows.
    assert!(continues_cluster('\u{200D}', Some(('\u{1F468}', false))));
}

// --- complex-script detection (Phase 3) ---

#[test]
fn complex_script_classifies_arabic_and_indic() {
    assert_eq!(complex_script('\u{0627}'), Some(ComplexScript::Arabic)); // ا
    assert_eq!(complex_script('\u{FE8D}'), Some(ComplexScript::Arabic)); // presentation form
    assert_eq!(complex_script('\u{0905}'), Some(ComplexScript::Devanagari)); // अ
    assert_eq!(complex_script('\u{0B95}'), Some(ComplexScript::Tamil)); // க
    assert_eq!(complex_script('\u{0E01}'), Some(ComplexScript::Thai)); // ก
}

#[test]
fn complex_script_is_none_for_latin_and_cjk() {
    assert_eq!(complex_script('a'), None);
    assert_eq!(complex_script('\u{4E2D}'), None); // 中 (CJK)
    assert_eq!(complex_script('\u{1F468}'), None); // emoji
    assert!(!needs_complex_shaping('a'));
    assert!(needs_complex_shaping('\u{0627}'));
}

#[test]
fn continues_complex_run_groups_same_script_only() {
    // Two Arabic letters continue the same run.
    assert!(continues_complex_run('\u{0644}', Some(('\u{0627}', false))));
    // Different complex scripts do not merge into one run.
    assert!(!continues_complex_run(
        '\u{0905}',
        Some(('\u{0627}', false))
    ));
    // A complex char after a non-complex one (or at row start) starts fresh.
    assert!(!continues_complex_run('\u{0627}', Some(('a', false))));
    assert!(!continues_complex_run('\u{0627}', None));
    // Latin never forms a contextual run.
    assert!(!continues_complex_run('b', Some(('a', false))));
}
