use super::*;

use neovm_edit::buffer::{Buffer, BufferId, BufferTextBackendKind, EmacsByteRange};

use neovm_value::heap_types::LispString;

use neovm_value::value::Value;

#[test]
fn translate_groups() {
    crate::test_utils::init_test_tracing();
    // Emacs \( \) → Rust ( )
    assert_eq!(translate_emacs_regex("\\(foo\\)"), "(foo)");
}

#[test]
fn translate_alternation() {
    crate::test_utils::init_test_tracing();
    // Emacs \| → Rust |
    assert_eq!(translate_emacs_regex("foo\\|bar"), "foo|bar");
}

#[test]
fn translate_literal_parens() {
    crate::test_utils::init_test_tracing();
    // Emacs literal ( ) → Rust \( \)
    assert_eq!(translate_emacs_regex("(foo)"), "\\(foo\\)");
}

#[test]
fn translate_literal_braces() {
    crate::test_utils::init_test_tracing();
    // Emacs literal { } → Rust \{ \}
    assert_eq!(translate_emacs_regex("{3}"), "\\{3\\}");
}

#[test]
fn translate_repetition_braces() {
    crate::test_utils::init_test_tracing();
    // Emacs \{3\} → Rust {3}
    assert_eq!(translate_emacs_regex("a\\{3\\}"), "a{3}");
}

#[test]
fn translate_literal_pipe() {
    crate::test_utils::init_test_tracing();
    // Emacs literal | → Rust \|
    assert_eq!(translate_emacs_regex("a|b"), "a\\|b");
}

#[test]
fn translate_word_boundary() {
    crate::test_utils::init_test_tracing();
    // Emacs \< \> → Rust \b
    assert_eq!(translate_emacs_regex("\\<word\\>"), "\\bword\\b");
}

#[test]
fn translate_symbol_boundary() {
    crate::test_utils::init_test_tracing();
    assert_eq!(translate_emacs_regex("\\_<word\\_>"), "\\bword\\b");
}

#[test]
fn translate_buffer_boundaries() {
    crate::test_utils::init_test_tracing();
    // Emacs \` → Rust \A, Emacs \' → Rust \z
    assert_eq!(translate_emacs_regex("\\`foo\\'"), "\\Afoo\\z");
}

#[test]
fn translate_character_class_passthrough() {
    crate::test_utils::init_test_tracing();
    // Character classes should pass through mostly unchanged
    assert_eq!(translate_emacs_regex("[a-z]"), "[a-z]");
    assert_eq!(translate_emacs_regex("[^0-9]"), "[^0-9]");
}

#[test]
fn translate_character_class_backslash_ranges_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(translate_emacs_regex("[+\\-*/=<>]"), "[+/=<>]");
}

#[test]
fn translate_easymenu_command_hint_regexp() {
    crate::test_utils::init_test_tracing();
    let emacs = r"^[^\]*\(\\\[\([^]]+\)]\)[^\]*$";
    assert_eq!(
        translate_emacs_regex(emacs),
        r"^[^\\]*(\\\[([^\]]+)])[^\\]*$"
    );
}

#[test]
fn replace_match_case_capitalizes_each_word_like_gnu() {
    crate::test_utils::init_test_tracing();
    assert_eq!(apply_match_case("[alice:5]", "Alice"), "[Alice:5]");
    assert_eq!(
        apply_match_case("h_hello w_world", "Hello World"),
        "H_Hello W_World"
    );
}

#[test]
fn replace_match_case_upcases_all_caps_matches() {
    crate::test_utils::init_test_tracing();
    assert_eq!(apply_match_case("foo-bar", "FOO"), "FOO-BAR");
}

#[test]
fn translate_reversed_range_classes() {
    crate::test_utils::init_test_tracing();
    // Reversed ranges are empty in Emacs.
    assert_eq!(translate_emacs_regex("[z-a]"), "[^\\s\\S]");
    assert_eq!(translate_emacs_regex("[^z-a]"), "[\\s\\S]");
}

#[test]
fn translate_backslash_w() {
    crate::test_utils::init_test_tracing();
    assert_eq!(translate_emacs_regex("\\w+"), "\\w+");
}

#[test]
fn compile_search_pattern_uses_backref_engine_for_supported_captures() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\([a-z]+\\)-\\([0-9]+\\)"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_uses_backref_engine_for_noncapturing_groups() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\(?:foo\\|bar\\)+"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_syntax_classes_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\(defun\\|defvar\\)\\s-+\\(\\w+\\)"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_category_classes_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("[ \t]\\|\\c|.\\|.\\c|"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_digit_classes_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\d+"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_char_class_escapes_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("[\\w-]+"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
    assert!(matches!(
        compile_search_pattern(&lisp_pat("[\\s-]+"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_lazy_quantifiers_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("a.*?b"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
    assert!(matches!(
        compile_search_pattern(&lisp_pat("a\\{2,4\\}?b"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_open_interval_quantifiers_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("a\\{,2\\}b"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_explicit_numbered_groups_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\(?1:[^}]*\\)"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\(?9:.*?\\)"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_symbol_boundaries_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\_<foo\\_>"), false),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn compile_search_pattern_routes_bracket_section_anchor_through_backref_engine() {
    crate::test_utils::init_test_tracing();
    assert!(matches!(
        compile_search_pattern(&lisp_pat("\\`\\[\\([^]]+\\)\\]\\'"), true),
        Ok(CompiledSearchPattern::Emacs(_))
    ));
}

#[test]
fn string_match_supported_capture_pattern_uses_backref_engine_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result =
        string_match_full_with_case_fold("\\([a-z]+\\)-\\([0-9]+\\)", "foo-123", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 7)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 3)));
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(4, 7)));
}

#[test]
fn string_match_treats_postfix_after_buffer_start_anchor_as_literal_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold(
        "\\`+\\([0-9]+\\)\\(?::\\([0-9]+\\)\\)?\\'",
        "+12:4",
        0,
        false,
        &mut md,
    );

    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 5)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(1, 3)));
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(4, 5)));
}

#[test]
fn string_match_noncapturing_group_pattern_uses_backref_engine_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result =
        string_match_full_with_case_fold("\\(?:foo\\|bar\\)+", "foobar", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 6)));
    assert_eq!(md.group_count(), GNU_SEARCH_REGS_BASE_CAPACITY);
}

#[test]
fn string_match_postfix_repeats_whole_shy_group_with_multi_char_exactn() {
    crate::test_utils::init_test_tracing();

    let mut md = None;
    let result = string_match_full_with_case_fold("\\(?:ab\\)?c", "c", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    assert_eq!(
        match_group(md.expect("match data").group(0)),
        Some(MatchGroup::new(0, 1))
    );

    let mut md = None;
    let result = string_match_full_with_case_fold("\\(?:ab\\)*c", "abababc", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    assert_eq!(
        match_group(md.expect("match data").group(0)),
        Some(MatchGroup::new(0, 7))
    );

    let mut md = None;
    let result = string_match_full_with_case_fold("\\(?:ab\\)+c", "c", 0, false, &mut md);
    assert_eq!(result, Ok(None));
}

#[test]
fn string_match_org_list_item_optional_counter_clause_can_be_absent() {
    crate::test_utils::init_test_tracing();
    let pattern = concat!(
        "^[ \t]*",
        "\\(\\(?:[-+*]\\|\\(?:[0-9]+\\|[A-Za-z]\\)[.)]\\)\\(?:[ \t]+\\|$\\)\\)",
        "\\(?:\\[@\\(?:start:\\)?\\([0-9]+\\|[A-Za-z]\\)\\][ \t]*\\)?",
    );

    let mut md = None;
    let result = string_match_full_with_case_fold(pattern, "- Reporting issues", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));

    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 2)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 2)));
    assert_eq!(md.group(2), None);
}

#[test]
fn string_match_syntax_class_pattern_uses_backref_engine_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold(
        "\\(defun\\|defvar\\)\\s-+\\(\\w+\\)",
        "defvar foo",
        0,
        false,
        &mut md,
    );
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 10)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 6)));
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(7, 10)));
}

#[test]
fn string_match_word_syntax_class_pattern_uses_backref_engine_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("\\sw+", "foo_bar", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 3)));
}

#[test]
fn string_match_category_escape_pattern_uses_backref_engine_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("\\c|.", "éx", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 2)));
}

#[test]
fn string_match_match_at_point_escape_does_not_match_plain_string() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("\\=foo", "foo", 0, false, &mut md);
    assert_eq!(result, Ok(None));
    assert!(md.is_none());
}

#[test]
fn string_match_match_at_point_escape_does_not_treat_start_as_point() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("\\=foo", "xxfoo", 2, false, &mut md);
    assert_eq!(result, Ok(None));
    assert!(md.is_none());
}

#[test]
fn string_match_match_at_point_escape_does_not_skip_past_start() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("\\=foo", "xxafoo", 2, false, &mut md);
    assert_eq!(result, Ok(None));
    assert!(md.is_none());
}

#[test]
fn string_match_digit_escape_uses_backref_engine_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("\\d+", "dddx", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 3)));
}

#[test]
fn string_match_control_escape_uses_backref_engine_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("a\\tb", "atb", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 3)));
}

#[test]
fn category_han_matches_cjk_unified_ideographs() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    assert_eq!(string_match_full("\\cC", "中", 0, &mut md), Ok(Some(0)));
    let mut md = None;
    assert_eq!(string_match_full("\\cC", "a", 0, &mut md), Ok(None));
}

#[test]
fn category_hiragana_matches_japanese_hiragana() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    assert_eq!(string_match_full("\\cH", "あ", 0, &mut md), Ok(Some(0)));
    let mut md = None;
    assert_eq!(string_match_full("\\cH", "ア", 0, &mut md), Ok(None));
}

#[test]
fn category_katakana_matches_japanese_katakana() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    assert_eq!(string_match_full("\\cK", "ア", 0, &mut md), Ok(Some(0)));
    let mut md = None;
    assert_eq!(string_match_full("\\cK", "あ", 0, &mut md), Ok(None));
}

#[test]
fn category_hangul_matches_korean_hangul() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    assert_eq!(string_match_full("\\ch", "한", 0, &mut md), Ok(Some(0)));
    let mut md = None;
    assert_eq!(string_match_full("\\ch", "中", 0, &mut md), Ok(None));
}

#[test]
fn category_latin_matches_ascii_letters() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    assert_eq!(string_match_full("\\cl", "a", 0, &mut md), Ok(Some(0)));
    let mut md = None;
    assert_eq!(string_match_full("\\cl", "中", 0, &mut md), Ok(None));
}

#[test]
fn string_match_alternation_takes_leftmost_first_without_posix() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("a\\|aa\\|aaa", "aaaa", 0, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(
        match_group(md.group(0)),
        Some(MatchGroup::new(0, 1)),
        "non-POSIX picks first alternative"
    );
}

#[test]
fn string_match_alternation_prefers_longest_under_posix_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result =
        string_match_full_with_case_fold_and_posix("a\\|aa\\|aaa", "aaaa", 0, false, true, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(
        match_group(md.group(0)),
        Some(MatchGroup::new(0, 3)),
        "POSIX picks the longest alternative"
    );
}

#[test]
fn string_match_grouped_alternation_leftmost_first_without_posix() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("\\(a\\|ab\\|abc\\)", "abcdef", 0, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 1)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 1)));
}

#[test]
fn string_match_grouped_alternation_longest_under_posix_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold_and_posix(
        "\\(a\\|ab\\|abc\\)",
        "abcdef",
        0,
        false,
        true,
        &mut md,
    );
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 3)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 3)));
}

#[test]
fn posix_longest_match_returns_match_when_non_posix_path_would_also_match() {
    // Sanity: even when the non-POSIX leftmost-first result is
    // already the longest, the POSIX path must still return it
    // (rather than returning None because the "backtrack harder"
    // logic couldn't beat it).
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold_and_posix("foo", "foo", 0, false, true, &mut md);
    assert_eq!(result, Ok(Some(0)));
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(0, 3))
    );
}

