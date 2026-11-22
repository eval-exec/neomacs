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
fn string_match_case_fold_non_latin_unicode_literals() {
    crate::test_utils::init_test_tracing();
    let mut md = None;

    let greek = string_match_full_with_case_fold("Ω", "ω", 0, true, &mut md);
    assert_eq!(greek.unwrap(), Some(0));

    let cyrillic = string_match_full_with_case_fold("Д", "д", 0, true, &mut md);
    assert_eq!(cyrillic.unwrap(), Some(0));
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
fn literal_search_reads_both_gap_halves_without_moving_the_gap() {
    crate::test_utils::init_test_tracing();
    let saved = neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global();
    let mut buf = Buffer::new(BufferId(1), Value::string("gap-search"), saved);
    buf.insert_lisp_string(&LispString::from_utf8(&"a".repeat(100)));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(49));
    buf.insert_lisp_string(&LispString::from_utf8("x"));
    let gap = buf.gap_position_lisp();
    assert_eq!(gap, 51);
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::ZERO);
    let mut regs = SearchRegisters::default();
    let missing = super::search_forward_into(
        &mut buf,
        &LispString::from_utf8("zzz"),
        None,
        true,
        false,
        &mut regs,
    )
    .unwrap();
    assert!(missing.is_none());
    assert_eq!(buf.gap_position_lisp(), gap);
    buf.goto_emacs_byte_pos(buf.total_emacs_byte_end_pos());
    let missing_back = super::search_backward_into(
        &mut buf,
        &LispString::from_utf8("zzz"),
        None,
        true,
        false,
        &mut regs,
    )
    .unwrap();
    assert!(missing_back.is_none());
    assert_eq!(buf.gap_position_lisp(), gap);

    let saved = neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global();
    let mut buf = Buffer::new(BufferId(2), Value::string("gap-straddle"), saved);
    buf.insert_lisp_string(&LispString::from_utf8("hello"));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(2));
    buf.insert_lisp_string(&LispString::from_utf8("Q"));
    buf.delete_emacs_byte_range(EmacsByteRange::new(
        neovm_edit::buffer::EmacsBytePos::new(2),
        neovm_edit::buffer::EmacsBytePos::new(3),
    ));
    let gap = buf.gap_position_lisp();
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::ZERO);
    let hit = super::search_forward_into(
        &mut buf,
        &LispString::from_utf8("el"),
        None,
        true,
        false,
        &mut regs,
    )
    .unwrap()
    .unwrap();
    assert_eq!(hit.get(), 3);
    assert_eq!(buf.gap_position_lisp(), gap);
    buf.goto_emacs_byte_pos(buf.total_emacs_byte_end_pos());
    let back = super::search_backward_into(
        &mut buf,
        &LispString::from_utf8("el"),
        None,
        true,
        false,
        &mut regs,
    )
    .unwrap()
    .unwrap();
    assert_eq!(back.get(), 1);
    assert_eq!(buf.gap_position_lisp(), gap);

    // The crossing window is counted in characters, not bytes: one character
    // of an "éé" pattern is two Emacs bytes, so a byte-counted window would
    // slice the continuation byte and miss the match.
    let saved = neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global();
    let mut buf = Buffer::new(BufferId(3), Value::string("gap-straddle-multibyte"), saved);
    buf.insert_lisp_string(&LispString::from_utf8("aééx"));
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(3));
    buf.insert_lisp_string(&LispString::from_utf8("Q"));
    buf.delete_emacs_byte_range(EmacsByteRange::new(
        neovm_edit::buffer::EmacsBytePos::new(3),
        neovm_edit::buffer::EmacsBytePos::new(4),
    ));
    let gap = buf.gap_position_lisp();
    assert_eq!(gap, 3);
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::ZERO);
    let hit = super::search_forward_into(
        &mut buf,
        &LispString::from_utf8("éé"),
        None,
        true,
        false,
        &mut regs,
    )
    .unwrap()
    .unwrap();
    assert_eq!(hit.get(), 5);
    assert_eq!(buf.gap_position_lisp(), gap);
    buf.goto_emacs_byte_pos(buf.total_emacs_byte_end_pos());
    let back = super::search_backward_into(
        &mut buf,
        &LispString::from_utf8("éé"),
        None,
        true,
        false,
        &mut regs,
    )
    .unwrap()
    .unwrap();
    assert_eq!(back.get(), 1);
    assert_eq!(buf.gap_position_lisp(), gap);
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

