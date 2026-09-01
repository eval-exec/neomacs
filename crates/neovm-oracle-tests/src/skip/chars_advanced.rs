//! Advanced oracle parity tests for `skip-chars-forward` and `skip-chars-backward`.
//!
//! Covers: character ranges, negation, return values, boundary conditions,
//! tokenizer implementation, word boundary detection, and complex multi-pass
//! scanning patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Character ranges: combined and overlapping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_advanced_combined_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple overlapping ranges and individual chars in one spec
    let form = r#"(with-temp-buffer
                    (insert "aZ9_-.$!rest")
                    (goto-char (point-min))
                    ;; a-z, A-Z, 0-9, underscore, hyphen, dot
                    (let ((skipped (skip-chars-forward "a-zA-Z0-9_\\-.")))
                      (list skipped
                            (point)
                            (buffer-substring (point-min) (point))
                            (buffer-substring (point) (point-max)))))"#;
    let expect = expect_test::expect![[r#""OK (6 7 \"aZ9_-.\" \"$!rest\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_advanced_hex_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Skip valid hex digits
    let form = r#"(with-temp-buffer
                    (insert "0xDEADbeef42XYZ")
                    (goto-char 3) ;; skip past "0x"
                    (let ((skipped (skip-chars-forward "0-9a-fA-F")))
                      (list skipped
                            (point)
                            (buffer-substring 3 (point)))))"#;
    let expect = expect_test::expect![[r#""OK (10 13 \"DEADbeef42\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Negation (^...) patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_advanced_negation_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Skip everything that is NOT a delimiter (parentheses, brackets, braces, comma, semicolon)
    let form = r#"(with-temp-buffer
                    (insert "hello_world.foo(bar, baz)")
                    (goto-char (point-min))
                    (let ((skipped (skip-chars-forward "^()[]{},:;")))
                      (list skipped
                            (point)
                            (buffer-substring (point-min) (point))
                            (char-after (point)))))"#;
    let expect = expect_test::expect![[r#""OK (15 16 \"hello_world.foo\" 40)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_advanced_negation_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Skip to end of line (everything except newline)
    let form = r#"(with-temp-buffer
                    (insert "first line content\nsecond line\nthird")
                    (goto-char (point-min))
                    (let ((first-skip (skip-chars-forward "^\n")))
                      (let ((at-newline (char-after (point)))
                            (line1 (buffer-substring (point-min) (point))))
                        ;; Skip the newline itself
                        (forward-char 1)
                        ;; Skip second line
                        (let ((second-start (point)))
                          (let ((second-skip (skip-chars-forward "^\n")))
                            (list first-skip line1
                                  second-skip (buffer-substring second-start (point))))))))"#;
    let expect = expect_test::expect![[r#""OK (18 \"first line content\" 11 \"second line\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Return value semantics: exact count
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_advanced_return_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify skip-chars-forward returns exact number of characters skipped,
    // and skip-chars-backward returns negative count
    let form = r#"(with-temp-buffer
                    (insert "   \t\t   hello   \t\t   ")
                    ;; Forward from start: count leading whitespace
                    (goto-char (point-min))
                    (let ((fwd-count (skip-chars-forward " \t"))
                          (fwd-pos (point)))
                      ;; Backward from end: count trailing whitespace
                      (goto-char (point-max))
                      (let ((bwd-count (skip-chars-backward " \t"))
                            (bwd-pos (point)))
                        ;; Forward skip of zero chars (no match at point)
                        (goto-char fwd-pos) ;; at 'h'
                        (let ((zero-count (skip-chars-forward " \t")))
                          (list fwd-count fwd-pos
                                bwd-count bwd-pos
                                zero-count)))))"#;
    let expect = expect_test::expect![[r#""OK (8 9 -8 14 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// skip-chars-backward from end of buffer with limit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_backward_from_end_with_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Skip backward from point-max, but respect limit parameter
    let form = r#"(with-temp-buffer
                    (insert "prefix===suffix")
                    (goto-char (point-max))
                    ;; Skip lowercase backwards, limited to position 10
                    (let ((skip1 (skip-chars-backward "a-z" 10)))
                      (let ((pos1 (point))
                            (text1 (buffer-substring (point) (point-max))))
                        ;; Now skip without limit
                        (goto-char (point-max))
                        (let ((skip2 (skip-chars-backward "a-z")))
                          (let ((pos2 (point))
                                (text2 (buffer-substring (point) (point-max))))
                            ;; Skip backward including '=' chars
                            (goto-char (point-max))
                            (let ((skip3 (skip-chars-backward "a-z=")))
                              (list skip1 pos1 text1
                                    skip2 pos2 text2
                                    skip3 (point))))))))"#;
    let expect = expect_test::expect![[r#""OK (-6 10 \"suffix\" -6 10 \"suffix\" -15 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: tokenizer using skip-chars-forward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_advanced_expression_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tokenize a mathematical expression into numbers, operators, parens, identifiers
    // Uses a function defined with fset + cleanup with fmakunbound
    let form = r#"(progn
                    (fset 'neovm--test-skip-tokenize
                          (lambda ()
                            "Tokenize buffer content from point to point-max."
                            (let ((tokens nil))
                              (while (< (point) (point-max))
                                ;; Skip whitespace
                                (let ((ws-skipped (skip-chars-forward " \t\n")))
                                  (when (and (> ws-skipped 0) (< (point) (point-max)))
                                    nil)) ;; whitespace consumed
                                (when (< (point) (point-max))
                                  (let ((start (point))
                                        (ch (char-after (point))))
                                    (cond
                                     ;; Number: integer or float
                                     ((and (>= ch ?0) (<= ch ?9))
                                      (skip-chars-forward "0-9")
                                      (when (and (< (point) (point-max))
                                                 (= (char-after (point)) ?.))
                                        (forward-char 1)
                                        (skip-chars-forward "0-9"))
                                      (setq tokens (cons (cons 'num (buffer-substring start (point)))
                                                         tokens)))
                                     ;; Identifier
                                     ((or (and (>= ch ?a) (<= ch ?z))
                                          (and (>= ch ?A) (<= ch ?Z))
                                          (= ch ?_))
                                      (skip-chars-forward "a-zA-Z0-9_")
                                      (setq tokens (cons (cons 'id (buffer-substring start (point)))
                                                         tokens)))
                                     ;; Parentheses
                                     ((or (= ch ?\() (= ch ?\)))
                                      (forward-char 1)
                                      (setq tokens (cons (cons 'paren (buffer-substring start (point)))
                                                         tokens)))
                                     ;; Operators (including multi-char)
                                     ((memq ch '(?+ ?- ?* ?/ ?= ?< ?> ?!))
                                      (forward-char 1)
                                      ;; Check for two-char operators
                                      (when (and (< (point) (point-max))
                                                 (= (char-after (point)) ?=))
                                        (forward-char 1))
                                      (setq tokens (cons (cons 'op (buffer-substring start (point)))
                                                         tokens)))
                                     ;; Comma, semicolon
                                     ((memq ch '(?, ?\;))
                                      (forward-char 1)
                                      (setq tokens (cons (cons 'sep (buffer-substring start (point)))
                                                         tokens)))
                                     ;; Unknown: skip one char
                                     (t
                                      (forward-char 1)
                                      (setq tokens (cons (cons 'unknown (buffer-substring start (point)))
                                                         tokens)))))))
                              (nreverse tokens))))
                    (unwind-protect
                        (with-temp-buffer
                          (insert "result = sin(3.14) + max(x, 42) * 2;")
                          (goto-char (point-min))
                          (neovm--test-skip-tokenize))
                      (fmakunbound 'neovm--test-skip-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((id . \"result\") (op . \"=\") (id . \"sin\") (paren . \"(\") (num . \"3.14\") (paren . \")\") (op . \"+\") (id . \"max\") (paren . \"(\") (id . \"x\") (sep . \",\") (num . \"42\") (paren . \")\") (op . \"*\") (num . \"2\") (sep . \";\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: word boundary detection using skip-chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_advanced_word_boundary_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Detect word boundaries and extract words with their boundary types
    // (start-of-buffer, after-space, after-punctuation, etc.)
    let form = r#"(progn
                    (defvar neovm--test-skip-word-results nil)
                    (unwind-protect
                        (with-temp-buffer
                          (insert "hello,world  foo-bar  (baz)  123abc")
                          (goto-char (point-min))
                          (setq neovm--test-skip-word-results nil)
                          (while (< (point) (point-max))
                            ;; Record what's before point
                            (let ((before-ctx
                                   (cond
                                    ((= (point) (point-min)) 'buffer-start)
                                    ((memq (char-before (point)) '(?\  ?\t ?\n)) 'after-space)
                                    ((memq (char-before (point)) '(?, ?\. ?\; ?\( ?\) ?-)) 'after-punct)
                                    (t 'after-other))))
                              ;; Try to grab a word
                              (let ((word-start (point)))
                                (let ((skipped (skip-chars-forward "a-zA-Z0-9")))
                                  (if (> skipped 0)
                                      (setq neovm--test-skip-word-results
                                            (cons (list before-ctx
                                                        (buffer-substring word-start (point))
                                                        word-start (point))
                                                  neovm--test-skip-word-results))
                                    ;; Not a word char, skip one char
                                    (forward-char 1))))))
                          (nreverse neovm--test-skip-word-results))
                      (makunbound 'neovm--test-skip-word-results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((buffer-start \"hello\" 1 6) (after-punct \"world\" 7 12) (after-space \"foo\" 14 17) (after-punct \"bar\" 18 21) (after-punct \"baz\" 24 27) (after-space \"123abc\" 30 36))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// skip-chars with special characters in charset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_advanced_special_chars_in_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test skip-chars with backslash, caret in non-first position, closing bracket
    let form = r#"(with-temp-buffer
                    (insert "abc^def^ghi rest")
                    (goto-char (point-min))
                    ;; Caret in non-first position is literal
                    (let ((s1 (skip-chars-forward "a-z^")))
                      (let ((p1 (point))
                            (t1 (buffer-substring (point-min) (point))))
                        ;; Now test with dash at end (literal dash)
                        (erase-buffer)
                        (insert "a-b-c-d end")
                        (goto-char (point-min))
                        (let ((s2 (skip-chars-forward "a-d-")))
                          (let ((p2 (point))
                                (t2 (buffer-substring (point-min) (point))))
                            (list s1 t1
                                  s2 t2))))))"#;
    let expect = expect_test::expect![[r#""OK (11 \"abc^def^ghi\" 7 \"a-b-c-d\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: balanced expression scanner with skip-chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_advanced_balanced_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Scan for balanced parentheses using skip-chars to find delimiters,
    // then manually track nesting depth
    let form = r#"(with-temp-buffer
                    (insert "a + (b * (c + d)) - (e / f)")
                    (goto-char (point-min))
                    (let ((paren-groups nil)
                          (depth 0)
                          (group-starts nil))
                      (while (< (point) (point-max))
                        ;; Skip to next paren
                        (skip-chars-forward "^()")
                        (when (< (point) (point-max))
                          (let ((ch (char-after (point))))
                            (cond
                             ((= ch ?\()
                              (setq depth (1+ depth))
                              (setq group-starts (cons (point) group-starts))
                              (forward-char 1))
                             ((= ch ?\))
                              (forward-char 1)
                              (when group-starts
                                (let ((start (car group-starts)))
                                  (setq group-starts (cdr group-starts))
                                  (setq paren-groups
                                        (cons (list depth
                                                    (buffer-substring start (point)))
                                              paren-groups))))
                              (setq depth (max 0 (1- depth))))))))
                      (nreverse paren-groups)))"#;
    let expect =
        expect_test::expect![[r#""OK ((2 \"(c + d)\") (1 \"(b * (c + d))\") (1 \"(e / f)\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