#[test]
fn string_match_backslash_w_in_charset_is_literal_like_gnu() {
    crate::test_utils::init_test_tracing();
    // `[\w-]+` is the set {`\`, `w`, `-`}. Against "foo-bar!" the
    // first character in that set is the `-` at position 3.
    let mut md = None;
    let result = string_match_full_with_case_fold("[\\w-]+", "foo-bar!", 0, false, &mut md);
    assert_eq!(result, Ok(Some(3)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(3, 4)));
}

#[test]
fn string_match_backslash_w_in_charset_matches_literal_backslash_and_w() {
    crate::test_utils::init_test_tracing();
    // Sanity: `[\w]` matches a literal `\` or `w`.
    let mut md = None;
    let result = string_match_full_with_case_fold("[\\w]", "w", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));

    let mut md = None;
    let result = string_match_full_with_case_fold("[\\w]", "\\", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));

    // A char that is neither `\` nor `w` must not match.
    let mut md = None;
    let result = string_match_full_with_case_fold("[\\w]", "a", 0, false, &mut md);
    assert_eq!(result, Ok(None));
}

#[test]
fn string_match_backslash_s_in_charset_is_literal_like_gnu() {
    crate::test_utils::init_test_tracing();
    // `[\s-]+` is the set {`\`, `s`, `-`}. " \tfoo" contains none of
    // those at any position, so GNU returns nil.
    let mut md = None;
    let result = string_match_full_with_case_fold("[\\s-]+", " \tfoo", 0, false, &mut md);
    assert_eq!(result, Ok(None));
}

#[test]
fn string_match_posix_word_class_with_dash_range_matches_identifiers() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("[[:word:]-]+", "foo-bar!", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 7)));
}

#[test]
fn string_match_posix_space_class_with_dash_range_matches_whitespace_runs() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("[[:space:]-]+", " \tfoo", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 2)));
}

#[test]
fn string_match_leading_dash_before_posix_class_is_literal_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("[-[:alnum:]]", "-", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    assert_eq!(
        match_group(md.expect("match data").group(0)),
        Some(MatchGroup::new(0, 1))
    );
}

#[test]
fn string_match_literal_before_posix_class_is_not_dropped() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("[a[:digit:]]", "a", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    assert_eq!(
        match_group(md.expect("match data").group(0)),
        Some(MatchGroup::new(0, 1))
    );

    let mut md = None;
    let result = string_match_full_with_case_fold("[a[:digit:]]", "5", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    assert_eq!(
        match_group(md.expect("match data").group(0)),
        Some(MatchGroup::new(0, 1))
    );
}

#[test]
fn string_match_optional_lazy_posix_class_keeps_leftmost_match() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold(
        "\\(\\([[:alnum:]]+?\\)-\\)?autoload",
        "cal-autoload",
        0,
        false,
        &mut md,
    );
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 12)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 4)));
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(0, 3)));
}

#[test]
fn string_match_loaddefs_prefixed_autoload_regexp_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold(
        "^;;;###\\(\\([-[:alnum:]]+?\\)-\\)?\\(autoload\\)",
        ";;;###cal-autoload",
        0,
        false,
        &mut md,
    );
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 18)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(6, 10)));
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(6, 9)));
    assert_eq!(match_group(md.group(3)), Some(MatchGroup::new(10, 18)));
}

#[test]
fn string_match_lazy_quantifier_preserves_fallback_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("a.*?b", "aXXbYYb", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 4)));
}

#[test]
fn string_match_lazy_plus_quantifier_prefers_shorter_match() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("a.+?b", "aXXbYYb", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 4)));
}

#[test]
fn string_match_lazy_optional_quantifier_prefers_zero_width_choice() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("ab??c", "abc", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 3)));
}

#[test]
fn string_match_lazy_counted_quantifier_prefers_shorter_match() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("a\\{2,4\\}?b", "aaaab", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 5)));
}

#[test]
fn string_match_open_interval_quantifier_matches_gnu_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("a\\{,2\\}b", "aab", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 3)));
}

/// GNU `regex-emacs.c` keeps capture-register writes made by the final
/// successful repetition.  In particular, the failure point for a zero-minimum
/// interval must not restore the capture written by a later enclosing
/// repetition unless matching actually backtracks through that point.
#[test]
fn zero_minimum_interval_keeps_capture_from_final_outer_repetition() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold(
        "\\(\\(\\)\\{0,1\\}.+?\\)\\{2\\}",
        "/a",
        0,
        false,
        &mut md,
    );

    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 2)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(1, 2)));
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(1, 1)));
}

#[test]
fn string_match_large_open_interval_failure_matches_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold(
        "[[:alnum:]]\\{,1000\\}::",
        "vector<int> v;\n",
        0,
        false,
        &mut md,
    );
    assert_eq!(result, Ok(None));
    assert!(md.is_none());
}

#[test]
fn string_match_interval_question_suffix_uses_gnu_postfix_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("a\\{2,4\\}?a", "aaaa", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 4)));
}

#[test]
fn string_match_interval_repeats_only_trailing_literal_like_gnu() {
    crate::test_utils::init_test_tracing();

    let mut md = None;
    let result = string_match_full_with_case_fold("ab\\{0,1\\}", "a", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    assert_eq!(
        match_group(md.expect("match data").group(0)),
        Some(MatchGroup::new(0, 1))
    );

    let mut md = None;
    let result = string_match_full_with_case_fold("ab\\{0\\}", "ab", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    assert_eq!(
        match_group(md.expect("match data").group(0)),
        Some(MatchGroup::new(0, 1))
    );
}

#[test]
fn string_match_explicit_numbered_group_preserves_group_slot() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("\\(?9:[A-Z]+\\)", "xxABCyy", 0, false, &mut md);
    assert_eq!(result, Ok(Some(2)));
    let md = md.expect("match data");
    assert_eq!(md.group_count(), 10);
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(2, 5)));
    assert!(md.groups_snapshot()[1..9].iter().all(Option::is_none));
    assert_eq!(match_group(md.group(9)), Some(MatchGroup::new(2, 5)));
}

#[test]
fn string_match_symbol_boundary_pattern_uses_backref_engine_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("\\_<foo\\_>", "x foo y", 0, false, &mut md);
    assert_eq!(result, Ok(Some(2)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(2, 5)));
}

#[test]
fn string_match_posix_upper_class_folds_to_alpha_under_case_fold() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result =
        string_match_full_with_case_fold("[[:upper:]]+", "helloWORLDfoo", 0, true, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 13)));
}

#[test]
fn string_match_posix_upper_class_folds_to_alpha_on_lisp_string() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let string = LispString::new("helloWORLDfoo".to_string(), false);
    let result = string_match_full_with_case_fold_source_lisp(
        "[[:upper:]]+",
        &string,
        SearchedString::Owned(LispString::from_utf8("helloWORLDfoo")),
        0,
        true,
        &mut md,
    );
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 13)));
}

#[test]
fn string_match_posix_case_classes_fold_multibyte_opposite_case() {
    crate::test_utils::init_test_tracing();

    for (pattern, input) in [("[[:lower:]]+", "Σa"), ("[[:upper:]]+", "σA")] {
        let mut md = None;
        let result = string_match_full_with_case_fold(pattern, input, 0, true, &mut md);
        assert_eq!(result, Ok(Some(0)), "pattern {pattern:?} on {input:?}");
        assert_eq!(
            match_group(md.expect("match data").group(0)),
            Some(MatchGroup::new(0, 2)),
            "pattern {pattern:?} on {input:?}"
        );
    }
}

#[test]
fn string_match_posix_alpha_and_alnum_include_unicode_marks() {
    crate::test_utils::init_test_tracing();

    for pattern in ["[[:alpha:]]+", "[[:alnum:]]+"] {
        let mut md = None;
        let result = string_match_full_with_case_fold(pattern, "\u{301}a", 0, false, &mut md);
        assert_eq!(result, Ok(Some(0)), "pattern {pattern:?}");
        assert_eq!(
            match_group(md.expect("match data").group(0)),
            Some(MatchGroup::new(0, 2)),
            "pattern {pattern:?}"
        );
    }
}

#[test]
fn posix_word_class_extends_via_buffer_syntax_table_override() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::syntax::{SyntaxClass, SyntaxEntry};

    let mut buf = make_test_buffer("foo_bar baz");
    // GNU-parity isolation: give this buffer its own copy of the
    // standard chartable so the mutation doesn't leak into other
    // buffers / tests.
    crate::emacs_core::syntax::SyntaxTable::isolate_for_buffer(&mut buf)
        .modify_syntax_entry('_', SyntaxEntry::simple(SyntaxClass::Word));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));

    let mut md = None;
    let matched = looking_at(&buf, &lisp_pat("[[:word:]]+"), false, &mut md).expect("compile ok");
    assert!(matched, "[[:word:]]+ should match `foo_bar`");
    let md = md.unwrap();
    assert_eq!(
        match_group(md.group(0)),
        Some(MatchGroup::new(1, 8)),
        "match should cover the whole `foo_bar`"
    );

    // Without the override, `_` is Symbol (not Word) in the
    // standard syntax table, so the match stops at index 3.
    let mut buf2 = make_test_buffer("foo_bar baz");
    buf2.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let matched = looking_at(&buf2, &lisp_pat("[[:word:]]+"), false, &mut md).expect("compile ok");
    assert!(matched);
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(1, 4)),
        "without override, match stops at `_`"
    );
}

#[test]
fn posix_class_word_matches_ascii_letters_and_digits_but_not_punct() {
    crate::test_utils::init_test_tracing();
    // Default standard-syntax word constituents: a-z A-Z 0-9. `_`,
    // `-`, and ASCII space are NOT word constituents in the standard
    // table so `[[:word:]]` must not match them. (Audit #8 tracks
    // threading the per-buffer syntax table through the matcher; in
    // default/standard syntax this is the GNU baseline.)
    let mut md = None;
    let r = string_match_full("[[:word:]]+", "foo42bar", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(0, 8))
    );

    let mut md = None;
    let r = string_match_full("[[:word:]]+", "!!!abc!!!", 0, &mut md);
    assert_eq!(r, Ok(Some(3)));
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(3, 6))
    );

    // `_` is symbol, not word, in the standard table -> does not match.
    let mut md = None;
    let r = string_match_full("^[[:word:]]+$", "_", 0, &mut md);
    assert_eq!(r, Ok(None));
}

#[test]
fn posix_class_alnum_and_alpha_match_multibyte_letters_like_gnu() {
    crate::test_utils::init_test_tracing();

    let mut md = None;
    let r = string_match_full("[[:alnum:]]+", "标签:tail", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(0, 2))
    );

    let mut md = None;
    let r = string_match_full("[[:alpha:]]+", "任务42", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(0, 2))
    );
}

#[test]
fn posix_class_nonascii_matches_only_chars_at_or_above_u0080() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let r = string_match_full("[[:nonascii:]]+", "abcéfg", 0, &mut md);
    assert_eq!(r, Ok(Some(3)));
    // `é` occupies one character slot (md positions are char indices
    // for string search).
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(3, 4))
    );

    // Pure ASCII input -> no match.
    let mut md = None;
    let r = string_match_full("[[:nonascii:]]", "abc123", 0, &mut md);
    assert_eq!(r, Ok(None));
}

#[test]
fn posix_class_multibyte_matches_only_non_ascii_chars() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let r = string_match_full("[[:multibyte:]]+", "abcé", 0, &mut md);
    assert_eq!(r, Ok(Some(3)));
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(3, 4))
    );

    let mut md = None;
    let r = string_match_full("[[:multibyte:]]", "x", 0, &mut md);
    assert_eq!(r, Ok(None));
}

#[test]
fn posix_class_unibyte_matches_every_ascii_char() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let r = string_match_full("[[:unibyte:]]+", "abc", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(0, 3))
    );
}

