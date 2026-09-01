//! Advanced oracle parity tests for `looking-at` and `looking-at-p`.
//!
//! Covers: complex regex patterns, match-data side effects (looking-at vs
//! looking-at-p), position-dependent matching, character classes, alternation,
//! and a lexer/scanner built on top of looking-at.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Complex regex with nested groups and quantifiers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_nested_groups_quantifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested capture groups with quantifiers: extract IP-like pattern
    let form = r#"(with-temp-buffer
                    (insert "192.168.1.42 remaining text")
                    (goto-char (point-min))
                    (let ((matched (looking-at
                                    "\\([0-9]+\\)\\.\\([0-9]+\\)\\.\\([0-9]+\\)\\.\\([0-9]+\\)")))
                      (list matched
                            (match-string 0)
                            (match-string 1)
                            (match-string 2)
                            (match-string 3)
                            (match-string 4)
                            (match-beginning 0)
                            (match-end 0))))"#;
    let expect =
        expect_test::expect![[r#""OK (t \"192.168.1.42\" \"192\" \"168\" \"1\" \"42\" 1 13)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// looking-at vs looking-at-p: match data side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_vs_looking_at_p_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // looking-at sets match data; looking-at-p does NOT.
    // We first set match data via string-match, then call looking-at-p,
    // and verify original match data is preserved.
    let form = r#"(with-temp-buffer
                    (insert "alpha-999 beta-888")
                    (goto-char (point-min))
                    ;; First, establish match data via looking-at
                    (looking-at "\\([a-z]+\\)-\\([0-9]+\\)")
                    (let ((la-m0 (match-string 0))
                          (la-m1 (match-string 1))
                          (la-m2 (match-string 2)))
                      ;; Now move point and use looking-at-p (should NOT alter match data)
                      (goto-char 11)
                      (let ((p-result (looking-at-p "\\([a-z]+\\)-\\([0-9]+\\)")))
                        ;; Match data should still reflect the FIRST looking-at call
                        (list la-m0 la-m1 la-m2
                              p-result
                              (match-string 0)
                              (match-string 1)
                              (match-string 2)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"alpha-999\" \"alpha\" \"999\" t \"alpha-999\" \"alpha\" \"999\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// looking-at at various buffer positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_multiple_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Move through buffer and check what pattern matches at each position
    let form = r#"(with-temp-buffer
                    (insert "  123abc!!end")
                    (let ((results nil))
                      ;; At position 1: spaces
                      (goto-char 1)
                      (setq results (cons (list 1
                                                (if (looking-at "[ \t]+") 'ws 'no)
                                                (if (looking-at "[0-9]") 'dig 'no)
                                                (if (looking-at "[a-z]") 'alpha 'no))
                                          results))
                      ;; At position 3: digits
                      (goto-char 3)
                      (setq results (cons (list 3
                                                (if (looking-at "[ \t]+") 'ws 'no)
                                                (if (looking-at "[0-9]+") 'dig 'no)
                                                (if (looking-at "[a-z]") 'alpha 'no))
                                          results))
                      ;; At position 6: letters
                      (goto-char 6)
                      (setq results (cons (list 6
                                                (if (looking-at "[ \t]+") 'ws 'no)
                                                (if (looking-at "[0-9]") 'dig 'no)
                                                (if (looking-at "[a-z]+") 'alpha 'no))
                                          results))
                      ;; At position 9: punctuation
                      (goto-char 9)
                      (setq results (cons (list 9
                                                (if (looking-at "[ \t]+") 'ws 'no)
                                                (if (looking-at "[0-9]") 'dig 'no)
                                                (if (looking-at "[!]+") 'punct 'no))
                                          results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 ws no no) (3 no dig no) (6 no no alpha) (9 no no punct))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Character classes and alternation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_char_classes_alternation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test Emacs character classes and alternation in looking-at
    let form = r#"(let ((test-cases '(("let x = 42;" . "\\(?:let\\|const\\|var\\) \\([a-zA-Z_]+\\)")
                                      ("const Y = 99;" . "\\(?:let\\|const\\|var\\) \\([a-zA-Z_]+\\)")
                                      ("var foo = 0;" . "\\(?:let\\|const\\|var\\) \\([a-zA-Z_]+\\)")
                                      ("if (true)" . "\\(?:let\\|const\\|var\\) \\([a-zA-Z_]+\\)")))
                        (results nil))
                    (dolist (tc test-cases)
                      (with-temp-buffer
                        (insert (car tc))
                        (goto-char (point-min))
                        (let ((matched (looking-at (cdr tc))))
                          (setq results
                                (cons (list (car tc)
                                            (if matched t nil)
                                            (when matched (match-string 1)))
                                      results)))))
                    (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"let x = 42;\" t \"x\") (\"const Y = 99;\" t \"Y\") (\"var foo = 0;\" t \"foo\") (\"if (true)\" nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// looking-at with anchors and multiline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_anchors_and_shy_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test anchors (^, $) and shy groups with looking-at
    let form = r#"(with-temp-buffer
                    (insert "first line\nsecond line\nthird line\n")
                    (let ((results nil))
                      ;; At start of buffer: ^ should match
                      (goto-char (point-min))
                      (setq results
                            (cons (list 'bol-at-start (if (looking-at "^first") t nil))
                                  results))
                      ;; Move to second line
                      (forward-line 1)
                      (setq results
                            (cons (list 'second-line (if (looking-at "^second") t nil))
                                  results))
                      ;; looking-at with shy group alternation on third line
                      (forward-line 1)
                      (looking-at "^\\(?:first\\|second\\|third\\) \\(.*\\)")
                      (setq results
                            (cons (list 'third-match
                                        (match-string 0)
                                        (match-string 1))
                                  results))
                      (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((bol-at-start t) (second-line t) (third-match \"third line\" \"line\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: token scanner/lexer using looking-at
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_lexer_scanner() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a complete lexer that classifies tokens at point using looking-at,
    // with proper cleanup of temp function via fmakunbound/unwind-protect
    let form = r#"(progn
                    (fset 'neovm--test-lex-classify
                          (lambda ()
                            "Classify the token at point, return (TYPE . LEXEME) and advance."
                            (cond
                             ;; Whitespace: skip
                             ((looking-at "[ \t\n]+")
                              (goto-char (match-end 0))
                              nil)
                             ;; Number literal (integer or float)
                             ((looking-at "[0-9]+\\(?:\\.[0-9]+\\)?")
                              (let ((tok (cons 'number (match-string 0))))
                                (goto-char (match-end 0))
                                tok))
                             ;; String literal (double-quoted)
                             ((looking-at "\"\\([^\"]*\\)\"")
                              (let ((tok (cons 'string (match-string 1))))
                                (goto-char (match-end 0))
                                tok))
                             ;; Keyword or identifier
                             ((looking-at "[a-zA-Z_][a-zA-Z0-9_]*")
                              (let* ((word (match-string 0))
                                     (kind (if (member word '("if" "else" "while" "return" "fn"))
                                               'keyword
                                             'ident)))
                                (goto-char (match-end 0))
                                (cons kind word)))
                             ;; Operators (multi-char first)
                             ((looking-at "\\(?:==\\|!=\\|<=\\|>=\\|&&\\|||\\)")
                              (let ((tok (cons 'operator (match-string 0))))
                                (goto-char (match-end 0))
                                tok))
                             ;; Single-char punctuation
                             ((looking-at ".")
                              (let ((tok (cons 'punct (match-string 0))))
                                (goto-char (match-end 0))
                                tok))
                             (t nil))))
                    (unwind-protect
                        (with-temp-buffer
                          (insert "fn add(x, y) { return x + y; }")
                          (goto-char (point-min))
                          (let ((tokens nil))
                            (while (< (point) (point-max))
                              (let ((tok (neovm--test-lex-classify)))
                                (when tok
                                  (setq tokens (cons tok tokens)))))
                            (nreverse tokens)))
                      (fmakunbound 'neovm--test-lex-classify)))"#;
    let expect = expect_test::expect![[
        r#""OK ((keyword . \"fn\") (ident . \"add\") (punct . \"(\") (ident . \"x\") (punct . \",\") (ident . \"y\") (punct . \")\") (punct . \"{\") (keyword . \"return\") (ident . \"x\") (punct . \"+\") (ident . \"y\") (punct . \";\") (punct . \"}\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// looking-at with save-excursion for non-destructive lookahead
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_lookahead_without_moving() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use save-excursion + looking-at to peek ahead without moving point
    let form = r#"(with-temp-buffer
                    (insert "apple 42\nbanana 99\ncherry 7\n")
                    (goto-char (point-min))
                    (let ((results nil))
                      (while (not (eobp))
                        (let ((line-start (point)))
                          ;; Classify the line without permanently moving point
                          (let ((has-big-num
                                 (save-excursion
                                   (end-of-line)
                                   (let ((eol (point)))
                                     (goto-char line-start)
                                     ;; Look for a 2+ digit number on this line
                                     (looking-at ".*\\b\\([0-9][0-9]+\\)\\b"))))
                                (fruit-name
                                 (when (looking-at "\\([a-z]+\\)")
                                   (match-string 1))))
                            (setq results
                                  (cons (list fruit-name (if has-big-num 'big 'small))
                                        results))))
                        (forward-line 1))
                      (nreverse results)))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"apple\" big) (\"banana\" big) (\"cherry\" small))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// looking-at with narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_looking_at_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify looking-at respects narrowed region boundaries
    let form = r#"(with-temp-buffer
                    (insert "HEADER:data-section:FOOTER")
                    (goto-char (point-min))
                    ;; Before narrowing: looking-at sees HEADER
                    (let ((before-narrow (if (looking-at "HEADER") t nil)))
                      ;; Narrow to just "data-section"
                      (narrow-to-region 8 20)
                      (goto-char (point-min))
                      ;; After narrowing: looking-at sees data
                      (let ((after-narrow-match (looking-at "\\([a-z]+\\)-\\([a-z]+\\)"))
                            (m0 nil) (m1 nil) (m2 nil))
                        (when after-narrow-match
                          (setq m0 (match-string 0)
                                m1 (match-string 1)
                                m2 (match-string 2)))
                        ;; looking-at should NOT see HEADER or FOOTER
                        (goto-char (point-min))
                        (let ((sees-header (looking-at "HEADER"))
                              (sees-footer (progn (goto-char (point-max))
                                                  (goto-char (point-min))
                                                  (looking-at ".*FOOTER"))))
                          (list before-narrow
                                after-narrow-match m0 m1 m2
                                sees-header sees-footer)))))"#;
    let expect =
        expect_test::expect![[r#""OK (t t \"data-section\" \"data\" \"section\" nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
