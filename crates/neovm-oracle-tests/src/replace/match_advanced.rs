//! Advanced oracle parity tests for `replace-match`.
//!
//! Covers: FIXEDCASE parameter, LITERAL parameter (backslash handling),
//! SUBEXP parameter (capture group replacement), replace-match after
//! re-search-forward and string-match, back-references (\1, \2) in
//! replacement, case-preserving replacement, and template engine pattern.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// replace-match: FIXEDCASE parameter (nil vs t)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_fixedcase_nil_adapts_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // With FIXEDCASE=nil, replace-match adapts the case of the replacement
    // to match the original text's case pattern.
    let form = r#"(let ((results nil))
      ;; Test with uppercase match -> replacement gets uppercased
      (let ((s "Hello WORLD foo"))
        (string-match "WORLD" s)
        (setq results (cons (replace-match "planet" nil nil s) results)))
      ;; Test with capitalized match -> replacement gets capitalized
      (let ((s "Hello World foo"))
        (string-match "World" s)
        (setq results (cons (replace-match "planet" nil nil s) results)))
      ;; Test with lowercase match -> replacement stays lowercase
      (let ((s "Hello world foo"))
        (string-match "world" s)
        (setq results (cons (replace-match "planet" nil nil s) results)))
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello PLANET foo\" \"Hello Planet foo\" \"Hello planet foo\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_replace_match_fixedcase_t_exact_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // With FIXEDCASE=t, replacement text is inserted exactly as given,
    // regardless of the case pattern in the matched text.
    let form = r#"(let ((results nil))
      (let ((s "Hello WORLD foo"))
        (string-match "WORLD" s)
        (setq results (cons (replace-match "pLaNeT" t nil s) results)))
      (let ((s "Hello World foo"))
        (string-match "World" s)
        (setq results (cons (replace-match "pLaNeT" t nil s) results)))
      (let ((s "all lowercase match"))
        (string-match "lowercase" s)
        (setq results (cons (replace-match "SCREAMING" t nil s) results)))
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello pLaNeT foo\" \"Hello pLaNeT foo\" \"all SCREAMING match\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-match: LITERAL parameter (nil vs t — backslash handling)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_literal_nil_expands_backslashes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LITERAL=nil: \& expands to whole match, \1 \2 expand to groups
    let form = r#"(let ((s "key=value pair=data"))
      (string-match "\\([a-z]+\\)=\\([a-z]+\\)" s)
      (list
        ;; \& = whole match
        (replace-match "[\\&]" t nil s)
        ;; \1 = first group, \2 = second group
        (replace-match "\\2->\\1" t nil s)
        ;; \\ = literal backslash in replacement
        (replace-match "\\1\\\\\\2" t nil s)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[key=value] pair=data\" \"value->key pair=data\" \"key\\\\value pair=data\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_replace_match_literal_t_no_backslash_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LITERAL=t: backslashes in replacement are NOT special,
    // \& and \1 are kept literally
    let form = r#"(let ((s "key=value pair=data"))
      (string-match "\\([a-z]+\\)=\\([a-z]+\\)" s)
      (list
        ;; With LITERAL=t, \& is literal text
        (replace-match "\\&" t t s)
        ;; With LITERAL=t, \1 is literal text
        (replace-match "\\1->\\2" t t s)
        ;; With LITERAL=t, backslash is literal
        (replace-match "a\\b\\c" t t s)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\\\\& pair=data\" \"\\\\1->\\\\2 pair=data\" \"a\\\\b\\\\c pair=data\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-match: SUBEXP parameter (replace specific capture group)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_subexp_replace_specific_group() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // SUBEXP parameter: replace only the specified capture group
    let form = r#"(let ((s "name:Alice age:30 city:Paris"))
      ;; Match a key:value pair with two groups
      (string-match "\\(name\\):\\([A-Za-z]+\\)" s)
      (list
        ;; Replace entire match (subexp=0 or nil)
        (replace-match "REPLACED" t nil s)
        ;; Replace only group 1 (the key)
        (replace-match "fullname" t t s 1)
        ;; Replace only group 2 (the value)
        (replace-match "Bob" t t s 2)
        ;; Verify match-string values
        (match-string 0 s)
        (match-string 1 s)
        (match-string 2 s)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"REPLACED age:30 city:Paris\" \"fullname:Alice age:30 city:Paris\" \"name:Bob age:30 city:Paris\" \"name:Alice\" \"name\" \"Alice\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-match: after re-search-forward (buffer context)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_after_re_search_forward_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use replace-match in a buffer after re-search-forward
    let form = r#"(with-temp-buffer
      (insert "The price is $42.99 and the tax is $5.00.")
      (goto-char (point-min))
      ;; Find first dollar amount
      (re-search-forward "\\$\\([0-9]+\\)\\.\\([0-9]+\\)" nil t)
      (let ((whole-match (match-string 0))
            (dollars (match-string 1))
            (cents (match-string 2)))
        ;; Replace entire match in buffer
        (replace-match "EUR \\1,\\2" nil nil nil)
        (let ((after-first (buffer-string)))
          ;; Find and replace second dollar amount
          (re-search-forward "\\$\\([0-9]+\\)\\.\\([0-9]+\\)" nil t)
          (replace-match "EUR \\1,\\2" nil nil nil)
          (list whole-match dollars cents
                after-first
                (buffer-string)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"$42.99\" \"42\" \"99\" \"The price is EUR 42,99 and the tax is $5.00.\" \"The price is EUR 42,99 and the tax is EUR 5,00.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-match: after string-match (string context)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_after_string_match_multiple_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // replace-match with string-match: multiple capture groups,
    // replacing them selectively and with back-references
    let form = r#"(let ((s "2026-03-02T14:30:00"))
      ;; ISO date pattern with groups: year, month, day, hour, min, sec
      (string-match
        "\\([0-9]\\{4\\}\\)-\\([0-9]\\{2\\}\\)-\\([0-9]\\{2\\}\\)T\\([0-9]\\{2\\}\\):\\([0-9]\\{2\\}\\):\\([0-9]\\{2\\}\\)"
        s)
      (let ((year (match-string 1 s))
            (month (match-string 2 s))
            (day (match-string 3 s))
            (hour (match-string 4 s))
            (minute (match-string 5 s))
            (second (match-string 6 s)))
        (list year month day hour minute second
              ;; Replace day group only
              (replace-match "15" t t s 3)
              ;; Replace hour group only
              (replace-match "09" t t s 4)
              ;; Full match replacement with back-refs
              (replace-match "\\3/\\2/\\1 \\4:\\5:\\6" t nil s))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"2026\" \"03\" \"02\" \"14\" \"30\" \"00\" \"2026-03-15T14:30:00\" \"2026-03-02T09:30:00\" \"02/03/2026 14:30:00\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-match: back-references (\1, \2 etc.) in replacement
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_backreferences_swap_and_transform() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use back-references to swap, duplicate, and rearrange groups
    let form = r#"(let ((results nil))
      ;; Swap first and last name
      (let ((s "Doe, John"))
        (string-match "\\([A-Za-z]+\\), \\([A-Za-z]+\\)" s)
        (setq results (cons (replace-match "\\2 \\1" t nil s) results)))
      ;; Duplicate a group
      (let ((s "hello world"))
        (string-match "\\([a-z]+\\) \\([a-z]+\\)" s)
        (setq results (cons (replace-match "\\1-\\1 \\2-\\2" t nil s) results)))
      ;; Wrap groups in delimiters
      (let ((s "foo:bar:baz"))
        (string-match "\\([a-z]+\\):\\([a-z]+\\):\\([a-z]+\\)" s)
        (setq results (cons (replace-match "[\\1][\\2][\\3]" t nil s) results)))
      ;; Rearrange into a different format
      (let ((s "rgb(255,128,0)"))
        (string-match "rgb(\\([0-9]+\\),\\([0-9]+\\),\\([0-9]+\\))" s)
        (setq results (cons (replace-match "R=\\1 G=\\2 B=\\3" t nil s) results)))
      (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (\"John Doe\" \"hello-hello world-world\" \"[foo][bar][baz]\" \"R=255 G=128 B=0\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: template engine using replace-match with capture groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_template_engine() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a simple template engine: scan a template string for {{key}}
    // placeholders, look up the key in an alist, and replace with the value.
    // Uses unwind-protect with fmakunbound for cleanup.
    let form = r#"(unwind-protect
      (progn
        ;; Define the template expansion function
        (defun test--expand-template (template bindings)
          "Expand {{KEY}} placeholders in TEMPLATE using BINDINGS alist."
          (let ((result template)
                (start 0))
            (while (string-match "{{\\([a-zA-Z_]+\\)}}" result start)
              (let* ((key (match-string 1 result))
                     (value (cdr (assoc key bindings))))
                (if value
                    (progn
                      (setq result (replace-match value t t result))
                      (setq start (+ (match-beginning 0) (length value))))
                  ;; Unknown key: leave placeholder, advance past it
                  (setq start (match-end 0)))))
            result))
        (let ((template "Dear {{name}}, your order #{{order_id}} for {{item}} is {{status}}.")
              (bindings '(("name" . "Alice")
                          ("order_id" . "12345")
                          ("item" . "Widget Pro")
                          ("status" . "shipped"))))
          (list
            ;; Basic expansion
            (test--expand-template template bindings)
            ;; Partial bindings (unknown keys left as-is)
            (test--expand-template template '(("name" . "Bob")))
            ;; Empty template
            (test--expand-template "" bindings)
            ;; No placeholders
            (test--expand-template "plain text" bindings)
            ;; Nested-looking (but not actually nested)
            (test--expand-template "{{name}} says {{name}}" '(("name" . "Eve")))
            ;; Value containing special regex chars
            (test--expand-template "result: {{name}}" '(("name" . "a.b*c+d"))))))
      ;; Cleanup
      (fmakunbound 'test--expand-template))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Dear Alice, your order #12345 for Widget Pro is shipped.\" \"Dear Bob, your order #{{order_id}} for {{item}} is {{status}}.\" \"\" \"plain text\" \"Eve says Eve\" \"result: a.b*c+d\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: case-preserving replacement (upper, lower, capitalized)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_case_preserving_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a case-preserving search-and-replace: detect the case
    // pattern of the match, apply the same pattern to the replacement.
    // Uses unwind-protect with fmakunbound for cleanup.
    let form = r#"(unwind-protect
      (progn
        (defun test--detect-case-pattern (str)
          "Return 'upper, 'lower, 'capitalized, or 'mixed."
          (cond
            ((string= str (upcase str)) 'upper)
            ((string= str (downcase str)) 'lower)
            ((and (> (length str) 0)
                  (let ((first (substring str 0 1))
                        (rest (substring str 1)))
                    (and (string= first (upcase first))
                         (string= rest (downcase rest)))))
             'capitalized)
            (t 'mixed)))

        (defun test--apply-case-pattern (pattern str)
          "Apply PATTERN to STR."
          (cond
            ((eq pattern 'upper) (upcase str))
            ((eq pattern 'lower) (downcase str))
            ((eq pattern 'capitalized)
             (concat (upcase (substring str 0 1))
                     (downcase (substring str 1))))
            (t str)))

        (defun test--case-preserving-replace (text from to)
          "Replace FROM with TO in TEXT, preserving case pattern of each match."
          (let ((result text)
                (case-fold-search t)
                (start 0))
            (while (string-match (regexp-quote from) result start)
              (let* ((matched (match-string 0 result))
                     (pattern (test--detect-case-pattern matched))
                     (replacement (test--apply-case-pattern pattern to)))
                (setq result (replace-match replacement t t result))
                (setq start (+ (match-beginning 0) (length replacement)))))
            result))

        (list
          ;; Basic case patterns
          (test--case-preserving-replace "Hello hello HELLO" "hello" "world")
          ;; Capitalized in sentence
          (test--case-preserving-replace "The Cat sat on the cat. CAT!" "cat" "dog")
          ;; Detect case pattern
          (test--detect-case-pattern "HELLO")
          (test--detect-case-pattern "hello")
          (test--detect-case-pattern "Hello")
          (test--detect-case-pattern "hElLo")
          ;; Multi-word replacement
          (test--case-preserving-replace "Find FOO and Foo in foo land" "foo" "bar")))
      ;; Cleanup
      (fmakunbound 'test--detect-case-pattern)
      (fmakunbound 'test--apply-case-pattern)
      (fmakunbound 'test--case-preserving-replace))"#;
    let expect = expect_test::expect![[
        r#""OK (\"World world WORLD\" \"The Dog sat on the dog. DOG!\" upper lower capitalized mixed \"Find BAR and Bar in bar land\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-match in buffer with multiple passes and position tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_buffer_multipass_with_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Perform multiple rounds of replace-match in a buffer, counting
    // replacements per round, and tracking how buffer positions shift.
    let form = r#"(with-temp-buffer
      (insert "foo bar foo baz foo qux foo")
      (let ((round1-count 0)
            (round2-count 0))
        ;; Round 1: replace all "foo" with "REPLACED"
        (goto-char (point-min))
        (while (re-search-forward "foo" nil t)
          (replace-match "REPLACED" t t)
          (setq round1-count (1+ round1-count)))
        (let ((after-round1 (buffer-string)))
          ;; Round 2: replace all "REPLACED" with "X"
          (goto-char (point-min))
          (while (re-search-forward "REPLACED" nil t)
            (replace-match "X" t t)
            (setq round2-count (1+ round2-count)))
          (list round1-count
                after-round1
                round2-count
                (buffer-string)
                (buffer-size)))))"#;
    let expect = expect_test::expect![[
        r#""OK (4 \"REPLACED bar REPLACED baz REPLACED qux REPLACED\" 4 \"X bar X baz X qux X\" 19)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// replace-match: interaction with save-match-data
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_match_save_match_data_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that replace-match uses current match data, and that
    // save-match-data properly isolates nested match operations.
    let form = r#"(let ((s "alpha:100 beta:200 gamma:300"))
      ;; Establish outer match
      (string-match "\\([a-z]+\\):\\([0-9]+\\)" s)
      (let ((outer-whole (match-string 0 s))
            (outer-key (match-string 1 s))
            (outer-val (match-string 2 s)))
        ;; Do a nested match inside save-match-data
        (save-match-data
          (string-match "\\([a-z]+\\):\\([0-9]+\\)" s 10)
          (let ((inner-key (match-string 1 s)))
            ;; Inner match should find "beta"
            inner-key))
        ;; After save-match-data, outer match data should be restored
        (let ((restored-key (match-string 1 s))
              (restored-val (match-string 2 s)))
          ;; Now do replace-match using the restored outer match data
          (let ((replaced (replace-match "\\1=\\2" t nil s)))
            (list outer-whole outer-key outer-val
                  restored-key restored-val
                  replaced)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"alpha:100\" \"alpha\" \"100\" \"alpha\" \"100\" \"alpha=100 beta:200 gamma:300\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