#[test]
fn posix_class_blank_is_only_space_and_tab_unlike_space() {
    crate::test_utils::init_test_tracing();
    // GNU ISBLANK: space and tab only. A newline must NOT match
    // `[[:blank:]]` but MUST match `[[:space:]]`. Before the audit
    // #7 fix, neomacs merged the two classes so this distinction was
    // silently wrong.
    let mut md = None;
    let r = string_match_full("[[:blank:]]", "\n", 0, &mut md);
    assert_eq!(r, Ok(None));

    let mut md = None;
    let r = string_match_full("[[:space:]]", "\n", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));

    let mut md = None;
    let r = string_match_full("[[:blank:]]", " ", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));

    let mut md = None;
    let r = string_match_full("[[:blank:]]", "\t", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));
}

#[test]
fn posix_class_print_includes_space_but_graph_excludes_it() {
    crate::test_utils::init_test_tracing();
    // GNU ISPRINT: c >= ' '. GNU ISGRAPH: c > ' '. The two classes
    // must differ on the space character. Before the fix neomacs
    // merged them so `[[:graph:]]` matched space.
    let mut md = None;
    let r = string_match_full("[[:print:]]", " ", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));

    let mut md = None;
    let r = string_match_full("[[:graph:]]", " ", 0, &mut md);
    assert_eq!(r, Ok(None));

    // Both classes must still match `a`.
    let mut md = None;
    let r = string_match_full("[[:graph:]]", "a", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));
    let mut md = None;
    let r = string_match_full("[[:print:]]", "a", 0, &mut md);
    assert_eq!(r, Ok(Some(0)));
}

#[test]
fn posix_class_unknown_name_signals_compile_error_like_gnu() {
    crate::test_utils::init_test_tracing();
    // GNU re_wctype_parse returns RECC_ERROR for unknown names and
    // the caller signals REG_ECTYPE (regex-emacs.c:1600, 2071). We
    // raise the equivalent Rust-level compile error instead of
    // silently ignoring the unknown class name.
    let mut md = None;
    let r = string_match_full("[[:notaclass:]]", "abc", 0, &mut md);
    assert!(r.is_err(), "expected compile error, got {:?}", r);
}

#[test]
fn string_match_anchored_operator_char_class_mirrors_gnu_bracket_closing() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result =
        string_match_full_with_case_fold("\\`[-+*/=<>!&|(){}\\[\\];,.]", "=", 0, true, &mut md);
    assert_eq!(result, Ok(None));
    assert!(md.is_none());
}

#[test]
fn string_match_anchored_operator_char_class_on_lisp_slice_mirrors_gnu_bracket_closing() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let source = LispString::new("x = 42;".to_string(), false);
    let slice = source.slice(2, source.byte_len()).expect("slice");
    let result = string_match_full_with_case_fold_source_lisp(
        "\\`[-+*/=<>!&|(){}\\[\\];,.]",
        &slice,
        SearchedString::Owned(slice.clone()),
        0,
        true,
        &mut md,
    );
    assert_eq!(result, Ok(None));
    assert!(md.is_none());
}

#[test]
fn owned_raw_unibyte_match_data_preserves_bytes() {
    crate::test_utils::init_test_tracing();
    let pattern = LispString::from_unibyte(vec![0xFF]);
    let haystack = LispString::from_unibyte(vec![0x80, 0xFF, 0x81]);
    let mut md = None;
    let result = string_match_full_with_case_fold_source_lisp_pattern_posix(
        &pattern,
        &haystack,
        SearchedString::Owned(haystack.clone()),
        0,
        true,
        false,
        &mut md,
    );
    assert_eq!(result, Ok(Some(1)));
    let md = md.expect("match data");
    let searched = md.searched_string().expect("searched string");
    let string = searched.as_lisp_string().expect("lisp string");
    let group = md.group(0).expect("full match");
    let byte_start = char_pos_to_byte_lisp_string(string, group.start());
    let byte_end = char_pos_to_byte_lisp_string(string, group.end());
    let slice = string.slice(byte_start, byte_end).expect("slice");
    assert!(!slice.is_multibyte());
    assert_eq!(slice.as_bytes(), &[0xFF]);
}

#[test]
fn heap_match_string_on_lisp_slice_mirrors_gnu_bracket_closing() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let source = LispString::new("x = 42;".to_string(), false);
    let slice = source.slice(2, source.byte_len()).expect("slice");
    let slice_val = neovm_value::value::Value::string(slice.as_utf8_str().unwrap_or(""));
    let stored_slice = slice_val.as_lisp_string().unwrap().clone();
    let result = string_match_full_with_case_fold_source_lisp(
        "\\`[-+*/=<>!&|(){}\\[\\];,.]",
        &stored_slice,
        SearchedString::Heap(slice_val),
        0,
        true,
        &mut md,
    );
    assert_eq!(result, Ok(None));
    assert!(md.is_none());
}

#[test]
fn heap_tokenizer_loop_mirrors_gnu_single_char_operator_behavior() {
    crate::test_utils::init_test_tracing();
    let code = LispString::new(
        "let x = 42; if x >= 10 && x != 0 { return x + 1; }".to_string(),
        false,
    );
    let keywords = ["if", "else", "while", "return", "let", "fn"];
    let patterns = [
        ("\\`[ \t\n]+", "skip"),
        ("\\`[0-9]+\\(?:\\.[0-9]+\\)?", "number"),
        ("\\`\"[^\"]*\"", "string"),
        ("\\`\\(?:==\\|!=\\|<=\\|>=\\|&&\\|||\\|->\\)", "operator"),
        ("\\`[-+*/=<>!&|(){}\\[\\];,.]", "operator"),
        ("\\`[a-zA-Z_][a-zA-Z0-9_]*", "identifier"),
    ];

    let mut pos = 0usize;
    let mut tokens = Vec::new();
    while pos < code.byte_len() {
        let rest = code.slice(pos, code.byte_len()).expect("rest slice");
        let rest_val = neovm_value::value::Value::string(rest.as_utf8_str().unwrap_or(""));
        let stored_rest = rest_val.as_lisp_string().unwrap().clone();
        let mut matched = false;

        for (pattern, mut kind) in patterns {
            if matched {
                break;
            }

            let mut md = None;
            if let Ok(Some(_)) = string_match_full_with_case_fold_source_lisp(
                pattern,
                &stored_rest,
                SearchedString::Heap(rest_val),
                0,
                true,
                &mut md,
            ) {
                let md = md.expect("match data");
                let text = extract_heap_match_string(&md, 0).expect("matched text");
                pos += text.len();
                if kind != "skip" {
                    if kind == "identifier" && keywords.contains(&text.as_str()) {
                        kind = "keyword";
                    }
                    tokens.push((kind.to_string(), text));
                }
                matched = true;
            }
        }

        if !matched {
            pos += 1;
        }
    }

    assert_eq!(
        tokens,
        vec![
            ("keyword".to_string(), "let".to_string()),
            ("identifier".to_string(), "x".to_string()),
            ("number".to_string(), "42".to_string()),
            ("keyword".to_string(), "if".to_string()),
            ("identifier".to_string(), "x".to_string()),
            ("operator".to_string(), ">=".to_string()),
            ("number".to_string(), "10".to_string()),
            ("operator".to_string(), "&&".to_string()),
            ("identifier".to_string(), "x".to_string()),
            ("operator".to_string(), "!=".to_string()),
            ("number".to_string(), "0".to_string()),
            ("keyword".to_string(), "return".to_string()),
            ("identifier".to_string(), "x".to_string()),
            ("number".to_string(), "1".to_string()),
        ]
    );
}

#[test]
fn string_match_bracket_section_anchor_pattern_matches_whole_string() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result =
        string_match_full_with_case_fold("\\`\\[\\([^]]+\\)\\]\\'", "[database]", 0, true, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 10)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(1, 9)));
}

#[test]
fn string_match_line_anchor_pattern_uses_backref_engine_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("^foo$", "foo", 0, false, &mut md);
    assert_eq!(result, Ok(Some(0)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 3)));
}

#[test]
fn string_match_line_anchor_pattern_respects_multiline_semantics() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full_with_case_fold("^foo$", "a\nfoo\nb", 0, false, &mut md);
    assert_eq!(result, Ok(Some(2)));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(2, 5)));
}

#[test]
fn translate_complex_pattern() {
    crate::test_utils::init_test_tracing();
    // Emacs: \(defun\|defvar\)\s-+\(\w+\)
    // Rust:  (defun|defvar)\s+(\w+)
    let emacs = "\\(defun\\|defvar\\)\\s-+\\(\\w+\\)";
    let rust = translate_emacs_regex(emacs);
    // After translation: (defun|defvar)\s+(\w+)
    assert_eq!(rust, "(defun|defvar)\\s+(\\w+)");
}

#[test]
fn translate_explicit_numbered_group_keeps_fallback_compilable() {
    crate::test_utils::init_test_tracing();
    let emacs = "\\(?9:.*?\\)";
    assert_eq!(translate_emacs_regex(emacs), "(.*?)");
}

#[test]
fn translate_open_interval_quantifier_keeps_fallback_compilable() {
    crate::test_utils::init_test_tracing();
    let emacs = "a\\{,2\\}b";
    assert_eq!(translate_emacs_regex(emacs), "a{0,2}b");
}

#[test]
fn translate_category_escape_keeps_fill_patterns_compilable() {
    crate::test_utils::init_test_tracing();
    let emacs = "[ \t]\\|\\c|.\\|.\\c|";
    let rust = translate_emacs_regex(emacs);
    assert_eq!(rust, "[ \t]|[^\\x00-\\x7F].|.[^\\x00-\\x7F]");
}

#[test]
fn translate_empty_pattern() {
    crate::test_utils::init_test_tracing();
    assert_eq!(translate_emacs_regex(""), "");
}

#[test]
fn translate_no_special_chars() {
    crate::test_utils::init_test_tracing();
    assert_eq!(translate_emacs_regex("hello"), "hello");
}

#[test]
fn translate_escaped_backslash() {
    crate::test_utils::init_test_tracing();
    assert_eq!(translate_emacs_regex("\\\\"), "\\\\");
}

#[test]
fn translate_multibyte_literals() {
    crate::test_utils::init_test_tracing();
    assert_eq!(translate_emacs_regex("\\(é\\)"), "(é)");
    assert_eq!(translate_emacs_regex("[éx]"), "[éx]");
    assert_eq!(translate_emacs_regex("\\é"), "é");
    assert_eq!(translate_emacs_regex("\\😀"), "😀");
}

#[test]
fn trivial_regexp_matches_gnu_meta_rules() {
    crate::test_utils::init_test_tracing();
    assert!(trivial_regexp_p("hello\\.txt".as_bytes()));
    assert!(trivial_regexp_p("\\😀".as_bytes()));
    assert!(!trivial_regexp_p("he.*o".as_bytes()));
    assert!(!trivial_regexp_p("\\(group\\)".as_bytes()));
    assert!(!trivial_regexp_p("\\1".as_bytes()));
    assert!(!trivial_regexp_p("trailing\\".as_bytes()));
}

#[test]
fn string_match_basic() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("he..o", "hello world", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(0));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 5)));
    assert_eq!(md.searched_string_text(), Some("hello world".to_string()));
}

#[test]
fn string_match_with_groups() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    // Emacs regex: \(\w+\)@\(\w+\)
    let result = string_match_full("\\(\\w+\\)@\\(\\w+\\)", "user@host", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(0));
    let md = md.unwrap();
    assert_eq!(md.group_count(), GNU_SEARCH_REGS_BASE_CAPACITY);
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 9)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 4))); // "user"
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(5, 9))); // "host"
}

#[test]
fn string_match_with_multibyte_group_literal() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("\\(é\\)", "aéx", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(1));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(1, 2))); // "é" in character positions
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(1, 2))); // capture group
}

#[test]
fn string_match_with_escaped_multibyte_literal() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("\\é", "aéx", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(1));
}

#[test]
fn string_match_with_multibyte_literal_repetition() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("é+", "aééx", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(1));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(1, 3)));
}

