//! Advanced oracle parity tests for match-data operations.
//!
//! Covers: match-data/set-match-data roundtrip, save-match-data preservation,
//! match-beginning/match-end with subgroups, match-string extraction,
//! match-data after string-match vs buffer re-search-forward, sequential
//! matches, capture group pipelines, and template substitution.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// match-data / set-match-data roundtrip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_data_set_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set match-data manually, then retrieve it and verify roundtrip.
    let form = r#"(progn
                    (set-match-data '(10 20 12 15 16 19))
                    (let ((md (match-data)))
                      (list
                        (match-beginning 0) (match-end 0)
                        (match-beginning 1) (match-end 1)
                        (match-beginning 2) (match-end 2)
                        (equal md '(10 20 12 15 16 19)))))"#;
    let expect = expect_test::expect![[r#""OK (10 20 12 15 16 19 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_match_data_set_then_search_overwrites() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Manually set match-data, then do a string-match. The search
    // should completely overwrite the manually set data.
    let form = r#"(progn
                    (set-match-data '(100 200 110 120))
                    (string-match "\\(abc\\)" "xyzabcdef")
                    (list (match-beginning 0) (match-end 0)
                          (match-beginning 1) (match-end 1)))"#;
    let expect = expect_test::expect![[r#""OK (3 6 3 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-match-data preserving across searches
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_match_data_nested_searches() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Outer search sets match-data. Inner save-match-data does another
    // search. After inner block, outer match-data is restored.
    let form = r#"(progn
                    (string-match "\\(hello\\) \\(world\\)" "hello world")
                    (let ((outer-0-beg (match-beginning 0))
                          (outer-1-str (match-string 1 "hello world"))
                          (outer-2-str (match-string 2 "hello world")))
                      (save-match-data
                        (string-match "\\(foo\\)\\(bar\\)" "foobar"))
                      ;; After save-match-data, original data should be restored
                      (list outer-0-beg outer-1-str outer-2-str
                            (match-beginning 0)
                            (match-string 1 "hello world")
                            (match-string 2 "hello world"))))"#;
    let expect = expect_test::expect![[r#""OK (0 \"hello\" \"world\" 0 \"hello\" \"world\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_save_match_data_deeply_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of save-match-data, each with a different search.
    let form = r#"(progn
                    (string-match "\\(aaa\\)" "aaa")
                    (let ((level0 (match-string 1 "aaa")))
                      (save-match-data
                        (string-match "\\(bbb\\)" "bbb")
                        (let ((level1 (match-string 1 "bbb")))
                          (save-match-data
                            (string-match "\\(ccc\\)" "ccc")
                            nil)
                          ;; After innermost save-match-data, level1 data restored
                          (setq level1 (list level1 (match-string 1 "bbb")))))
                      ;; After outer save-match-data, level0 data restored
                      (list level0 (match-string 1 "aaa"))))"#;
    let expect = expect_test::expect![[r#""OK (\"aaa\" \"aaa\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// match-beginning / match-end with multiple subgroups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_beginning_end_multiple_subgroups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Regex with 4 capture groups, testing all positions.
    let form = r#"(progn
                    (string-match
                     "\\([0-9]+\\)-\\([a-z]+\\)-\\([A-Z]+\\)-\\([0-9]+\\)"
                     "123-abc-XYZ-789")
                    (list
                      (match-beginning 0) (match-end 0)
                      (match-beginning 1) (match-end 1)
                      (match-beginning 2) (match-end 2)
                      (match-beginning 3) (match-end 3)
                      (match-beginning 4) (match-end 4)))"#;
    let expect = expect_test::expect![[r#""OK (0 15 0 3 4 7 8 11 12 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_match_beginning_end_optional_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Optional group that does not participate in the match.
    // match-beginning/match-end should return nil for unmatched groups.
    let form = r#"(progn
                    (string-match "\\(foo\\)\\(-\\(bar\\)\\)?" "foo")
                    (list
                      (match-beginning 0) (match-end 0)
                      (match-beginning 1) (match-end 1)
                      (match-beginning 2) (match-end 2)
                      (match-beginning 3) (match-end 3)))"#;
    let expect = expect_test::expect![[r#""OK (0 3 0 3 nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// match-string with subgroup extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_all_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract multiple groups using match-string on a complex pattern.
    let form = r#"(let ((str "2026-03-02T15:30:45"))
                    (string-match
                     "\\([0-9]\\{4\\}\\)-\\([0-9]\\{2\\}\\)-\\([0-9]\\{2\\}\\)T\\([0-9]\\{2\\}\\):\\([0-9]\\{2\\}\\):\\([0-9]\\{2\\}\\)"
                     str)
                    (list
                      (match-string 0 str)
                      (match-string 1 str)
                      (match-string 2 str)
                      (match-string 3 str)
                      (match-string 4 str)
                      (match-string 5 str)
                      (match-string 6 str)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"2026-03-02T15:30:45\" \"2026\" \"03\" \"02\" \"15\" \"30\" \"45\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// match-data after string-match vs re-search-forward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_data_string_match_vs_buffer_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-match returns integer match-data positions.
    // re-search-forward in buffer also sets match-data with markers.
    // Both should agree on the logical positions.
    let form = r#"(let ((pattern "\\([a-z]+\\)-\\([0-9]+\\)")
                        (text "item-42 other-99"))
                    (string-match pattern text)
                    (let ((str-md (list (match-beginning 0) (match-end 0)
                                        (match-beginning 1) (match-end 1)
                                        (match-beginning 2) (match-end 2))))
                      (let ((buf-md
                             (with-temp-buffer
                               (insert text)
                               (goto-char (point-min))
                               (re-search-forward pattern nil t)
                               (list (match-beginning 0) (match-end 0)
                                     (match-beginning 1) (match-end 1)
                                     (match-beginning 2) (match-end 2)))))
                        ;; Buffer positions are 1-indexed, string positions 0-indexed.
                        ;; So buffer positions = string positions + 1.
                        (list str-md buf-md
                              (= (1+ (nth 0 str-md)) (nth 0 buf-md))
                              (= (1+ (nth 1 str-md)) (nth 1 buf-md))))))"#;
    let expect = expect_test::expect![[r#""OK ((0 7 0 4 5 7) (1 8 1 5 6 8) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple sequential matches affecting match-data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_data_sequential_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each string-match completely overwrites the previous match-data.
    let form = r#"(let ((results nil))
                    ;; First match: 3 groups
                    (string-match "\\(a\\)\\(b\\)\\(c\\)" "abcdef")
                    (setq results
                          (cons (list (match-string 1 "abcdef")
                                      (match-string 2 "abcdef")
                                      (match-string 3 "abcdef"))
                                results))
                    ;; Second match: only 1 group
                    (string-match "\\(xyz\\)" "xyz123")
                    (setq results
                          (cons (list (match-string 1 "xyz123")
                                      (match-string 2 "xyz123")
                                      (match-string 3 "xyz123"))
                                results))
                    ;; Third match: 2 groups
                    (string-match "\\([0-9]+\\)\\.\\([0-9]+\\)" "3.14")
                    (setq results
                          (cons (list (match-string 1 "3.14")
                                      (match-string 2 "3.14"))
                                results))
                    (nreverse results))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"a\" \"b\" \"c\") (\"xyz\" nil nil) (\"3\" \"14\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: capture group extraction pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_data_extraction_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse key=value pairs from a string using iterative string-match
    // with a start position, building an alist.
    let form = r#"(let ((str "name=Alice age=30 city=London country=UK")
                        (pattern "\\([a-z]+\\)=\\([^ ]+\\)")
                        (start 0)
                        (pairs nil))
                    (while (string-match pattern str start)
                      (setq pairs
                            (cons (cons (match-string 1 str)
                                        (match-string 2 str))
                                  pairs))
                      (setq start (match-end 0)))
                    (nreverse pairs))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"name\" . \"Alice\") (\"age\" . \"30\") (\"city\" . \"London\") (\"country\" . \"UK\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_match_data_collect_all_matches() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Collect all occurrences of a pattern in a buffer using
    // re-search-forward in a loop.
    let form = r#"(with-temp-buffer
                    (insert "Error: line 10\nWarning: line 25\nError: line 42\nInfo: line 50")
                    (goto-char (point-min))
                    (let ((errors nil))
                      (while (re-search-forward
                              "Error: line \\([0-9]+\\)" nil t)
                        (setq errors
                              (cons (string-to-number (match-string 1))
                                    errors)))
                      (nreverse errors)))"#;
    let expect = expect_test::expect![[r#""OK (10 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: match-data-based template substitution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_data_template_substitution() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use match-data to perform template-like substitution:
    // reformat "LAST, FIRST" -> "FIRST LAST" using captures.
    let form = r#"(let ((names '("Smith, John" "Doe, Jane" "Park, Alice"))
                        (pattern "\\([A-Za-z]+\\), \\([A-Za-z]+\\)")
                        (results nil))
                    (dolist (name names)
                      (when (string-match pattern name)
                        (let ((last-name (match-string 1 name))
                              (first-name (match-string 2 name)))
                          (setq results
                                (cons (concat first-name " " last-name)
                                      results)))))
                    (nreverse results))"#;
    let expect = expect_test::expect![[r#""OK (\"John Smith\" \"Jane Doe\" \"Alice Park\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_match_data_replace_with_captures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Iteratively replace patterns in a buffer using match-data,
    // transforming "word123" into "WORD(123)".
    let form = r#"(with-temp-buffer
                    (insert "foo42 bar7 baz999")
                    (goto-char (point-min))
                    (while (re-search-forward "\\([a-z]+\\)\\([0-9]+\\)" nil t)
                      (let ((word (upcase (match-string 1)))
                            (num (match-string 2)))
                        (replace-match (concat word "(" num ")") t t)))
                    (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"FOO(42) BAR(7) BAZ(999)\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_match_data_with_replace_match_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use replace-match with subexpression replacement.
    // Swap two capture groups in place.
    let form = r#"(with-temp-buffer
                    (insert "hello-world")
                    (goto-char (point-min))
                    (when (re-search-forward "\\([a-z]+\\)-\\([a-z]+\\)" nil t)
                      (let ((first (match-string 1))
                            (second (match-string 2)))
                        (replace-match (concat second "-" first) t t)))
                    (buffer-string))"#;
    let expect = expect_test::expect![[r#""OK \"world-hello\"""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
