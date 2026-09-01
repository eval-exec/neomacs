//! Oracle parity tests for `set-match-data`, `match-data`, `match-beginning`,
//! `match-end`, and `match-string` with complex patterns.
//!
//! Covers: set-match-data with explicit position lists, match-data return
//! values, match-beginning/match-end for group 0 and sub-groups,
//! match-string extraction, save-match-data interaction, multi-group regex
//! patterns, and match data manipulation for custom replace functions.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1. set-match-data with explicit position lists and retrieval
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_set_match_data_explicit_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // set-match-data takes a list of (beg end beg end ...) for each group.
    // Test with varying numbers of groups, then retrieve via match-beginning,
    // match-end, and match-data.
    let form = r#"(let ((results nil))
      ;; 1 group (group 0 only)
      (set-match-data '(5 10))
      (push (list (match-beginning 0) (match-end 0)
                  (match-beginning 1) (match-end 1))
            results)

      ;; 3 groups
      (set-match-data '(0 20 0 5 6 12 13 20))
      (push (list (match-beginning 0) (match-end 0)
                  (match-beginning 1) (match-end 1)
                  (match-beginning 2) (match-end 2)
                  (match-beginning 3) (match-end 3))
            results)

      ;; Empty match data (clears)
      (set-match-data nil)
      (push (match-data) results)

      ;; Odd-length list: last element ignored/nil-paired
      (set-match-data '(1 2 3))
      (push (list (match-beginning 0) (match-end 0)
                  (match-beginning 1) (match-end 1))
            results)

      ;; Large positions
      (set-match-data '(1000 2000 1000 1500 1500 2000))
      (push (list (match-beginning 0) (match-end 0)
                  (match-beginning 1) (match-end 1)
                  (match-beginning 2) (match-end 2))
            results)

      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 10 nil nil) (0 20 0 5 6 12 13 20) nil (1 2 nil nil) (1000 2000 1000 1500 1500 2000))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2. match-data returns correct list after successful and failed matches
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_data_after_success_and_failure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify match-data returns a list of integers after string-match,
    // and that a failed match clears match data.
    let form = r#"(let ((results nil))
      ;; Successful match with groups
      (string-match "\\(foo\\)\\(bar\\)" "foobar")
      (push (match-data) results)

      ;; Successful match, different groups
      (string-match "\\([0-9]+\\)\\.\\([0-9]+\\)\\.\\([0-9]+\\)" "1.2.3")
      (push (match-data) results)

      ;; Match at offset
      (string-match "\\(xyz\\)" "abcxyzdef" 3)
      (push (match-data) results)

      ;; After successful match, match-data has the right type
      (string-match "\\(a\\)" "a")
      (push (listp (match-data)) results)

      ;; match-data after set-match-data
      (set-match-data '(10 20 12 18))
      (push (match-data) results)

      ;; Roundtrip: set then get
      (let ((orig '(0 10 2 4 6 8)))
        (set-match-data orig)
        (push (equal (match-data) orig) results))

      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 6 0 3 3 6) (0 5 0 1 2 3 4 5) (3 6 3 6) t (10 20 12 18) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 3. match-beginning/match-end for group 0 and sub-groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_beginning_end_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test match-beginning and match-end with many groups, optional groups,
    // and unmatched groups returning nil.
    let form = r#"(let ((results nil))
      ;; 5-group regex
      (string-match
        "\\([a-z]\\)\\([a-z]\\)\\([a-z]\\)\\([a-z]\\)\\([a-z]\\)"
        "abcde")
      (push (list
              (match-beginning 0) (match-end 0)
              (match-beginning 1) (match-end 1)
              (match-beginning 2) (match-end 2)
              (match-beginning 3) (match-end 3)
              (match-beginning 4) (match-end 4)
              (match-beginning 5) (match-end 5))
            results)

      ;; Optional group that participates
      (string-match "\\(foo\\)\\(-\\(bar\\)\\)?" "foo-bar")
      (push (list
              (match-beginning 0) (match-end 0)
              (match-beginning 1) (match-end 1)
              (match-beginning 2) (match-end 2)
              (match-beginning 3) (match-end 3))
            results)

      ;; Optional group that does NOT participate -> nil
      (string-match "\\(foo\\)\\(-\\(bar\\)\\)?" "foo")
      (push (list
              (match-beginning 0) (match-end 0)
              (match-beginning 1) (match-end 1)
              (match-beginning 2) (match-end 2)
              (match-beginning 3) (match-end 3))
            results)

      ;; Alternation: one branch matches, other groups are nil
      (string-match "\\(alpha\\)\\|\\(beta\\)" "beta")
      (push (list
              (match-beginning 0) (match-end 0)
              (match-beginning 1) (match-end 1)
              (match-beginning 2) (match-end 2))
            results)

      ;; Match not at start of string
      (string-match "\\(world\\)" "hello world")
      (push (list (match-beginning 0) (match-end 0)
                  (match-beginning 1) (match-end 1))
            results)

      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 5 0 1 1 2 2 3 3 4 4 5) (0 7 0 3 3 7 4 7) (0 3 0 3 nil nil nil nil) (0 4 nil nil 0 4) (6 11 6 11))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4. match-string for group 0 and sub-groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_string_all_groups_and_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test match-string with group 0 and sub-groups, unmatched groups
    // returning nil, and match-string in both string and buffer contexts.
    let form = r#"(let ((results nil))
      ;; String context: multiple groups
      (let ((s "John Doe, age 30"))
        (string-match "\\([A-Za-z]+\\) \\([A-Za-z]+\\), age \\([0-9]+\\)" s)
        (push (list (match-string 0 s)
                    (match-string 1 s)
                    (match-string 2 s)
                    (match-string 3 s))
              results))

      ;; Unmatched optional group returns nil
      (let ((s "hello"))
        (string-match "\\(hello\\)\\( world\\)?" s)
        (push (list (match-string 0 s)
                    (match-string 1 s)
                    (match-string 2 s))
              results))

      ;; Buffer context
      (push
       (with-temp-buffer
         (insert "error: line 42")
         (goto-char (point-min))
         (re-search-forward "\\(error\\): line \\([0-9]+\\)" nil t)
         (list (match-string 0) (match-string 1) (match-string 2)))
       results)

      ;; match-string with manually set match-data and a known string
      (let ((s "abcdefghij"))
        (set-match-data '(0 10 0 3 3 6 6 10))
        (push (list (match-string 0 s)
                    (match-string 1 s)
                    (match-string 2 s)
                    (match-string 3 s))
              results))

      ;; Empty match
      (let ((s "aab"))
        (string-match "\\(a*\\)" s)
        (push (list (match-string 0 s)
                    (match-string 1 s)
                    (match-beginning 0) (match-end 0))
              results))

      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"John Doe, age 30\" \"John\" \"Doe\" \"30\") (\"hello\" \"hello\" nil) (\"error: line 42\" \"error\" \"42\") (\"abcdefghij\" \"abc\" \"def\" \"ghij\") (\"aa\" \"aa\" 0 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 5. save-match-data interaction with set-match-data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_match_data_with_set_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-match-data should preserve and restore match data across
    // inner set-match-data and string-match operations.
    let form = r#"(progn
      ;; Establish initial match data
      (string-match "\\(alpha\\)-\\(beta\\)" "alpha-beta")
      (let ((outer-g1-beg (match-beginning 1))
            (outer-g2-end (match-end 2))
            (outer-md (match-data)))
        ;; Inner save-match-data: override with set-match-data
        (save-match-data
          (set-match-data '(100 200 100 150 150 200))
          (let ((inner-beg (match-beginning 0))
                (inner-end (match-end 0)))
            ;; Verify inner data is active
            (list inner-beg inner-end)))
        ;; After save-match-data: original should be restored
        (let ((restored-g1-beg (match-beginning 1))
              (restored-g2-end (match-end 2))
              (restored-md (match-data)))
          ;; Nested save-match-data with string-match
          (save-match-data
            (string-match "\\(xyz\\)" "xyz")
            nil)
          ;; Still restored after second save-match-data
          (let ((still-g1-beg (match-beginning 1)))
            (list outer-g1-beg outer-g2-end
                  restored-g1-beg restored-g2-end
                  still-g1-beg
                  (equal outer-md restored-md)))))"#;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 6. Complex: multi-group regex with nested groups and alternation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_data_multi_group_nested_alternation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Complex regex patterns: nested groups, alternation affecting which
    // groups participate, and iterative matching collecting all groups.
    let form = r#"(let ((results nil))
      ;; Nested groups: (\(outer \(inner\)\))
      (let ((s "outer inner end"))
        (string-match "\\(outer \\(inner\\)\\)" s)
        (push (list (match-string 0 s)
                    (match-string 1 s)
                    (match-string 2 s)
                    (match-beginning 1) (match-end 1)
                    (match-beginning 2) (match-end 2))
              results))

      ;; Alternation with groups: only matched branch has non-nil groups
      (let ((s1 "cat") (s2 "dog"))
        (string-match "\\(cat\\)\\|\\(dog\\)\\|\\(bird\\)" s1)
        (push (list (match-string 1 s1)
                    (match-string 2 s1)
                    (match-string 3 s1))
              results)
        (string-match "\\(cat\\)\\|\\(dog\\)\\|\\(bird\\)" s2)
        (push (list (match-string 1 s2)
                    (match-string 2 s2)
                    (match-string 3 s2))
              results))

      ;; Iterative matching collecting all group values
      (let ((s "a1b2c3d4")
            (collected nil)
            (start 0))
        (while (string-match "\\([a-z]\\)\\([0-9]\\)" s start)
          (push (list (match-string 1 s) (match-string 2 s)) collected)
          (setq start (match-end 0)))
        (push (nreverse collected) results))

      ;; Zero-length match at boundary
      (let ((s "abc"))
        (string-match "\\(\\)" s)
        (push (list (match-beginning 0) (match-end 0)
                    (match-beginning 1) (match-end 1))
              results))

      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"outer inner\" \"outer inner\" \"inner\" 0 11 6 11) (\"cat\" nil nil) (nil \"dog\" nil) ((\"a\" \"1\") (\"b\" \"2\") (\"c\" \"3\") (\"d\" \"4\")) (0 0 0 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 7. Complex: match data manipulation for custom replace function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_data_custom_replace_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a custom replace function that uses match-data to build
    // transformed replacement strings based on captured group content.
    let form = r#"(unwind-protect
      (progn
        ;; Custom replace: for each "word:number", multiply the number by 2
        ;; and uppercase the word.
        (defun test--transform-replace (str pattern transform-fn)
          "Replace all matches of PATTERN in STR using TRANSFORM-FN.
           TRANSFORM-FN receives the match-data and STR, returns replacement."
          (let ((result str)
                (start 0)
                (replacements nil))
            ;; First pass: collect all matches
            (while (string-match pattern result start)
              (let ((md (match-data)))
                (push (list md (funcall transform-fn md result)) replacements)
                (setq start (match-end 0))))
            ;; Second pass: apply replacements in reverse order
            (dolist (r (nreverse replacements))
              (let ((md (car r))
                    (replacement (cadr r)))
                (set-match-data md)
                (setq result (replace-match replacement t t result))))
            result))

        ;; Transform function: uppercase word, double number
        (defun test--word-num-transform (md str)
          (set-match-data md)
          (let ((word (upcase (match-string 1 str)))
                (num (* 2 (string-to-number (match-string 2 str)))))
            (format "%s:%d" word num)))

        (list
          ;; Basic transformation
          (test--transform-replace
            "foo:10 bar:20 baz:30"
            "\\([a-z]+\\):\\([0-9]+\\)"
            'test--word-num-transform)
          ;; Single match
          (test--transform-replace
            "item:5"
            "\\([a-z]+\\):\\([0-9]+\\)"
            'test--word-num-transform)
          ;; No matches
          (test--transform-replace
            "no matches here"
            "\\([a-z]+\\):\\([0-9]+\\)"
            'test--word-num-transform)
          ;; Verify match-data is correct after operations
          (progn
            (string-match "\\(test\\)" "test")
            (list (match-beginning 0) (match-end 0)
                  (match-string 1 "test")))))
      ;; Cleanup
      (fmakunbound 'test--transform-replace)
      (fmakunbound 'test--word-num-transform))"#;
    let expect = expect_test::expect![[
        r#""OK (\"FOO:20 BAR:40 BAZ:60\" \"ITEM:10\" \"no matches here\" (0 4 \"test\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 8. match-data with buffer markers vs string integers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_match_data_markers_vs_integers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-match sets integer positions in match-data.
    // re-search-forward sets marker objects. Both should yield the same
    // logical positions when compared, and match-beginning/match-end
    // always return integers regardless.
    let form = r#"(let ((text "hello world foo")
                        (pattern "\\([a-z]+\\) \\([a-z]+\\)"))
      ;; String context
      (string-match pattern text)
      (let ((str-md (match-data))
            (str-b0 (match-beginning 0))
            (str-e0 (match-end 0))
            (str-b1 (match-beginning 1))
            (str-e1 (match-end 1))
            (str-b2 (match-beginning 2))
            (str-e2 (match-end 2)))
        ;; Buffer context
        (with-temp-buffer
          (insert text)
          (goto-char (point-min))
          (re-search-forward pattern nil t)
          (let ((buf-b0 (match-beginning 0))
                (buf-e0 (match-end 0))
                (buf-b1 (match-beginning 1))
                (buf-e1 (match-end 1))
                (buf-b2 (match-beginning 2))
                (buf-e2 (match-end 2)))
            (list
              ;; String positions (0-indexed)
              str-b0 str-e0 str-b1 str-e1 str-b2 str-e2
              ;; Buffer positions (1-indexed)
              buf-b0 buf-e0 buf-b1 buf-e1 buf-b2 buf-e2
              ;; Buffer = string + 1
              (= buf-b0 (1+ str-b0))
              (= buf-e0 (1+ str-e0))
              ;; match-beginning always returns integer
              (integerp str-b0)
              (integerp buf-b0))))))"#;
    let expect = expect_test::expect![[r#""OK (0 11 0 5 6 11 1 12 1 6 7 12 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