#[test]
fn string_match_multibyte_charset_range_matches_interior_character() {
    crate::test_utils::init_test_tracing();
    let pattern = lisp_pat("[À-Å]");
    let compiled = regex_emacs::regex_compile_lisp_with_translation(&pattern, false, None).unwrap();
    assert_eq!(compiled.multibyte_charsets.get(&0), Some(&vec![('À', 'Å')]));
    let folded = regex_emacs::regex_compile_lisp_with_translation(
        &pattern,
        false,
        Some(regex_emacs::CaseTranslation::standard()),
    )
    .unwrap();
    assert!(
        folded
            .multibyte_charsets
            .get(&0)
            .is_some_and(|ranges| ranges.contains(&('à', 'å'))),
        "case-folded range should include GNU's translated image"
    );
    assert!(
        compiled.fastmap[0xC3],
        "UTF-8 lead byte should be searchable"
    );
    let haystack = LispString::from_utf8("Ä");
    assert!(
        regex_emacs::re_match(
            &compiled,
            haystack.as_bytes(),
            0,
            haystack.byte_len(),
            &DefaultSyntaxLookup,
            STRING_MATCH_AT_DOT_UNREACHABLE,
        )
        .is_some(),
        "anchored charset match should accept the interior codepoint"
    );

    let mut md = None;
    let result = string_match_full("[À-Å]", "Ä", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(0));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 1)));
}

#[test]
fn string_match_trivial_escaped_literal_uses_character_positions() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("\\.", "a.b", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(1));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(1, 2)));
}

#[test]
fn string_match_backreference_reuses_captured_text() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("\\(..\\)\\1", "zzabab", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(2));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(2, 6)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(2, 4)));
}

#[test]
fn looking_at_string_backreference_matches_at_start() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let matched = looking_at_string("\\(x\\)\\1\\1", "xxx!", false, &mut md).unwrap();
    assert!(matched);
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 3)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 1)));
}

#[test]
fn re_search_forward_backreference_word_boundary() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("the the cat");
    let mut md = None;
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("\\b\\(\\w+\\) \\1\\b"),
        None,
        false,
        false,
        &mut md,
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(7));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(1, 8)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(1, 4)));
}

#[test]
fn string_match_backreference_with_char_class_group() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("\\([a-z]+\\) \\1", "the the cat", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(0));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(0, 7)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 3)));
}

#[test]
fn string_match_template_interpolation_pattern() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full(r"{{\([^}]+\)}}", "x {{name}} y", 0, &mut md).unwrap();
    assert_eq!(result, Some(2));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(2, 10)));
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(4, 8)));
}

#[test]
fn string_match_template_foreach_pattern() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full(
        r"{%foreach \([^ ]+\) in \([^%]+\)%}\(\(?:.\|\n\)*?\){%endforeach%}",
        "Items: {%foreach x in items%}[{{x}}] {%endforeach%}",
        0,
        &mut md,
    )
    .unwrap();
    assert_eq!(result, Some(7));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(17, 18)));
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(22, 27)));
    assert_eq!(match_group(md.group(3)), Some(MatchGroup::new(29, 37)));
}

#[test]
fn string_match_template_conditional_pattern() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full(
        r"{%if \([^%]+\)%}\(\(?:.\|\n\)*?\){%else%}\(\(?:.\|\n\)*?\){%endif%}",
        "{%if admin%}[ADMIN]{%else%}[USER]{%endif%}",
        0,
        &mut md,
    )
    .unwrap();
    assert_eq!(result, Some(0));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(5, 10)));
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(12, 19)));
    assert_eq!(match_group(md.group(3)), Some(MatchGroup::new(27, 33)));
}

#[test]
fn string_match_with_start_offset() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("world", "hello world", 6, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(6));
}

#[test]
fn string_match_no_match() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let result = string_match_full("xyz", "hello world", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
    assert!(md.is_none());
}

#[test]
fn string_match_emacs_alternation() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    // Emacs regex: \(foo\|bar\)
    let result = string_match_full("\\(foo\\|bar\\)", "test bar baz", 0, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(5));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(5, 8))); // "bar"
}

#[test]
fn bounded_search_view_agrees_across_fragmented_backends() {
    crate::test_utils::init_test_tracing();
    let text = "aaé中\naa TAIL";
    let positions: Vec<_> = text
        .char_indices()
        .map(|(pos, _)| pos)
        .chain(std::iter::once(text.len()))
        .collect();
    for pattern in ["a", "a$", "a\\b", "\\(é中\\)", "^\\(a*\\)"] {
        for &start in &positions {
            for &bound in positions.iter().filter(|&&pos| pos >= start) {
                let search = |buf: &mut Buffer| {
                    buf.goto_emacs_byte_pos(EmacsBytePos::new(start));
                    let mut md = None;
                    let result = re_search_forward(
                        buf,
                        &lisp_pat(pattern),
                        Some(bound),
                        true,
                        false,
                        &mut md,
                    );
                    buffer_search_snapshot(result, buf, &md)
                };
                let expected = search(&mut make_test_buffer(text));
                for kind in implemented_text_backends() {
                    for &gap in &positions {
                        let mut buf = make_test_buffer_with_backend(text, kind);
                        buf.goto_emacs_byte_pos(EmacsBytePos::new(gap));
                        buf.insert("x");
                        buf.delete_emacs_byte_range(EmacsByteRange::from_usize(gap, gap + 1));
                        assert_eq!(
                            search(&mut buf),
                            expected,
                            "{kind:?}, {pattern:?}, gap={gap}, {start}..={bound}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn implemented_text_backends_match_literal_search_semantics() {
    crate::test_utils::init_test_tracing();
    let baseline = literal_search_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(literal_search_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_regex_search_semantics() {
    crate::test_utils::init_test_tracing();
    let baseline = regex_search_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(regex_search_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_looking_at_semantics() {
    crate::test_utils::init_test_tracing();
    let baseline = looking_at_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(looking_at_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_replace_match_after_regex_search() {
    crate::test_utils::init_test_tracing();
    let baseline = replace_match_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(replace_match_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn implemented_text_backends_match_unibyte_search_semantics() {
    crate::test_utils::init_test_tracing();
    let baseline = unibyte_search_backend_trace(BufferTextBackendKind::GapBuffer);

    for kind in implemented_text_backends() {
        assert_eq!(unibyte_search_backend_trace(kind), baseline, "{kind:?}");
    }
}

#[test]
fn search_forward_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("world"), None, false, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(11)); // end of "world"
    assert_eq!(buf.point_char_pos().get(), 0);
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(7, 12)));
}

#[test]
fn search_forward_not_found_noerror() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("xyz"), None, true, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
    assert_eq!(buf.point_char_pos().get(), 0); // point unchanged
}

#[test]
fn search_forward_not_found_error() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("xyz"), None, false, false, &mut md);
    assert!(result.is_err());
}

#[test]
fn search_forward_case_fold_true() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("A");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("a"), None, false, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(1));
}

#[test]
fn search_forward_case_fold_true_unicode_literal() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("Äx");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("ä"), None, false, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some('Ä'.len_utf8()));
}

#[test]
fn string_match_case_fold_non_latin_unicode_literals() {
    crate::test_utils::init_test_tracing();
    let mut md = None;

    let greek = string_match_full_with_case_fold("Ω", "ω", 0, true, &mut md);
    assert_eq!(greek.unwrap(), Some(0));

    let cyrillic = string_match_full_with_case_fold("Д", "д", 0, true, &mut md);
    assert_eq!(cyrillic.unwrap(), Some(0));
}

#[test]
fn search_forward_case_fold_true_ascii_literal_in_non_ascii_buffer() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("α GENERATED-AUTOLOAD-FILE");
    let mut md = None;
    let result = search_forward(
        &mut buf,
        &lisp_pat("generated-autoload-file"),
        None,
        false,
        true,
        &mut md,
    );
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some("α GENERATED-AUTOLOAD-FILE".len()));
    assert_eq!(
        match_group(md.unwrap().group(0)),
        Some(MatchGroup::new(
            "α ".len(),
            "α GENERATED-AUTOLOAD-FILE".len(),
        ))
    );
}

#[test]
fn search_forward_case_fold_true_ascii_literal_does_not_unicode_fold_kelvin() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("xxKelvin");
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("kelvin"), None, true, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
    assert!(md.is_none());
}

#[test]
fn re_search_forward_trivial_regexp_follows_literal_case_fold_path() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("A.b");
    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("a\\."), None, false, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(2));
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(1, 3)));
}

#[test]
fn search_forward_with_bound() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    // Search only within first 5 bytes — "world" starts at 6 so should not be found
    let result = search_forward(&mut buf, &lisp_pat("world"), Some(5), true, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
}

#[test]
fn search_forward_from_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("aaa bbb aaa");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(4)); // after "aaa "
    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("aaa"), None, false, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(11)); // second "aaa" at end
}

#[test]
fn search_backward_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11)); // end of buffer
    let mut md = None;
    let result = search_backward(&mut buf, &lisp_pat("hello"), None, false, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(0)); // beginning of "hello"
    assert_eq!(buf.point_char_pos().get(), 11);
}

#[test]
fn search_backward_not_found() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11));
    let mut md = None;
    let result = search_backward(&mut buf, &lisp_pat("xyz"), None, true, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), None);
}

#[test]
fn search_backward_finds_last_occurrence() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("aaa bbb aaa");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(11)); // end
    let mut md = None;
    let result = search_backward(&mut buf, &lisp_pat("aaa"), None, false, false, &mut md);
    assert!(result.is_ok());
    // Should find the LAST "aaa" (at position 8)
    assert_eq!(result.unwrap(), Some(8));
    assert_eq!(buf.point_char_pos().get(), 11);
}

#[test]
fn search_backward_case_fold_true_unicode_literal() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("Ää");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new("Ää".len()));
    let mut md = None;
    let result = search_backward(&mut buf, &lisp_pat("ä"), None, false, true, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some('Ä'.len_utf8()));
    assert_eq!(buf.point_emacs_byte_pos().get(), "Ää".len());
    assert_eq!(buf.point_char_pos().get(), 2);
}

#[test]
fn re_search_forward_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("foo 123 bar");
    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("[0-9]+"), None, false, false, &mut md);
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), Some(7)); // end of "123"
    assert_eq!(buf.point_char_pos().get(), 0);
    let md = md.unwrap();
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(5, 8)));
}

#[test]
fn re_search_forward_with_groups() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("name: John");
    let mut md = None;
    // Emacs regex: \(\w+\): \(\w+\)
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("\\(\\w+\\): \\(\\w+\\)"),
        None,
        false,
        false,
        &mut md,
    );
    assert!(result.is_ok());
    let md = md.unwrap();
    assert_eq!(md.group_count(), GNU_SEARCH_REGS_BASE_CAPACITY);
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(1, 5))); // "name"
    assert_eq!(match_group(md.group(2)), Some(MatchGroup::new(7, 11))); // "John"
}

#[test]
fn re_search_forward_multiline_anchor_respects_real_line_start() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("alpha=1\nbeta=2\ngamma=3\n");
    let mut md = None;

    let first = re_search_forward(
        &mut buf,
        &lisp_pat("^\\([^=]+\\)=\\([0-9]+\\)$"),
        None,
        false,
        false,
        &mut md,
    )
    .expect("first search should succeed");
    assert_eq!(first, Some("alpha=1".len()));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(first.unwrap()));
    let first_md = md.as_ref().expect("match data for first search");
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(
            &buf,
            first_md.group(1).unwrap(),
        )),
        "alpha"
    );

    let second = re_search_forward(
        &mut buf,
        &lisp_pat("^\\([^=]+\\)=\\([0-9]+\\)$"),
        None,
        false,
        false,
        &mut md,
    )
    .expect("second search should succeed");
    assert_eq!(second, Some("alpha=1\nbeta=2".len()));
    let second_md = md.as_ref().expect("match data for second search");
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(
            &buf,
            second_md.group(1).unwrap(),
        )),
        "beta"
    );
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(
            &buf,
            second_md.group(2).unwrap(),
        )),
        "2"
    );
}

