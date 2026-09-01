//! Oracle parity tests for `skip-chars-forward`, `skip-chars-backward`,
//! and `skip-syntax-forward`, `skip-syntax-backward`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// skip-chars-forward basic
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_forward_alpha() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "abcdef 12345")
                    (goto-char (point-min))
                    (let ((skipped (skip-chars-forward "a-z")))
                      (list skipped (point)
                            (char-after (point)))))"#;
    let expect = expect_test::expect![[r#""OK (6 7 32)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_forward_digits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "12345abc")
                    (goto-char (point-min))
                    (let ((skipped (skip-chars-forward "0-9")))
                      (list skipped (point))))"#;
    let expect = expect_test::expect![[r#""OK (5 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_forward_complement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // ^negates the character set
    let form = r#"(with-temp-buffer
                    (insert "hello world!")
                    (goto-char (point-min))
                    (let ((skipped (skip-chars-forward "^ ")))
                      (list skipped (point)
                            (buffer-substring (point-min) (point)))))"#;
    let expect = expect_test::expect![[r#""OK (5 6 \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_forward_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LIM parameter bounds the skip
    let form = r#"(with-temp-buffer
                    (insert "aaaaaabbbbbbb")
                    (goto-char (point-min))
                    (let ((skipped (skip-chars-forward "a-z" 5)))
                      (list skipped (point))))"#;
    let expect = expect_test::expect![[r#""OK (4 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_forward_mixed_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple ranges and individual chars
    let form = r#"(with-temp-buffer
                    (insert "abc123XYZ_-!@#")
                    (goto-char (point-min))
                    (let ((skipped (skip-chars-forward "a-zA-Z0-9_-")))
                      (list skipped (point)
                            (buffer-substring (point-min) (point)))))"#;
    let expect = expect_test::expect![[r#""OK (11 12 \"abc123XYZ_-\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_forward_no_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nothing to skip — returns 0
    let form = r#"(with-temp-buffer
                    (insert "!@#$%")
                    (goto-char (point-min))
                    (let ((skipped (skip-chars-forward "a-z")))
                      (list skipped (point))))"#;
    let expect = expect_test::expect![[r#""OK (0 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// skip-chars-backward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_backward_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-max))
                    (let ((skipped (skip-chars-backward "a-z")))
                      (list skipped (point)
                            (buffer-substring (point) (point-max)))))"#;
    let expect = expect_test::expect![[r#""OK (-5 7 \"world\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_backward_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "aaa bbb ccc")
                    (goto-char (point-max))
                    ;; LIM = 8, so don't skip past position 8
                    (let ((skipped (skip-chars-backward "a-z " 8)))
                      (list skipped (point))))"#;
    let expect = expect_test::expect![[r#""OK (-4 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_backward_complement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world!")
                    (goto-char (point-max))
                    (let ((skipped (skip-chars-backward "^h")))
                      (list skipped (point))))"#;
    let expect = expect_test::expect![[r#""OK (-11 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// skip-syntax-forward / skip-syntax-backward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_forward_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // "w" = word constituent
    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-min))
                    (let ((skipped (skip-syntax-forward "w")))
                      (list skipped (point)
                            (buffer-substring (point-min) (point)))))"#;
    let expect = expect_test::expect![[r#""OK (5 6 \"hello\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_syntax_forward_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // " " = whitespace
    let form = r#"(with-temp-buffer
                    (insert "   \t\t  hello")
                    (goto-char (point-min))
                    (let ((skipped (skip-syntax-forward " ")))
                      (list skipped (point))))"#;
    let expect = expect_test::expect![[r#""OK (7 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_syntax_forward_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-min))
                    (let ((skipped (skip-syntax-forward "w" 4)))
                      (list skipped (point))))"#;
    let expect = expect_test::expect![[r#""OK (3 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_syntax_backward_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (goto-char (point-max))
                    (let ((skipped (skip-syntax-backward "w")))
                      (list skipped (point)
                            (buffer-substring (point) (point-max)))))"#;
    let expect = expect_test::expect![[r#""OK (-5 7 \"world\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: tokenizer using skip-chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simple tokenizer: split into words, numbers, and punctuation
    let form = r#"(with-temp-buffer
                    (insert "foo = 42 + bar * 3;")
                    (goto-char (point-min))
                    (let ((tokens nil))
                      (while (< (point) (point-max))
                        ;; Skip whitespace
                        (skip-chars-forward " \t\n")
                        (when (< (point) (point-max))
                          (let ((start (point))
                                (c (char-after (point))))
                            (cond
                             ;; Word
                             ((and (>= c ?a) (<= c ?z))
                              (skip-chars-forward "a-zA-Z_")
                              (setq tokens
                                    (cons (cons 'word
                                                (buffer-substring start (point)))
                                          tokens)))
                             ;; Number
                             ((and (>= c ?0) (<= c ?9))
                              (skip-chars-forward "0-9")
                              (setq tokens
                                    (cons (cons 'num
                                                (buffer-substring start (point)))
                                          tokens)))
                             ;; Punctuation
                             (t
                              (forward-char 1)
                              (setq tokens
                                    (cons (cons 'punct
                                                (buffer-substring start (point)))
                                          tokens)))))))
                      (nreverse tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((word . \"foo\") (punct . \"=\") (num . \"42\") (punct . \"+\") (word . \"bar\") (punct . \"*\") (num . \"3\") (punct . \";\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: balanced expression finder using skip
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_word_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract all words and their positions
    let form = r#"(with-temp-buffer
                    (insert "the quick brown fox jumps over the lazy dog")
                    (goto-char (point-min))
                    (let ((words nil))
                      (while (< (point) (point-max))
                        (skip-chars-forward "^a-zA-Z")
                        (when (< (point) (point-max))
                          (let ((start (point)))
                            (skip-chars-forward "a-zA-Z")
                            (setq words
                                  (cons (list (buffer-substring start (point))
                                              start (point))
                                        words)))))
                      (nreverse words)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"the\" 1 4) (\"quick\" 5 10) (\"brown\" 11 16) (\"fox\" 17 20) (\"jumps\" 21 26) (\"over\" 27 31) (\"the\" 32 35) (\"lazy\" 36 40) (\"dog\" 41 44))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CSV field parser using skip-chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_csv_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse CSV with quoted fields
    let form = r#"(with-temp-buffer
                    (insert "name,age,city\nAlice,30,Boston\nBob,25,\"New York\"")
                    (goto-char (point-min))
                    (let ((rows nil))
                      (while (< (point) (point-max))
                        (let ((fields nil)
                              (eol (save-excursion
                                     (end-of-line)
                                     (point))))
                          (while (< (point) eol)
                            (let ((start (point)))
                              (if (= (char-after (point)) ?\")
                                  ;; Quoted field
                                  (progn
                                    (forward-char 1)
                                    (let ((fstart (point)))
                                      (skip-chars-forward "^\"")
                                      (setq fields
                                            (cons (buffer-substring
                                                   fstart (point))
                                                  fields))
                                      (when (< (point) (point-max))
                                        (forward-char 1))))
                                ;; Unquoted field
                                (skip-chars-forward "^,\n")
                                (setq fields
                                      (cons (buffer-substring start (point))
                                            fields)))
                              ;; Skip comma
                              (when (and (< (point) (point-max))
                                         (= (char-after (point)) ?,))
                                (forward-char 1))))
                          (setq rows (cons (nreverse fields) rows))
                          ;; Skip newline
                          (when (< (point) (point-max))
                            (forward-char 1))))
                      (nreverse rows)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"name\" \"age\" \"city\") (\"Alice\" \"30\" \"Boston\") (\"Bob\" \"25\" \"New York\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: identifier extraction using skip-syntax
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_extract_identifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract all word-syntax tokens from a code-like string
    let form = r#"(with-temp-buffer
                    (insert "(defun calculate (x y) (+ (* x x) (* y y)))")
                    (goto-char (point-min))
                    (let ((ids nil))
                      (while (< (point) (point-max))
                        (skip-syntax-forward "^w_")
                        (when (< (point) (point-max))
                          (let ((start (point)))
                            (skip-syntax-forward "w_")
                            (when (> (point) start)
                              (setq ids
                                    (cons (buffer-substring start (point))
                                          ids))))))
                      (nreverse ids)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"defun\" \"calculate\" \"x\" \"y\" \"+\" \"*\" \"x\" \"x\" \"*\" \"y\" \"y\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
