//! Oracle parity tests for `replace-match` with ALL parameter combinations.
//!
//! Covers: NEWTEXT (plain string), FIXEDCASE (nil vs t), LITERAL (nil vs t),
//! STRING (nil=buffer vs string), SUBEXP (sub-group replacement),
//! replace-match after string-match vs re-search-forward, case-preserving
//! replacement patterns, back-reference replacement (\1, \2, \&, \\), and
//! complex multi-pass replacement pipelines.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// 1. NEWTEXT: plain string replacement (all other params default/nil)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_newtext_plain_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Basic replace-match with only NEWTEXT provided, exercising string context
    // and buffer context, empty replacement, and replacement longer/shorter
    // than matched text.
    let form = r#"(let ((results nil))
      ;; String context: simple replacement
      (let ((s "hello world"))
        (string-match "world" s)
        (push (replace-match "earth" t t s) results))
      ;; String context: replace with empty string
      (let ((s "remove-this-part"))
        (string-match "-this-part" s)
        (push (replace-match "" t t s) results))
      ;; String context: replacement longer than match
      (let ((s "ab"))
        (string-match "ab" s)
        (push (replace-match "ABCDEF" t t s) results))
      ;; String context: replacement same length
      (let ((s "foo"))
        (string-match "foo" s)
        (push (replace-match "bar" t t s) results))
      ;; Buffer context: simple replacement
      (push
       (with-temp-buffer
         (insert "hello world")
         (goto-char (point-min))
         (re-search-forward "world" nil t)
         (replace-match "earth" t t)
         (buffer-string))
       results)
      ;; Buffer context: replace with empty string
      (push
       (with-temp-buffer
         (insert "abc-def")
         (goto-char (point-min))
         (re-search-forward "-def" nil t)
         (replace-match "" t t)
         (buffer-string))
       results)
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello earth\" \"remove\" \"ABCDEF\" \"bar\" \"hello earth\" \"abc\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 2. FIXEDCASE nil vs t with various case patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_fixedcase_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // FIXEDCASE=nil: Emacs tries to adapt replacement case to match.
    // FIXEDCASE=t: replacement used exactly as-is.
    // Test with all-upper, all-lower, capitalized, and mixed-case matches.
    let form = r#"(let ((results nil))
      ;; ALL UPPER match, fixedcase=nil -> replacement uppercased
      (let ((s "HELLO world"))
        (string-match "HELLO" s)
        (push (replace-match "greetings" nil nil s) results))
      ;; ALL UPPER match, fixedcase=t -> replacement as-is
      (let ((s "HELLO world"))
        (string-match "HELLO" s)
        (push (replace-match "greetings" t nil s) results))
      ;; Capitalized match, fixedcase=nil -> replacement capitalized
      (let ((s "Hello world"))
        (string-match "Hello" s)
        (push (replace-match "greetings" nil nil s) results))
      ;; Capitalized match, fixedcase=t -> replacement as-is
      (let ((s "Hello world"))
        (string-match "Hello" s)
        (push (replace-match "greetings" t nil s) results))
      ;; Lower match, fixedcase=nil -> replacement stays lower
      (let ((s "hello world"))
        (string-match "hello" s)
        (push (replace-match "greetings" nil nil s) results))
      ;; Lower match, fixedcase=t -> replacement as-is
      (let ((s "hello world"))
        (string-match "hello" s)
        (push (replace-match "GREETINGS" t nil s) results))
      ;; Mixed case match, fixedcase=nil -> replacement as-is (mixed = no clear pattern)
      (let ((s "hElLo world"))
        (string-match "hElLo" s)
        (push (replace-match "greetings" nil nil s) results))
      ;; Single char upper, fixedcase=nil
      (let ((s "A quick fox"))
        (string-match "A" s)
        (push (replace-match "the" nil nil s) results))
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"GREETINGS world\" \"greetings world\" \"Greetings world\" \"greetings world\" \"greetings world\" \"GREETINGS world\" \"greetings world\" \"THE quick fox\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 3. LITERAL nil vs t with back-references and special characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_literal_exhaustive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LITERAL=nil: \& -> whole match, \1..\9 -> groups, \\ -> literal backslash
    // LITERAL=t: backslashes not special at all
    let form = r#"(let ((s "foo-bar baz"))
      (string-match "\\([a-z]+\\)-\\([a-z]+\\)" s)
      (list
        ;; LITERAL=nil: \& expands to whole match "foo-bar"
        (replace-match "[\\&]" t nil s)
        ;; LITERAL=nil: \1 and \2 expand to groups
        (replace-match "\\2_\\1" t nil s)
        ;; LITERAL=nil: \\ produces literal backslash
        (replace-match "\\1\\\\\\2" t nil s)
        ;; LITERAL=nil: \& with surrounding text
        (replace-match "<<\\&>>" t nil s)
        ;; LITERAL=t: \& kept literally
        (replace-match "\\&" t t s)
        ;; LITERAL=t: \1 kept literally
        (replace-match "\\1-\\2" t t s)
        ;; LITERAL=t: \\ kept literally
        (replace-match "a\\\\b" t t s)
        ;; LITERAL=t: plain string (no backslashes)
        (replace-match "replacement" t t s)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[foo-bar] baz\" \"bar_foo baz\" \"foo\\\\bar baz\" \"<<foo-bar>> baz\" \"\\\\& baz\" \"\\\\1-\\\\2 baz\" \"a\\\\\\\\b baz\" \"replacement baz\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 4. STRING parameter: nil (buffer) vs string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_string_param_buffer_vs_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When STRING=nil, replace-match modifies the current buffer.
    // When STRING is provided, it returns a new string.
    let form = r#"(let ((results nil))
      ;; STRING parameter = a string: returns new string, original unchanged
      (let* ((s "alpha beta gamma")
             (ignored (string-match "beta" s))
             (new-s (replace-match "BETA" t t s)))
        (push (list s new-s (string= s "alpha beta gamma")) results))
      ;; STRING parameter = nil: modifies buffer in place
      (push
       (with-temp-buffer
         (insert "alpha beta gamma")
         (goto-char (point-min))
         (re-search-forward "beta" nil t)
         (replace-match "BETA" t t)
         (buffer-string))
       results)
      ;; String context with groups
      (let ((s "key=val"))
        (string-match "\\([a-z]+\\)=\\([a-z]+\\)" s)
        (push (replace-match "\\2->\\1" t nil s) results))
      ;; Buffer context with groups
      (push
       (with-temp-buffer
         (insert "key=val")
         (goto-char (point-min))
         (re-search-forward "\\([a-z]+\\)=\\([a-z]+\\)" nil t)
         (replace-match "\\2->\\1" t nil)
         (buffer-string))
       results)
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"alpha beta gamma\" \"alpha BETA gamma\" t) \"alpha BETA gamma\" \"val->key\" \"val->key\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 5. SUBEXP parameter: replace specific sub-group
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_subexp_all_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test SUBEXP with 0 (whole match), 1, 2, 3 for a 3-group regex.
    // Also test that replacing a subexp preserves the rest of the match.
    let form = r#"(let ((results nil))
      ;; 3-group regex on a string
      (let ((s "aaa-bbb-ccc"))
        (string-match "\\([a-z]+\\)-\\([a-z]+\\)-\\([a-z]+\\)" s)
        ;; Replace group 0 (entire match)
        (push (replace-match "WHOLE" t t s 0) results)
        ;; Replace group 1 only
        (push (replace-match "XXX" t t s 1) results)
        ;; Replace group 2 only
        (push (replace-match "YYY" t t s 2) results)
        ;; Replace group 3 only
        (push (replace-match "ZZZ" t t s 3) results))
      ;; Nested groups: (\(a\(b\)c\))
      (let ((s "xabcy"))
        (string-match "\\(a\\(b\\)c\\)" s)
        ;; Replace outer group 1
        (push (replace-match "OUTER" t t s 1) results)
        ;; Replace inner group 2
        (push (replace-match "INNER" t t s 2) results))
      ;; SUBEXP in buffer context
      (push
       (with-temp-buffer
         (insert "start-middle-end")
         (goto-char (point-min))
         (re-search-forward "\\(start\\)-\\(middle\\)-\\(end\\)" nil t)
         (replace-match "CENTER" t t nil 2)
         (buffer-string))
       results)
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"WHOLE\" \"XXX-bbb-ccc\" \"aaa-YYY-ccc\" \"aaa-bbb-ZZZ\" \"xOUTERy\" \"xaINNERcy\" \"start-CENTER-end\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 6. replace-match after string-match vs re-search-forward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_string_match_vs_re_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that replace-match works identically after string-match
    // (string context) and re-search-forward (buffer context), producing
    // equivalent results for the same input and pattern.
    let form = r#"(let ((text "price: $42.99")
                        (pattern "\\$\\([0-9]+\\)\\.\\([0-9]+\\)"))
      ;; String context via string-match
      (string-match pattern text)
      (let ((str-result (replace-match "EUR \\1,\\2" t nil text))
            (str-g0 (match-string 0 text))
            (str-g1 (match-string 1 text))
            (str-g2 (match-string 2 text)))
        ;; Buffer context via re-search-forward
        (let ((buf-results
               (with-temp-buffer
                 (insert text)
                 (goto-char (point-min))
                 (re-search-forward pattern nil t)
                 (let ((g0 (match-string 0))
                       (g1 (match-string 1))
                       (g2 (match-string 2)))
                   (replace-match "EUR \\1,\\2" t nil)
                   (list (buffer-string) g0 g1 g2)))))
          (list str-result str-g0 str-g1 str-g2
                (car buf-results)
                (nth 1 buf-results) (nth 2 buf-results) (nth 3 buf-results)
                ;; Groups should match (accounting for 0-based vs 1-based positions)
                (string= str-g0 (nth 1 buf-results))
                (string= str-g1 (nth 2 buf-results))
                (string= str-g2 (nth 3 buf-results))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"price: EUR 42,99\" \"$42.99\" \"42\" \"99\" \"price: EUR 42,99\" \"$42.99\" \"42\" \"99\" t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 7. Complex: case-preserving replacement with all FIXEDCASE/LITERAL combos
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_fixedcase_literal_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test all 4 combinations of FIXEDCASE x LITERAL for the same match.
    // FIXEDCASE=nil,LITERAL=nil: case-adapt + backslash expansion
    // FIXEDCASE=nil,LITERAL=t: case-adapt + literal backslash
    // FIXEDCASE=t,LITERAL=nil: exact case + backslash expansion
    // FIXEDCASE=t,LITERAL=t: exact case + literal backslash
    let form = r#"(let ((results nil))
      ;; Uppercase match with groups
      (let ((s "FOO-BAR"))
        (string-match "\\([A-Z]+\\)-\\([A-Z]+\\)" s)
        ;; fixedcase=nil, literal=nil
        (push (replace-match "\\2-\\1" nil nil s) results)
        ;; fixedcase=nil, literal=t
        (push (replace-match "\\2-\\1" nil t s) results)
        ;; fixedcase=t, literal=nil
        (push (replace-match "\\2-\\1" t nil s) results)
        ;; fixedcase=t, literal=t
        (push (replace-match "\\2-\\1" t t s) results))
      ;; Capitalized match with groups
      (let ((s "Hello-World"))
        (string-match "\\([A-Za-z]+\\)-\\([A-Za-z]+\\)" s)
        ;; fixedcase=nil, literal=nil
        (push (replace-match "new-\\2" nil nil s) results)
        ;; fixedcase=nil, literal=t
        (push (replace-match "new-\\2" nil t s) results)
        ;; fixedcase=t, literal=nil
        (push (replace-match "new-\\2" t nil s) results)
        ;; fixedcase=t, literal=t
        (push (replace-match "new-\\2" t t s) results))
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"BAR-FOO\" \"\\\\2-\\\\1\" \"BAR-FOO\" \"\\\\2-\\\\1\" \"New-World\" \"New-\\\\2\" \"new-World\" \"new-\\\\2\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 8. Complex: back-reference replacement (\1, \2, \&, \\) in multi-group regex
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_backrefs_complex_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-step pipeline: parse structured data, rearrange with backrefs,
    // and verify sequential replacements.
    let form = r#"(unwind-protect
      (progn
        ;; Function to reformat "YYYY-MM-DD" -> "DD/MM/YYYY"
        (defun test--reformat-date (s)
          (if (string-match "\\([0-9]\\{4\\}\\)-\\([0-9]\\{2\\}\\)-\\([0-9]\\{2\\}\\)" s)
              (replace-match "\\3/\\2/\\1" t nil s)
            s))

        ;; Function to swap "last, first" -> "first last"
        (defun test--swap-names (s)
          (if (string-match "\\([A-Za-z]+\\), \\([A-Za-z]+\\)" s)
              (replace-match "\\2 \\1" t nil s)
            s))

        ;; Function to wrap matched groups
        (defun test--wrap-groups (s)
          (if (string-match "\\([a-z]+\\):\\([0-9]+\\)" s)
              (replace-match "[\\1]=<\\2>" t nil s)
            s))

        (list
          ;; Date reformatting
          (test--reformat-date "2026-03-02")
          (test--reformat-date "1999-12-31")
          (test--reformat-date "no-date-here")
          ;; Name swapping
          (test--swap-names "Doe, John")
          (test--swap-names "Smith, Alice")
          ;; Group wrapping
          (test--wrap-groups "count:42 extra")
          (test--wrap-groups "level:100")
          ;; \& whole match reference
          (let ((s "hello"))
            (string-match "hello" s)
            (replace-match "<<\\&>>" t nil s))
          ;; \\ literal backslash
          (let ((s "path"))
            (string-match "path" s)
            (replace-match "C:\\\\Users\\\\\\&" t nil s))
          ;; Multiple groups duplicated
          (let ((s "ab"))
            (string-match "\\(a\\)\\(b\\)" s)
            (replace-match "\\1\\1\\2\\2\\1\\2" t nil s))))
      ;; Cleanup
      (fmakunbound 'test--reformat-date)
      (fmakunbound 'test--swap-names)
      (fmakunbound 'test--wrap-groups))"#;
    let expect = expect_test::expect![[
        r#""OK (\"02/03/2026\" \"31/12/1999\" \"no-date-here\" \"John Doe\" \"Alice Smith\" \"[count]=<42> extra\" \"[level]=<100>\" \"<<hello>>\" \"C:\\\\Users\\\\path\" \"aabbab\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 9. replace-match with iterative replacement in buffer (while loop)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_iterative_buffer_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Replace all occurrences in a buffer iteratively, with different
    // replacement lengths, tracking position and count.
    let form = r#"(with-temp-buffer
      (insert "The cat sat on the mat. The cat ate the rat.")
      (let ((count 0))
        ;; Replace "cat" with "dog" (same length)
        (goto-char (point-min))
        (while (re-search-forward "\\bcat\\b" nil t)
          (replace-match "dog" t t)
          (setq count (1+ count)))
        (let ((after-cat (buffer-string))
              (cat-count count))
          ;; Replace "the" (case-insensitive) with "a" (shorter)
          (setq count 0)
          (let ((case-fold-search t))
            (goto-char (point-min))
            (while (re-search-forward "\\bthe\\b" nil t)
              (replace-match "a" t t)
              (setq count (1+ count))))
          (list cat-count after-cat
                count (buffer-string)
                (buffer-size)))))"#;
    let expect = expect_test::expect![[
        r#""OK (2 \"The dog sat on the mat. The dog ate the rat.\" 4 \"a dog sat on a mat. a dog ate a rat.\" 36)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// 10. Complex: multi-pass search-and-replace with position tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_multipass_with_backref_and_subexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Chain multiple replacement strategies: first do SUBEXP replacement,
    // then do backref replacement, verifying intermediate states.
    let form = r#"(let ((results nil))
      ;; Step 1: Use SUBEXP to replace just the value in "key=value" pairs
      (let ((s "host=localhost port=8080 db=mydb"))
        (string-match "\\(host\\)=\\([a-z]+\\)" s)
        (let ((s1 (replace-match "example.com" t t s 2)))
          (push s1 results)
          ;; Step 2: Use backrefs to reformat remaining pair
          (string-match "\\(port\\)=\\([0-9]+\\)" s1)
          (let ((s2 (replace-match "\\1: \\2" t nil s1)))
            (push s2 results)
            ;; Step 3: Replace with empty subexp
            (string-match "\\(db\\)=\\([a-z]+\\)" s2)
            (push (replace-match "" t t s2 2) results))))
      ;; Step 4: Buffer context SUBEXP replacement
      (push
       (with-temp-buffer
         (insert "color:red size:large")
         (goto-char (point-min))
         (re-search-forward "\\(color\\):\\([a-z]+\\)" nil t)
         (replace-match "blue" t t nil 2)
         (buffer-string))
       results)
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"host=example.com port=8080 db=mydb\" \"host=example.com port: 8080 db=mydb\" \"host=example.com port: 8080 db=\" \"color:blue size:large\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