#[test]
fn re_search_forward_bound_is_not_artificial_line_end() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("* TODO Alpha\n:LOGBOOK:\nCLOCK: \n:END:\n* TODO Beta\n");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(30));
    let mut md = None;

    // GNU `search_buffer_re` passes the full visible buffer to
    // `re_search_2` and passes the search bound separately as STOP.
    // Therefore `$` does not match merely because the bound is just after
    // point; it only matches before a real newline or at the real string end.
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("^[ \t]*$"),
        Some(31),
        true,
        false,
        &mut md,
    );

    assert_eq!(result, Ok(None));
    assert_eq!(buf.point_emacs_byte_pos().get(), 30);
    assert!(md.is_none());
}

#[test]
fn re_search_backward_basic() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("abc 123 def 456");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(15)); // end
    let mut md = None;
    let result = re_search_backward(&mut buf, &lisp_pat("[0-9]+"), None, false, false, &mut md);
    assert!(result.is_ok());
    // GNU re-search-backward scans positions backward and matches at the
    // first position where the regex succeeds.  From point-max (15/0-indexed=14),
    // position 14 is '6' which matches [0-9]+.  So match-beginning is 14.
    assert_eq!(result.unwrap(), Some(14));
    assert_eq!(buf.point_char_pos().get(), 15);
}

#[test]
fn re_search_backward_rejects_match_extending_past_point() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("ab12cd");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(2)); // point at the start of the current match
    let mut md = None;
    let result = re_search_backward(&mut buf, &lisp_pat("[0-9]+"), Some(0), true, false, &mut md);
    assert_eq!(result, Ok(None));
    assert!(md.is_none());
    assert_eq!(buf.point_char_pos().get(), 2);
}

#[test]
fn re_search_backward_finds_nullable_match_at_point() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("abc\n");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(3)); // point before trailing newline
    let mut md = None;
    let result = re_search_backward(
        &mut buf,
        &lisp_pat("\\(?:$\\)\\="),
        Some(0),
        true,
        false,
        &mut md,
    );
    assert_eq!(result, Ok(Some(3)));
    assert_eq!(buf.point_char_pos().get(), 3);
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(4, 4)));
}

#[test]
fn re_search_backward_log_line_loop_progresses() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer(
        "[09:01:00] INFO: Server started\n[09:02:15] INFO: Connection from 10.0.0.1\n[09:03:30] WARN: High memory usage detected",
    );
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
        buf.total_emacs_byte_len().get(),
    ));
    let mut md = None;
    let pattern = "\\[\\([0-9:]+\\)\\] \\(INFO\\|WARN\\|ERROR\\): \\(.*\\)$";
    let mut positions = Vec::new();

    for _ in 0..4 {
        let Some(pos) =
            re_search_backward(&mut buf, &lisp_pat(pattern), None, true, false, &mut md).unwrap()
        else {
            break;
        };
        positions.push((pos, md.as_ref().and_then(|data| match_group(data.group(0)))));
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(pos));
    }

    assert_eq!(
        positions,
        vec![
            (74, Some(MatchGroup::new(75, 118))),
            (32, Some(MatchGroup::new(33, 74))),
            (0, Some(MatchGroup::new(1, 32)))
        ]
    );
}

#[test]
fn re_search_forward_finds_nullable_match_at_buffer_end() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("abc");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(3));
    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("\\="), None, true, false, &mut md);
    assert_eq!(result, Ok(Some(3)));
    assert_eq!(buf.point_char_pos().get(), 3);
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(4, 4)));
}

#[test]
fn looking_at_matches() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("hello"), true, &mut md);
    assert!(result.is_ok());
    assert!(result.unwrap());
    assert!(md.is_some());
}

#[test]
fn looking_at_no_match() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("world"), true, &mut md);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn looking_at_from_middle() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(6)); // "world"
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("world"), true, &mut md);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn looking_at_defaults_to_case_fold() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("A");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("a"), true, &mut md);
    assert!(result.is_ok());
    assert!(result.unwrap());
}

#[test]
fn looking_at_respects_case_fold_false() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("A");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    let result = looking_at(&buf, &lisp_pat("a"), false, &mut md);
    assert!(result.is_ok());
    assert!(!result.unwrap());
}

#[test]
fn looking_at_case_fold_preserves_unibyte_raw_bytes_in_multibyte_buffer() {
    crate::test_utils::init_test_tracing();
    let magic = LispString::from_unibyte(vec![0xed, 0xab, 0xee, 0xdb, 0x03, 0x00]);
    let mut buf = make_test_buffer("");
    buf.insert_lisp_string(&magic);
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::ZERO);

    let mut md = None;
    assert_eq!(looking_at(&buf, &magic, true, &mut md), Ok(true));
    assert_eq!(
        match_group(md.expect("match data").group(0)),
        Some(MatchGroup::new(1, 7))
    );
}

#[test]
fn looking_at_with_groups() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("foo123bar");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    // Emacs: \(\w+\)\([0-9]+\)
    let result = looking_at(&buf, &lisp_pat("\\(\\w+\\)\\([0-9]+\\)"), true, &mut md);
    assert!(result.is_ok());
    assert!(result.unwrap());
    let md = md.unwrap();
    // \w+ is greedy, matches "foo123bar" leaving nothing for [0-9]+
    // Actually \w includes digits, so \w+ matches everything
    // Let's check what actually happens
    assert!(md.group(0).is_some());
}

#[test]
fn looking_at_character_class_backslash_range_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let buf = make_test_buffer("/");
    let result = looking_at(&buf, &lisp_pat("[+\\-*/=<>]"), false, &mut md);
    assert_eq!(result, Ok(true));
    let md = md.expect("match data");
    assert_eq!(match_group(md.group(0)), Some(MatchGroup::new(1, 2)));

    let mut md = None;
    let buf = make_test_buffer("*");
    assert_eq!(
        looking_at(&buf, &lisp_pat("[+\\-*/=<>]"), false, &mut md),
        Ok(false)
    );

    let mut md = None;
    let buf = make_test_buffer("-");
    assert_eq!(
        looking_at(&buf, &lisp_pat("[+\\-*/=<>]"), false, &mut md),
        Ok(false)
    );
}

#[test]
fn replace_match_literal() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    let mut md = None;
    let _ = re_search_forward(&mut buf, &lisp_pat("world"), None, false, false, &mut md);
    let result = replace_match_buffer(&mut buf, "rust", false, true, 0, &md);
    assert!(result.is_ok());
    let content = buf.buffer_substring_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        0,
        buf.total_emacs_byte_len().get(),
    ));
    assert_eq!(content, "hello rust");
}

#[test]
fn replace_match_with_backref() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("hello world");
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let mut md = None;
    // Match "hello" with a group
    let _ = re_search_forward(
        &mut buf,
        &lisp_pat("\\(hello\\)"),
        None,
        false,
        false,
        &mut md,
    );
    let result = replace_match_buffer(&mut buf, "\\1 there", false, false, 0, &md);
    assert!(result.is_ok());
    let content = buf.buffer_substring_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        0,
        buf.total_emacs_byte_len().get(),
    ));
    assert_eq!(content, "hello there world");
}

#[test]
fn replace_match_buffer_preserves_unibyte_raw_bytes() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("raw"),
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.set_multibyte_value(false);
    buf.insert_lisp_string(&neovm_value::heap_types::LispString::from_unibyte(vec![
        0xFF,
    ]));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));

    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("."), None, false, false, &mut md);
    assert_eq!(result, Ok(Some(1)));

    let result = replace_match_buffer(&mut buf, "\\&", false, false, 0, &md);
    assert!(result.is_ok());

    let content = buf.buffer_substring_lisp_string_range(buf.full_emacs_byte_range());
    assert!(!content.is_multibyte());
    assert_eq!(content.as_bytes(), &[0xFF]);
    assert_eq!(buf.total_emacs_byte_len().get(), 1);
}

#[test]
fn replace_match_applies_case_pattern() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("FOO", "FOO", 0, &mut md);
    let replaced = replace_match_string_str("FOO", "bar", false, false, 0, &md).unwrap();
    assert_eq!(replaced, b"BAR");

    let _ = string_match_full("Foo", "Foo", 0, &mut md);
    let replaced = replace_match_string_str("Foo", "bar", false, false, 0, &md).unwrap();
    assert_eq!(replaced, b"Bar");
}

#[test]
fn replace_match_subexp_replaces_requested_group() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("\\([a-z]+\\)\\([0-9]+\\)", "abc123", 0, &mut md);
    let replaced = replace_match_string_str("abc123", "X", false, false, 2, &md).unwrap();
    assert_eq!(replaced, b"abcX");
}

#[test]
fn replace_match_subexp_errors_when_missing() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("\\([a-z]+\\)?\\([0-9]+\\)", "123", 0, &mut md);
    let err = replace_match_string_str("123", "X", false, false, 1, &md).unwrap_err();
    assert_eq!(err, REPLACE_MATCH_SUBEXP_MISSING);
}

#[test]
fn replace_match_preserves_multibyte_replacement_literals() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("x", "x", 0, &mut md);
    let replaced = replace_match_string_str("x", "éz", false, false, 0, &md).unwrap();
    assert_eq!(replaced, "éz".as_bytes());
}

#[test]
fn replace_match_preserves_multibyte_replacement_with_backref() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("\\(x\\)", "x", 0, &mut md);
    let replaced = replace_match_string_str("x", "\\1é", false, false, 0, &md).unwrap();
    assert_eq!(replaced, "xé".as_bytes());
}

#[test]
fn replace_match_rejects_backslash_zero_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("foo", "foo", 0, &mut md);
    let err = replace_match_string_str("foo", "\\0", false, false, 0, &md)
        .expect_err("\\0 must be rejected by replace-match");
    assert_eq!(err, "Invalid use of `\\' in replacement text");
}

#[test]
fn replace_match_rejects_unknown_backslash_escape_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("foo", "foo", 0, &mut md);

    // `\n` must error, not emit literal `\n`.
    let err = replace_match_string_str("foo", "a\\nb", false, false, 0, &md)
        .expect_err("\\n in replacement must be rejected");
    assert_eq!(err, "Invalid use of `\\' in replacement text");

    // An arbitrary ASCII letter must error too.
    let err = replace_match_string_str("foo", "\\x", false, false, 0, &md)
        .expect_err("\\x in replacement must be rejected");
    assert_eq!(err, "Invalid use of `\\' in replacement text");

    // A non-ASCII character must error too.
    let err = replace_match_string_str("foo", "\\é", false, false, 0, &md)
        .expect_err("\\<non-ascii> in replacement must be rejected");
    assert_eq!(err, "Invalid use of `\\' in replacement text");
}

#[test]
fn replace_match_passes_backslash_question_literally() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("foo", "foo", 0, &mut md);
    let replaced = replace_match_string_str("foo", "\\?", false, true, 0, &md)
        .expect("\\? must be accepted in non-literal replacement");
    // With `literal=true` the template is copied verbatim, matching
    // GNU's pass-through semantics from the other path.
    assert_eq!(replaced, b"\\?");

    let replaced = replace_match_string_str("foo", "a\\?b", false, false, 0, &md)
        .expect("\\? must be accepted in non-literal replacement");
    assert_eq!(replaced, b"a\\?b");
}

#[test]
fn search_forward_then_match_string() {
    crate::test_utils::init_test_tracing();
    let mut buf = make_test_buffer("The quick brown fox");
    let mut md = None;
    let _ = re_search_forward(
        &mut buf,
        &lisp_pat("\\(quick\\) \\(brown\\)"),
        None,
        false,
        false,
        &mut md,
    );
    let md = md.as_ref().unwrap();

    // match-string 0 = "quick brown"
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(&buf, md.group(0).unwrap(),)),
        "quick brown"
    );

    // match-string 1 = "quick"
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(&buf, md.group(1).unwrap(),)),
        "quick"
    );

    // match-string 2 = "brown"
    assert_eq!(
        buf.buffer_substring_range(buffer_match_group_byte_range(&buf, md.group(2).unwrap(),)),
        "brown"
    );
}