fn match_group(group: Option<MatchGroup>) -> Option<MatchGroup> {
    group
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

#[derive(Clone, Debug, PartialEq, Eq)]
struct BufferSearchSnapshot<T> {
    result: Result<T, String>,
    pt_byte: usize,
    pt: usize,
    full_bytes: Vec<u8>,
    multibyte: bool,
    match_data: Option<MatchDataSnapshot>,
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
struct MatchDataSnapshot {
    groups: Vec<Option<MatchGroup>>,
    searched_buffer: Option<BufferId>,
    searched_string_is_some: bool,
}

/// Reference implementation for the literal searchers: the exact
/// sliding-window shapes the linear-time versions replaced.
#[cfg(test)]
mod literal_search_linear_equivalence {
    use super::*;

    fn naive_find(text: &[u8], needle: &[u8], fold: bool) -> Option<usize> {
        if needle.is_empty() {
            return Some(0);
        }
        if needle.len() > text.len() {
            return None;
        }
        text.windows(needle.len()).position(|w| {
            if fold {
                w.iter()
                    .zip(needle.iter())
                    .all(|(l, r)| l.eq_ignore_ascii_case(r))
            } else {
                w == needle
            }
        })
    }

    fn naive_rfind(text: &[u8], needle: &[u8], fold: bool) -> Option<usize> {
        if needle.is_empty() {
            return Some(text.len());
        }
        if needle.len() > text.len() {
            return None;
        }
        text.windows(needle.len()).rposition(|w| {
            if fold {
                w.iter()
                    .zip(needle.iter())
                    .all(|(l, r)| l.eq_ignore_ascii_case(r))
            } else {
                w == needle
            }
        })
    }

    fn check(text: &[u8], needle: &[u8]) {
        for fold in [false, true] {
            let want_find = naive_find(text, needle, fold);
            let want_rfind = naive_rfind(text, needle, fold);
            let got_find =
                literal_find_emacs_bytes(text, needle, false, fold).map(|group| group.start);
            let got_rfind =
                literal_rfind_emacs_bytes(text, needle, false, fold).map(|group| group.start);
            // The unibyte fold path routes through the Emacs case table for
            // non-ASCII bytes; restrict fold equivalence checks to ASCII
            // needles, where the searcher takes the linear folded path.
            if !fold || needle.is_ascii() {
                assert_eq!(
                    got_find, want_find,
                    "find text={text:?} needle={needle:?} fold={fold}"
                );
                assert_eq!(
                    got_rfind, want_rfind,
                    "rfind text={text:?} needle={needle:?} fold={fold}"
                );
            }
        }
    }

    #[test]
    fn adversarial_repetitive_inputs_match_naive_reference() {
        crate::test_utils::init_test_tracing();
        // Classic quadratic killers: long runs of one byte with a needle
        // that almost matches at every position.
        let long_a = vec![b'a'; 4096];
        let mut needle_ab = vec![b'a'; 63];
        needle_ab.push(b'b');
        check(&long_a, &needle_ab);

        let mut text_ab = long_a.clone();
        text_ab.extend_from_slice(&needle_ab);
        check(&text_ab, &needle_ab);

        // Period-2 repetition with a mismatching tail.
        let abab: Vec<u8> = std::iter::repeat_n([b'a', b'b'], 1024).flatten().collect();
        let mut needle_abac: Vec<u8> = std::iter::repeat_n([b'a', b'b'], 16).flatten().collect();
        needle_abac.push(b'a');
        needle_abac.push(b'c');
        check(&abab, &needle_abac);
    }

    #[test]
    fn case_fold_and_boundary_cases_match_naive_reference() {
        crate::test_utils::init_test_tracing();
        check(b"", b"");
        check(b"", b"x");
        check(b"x", b"");
        check(b"HELLO world HeLLo", b"hello");
        check(b"HELLO world HeLLo", b"HELLO");
        check(b"aAaAaAaAaAaAaAaAb", b"AAAAb");
        // Raw non-ASCII bytes in the haystack with an ASCII needle: fold
        // comparison must never treat 0x80+ bytes as ASCII letters.
        check(b"\xC3\xA9caf\xC3\xA9 CAFE cafe", b"cafe");
        check(b"\xFFcafe\xFF", b"CAFE");
        // Needle exactly at start / end.
        check(b"needle in haystack", b"needle");
        check(b"in haystack needle", b"needle");
    }

    #[test]
    fn randomized_small_cases_match_naive_reference() {
        crate::test_utils::init_test_tracing();
        // Deterministic xorshift; small alphabet maximizes near-matches.
        let mut state = 0x9E3779B97F4A7C15u64;
        let mut next = move || {
            state ^= state >> 12;
            state ^= state << 25;
            state ^= state >> 27;
            state = state.wrapping_mul(0x2545F4914F6CDD1D);
            state
        };
        for _ in 0..400 {
            let text_len = (next() % 48) as usize;
            let needle_len = (next() % 8) as usize;
            let text: Vec<u8> = (0..text_len)
                .map(|_| b"aAbB"[(next() % 4) as usize])
                .collect();
            let needle: Vec<u8> = (0..needle_len)
                .map(|_| b"aAbB"[(next() % 4) as usize])
                .collect();
            check(&text, &needle);
        }
    }
}

fn literal_gap_test_buffer(text: &str) -> Buffer {
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("literal-gap"),
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.insert(text);
    buf
}

/// Park the gap at byte POS the way an edit leaves it: insert a char there
/// and delete it again.  Point is kept.
fn park_gap_at_keeping_point(buf: &mut Buffer, pos: usize) {
    use neovm_edit::buffer::EmacsBytePos;
    let point = buf.point_emacs_byte_pos();
    buf.goto_emacs_byte_pos(EmacsBytePos::new(pos));
    buf.insert("x");
    buf.delete_emacs_byte_range(EmacsByteRange::new(
        EmacsBytePos::new(pos),
        EmacsBytePos::new(pos + 1),
    ));
    buf.goto_emacs_byte_pos(point);
}

#[derive(Clone, Copy, Debug)]
enum LiteralDirection {
    Forward,
    Backward,
}

/// One literal search from byte START: the point it returns and match group 0.
fn literal_search_at(
    buf: &mut Buffer,
    direction: LiteralDirection,
    pattern: &LispString,
    start: usize,
    bound: Option<usize>,
    case_fold: bool,
) -> Option<(usize, MatchGroup)> {
    buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(start));
    let mut regs = SearchRegisters::default();
    let found = match direction {
        LiteralDirection::Forward => {
            super::search_forward_into(buf, pattern, bound, true, case_fold, &mut regs)
        }
        LiteralDirection::Backward => {
            super::search_backward_into(buf, pattern, bound, true, case_fold, &mut regs)
        }
    }
    .expect("noerror literal search does not fail")?;
    let mut published = None;
    regs.publish_buffer_into(buf, &mut published);
    Some((found.get(), published?.group(0)?))
}

/// Every hit of a COUNT-style loop (repeated searches continuing from the
/// last hit, as `search-forward' with COUNT > 1 or COUNT < 0 does), with the
/// gap re-parked at GAP before each step when GAP is given.
fn literal_search_hits(
    buf: &mut Buffer,
    direction: LiteralDirection,
    pattern: &LispString,
    bound: Option<usize>,
    case_fold: bool,
    gap: Option<usize>,
) -> Vec<(usize, MatchGroup)> {
    let mut point = match direction {
        LiteralDirection::Forward => 0,
        LiteralDirection::Backward => buf.accessible_emacs_byte_region().end().get(),
    };
    let mut hits = Vec::new();
    loop {
        if let Some(gap) = gap {
            park_gap_at_keeping_point(buf, gap);
        }
        let Some(hit) = literal_search_at(buf, direction, pattern, point, bound, case_fold) else {
            return hits;
        };
        if hit.0 == point {
            // An empty match does not advance; one hit is enough.
            hits.push(hit);
            return hits;
        }
        point = hit.0;
        hits.push(hit);
    }
}

#[test]
fn literal_search_results_do_not_depend_on_gap_position() {
    crate::test_utils::init_test_tracing();
    let text = "αβγ Éclair défun xyz défun ÉCLAIR tail défundéfun";
    let char_boundaries: Vec<usize> = text
        .char_indices()
        .map(|(i, _)| i)
        .chain([text.len()])
        .collect();
    // (pattern, case-fold): a hit straddling the gap at every offset, a hit
    // ending exactly at the gap, a case-folded multibyte pattern, and a
    // pattern longer than either gap half.
    let cases = [
        ("défun", false),
        ("DÉFUN", true),
        ("éclair", true),
        ("Éclair", false),
        ("fun", false),
        ("missing", false),
        (text, false),
    ];
    let bounds = [None, Some(9), Some(22), Some(40), Some(text.len())];
    let mut reference = literal_gap_test_buffer(text);
    let mut buf = literal_gap_test_buffer(text);
    for (pattern, case_fold) in cases {
        let pattern = LispString::from_utf8(pattern);
        for direction in [LiteralDirection::Forward, LiteralDirection::Backward] {
            for bound in bounds {
                let expected = literal_search_hits(
                    &mut reference,
                    direction,
                    &pattern,
                    bound,
                    case_fold,
                    None,
                );
                for &gap in &char_boundaries {
                    let got = literal_search_hits(
                        &mut buf,
                        direction,
                        &pattern,
                        bound,
                        case_fold,
                        Some(gap),
                    );
                    assert_eq!(
                        got, expected,
                        "{direction:?} {pattern:?} bound {bound:?} case-fold {case_fold} gap at {gap}"
                    );
                }
            }
        }
    }
}

/// The gap's byte position in an ASCII buffer.
fn ascii_gap_byte(buf: &Buffer) -> usize {
    usize::try_from(buf.gap_position_lisp() - 1).expect("gap position is at least 1")
}

#[test]
fn literal_search_loop_with_gap_in_span_copies_nothing() {
    use std::sync::atomic::Ordering;
    crate::test_utils::init_test_tracing();
    // About 1 MB, "defun" once per 505 bytes: 2,000 hits.
    let unit = format!("{}(defun marker)\n", "x".repeat(490));
    let text = unit.repeat(2_000);
    let pattern = LispString::from_utf8("defun");
    for (direction, parked) in [
        (LiteralDirection::Forward, text.len() - 10),
        (LiteralDirection::Backward, 10),
    ] {
        let mut buf = literal_gap_test_buffer(&text);
        park_gap_at_keeping_point(&mut buf, parked);
        let parked_gap = ascii_gap_byte(&buf);
        assert_eq!(parked_gap, parked);
        let copied_before = super::SEARCH_COPIED_BYTES.load(Ordering::Relaxed);
        let mut point = match direction {
            LiteralDirection::Forward => 0,
            LiteralDirection::Backward => text.len(),
        };
        let mut hits = 0;
        while let Some((found, _)) =
            literal_search_at(&mut buf, direction, &pattern, point, None, false)
        {
            point = found;
            hits += 1;
        }
        assert_eq!(hits, 2_000, "{direction:?}");
        let copied = super::SEARCH_COPIED_BYTES.load(Ordering::Relaxed) - copied_before;
        assert_eq!(copied, 0, "{direction:?}: bytes copied out of the buffer");
        // GNU `search_buffer_non_re' reads both gap halves through
        // BYTE_POS_ADDR (search.c:1298) and never calls move_gap: after the
        // loop `gap-position' is exactly what the last edit left.
        assert_eq!(
            ascii_gap_byte(&buf),
            parked_gap,
            "{direction:?}: a literal search must leave the gap where the edit left it"
        );
    }
}

#[test]
fn literal_search_after_edit_keeps_gap_at_the_edit() {
    use std::sync::atomic::Ordering;
    crate::test_utils::init_test_tracing();
    // The shape of a command hook: insert at P, search forward from P,
    // delete the char again, 200 times.
    let unit = format!("{}(defun marker)\n", "x".repeat(490));
    let text = unit.repeat(2_000);
    let pattern = LispString::from_utf8("defun");
    let mut buf = literal_gap_test_buffer(&text);
    let p = text.len() / 10;
    let copied_before = super::SEARCH_COPIED_BYTES.load(Ordering::Relaxed);
    for _ in 0..200 {
        buf.goto_emacs_byte_pos(neovm_edit::buffer::EmacsBytePos::new(p));
        buf.insert("x");
        // The edit parks the gap after the inserted character. GNU's literal
        // search leaves it there; the search must not move it (search.c:1298).
        let gap_after_edit = ascii_gap_byte(&buf);
        assert_eq!(
            gap_after_edit,
            p + 1,
            "insert must park the gap at the edit"
        );
        assert!(
            literal_search_at(
                &mut buf,
                LiteralDirection::Forward,
                &pattern,
                p,
                None,
                false
            )
            .is_some()
        );
        assert_eq!(
            ascii_gap_byte(&buf),
            gap_after_edit,
            "search moved the gap from the edit"
        );
        buf.delete_emacs_byte_range(EmacsByteRange::new(
            neovm_edit::buffer::EmacsBytePos::new(p),
            neovm_edit::buffer::EmacsBytePos::new(p + 1),
        ));
    }
    let copied = super::SEARCH_COPIED_BYTES.load(Ordering::Relaxed) - copied_before;
    assert_eq!(copied, 0, "bytes copied out of the buffer");
}

fn backward_search_buffer(text: &str) -> Buffer {
    let mut buf = Buffer::new(
        BufferId(1),
        Value::string("backward-search"),
        neovm_edit::buffer::shared::SavedPointBeforeCommand::new_editor_global(),
    );
    buf.insert(text);
    buf
}

fn byte_pos(pos: usize) -> neovm_edit::buffer::EmacsBytePos {
    neovm_edit::buffer::EmacsBytePos::new(pos)
}

/// Where the gap starts, read through the only view a search has of it: a
/// range is one piece exactly when the gap does not lie strictly inside it.
/// A gap at the buffer start reads as the buffer end; both are an edge.
fn gap_start(buf: &Buffer) -> usize {
    let (mut lo, mut hi) = (0, buf.total_emacs_byte_end_pos().get());
    while lo < hi {
        let mid = lo + (hi - lo).div_ceil(2);
        if buf.has_contiguous_emacs_byte_range(EmacsByteRange::new(byte_pos(0), byte_pos(mid))) {
            lo = mid;
        } else {
            hi = mid - 1;
        }
    }
    lo
}

/// Park the gap at POS the way an edit does: insert a character there and
/// delete it again.
fn park_gap_at(buf: &mut Buffer, pos: usize) {
    buf.goto_emacs_byte_pos(byte_pos(pos));
    buf.insert("Q");
    buf.delete_emacs_byte_range(EmacsByteRange::new(byte_pos(pos), byte_pos(pos + 1)));
}

/// Search backward from POINT with each of the three buffer entry points,
/// assert that they agree, and return the match start.
fn backward_search_all_entries(
    buf: &mut Buffer,
    pattern: &str,
    point: usize,
    bound: Option<usize>,
) -> Option<usize> {
    let pattern = LispString::from_utf8(pattern);
    let context = BufferRegexpMatchContext::new(
        crate::emacs_core::syntax::SyntaxProperties::Ignore,
        crate::emacs_core::regex_emacs::WordBoundaryLookup::default(),
    );
    let mut regs = SearchRegisters::default();
    buf.goto_emacs_byte_pos(byte_pos(point));
    let lisp = re_search_backward_lisp_with_posix_into(
        buf, &pattern, bound, true, false, false, context, &mut regs,
    )
    .unwrap()
    .map(|pos| pos.get());
    buf.goto_emacs_byte_pos(byte_pos(point));
    let syntax = buffer_syntax_lookup(buf);
    let compiled = compile_lisp_pattern_with_posix_translation(
        &pattern,
        false,
        false,
        buf.get_multibyte(),
        None,
        &syntax,
    )
    .unwrap();
    let precompiled =
        re_search_backward_compiled_into(buf, &compiled, bound, true, context, &mut regs)
            .unwrap()
            .map(|pos| pos.get());
    buf.goto_emacs_byte_pos(byte_pos(point));
    let raw = re_search_backward(buf, &pattern, bound, true, false)
        .unwrap()
        .map(|found| found.into_parts().1.get());
    buf.goto_emacs_byte_pos(byte_pos(point));
    assert_eq!(
        (lisp, precompiled),
        (raw, raw),
        "{pattern:?} from {point} bound {bound:?}"
    );
    raw
}

#[test]
fn re_search_backward_after_an_edit_moves_the_gap_by_a_character_not_the_buffer() {
    crate::test_utils::init_test_tracing();
    // What a `looking-back' hook does after each keystroke in a 1 MB buffer:
    // type a character mid-buffer, then search back to the line start.  The
    // search reads [line start, point] and the character after point; after
    // the insert the gap sits at point, so that stretch is one piece but for
    // one character.  Asking for the whole region instead pushed the gap to
    // a region edge and the next insert dragged it back: 100 rounds moved
    // about 100 MB.
    let line = "abcdefg ijklmno qrstuvw yzabcde ghijklm opqrstu wxyzabc efghijk mnopq\n";
    let mut buf = backward_search_buffer(&line.repeat(1_000_000 / line.len()));
    let line_start = (buf.total_emacs_byte_end_pos().get() / 2 / line.len()) * line.len();
    // The first edit brings the gap from the buffer end to the edit point;
    // that move is the edit's, so the count starts after it.
    park_gap_at(&mut buf, line_start + 10);
    let pattern = LispString::from_utf8("\\(?:^[^\n]*x\\)\\=");
    let context = BufferRegexpMatchContext::new(
        crate::emacs_core::syntax::SyntaxProperties::Ignore,
        crate::emacs_core::regex_emacs::WordBoundaryLookup::default(),
    );
    let syntax = buffer_syntax_lookup(&buf);
    let compiled = compile_lisp_pattern_with_posix_translation(
        &pattern,
        false,
        false,
        buf.get_multibyte(),
        None,
        &syntax,
    )
    .unwrap();
    let mut regs = SearchRegisters::default();
    let mut gap = gap_start(&buf);
    let mut travelled = 0;
    let mut note_gap = |buf: &Buffer| {
        let now = gap_start(buf);
        travelled += now.abs_diff(gap);
        gap = now;
    };
    for round in 0..100 {
        buf.insert("x");
        note_gap(&buf);
        let point = buf.point_emacs_byte_pos();
        let found = match round % 3 {
            0 => re_search_backward_lisp_with_posix_into(
                &mut buf,
                &pattern,
                Some(line_start),
                true,
                false,
                false,
                context,
                &mut regs,
            )
            .unwrap(),
            1 => re_search_backward_compiled_into(
                &mut buf,
                &compiled,
                Some(line_start),
                true,
                context,
                &mut regs,
            )
            .unwrap(),
            _ => re_search_backward(&mut buf, &pattern, Some(line_start), true, false)
                .unwrap()
                .map(|found| found.into_parts().1),
        };
        assert_eq!(
            found.map(|pos| pos.get()),
            Some(line_start),
            "round {round}"
        );
        note_gap(&buf);
        buf.goto_emacs_byte_pos(point);
    }
    assert!(
        travelled < 10_000,
        "100 backward searches after an edit moved the gap over {travelled} bytes"
    );
}

#[test]
fn re_search_backward_answers_the_same_with_the_gap_parked_at_point() {
    crate::test_utils::init_test_tracing();
    // Every assertion that reads past the match: `\`' (only at the region
    // start), `^', `\b' and `\B' at BOUND (the character before it), `$',
    // `\b', `\>' and `\=' at point (the character after it), `\''.  Each
    // case runs twice, once on a buffer that was never edited (gap at the
    // end) and once with an edit parking the gap at point.
    let text = "abc def\nghi jkl\nmno\n";
    let cases: &[(&str, usize, Option<usize>, Option<usize>)] = &[
        ("\\`", 12, None, Some(0)),
        ("\\`", 12, Some(8), None),
        ("\\`ghi", 12, Some(8), None),
        ("^g", 12, Some(8), Some(8)),
        ("^h", 12, Some(9), None),
        ("\\bh", 12, Some(9), None),
        ("\\Bh", 12, Some(9), Some(9)),
        ("f$", 7, Some(4), Some(6)),
        ("e$", 6, Some(4), None),
        ("def$", 7, None, Some(4)),
        ("c\\b", 3, Some(0), Some(2)),
        ("b\\b", 2, Some(0), None),
        ("abc\\>", 3, None, Some(0)),
        ("ab\\>", 2, None, None),
        ("i \\=", 12, None, Some(10)),
        ("\\=", 12, Some(8), Some(12)),
        ("l\\'", 15, None, None),
        ("o\n\\'", 20, Some(16), Some(18)),
        ("ghi jkl", 20, None, Some(8)),
        ("ghi jkl", 14, None, None),
    ];
    for &(pattern, point, bound, expected) in cases {
        let mut buf = backward_search_buffer(text);
        assert_eq!(
            backward_search_all_entries(&mut buf, pattern, point, bound),
            expected,
            "unedited: {pattern:?} from {point} bound {bound:?}"
        );
        park_gap_at(&mut buf, point);
        assert_eq!(
            backward_search_all_entries(&mut buf, pattern, point, bound),
            expected,
            "gap at point: {pattern:?} from {point} bound {bound:?}"
        );
    }

    // A match that begins before the gap and ends after it, BOUND nil.
    let mut buf = backward_search_buffer(text);
    park_gap_at(&mut buf, 10);
    assert_eq!(
        backward_search_all_entries(&mut buf, "ghi jkl", 20, None),
        Some(8)
    );

    // COUNT > 1 repeats the search from each match start.
    let mut buf = backward_search_buffer(text);
    park_gap_at(&mut buf, 20);
    let mut point = 20;
    let mut starts = Vec::new();
    for _ in 0..3 {
        point = backward_search_all_entries(&mut buf, "[a-z]+", point, None).unwrap();
        starts.push(point);
    }
    assert_eq!(starts, [18, 17, 16]);

    // Narrowed: `\`' and `\'' are the region's edges, wherever the gap is.
    let mut buf = backward_search_buffer(text);
    buf.narrow_to_emacs_byte_range(EmacsByteRange::new(byte_pos(4), byte_pos(15)));
    park_gap_at(&mut buf, 12);
    assert_eq!(
        backward_search_all_entries(&mut buf, "\\`d", 12, None),
        Some(4)
    );
    assert_eq!(
        backward_search_all_entries(&mut buf, "\\`", 12, Some(5)),
        None
    );
    assert_eq!(
        backward_search_all_entries(&mut buf, "^g", 12, Some(8)),
        Some(8)
    );
    park_gap_at(&mut buf, 15);
    assert_eq!(
        backward_search_all_entries(&mut buf, "l\\'", 15, None),
        Some(14)
    );
    assert_eq!(
        backward_search_all_entries(&mut buf, "\\`def", 15, None),
        Some(4)
    );
}

#[test]
fn re_search_backward_keeps_the_whole_multibyte_character_after_point() {
    crate::test_utils::init_test_tracing();
    // Two-byte characters: λ0 μ2 ν4 SPC6 ξ7 ο9 π11 NL13.  Word boundaries at
    // point read the whole of π, at BOUND the whole of ξ.
    let text = "λμν ξοπ\n";
    let cases: &[(&str, usize, Option<usize>, Option<usize>)] = &[
        ("ο\\b", 11, Some(7), None),
        ("ο\\B", 11, Some(7), Some(9)),
        ("ο\\>", 11, Some(7), None),
        ("π$", 13, Some(7), Some(11)),
        ("\\bο", 13, Some(9), None),
        ("\\Bο", 13, Some(9), Some(9)),
        ("\\bξ", 13, Some(7), Some(7)),
    ];
    for &(pattern, point, bound, expected) in cases {
        let mut buf = backward_search_buffer(text);
        assert_eq!(
            backward_search_all_entries(&mut buf, pattern, point, bound),
            expected,
            "unedited: {pattern:?} from {point} bound {bound:?}"
        );
        park_gap_at(&mut buf, point);
        assert_eq!(
            backward_search_all_entries(&mut buf, pattern, point, bound),
            expected,
            "gap at point: {pattern:?} from {point} bound {bound:?}"
        );
    }
}