#[test]
fn string_match_then_match_data() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let _ = string_match_full("\\([0-9]+\\)-\\([0-9]+\\)", "date: 2024-01-15", 0, &mut md);
    let md = md.as_ref().unwrap();
    let string = md.searched_string_text().unwrap();

    // match-beginning 0
    let group0 = md.group(0).unwrap();
    assert_eq!(group0.start(), 6); // "2024-01"

    // Group 1: "2024"
    let group1 = md.group(1).unwrap();
    assert_eq!(&string[group1.start()..group1.end()], "2024");

    // Group 2: "01"
    let group2 = md.group(2).unwrap();
    assert_eq!(&string[group2.start()..group2.end()], "01");
}

#[test]
fn string_match_optional_group() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    // Pattern with an optional group: \(foo\)\(bar\)?
    let _ = string_match_full("\\(foo\\)\\(bar\\)?", "fooXYZ", 0, &mut md);
    let md = md.as_ref().unwrap();
    assert_eq!(match_group(md.group(1)), Some(MatchGroup::new(0, 3))); // "foo"
    assert_eq!(md.group(2), None); // optional group didn't match
}

#[test]
fn string_match_start_offset_respects_real_line_start() {
    crate::test_utils::init_test_tracing();
    let mut md = None;
    let source = "alpha=1\nbeta=2\ngamma=3";
    let start = "alpha=1".len();
    let result = string_match_full("^\\([^=]+\\)=\\([0-9]+\\)$", source, start, &mut md)
        .expect("string match should succeed");
    assert_eq!(result, Some("alpha=1\n".chars().count()));

    let md = md.as_ref().expect("match data");
    let searched = md.searched_string_text().expect("searched string");
    let group1 = md.group(1).unwrap();
    let s1 = group1.start();
    let e1 = group1.end();
    let byte_s1 = searched
        .char_indices()
        .nth(s1)
        .map(|(i, _)| i)
        .unwrap_or(searched.len());
    let byte_e1 = searched
        .char_indices()
        .nth(e1)
        .map(|(i, _)| i)
        .unwrap_or(searched.len());
    assert_eq!(&searched[byte_s1..byte_e1], "beta");
}

#[test]
fn test_lazy_interval() {
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::regex_emacs::{DefaultSyntaxLookup, search_pattern};
    let syn = DefaultSyntaxLookup;
    // Greedy: a\{1,3\} on "aaab" matches "aaa"
    let r = search_pattern("a\\{1,3\\}b", "aaab", 0, false, &syn, 0);
    let (_, regs) = r.unwrap().expect("should match");
    assert_eq!(regs.start[0], 0);
    assert_eq!(regs.end[0], 4); // matches "aaab"
}

/// The `SyntaxSpecSet` fusion peephole must (a) actually fire on the
/// font-lock symbol patterns it targets, and (b) preserve match semantics
/// byte-for-byte.  `SyntaxSpecSet` is opcode 33.
#[test]
fn regex_syntax_class_fusion() {
    const SYNTAX_SPEC_SET: u8 = 33;
    let syn = DefaultSyntaxLookup;

    // (a) Fusion fires on the real font-lock alternations.
    for pat in [
        r"\(?:\w\|\s_\)",               // FinalBare: two positive branches
        r"\(?:\w\|\s_\)+",              // ...inside a `+` loop
        r"\(?:\w\|\s_\|\\.\)+",         // Chain: run of 2 + a non-syntax branch
        r"(\(\(?:\w\|\s_\|\\.\)+\)\_>", // the sexp-head-kw bench pattern
    ] {
        let cp = regex_bench_compile(pat, false);
        assert!(
            cp.bytecode().contains(&SYNTAX_SPEC_SET),
            "fusion must emit SyntaxSpecSet for {pat:?}"
        );
    }

    // Negated alternations must NOT fuse (set algebra differs): `\Sw\|\S_`
    // matches every char, so a "not in {word,symbol}" set would be wrong.
    let neg = regex_bench_compile(r"\(?:\Sw\|\S_\)+", false);
    assert!(
        !neg.bytecode().contains(&SYNTAX_SPEC_SET),
        "negated syntax alternations must not fuse"
    );

    // (b) Match equivalence: the fused pattern behaves exactly like the
    // alternation it replaced across representative inputs.  `-` is symbol
    // syntax, space is whitespace, `\` is escape.
    let sym = regex_bench_compile(r"\_<\(?:\w\|\s_\|\\.\)+\_>", false);
    for (text, want) in [
        ("foo-bar", Some((0usize, 7usize))), // word + symbol run
        ("a b", Some((0, 1))),               // stops at whitespace
        ("x_y1", Some((0, 4))),              // word/symbol mix
        (" 123", Some((1, 4))),              // leading space skipped
    ] {
        let bytes = text.as_bytes();
        let got = regex_emacs::re_search(&sym, bytes, 0, bytes.len() as isize, &syn, 0)
            .map(|(_p, r)| (r.start[0] as usize, r.end[0] as usize));
        assert_eq!(got, want, "fused match mismatch for {text:?}");
    }

    // A single positive syntax branch stays a plain SyntaxSpec (nothing to
    // fuse) and still matches.
    let single = regex_bench_compile(r"\w+", false);
    assert!(!single.bytecode().contains(&SYNTAX_SPEC_SET));
    let got = regex_emacs::re_search(&single, b"abc!", 0, 4, &syn, 0)
        .map(|(_p, r)| (r.start[0], r.end[0]));
    assert_eq!(got, Some((0, 3)));
}

/// (c) Backtracking-heavy: `a*a*b` — the classic quadratic/cubic
/// backtracker — anchored at position 0 (`looking-at` shape) so the cost
/// is the failure-stack churn itself, not the outer scan loop.
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_backtracking() {
    crate::test_utils::init_test_tracing();
    let n = 2048usize;
    let mut text = "a".repeat(n);
    text.push('!');
    let bytes = text.as_bytes();
    let iters = 7;

    let cp = regex_bench_compile("a*a*b", false);
    let syn = DefaultSyntaxLookup;
    let t_match = regex_bench_min(iters, || {
        assert!(regex_emacs::re_match(&cp, bytes, 0, bytes.len(), &syn, 0).is_none());
    });

    // The same catastrophe under a scan (what `re-search-forward` does):
    // every start position re-runs the quadratic failure.
    let n_scan = 256usize;
    let mut scan_text = "a".repeat(n_scan);
    scan_text.push('!');
    let scan_bytes = scan_text.as_bytes();
    let t_scan = regex_bench_min(iters, || {
        assert!(
            regex_emacs::re_search(&cp, scan_bytes, 0, scan_bytes.len() as isize, &syn, 0)
                .is_none()
        );
    });

    panic!(
        "BENCH regex backtracking a*a*b: anchored fail over {n} a's: {t_match:?} | \
         scan fail over {n_scan} a's: {t_scan:?} \
         (NOTE: no failure-stack limit in re_match — GNU signals at re_max_failures)"
    );
}

/// (2) Pattern-compilation cost: fresh `regex_compile_lisp` per pattern
/// (a cache miss) vs the LRU cache hit path (`compile_lisp_pattern_with_posix`),
/// vs one engine scan — how hot is compilation relative to matching?
#[test]
#[cfg(feature = "perf-probes")]
fn regex_bench_compile_cost() {
    crate::test_utils::init_test_tracing();
    let mut report = String::from("BENCH regex compile cost:\n");

    for (name, pat) in REGEX_BENCH_FONTLOCK_PATTERNS {
        let lisp = LispString::from_utf8(pat);
        let compiles = 200u32;
        let t_miss = regex_bench_min(5, || {
            for _ in 0..compiles {
                let cp =
                    regex_emacs::regex_compile_lisp(&lisp, false, false).expect("pattern compiles");
                std::hint::black_box(&cp);
            }
        });
        // Cached path: same key each call — a hit after the first.
        let t_hit = regex_bench_min(5, || {
            for _ in 0..compiles {
                let cp = compile_lisp_pattern_with_posix(&lisp, false, false, true)
                    .expect("pattern compiles");
                std::hint::black_box(&cp);
            }
        });
        report.push_str(&format!(
            "  {name:<16} compile-miss {:>8.2?}/ea  cache-hit {:>8.2?}/ea\n",
            t_miss / compiles,
            t_hit / compiles,
        ));
    }
    panic!("{report}");
}

/// Per-opcode `analyze_first` gates (GNU regex-emacs.c:3062-3234).
#[test]
fn regex_fastmap_syntax_pattern_gates() {
    // `\_>` after a literal: match-time syntax use must NOT disable the
    // fastmap; the leading exactn provides the single candidate byte.
    let cp = regex_bench_compile("(defun\\_>", false);
    assert!(cp.uses_syntax, "\\_> consults syntax at match time");
    assert!(!cp.used_syntax, "no table content is baked for \\_>");
    assert!(cp.fastmap_accurate && !cp.can_be_null, "fastmap stays live");
    assert!(cp.fastmap[b'(' as usize]);
    assert!(!cp.fastmap[b'd' as usize]);

    // Zero-width `\<` contributes the following atom (GNU: "not
    // succeeded yet", keep walking).
    let cp = regex_bench_compile("\\<defun", false);
    assert!(!cp.can_be_null);
    assert!(cp.fastmap[b'd' as usize]);
    assert!(!cp.fastmap[b'e' as usize]);

    // Leading `\w` (syntaxspec): GNU analyze_first aborts ("This match
    // depends on text properties") -> can_be_null -> no skip.
    let cp = regex_bench_compile("\\w+", false);
    assert!(cp.can_be_null, "leading syntaxspec disables the skip");

    // Fixed POSIX class: fastmap = ASCII members + multibyte leads; no
    // syntax-table axis.
    let cp = regex_bench_compile("[[:digit:]]x", false);
    assert!(!cp.can_be_null && !cp.used_syntax);
    assert!(cp.fastmap[b'5' as usize]);
    assert!(!cp.fastmap[b'a' as usize]);
    assert!(cp.fastmap[0xC3], "class charsets admit multibyte leads");

    // Syntax-dependent POSIX class: `used_syntax` (GNU
    // regex-emacs.c:2096-2101) + standard-table ASCII word members.
    let cp = regex_bench_compile("[[:word:]]+z", false);
    assert!(cp.used_syntax && !cp.can_be_null);
    assert!(cp.fastmap[b'a' as usize] && cp.fastmap[b'0' as usize]);
    assert!(
        !cp.fastmap[b'-' as usize],
        "standard table: '-' is not a word constituent"
    );

    // Rebake against a table where '-' IS a word constituent.
    let mut cp_dash = cp.clone();
    regex_emacs::recompute_fastmap(&mut cp_dash, &DashWordSyntaxLookup);
    assert!(cp_dash.fastmap[b'-' as usize]);

    // keyword-colon: `\_<` passes through to the exactn `:`.
    let cp = regex_bench_compile("\\_<:\\(?:\\w\\|\\s_\\|\\\\.\\)+\\_>", false);
    assert!(!cp.can_be_null);
    assert!(cp.fastmap[b':' as usize]);
    assert!(!cp.fastmap[b'a' as usize]);
}

/// GNU's fail-stack budget (`emacs_re_max_failures`): a pattern whose
/// live choice-point stack grows past it aborts with "Stack overflow in
/// regexp matcher" instead of churning unboundedly.  `\(a*\)*b` pushes
/// one failure point per input char in its first outer iteration, so a
/// long-enough run of a's deterministically overflows.
#[test]
fn regex_fail_stack_overflow_flags_engine() {
    let n = 400_000usize;
    let text = "a".repeat(n);
    let cp = regex_bench_compile("\\(a*\\)*b", false);
    let syn = DefaultSyntaxLookup;
    assert!(
        regex_emacs::re_search(&cp, text.as_bytes(), 0, text.len() as isize, &syn, 0).is_none(),
        "no match once the matcher aborts"
    );
    assert!(
        regex_emacs::take_matcher_overflow(),
        "the fail-stack limit must flag overflow for the front-end"
    );
    // A pattern within the budget: no overflow, plain no-match.  (NOTE:
    // `\(a*\)*b` over a SMALL run of a's is the classic exponential
    // backtracker with a bounded live stack — GNU's limit does not stop
    // it either, only C-g does — so the in-budget control uses `a*b`.)
    let small = "a".repeat(64);
    let cp_ok = regex_bench_compile("a*b", false);
    assert!(
        regex_emacs::re_search(&cp_ok, small.as_bytes(), 0, small.len() as isize, &syn, 0)
            .is_none()
    );
    assert!(!regex_emacs::take_matcher_overflow());
}

/// `resolve_smart_jumps`: a simple greedy loop whose body excludes the
/// continuation becomes GNU's one-push keep-string fast loop; an
/// overlapping continuation keeps the safe backtracking loop.  No
/// `on_failure_jump_smart` survives compilation.
#[test]
fn regex_smart_loop_resolution() {
    // `[ \t']*(` — the el-defs/catch-throw hot loop: exclusive.
    let cp = regex_bench_compile("[ \t']*(", false);
    assert!(
        cp.bytecode()
            .contains(&(regex_emacs::RegexOp::OnFailureKeepStringJump as u8)),
        "exclusive simple loop must resolve to on_failure_keep_string_jump"
    );
    assert!(
        !cp.bytecode()
            .contains(&(regex_emacs::RegexOp::OnFailureJumpSmart as u8)),
        "no unresolved smart jump may survive compilation"
    );

    // `a*a` — body overlaps continuation: must stay a backtracking loop.
    let cp2 = regex_bench_compile("a*a", false);
    assert!(
        !cp2.bytecode()
            .contains(&(regex_emacs::RegexOp::OnFailureKeepStringJump as u8)),
        "overlapping loop must NOT use the keep-string fast loop"
    );
    assert!(
        !cp2.bytecode()
            .contains(&(regex_emacs::RegexOp::OnFailureJumpSmart as u8))
    );

    // Behavior: the non-exclusive loop still backtracks (greedy [0-9]*
    // gives back the final digit for the trailing `3`).
    let syn = DefaultSyntaxLookup;
    let cp3 = regex_bench_compile("[0-9]*3", false);
    let (_pos, regs) = regex_emacs::re_search(&cp3, b"123", 0, 3, &syn, 0).expect("matches");
    assert_eq!((regs.start[0], regs.end[0]), (0, 3), "backtracking loop");

    // And the exclusive loop consumes greedily with the continuation
    // matched right after the loop's stop position.
    let cp4 = regex_bench_compile("[0-9]*x", false);
    let (_pos, regs) = regex_emacs::re_search(&cp4, b"0919x!", 0, 6, &syn, 0).expect("matches");
    assert_eq!((regs.start[0], regs.end[0]), (0, 5), "keep-string loop");

    // `.*\n` — GNU's motivating example for keep-string loops.
    let cp5 = regex_bench_compile(".*\n", false);
    assert!(
        cp5.bytecode()
            .contains(&(regex_emacs::RegexOp::OnFailureKeepStringJump as u8))
    );
    let (_pos, regs) = regex_emacs::re_search(&cp5, b"ab\ncd", 0, 5, &syn, 0).expect("matches");
    assert_eq!((regs.start[0], regs.end[0]), (0, 3));
}

#[test]
fn literal_register_reuse_discards_captures_and_publishes_character_positions() {
    crate::test_utils::init_test_tracing();
    for kind in implemented_text_backends() {
        for text in ["xab ab", "éab 中ab"] {
            for backward in [false, true] {
                let mut buf = make_test_buffer_with_backend(text, kind);
                // A prior result with enough captures to spill inline storage.
                let mut regs = SearchRegisters(SearchRegistersKind::Groups(EngineMatchData::new(
                    std::iter::repeat_n(Some(MatchGroup::new(0, 1)), 12).collect(),
                )));
                let mut published = None;
                regs.publish_buffer_into(&buf, &mut published);
                for pattern in ["ab", "b", "", "absent"] {
                    let start = if backward { text.len() } else { 0 };
                    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(start));
                    let old = match_data_snapshot(&published);
                    let result = if backward {
                        super::search_backward_into(
                            &mut buf,
                            &lisp_pat(pattern),
                            None,
                            true,
                            false,
                            &mut regs,
                        )
                    } else {
                        super::search_forward_into(
                            &mut buf,
                            &lisp_pat(pattern),
                            None,
                            true,
                            false,
                            &mut regs,
                        )
                    }
                    .unwrap();
                    let expected_start = if backward {
                        text.rfind(pattern)
                    } else {
                        text.find(pattern)
                    };
                    match (result, expected_start) {
                        (Some(point), Some(byte_start)) => {
                            let byte_end = byte_start + pattern.len();
                            assert_eq!(point.get(), if backward { byte_start } else { byte_end });
                            regs.publish_buffer_into(&buf, &mut published);
                            let data = published.as_ref().unwrap();
                            assert_eq!(data.source(), MatchDataSource::Buffer(buf.id));
                            assert_eq!(
                                data.group(0),
                                Some(MatchGroup::new(
                                    text[..byte_start].chars().count() + 1,
                                    text[..byte_end].chars().count() + 1,
                                ))
                            );
                            let groups = data.groups_snapshot();
                            assert_eq!(groups.len(), GNU_SEARCH_REGS_BASE_CAPACITY);
                            assert!(groups[1..].iter().all(Option::is_none));
                        }
                        (None, None) => assert_eq!(match_data_snapshot(&published), old),
                        other => panic!("literal result disagrees: {other:?}"),
                    }
                }
            }
        }
    }
}

#[test]
fn literal_register_reuse_preserves_raw_unibyte_coordinates() {
    crate::test_utils::init_test_tracing();
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("raw-registers"),
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.set_multibyte_value(false);
    buf.insert_lisp_string(&LispString::from_unibyte(vec![0xff, b'a', b'b']));
    let mut regs = SearchRegisters::default();
    let mut published = None;
    for backward in [false, true] {
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(if backward {
            3
        } else {
            0
        }));
        let pattern = LispString::from_unibyte(vec![b'a', b'b']);
        let result = if backward {
            super::search_backward_into(&mut buf, &pattern, None, false, false, &mut regs)
        } else {
            super::search_forward_into(&mut buf, &pattern, None, false, false, &mut regs)
        }
        .unwrap()
        .unwrap();
        assert_eq!(result.get(), if backward { 1 } else { 3 });
        regs.publish_buffer_into(&buf, &mut published);
        assert_eq!(
            published.as_ref().unwrap().group(0),
            Some(MatchGroup::new(2, 4))
        );
        assert_eq!(
            published.as_ref().unwrap().groups_snapshot().len(),
            GNU_SEARCH_REGS_BASE_CAPACITY
        );
    }
}

#[test]
fn composition_match_length_preserves_native_regexp_extent() {
    crate::test_utils::init_test_tracing();
    let tail = " untouched界".repeat(8192);
    for (pattern, prefix, expected) in [
        (r"\(é界\)\1", "é界é界", Some(4)),
        (r"\(a+\)ab", "aaab", Some(4)),
        ("a*", "界", Some(0)),
        ("É", "é", None),
        ("missing", "é界", None),
    ] {
        let text = format!("{prefix}{tail}");
        assert_eq!(
            looking_at_lisp_pattern_length_with_syntax(
                &LispString::from_utf8(pattern),
                &text,
                &DefaultSyntaxLookup,
            )
            .expect("valid native regexp"),
            expected,
            "pattern {pattern:?}",
        );
    }
    assert!(
        looking_at_lisp_pattern_length_with_syntax(
            &LispString::from_utf8("["),
            &tail,
            &DefaultSyntaxLookup,
        )
        .is_err()
    );
}

#[test]
fn retained_match_registers_preserve_existing_layout() {
    // This is the representation before failure could retag Lisp registers.
    // Its size/alignment (including Option's niche) must remain unchanged.
    #[allow(dead_code)]
    enum PreviousKind {
        StringChars {
            groups: smallvec::SmallVec<[Option<CharRange>; GNU_SEARCH_REGS_BASE_CAPACITY]>,
            searched: Option<SearchedString>,
        },
        Buffer {
            id: BufferId,
            groups: smallvec::SmallVec<[Option<LispCharMatchRange>; GNU_SEARCH_REGS_BASE_CAPACITY]>,
        },
    }
    #[allow(dead_code)]
    struct PreviousMatchData {
        kind: PreviousKind,
        #[cfg(debug_assertions)]
        read_mask: std::cell::Cell<u64>,
    }
    assert_eq!(
        std::mem::size_of::<MatchData>(),
        std::mem::size_of::<PreviousMatchData>()
    );
    assert_eq!(
        std::mem::align_of::<MatchData>(),
        std::mem::align_of::<PreviousMatchData>()
    );
    assert_eq!(
        std::mem::size_of::<Option<MatchData>>(),
        std::mem::size_of::<Option<PreviousMatchData>>()
    );
}

fn match_group(group: Option<MatchGroup>) -> Option<MatchGroup> {
    group
}

fn search_forward(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    super::search_forward(buf, pattern, bound, noerror, case_fold)
        .map(|success| commit_test_search_success(success, match_data))
}

fn search_backward(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    super::search_backward(buf, pattern, bound, noerror, case_fold)
        .map(|success| commit_test_search_success(success, match_data))
}

fn re_search_forward(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    super::re_search_forward(buf, pattern, bound, noerror, case_fold)
        .map(|success| commit_test_search_success(success, match_data))
}

fn re_search_backward(
    buf: &mut Buffer,
    pattern: &LispString,
    bound: Option<usize>,
    noerror: bool,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<Option<usize>, String> {
    super::re_search_backward(buf, pattern, bound, noerror, case_fold)
        .map(|success| commit_test_search_success(success, match_data))
}

fn looking_at(
    buf: &Buffer,
    pattern: &LispString,
    case_fold: bool,
    match_data: &mut Option<MatchData>,
) -> Result<bool, String> {
    super::looking_at(buf, pattern, case_fold).map(|published_match_data| {
        if let Some(published_match_data) = published_match_data {
            *match_data = Some(published_match_data);
            true
        } else {
            false
        }
    })
}

/// `replace_match_string` is byte-native (issue #131). These string tests pass
/// Rust `&str` sources mirroring `string_match_full`, which searches
/// `LispString::from_utf8(source)`; this helper threads the matching Emacs-bytes
/// and `STRING_MULTIBYTE` flag so the existing assertions stay byte-faithful.
fn replace_match_string_str(
    source: &str,
    newtext: &str,
    fixedcase: bool,
    literal: bool,
    subexp: usize,
    match_data: &Option<MatchData>,
) -> Result<Vec<u8>, String> {
    let source_lisp = LispString::from_utf8(source);
    replace_match_string(
        source_lisp.as_bytes(),
        source_lisp.is_multibyte(),
        newtext,
        fixedcase,
        literal,
        subexp,
        match_data,
    )
}

/// Build a multibyte `LispString` search pattern from a Rust literal (issue #131:
/// `search_forward`/`search_backward` now take the faithful pattern, not a storage
/// `&str`).
fn lisp_pat(s: &str) -> LispString {
    LispString::from_utf8(s)
}

fn buffer_match_group_byte_range(buf: &Buffer, group: MatchGroup) -> EmacsByteRange {
    EmacsByteRange::new(
        buf.lisp_pos_to_emacs_byte_pos(neovm_edit::buffer::LispCharPos1::from_one_based_usize(
            group.start(),
        )),
        buf.lisp_pos_to_emacs_byte_pos(neovm_edit::buffer::LispCharPos1::from_one_based_usize(
            group.end(),
        )),
    )
}

fn extract_heap_match_string(md: &MatchData, group: usize) -> Option<String> {
    let searched = match md.searched_string()? {
        SearchedString::Heap(val) => SearchedString::Heap(*val),
        SearchedString::Owned(text) => SearchedString::Owned(text.clone()),
    };
    let group = match_group(md.group(group))?;
    let string = searched.as_lisp_string()?;
    let byte_start = char_pos_to_byte_lisp_string(string, group.start());
    let byte_end = char_pos_to_byte_lisp_string(string, group.end());
    string
        .slice(byte_start, byte_end)
        .and_then(|slice| slice.as_utf8_str().map(str::to_owned))
}

fn make_test_buffer(text: &str) -> Buffer {
    make_test_buffer_with_backend(text, BufferTextBackendKind::GapBuffer)
}

fn make_test_buffer_with_backend(text: &str, kind: BufferTextBackendKind) -> Buffer {
    let implemented_kind = kind.implemented().expect("test backend is implemented");
    let mut buf = Buffer::new_with_text_backend_kind(
        BufferId(1),
        Value::string("test"),
        implemented_kind,
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.insert(text);
    // Reset point to beginning
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    // zv was updated by insert
    buf
}

fn implemented_text_backends() -> impl Iterator<Item = BufferTextBackendKind> {
    BufferTextBackendKind::implemented_variants()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct MatchDataSnapshot {
    groups: Vec<Option<MatchGroup>>,
    searched_buffer: Option<BufferId>,
    searched_string_is_some: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BufferSearchSnapshot<T> {
    result: Result<T, String>,
    pt_byte: usize,
    pt: usize,
    full_bytes: Vec<u8>,
    multibyte: bool,
    match_data: Option<MatchDataSnapshot>,
}

fn match_data_snapshot(match_data: &Option<MatchData>) -> Option<MatchDataSnapshot> {
    match_data.as_ref().map(|data| {
        let source = data.source();
        MatchDataSnapshot {
            groups: data.groups_snapshot(),
            searched_buffer: match source {
                MatchDataSource::String => None,
                MatchDataSource::Buffer(buffer_id) => Some(buffer_id),
            },
            searched_string_is_some: source.is_string(),
        }
    })
}

fn buffer_search_snapshot<T>(
    result: Result<T, String>,
    buf: &Buffer,
    match_data: &Option<MatchData>,
) -> BufferSearchSnapshot<T> {
    let mut full_bytes = Vec::new();
    buf.copy_emacs_byte_range_to(buf.full_emacs_byte_range(), &mut full_bytes);
    BufferSearchSnapshot {
        result,
        pt_byte: buf.point_emacs_byte_pos().get(),
        pt: buf.point_char_pos().get(),
        full_bytes,
        multibyte: buf.get_multibyte(),
        match_data: match_data_snapshot(match_data),
    }
}

fn literal_search_backend_trace(
    kind: BufferTextBackendKind,
) -> Vec<BufferSearchSnapshot<Option<usize>>> {
    let mut buf = make_fragmented_search_buffer(kind);
    let mut snapshots = Vec::new();

    let mut md = None;
    let result = search_forward(&mut buf, &lisp_pat("foo"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
        buf.total_emacs_byte_len().get(),
    ));
    let result = search_backward(&mut buf, &lisp_pat("foo"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    let result = search_forward(&mut buf, &lisp_pat("beta"), None, false, true, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    let narrow_start = "α foo\n".len();
    let narrow_end = "α foo\nBeta 123".len();
    buf.narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        narrow_start,
        narrow_end,
    ));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(narrow_start));
    let result = search_forward(&mut buf, &lisp_pat("foo"), None, true, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(narrow_start));
    let result = search_forward(&mut buf, &lisp_pat("Beta"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    snapshots
}

fn regex_search_backend_trace(
    kind: BufferTextBackendKind,
) -> Vec<BufferSearchSnapshot<Option<usize>>> {
    let mut buf = make_fragmented_search_buffer(kind);
    let mut snapshots = Vec::new();

    let mut md = None;
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("\\([^ \n]+\\) \\([0-9]+\\)"),
        None,
        false,
        false,
        &mut md,
    );
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
        buf.total_emacs_byte_len().get(),
    ));
    let result = re_search_backward(
        &mut buf,
        &lisp_pat("\\(foo\\)\\([0-9]+\\)"),
        None,
        false,
        false,
        &mut md,
    );
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    let narrow_start = "α foo\n".len();
    let narrow_end = "α foo\nBeta 123".len();
    buf.narrow_to_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        narrow_start,
        narrow_end,
    ));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(narrow_start));
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("^\\([^ ]+\\) \\([0-9]+\\)$"),
        None,
        false,
        false,
        &mut md,
    );
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    snapshots
}

fn looking_at_backend_trace(kind: BufferTextBackendKind) -> Vec<BufferSearchSnapshot<bool>> {
    let mut buf = make_fragmented_search_buffer(kind);
    let mut snapshots = Vec::new();

    let mut md = None;
    let gamma_line = "α foo\nBeta 123\n".len();
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(gamma_line));
    let result = looking_at(&buf, &lisp_pat("\\(.\\) foo\\([0-9]+\\)"), false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new("α ".len()));
    let result = looking_at(&buf, &lisp_pat("foo"), false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    snapshots
}

fn replace_match_backend_trace(kind: BufferTextBackendKind) -> BufferSearchSnapshot<()> {
    let mut buf = make_fragmented_search_buffer(kind);
    let mut md = None;
    let result = re_search_forward(
        &mut buf,
        &lisp_pat("\\(foo\\)\\([0-9]+\\)"),
        None,
        false,
        false,
        &mut md,
    );
    assert!(
        result.is_ok(),
        "setup search failed for {kind:?}: {result:?}"
    );

    let result = replace_match_buffer(&mut buf, "\\1-\\2", false, false, 0, &md);
    buffer_search_snapshot(result.map(|_| ()), &buf, &md)
}

fn unibyte_search_backend_trace(
    kind: BufferTextBackendKind,
) -> Vec<BufferSearchSnapshot<Option<usize>>> {
    let mut buf = make_unibyte_search_buffer(kind);
    let mut snapshots = Vec::new();

    let mut md = None;
    let result = re_search_forward(&mut buf, &lisp_pat("."), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    let result = search_forward(&mut buf, &lisp_pat("a"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    md = None;
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(
        buf.total_emacs_byte_len().get(),
    ));
    let result = search_backward(&mut buf, &lisp_pat("b"), None, false, false, &mut md);
    snapshots.push(buffer_search_snapshot(result, &buf, &md));

    snapshots
}

/// Real GNU Emacs 31 emacs-lisp-mode font-lock matcher regexps.
const REGEX_BENCH_FONTLOCK_PATTERNS: &[(&str, &str)] = &[
    (
        "el-defs",
        "(\\(cl-def\\(?:generic\\|m\\(?:acro\\|ethod\\)\\|s\\(?:\\(?:truc\\|ubs\\)t\\)\\|type\\|un\\)\\|def\\(?:a\\(?:dvice\\|lias\\)\\|c\\(?:lass\\|onst\\|ustom\\)\\|face\\|g\\(?:eneric\\|roup\\)\\|ine-\\(?:advice\\|derived-mode\\|error\\|g\\(?:\\(?:eneric\\|lobalized-minor\\)-mode\\)\\|inline\\|minor-mode\\|skeleton\\|widget\\)\\|m\\(?:acro\\|ethod\\)\\|subst\\|theme\\|un\\|var\\(?:-local\\|alias\\)?\\)\\|ert-deftest\\)\\_>[ \t']*\\(([ \t']*\\)?\\(\\(setf\\)[ \t]+\\(?:\\w\\|\\s_\\|\\\\.\\)+\\|\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)?",
    ),
    ("sexp-head-kw", "(\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)\\_>"),
    (
        "autoload-cookie",
        "^;;;###\\(\\([-[:alnum:]]+?\\)-\\)?\\(autoload\\)",
    ),
    (
        "el-errors",
        "(\\(cl-\\(?:assert\\|check-type\\)\\|error\\|signal\\|user-error\\|warn\\)\\_>",
    ),
    (
        "catch-throw",
        "(\\(catch\\|throw\\|featurep\\|provide\\|require\\)\\_>[ \t']*\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)?",
    ),
    (
        "quoted-symbol",
        "[`\u{2018}']\\(\\(?:\\w\\|\\s_\\|\\\\.\\)+\\)['\u{2019}]",
    ),
    ("keyword-colon", "\\_<:\\(?:\\w\\|\\s_\\|\\\\.\\)+\\_>"),
    ("backslash-esc", "\\(\\\\\\)\\([^\"\\]\\)"),
];

fn regex_bench_min(iters: u32, mut f: impl FnMut()) -> std::time::Duration {
    // Warm once (compile caches, page in the haystack).
    f();
    let mut best = std::time::Duration::MAX;
    for _ in 0..iters {
        let t = std::time::Instant::now();
        f();
        best = best.min(t.elapsed());
    }
    best
}

fn regex_bench_compile(pattern: &str, case_fold: bool) -> regex_emacs::CompiledPattern {
    regex_emacs::regex_compile_lisp(&LispString::from_utf8(pattern), false, case_fold)
        .expect("bench pattern should compile")
}

/// Syntax lookup with `-` promoted to a word constituent (as lisp modes
/// do via `modify-syntax-entry`) — exercises the buffer-table rebake
/// path of the fastmap without needing a full evaluator.
struct DashWordSyntaxLookup;

impl regex_emacs::SyntaxLookup for DashWordSyntaxLookup {
    fn char_syntax(&self, c: char) -> crate::emacs_core::syntax::SyntaxClass {
        if c == '-' {
            crate::emacs_core::syntax::SyntaxClass::Word
        } else {
            crate::emacs_core::syntax::standard_syntax_class_for_char(c)
        }
    }

    fn char_has_category(&self, c: char, cat: u8) -> bool {
        DefaultSyntaxLookup.char_has_category(c, cat)
    }

    fn cache_key(&self) -> regex_emacs::SyntaxCacheKey {
        // Test-only: distinct from `Standard` so it can never satisfy a
        // standard-baked cache entry.
        regex_emacs::SyntaxCacheKey::External {
            id: usize::MAX - 1,
            epoch: 0,
        }
    }
}

fn commit_test_search_success(
    success: Option<BufferSearchSuccess>,
    match_data: &mut Option<MatchData>,
) -> Option<usize> {
    success.map(|success| {
        let (_buffer_id, point, published_match_data) = success.into_parts();
        *match_data = Some(published_match_data);
        point.get()
    })
}

fn make_fragmented_search_buffer(kind: BufferTextBackendKind) -> Buffer {
    let mut buf = make_test_buffer_with_backend("α foo\nBeta 123\nγ foo42\nomega", kind);

    let first_fragment = "α ".len();
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(first_fragment));
    buf.insert("tmp");
    buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        first_fragment,
        first_fragment + "tmp".len(),
    ));

    let second_fragment = "α foo\nBeta ".len();
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(second_fragment));
    buf.insert("xx");
    buf.delete_emacs_byte_range(neovm_edit::buffer::EmacsByteRange::from_usize(
        second_fragment,
        second_fragment + "xx".len(),
    ));

    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    assert_eq!(buf.buffer_string(), "α foo\nBeta 123\nγ foo42\nomega");
    assert_eq!(buf.text_backend_kind(), kind);
    buf
}

fn make_unibyte_search_buffer(kind: BufferTextBackendKind) -> Buffer {
    let implemented_kind = kind.implemented().expect("test backend is implemented");
    let mut buf = Buffer::new_with_text_backend_kind(
        BufferId(1),
        Value::string("raw"),
        implemented_kind,
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.set_multibyte_value(false);
    buf.insert_lisp_string(&LispString::from_unibyte(vec![0xFF, b'a', b'b', 0x80]));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(0));
    assert_eq!(buf.text_backend_kind(), kind);
    buf
}
