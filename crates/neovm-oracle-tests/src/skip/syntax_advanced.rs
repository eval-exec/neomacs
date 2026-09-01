//! Advanced oracle parity tests for `skip-syntax-forward` and `skip-syntax-backward`.
//!
//! Covers: word constituents (w), whitespace ( ), symbol constituents (_),
//! mixed syntax classes, backward scanning, return values, identifier parsing,
//! and word extraction via syntax-based scanning.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Skip word constituents (syntax class w)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_adv_word_constituents() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // skip-syntax-forward "w" should skip letters and digits (word constituents)
    // but stop at punctuation, whitespace, and symbol chars
    let form = r#"(with-temp-buffer
                    (let ((st (make-syntax-table)))
                      (modify-syntax-entry ?_ "_" st)  ;; underscore is symbol, not word
                      (modify-syntax-entry '(?a . ?z) "w" st)
                      (modify-syntax-entry '(?A . ?Z) "w" st)
                      (modify-syntax-entry '(?0 . ?9) "w" st)
                      (set-syntax-table st)
                      (insert "hello42_world foo+bar")
                      (goto-char (point-min))
                      ;; Skip word constituents: should stop at underscore
                      (let ((s1 (skip-syntax-forward "w"))
                            (p1 (point)))
                        (let ((t1 (buffer-substring (point-min) (point))))
                          ;; Skip the underscore (symbol constituent)
                          (let ((s2 (skip-syntax-forward "_"))
                                (p2 (point)))
                            ;; Skip next word
                            (let ((s3 (skip-syntax-forward "w"))
                                  (p3 (point)))
                              (let ((t3 (buffer-substring p2 (point))))
                                ;; Skip space
                                (let ((s4 (skip-syntax-forward " "))
                                      (p4 (point)))
                                  ;; Skip next word
                                  (let ((s5 (skip-syntax-forward "w"))
                                        (p5 (point)))
                                    (let ((t5 (buffer-substring p4 (point))))
                                      (list s1 t1 s2 s3 t3 s4 s5 t5
                                            p1 p2 p3 p4 p5)))))))))))"#;
    let expect =
        expect_test::expect![[r#""OK (7 \"hello42\" 1 5 \"world\" 1 3 \"foo\" 8 9 14 15 18)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Skip whitespace (syntax class space)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_adv_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Whitespace skipping: spaces, tabs, newlines all have whitespace syntax
    let form = r#"(with-temp-buffer
                    (insert "   \t\t  \n  \n\t  hello   \t  world  \n ")
                    (goto-char (point-min))
                    ;; Skip leading whitespace
                    (let ((s1 (skip-syntax-forward " "))
                          (p1 (point)))
                      ;; Now at 'h', skip word
                      (skip-syntax-forward "w")
                      ;; Skip middle whitespace
                      (let ((s2 (skip-syntax-forward " "))
                            (p2 (point)))
                        ;; Now at 'w', skip word
                        (skip-syntax-forward "w")
                        ;; Skip trailing whitespace
                        (let ((s3 (skip-syntax-forward " "))
                              (p3 (point)))
                          ;; Should be at end
                          (list s1 p1 s2 p2 s3 p3
                                (= (point) (point-max)))))))"#;
    let expect = expect_test::expect![[r#""OK (14 15 6 26 4 35 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Skip symbol constituents (syntax class _)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_adv_symbol_constituents() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In a syntax table where _ and - are symbol constituents,
    // skip-syntax-forward "_" should skip them but not word chars
    let form = r#"(with-temp-buffer
                    (let ((st (make-syntax-table)))
                      (modify-syntax-entry ?_ "_" st)
                      (modify-syntax-entry ?- "_" st)
                      (modify-syntax-entry ?$ "_" st)
                      (modify-syntax-entry '(?a . ?z) "w" st)
                      (set-syntax-table st)
                      (insert "__$--hello_world")
                      (goto-char (point-min))
                      ;; Skip symbol constituents from beginning
                      (let ((s1 (skip-syntax-forward "_"))
                            (p1 (point)))
                        (let ((t1 (buffer-substring (point-min) (point))))
                          ;; Skip word chars
                          (let ((s2 (skip-syntax-forward "w"))
                                (p2 (point)))
                            ;; Skip the middle underscore (symbol)
                            (let ((s3 (skip-syntax-forward "_"))
                                  (p3 (point)))
                              ;; Skip remaining word chars
                              (let ((s4 (skip-syntax-forward "w"))
                                    (p4 (point)))
                                (list s1 t1 s2 s3 s4 p1 p2 p3 p4))))))))"#;
    let expect = expect_test::expect![[r#""OK (5 \"__$--\" 5 1 5 6 11 12 17)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Skip mixed syntax classes (w_) - word and symbol together
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_adv_mixed_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Passing "w_" to skip-syntax-forward should skip both word and symbol chars
    let form = r#"(with-temp-buffer
                    (let ((st (make-syntax-table)))
                      (modify-syntax-entry ?_ "_" st)
                      (modify-syntax-entry ?- "_" st)
                      (modify-syntax-entry '(?a . ?z) "w" st)
                      (modify-syntax-entry '(?A . ?Z) "w" st)
                      (modify-syntax-entry '(?0 . ?9) "w" st)
                      (set-syntax-table st)
                      (insert "hello_world-v2 = some-thing_else + 42")
                      (goto-char (point-min))
                      ;; "w_" skips both word and symbol constituents
                      (let ((s1 (skip-syntax-forward "w_"))
                            (p1 (point)))
                        (let ((t1 (buffer-substring (point-min) (point))))
                          ;; Skip whitespace
                          (skip-syntax-forward " ")
                          ;; Skip punctuation (the '=')
                          (let ((s2 (skip-syntax-forward "."))
                                (p2 (point)))
                            ;; Skip whitespace
                            (skip-syntax-forward " ")
                            ;; Skip next mixed word/symbol identifier
                            (let ((start3 (point)))
                              (let ((s3 (skip-syntax-forward "w_"))
                                    (p3 (point)))
                                (let ((t3 (buffer-substring start3 (point))))
                                  (list s1 t1 s2 s3 t3
                                        p1 p2 p3)))))))))"#;
    let expect = expect_test::expect![[r#""OK (14 \"hello_world-v2\" 0 1 \"=\" 15 16 17)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// skip-syntax-backward from various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_adv_backward_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test skip-syntax-backward from end, middle, and after whitespace
    let form = r#"(with-temp-buffer
                    (let ((st (make-syntax-table)))
                      (modify-syntax-entry ?_ "_" st)
                      (modify-syntax-entry '(?a . ?z) "w" st)
                      (modify-syntax-entry '(?A . ?Z) "w" st)
                      (modify-syntax-entry '(?0 . ?9) "w" st)
                      (set-syntax-table st)
                      (insert "alpha beta_gamma   delta")
                      ;; Backward from end: skip word
                      (goto-char (point-max))
                      (let ((s1 (skip-syntax-backward "w"))
                            (p1 (point)))
                        (let ((t1 (buffer-substring (point) (point-max))))
                          ;; Backward skip whitespace
                          (let ((s2 (skip-syntax-backward " "))
                                (p2 (point)))
                            ;; Backward skip word (should get "gamma")
                            (let ((end3 (point)))
                              (let ((s3 (skip-syntax-backward "w"))
                                    (p3 (point)))
                                (let ((t3 (buffer-substring (point) end3)))
                                  ;; Backward skip symbol (the underscore)
                                  (let ((s4 (skip-syntax-backward "_"))
                                        (p4 (point)))
                                    ;; Backward skip word+symbol together
                                    (let ((end5 (point)))
                                      (let ((s5 (skip-syntax-backward "w_"))
                                            (p5 (point)))
                                        (let ((t5 (buffer-substring (point) end5)))
                                          ;; Backward skip whitespace to reach "alpha"
                                          (skip-syntax-backward " ")
                                          (let ((end6 (point)))
                                            (let ((s6 (skip-syntax-backward "w"))
                                                  (p6 (point)))
                                              (let ((t6 (buffer-substring (point) end6)))
                                                (list s1 t1 s2 s3 t3 s4 s5 t5 s6 t6
                                                      p1 p2 p3 p4 p5 p6))))))))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (-5 \"delta\" -3 -5 \"gamma\" -1 -4 \"beta\" -5 \"alpha\" 20 17 12 11 7 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Return value: count of characters skipped (positive forward, negative backward)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_adv_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify exact return values for forward (positive) and backward (negative)
    let form = r#"(with-temp-buffer
                    (let ((st (make-syntax-table)))
                      (modify-syntax-entry '(?a . ?z) "w" st)
                      (modify-syntax-entry ?_ "_" st)
                      (set-syntax-table st)
                      (insert "   abcdef___ghi   ")
                      ;; Forward: skip 3 spaces
                      (goto-char (point-min))
                      (let ((fwd-ws (skip-syntax-forward " ")))
                        ;; Forward: skip 6 word chars
                        (let ((fwd-w (skip-syntax-forward "w")))
                          ;; Forward: skip 3 symbol chars
                          (let ((fwd-sym (skip-syntax-forward "_")))
                            ;; Forward: skip 0 (not whitespace here)
                            (let ((fwd-zero (skip-syntax-forward " ")))
                              ;; Backward from end: skip 3 spaces
                              (goto-char (point-max))
                              (let ((bwd-ws (skip-syntax-backward " ")))
                                ;; Backward: skip 3 word chars
                                (let ((bwd-w (skip-syntax-backward "w")))
                                  ;; Backward: skip 0 (not word chars here)
                                  (let ((bwd-zero (skip-syntax-backward "w")))
                                    (list fwd-ws fwd-w fwd-sym fwd-zero
                                          bwd-ws bwd-w bwd-zero
                                          ;; Verify signs: forward positive, backward negative
                                          (> fwd-ws 0) (> fwd-w 0)
                                          (< bwd-ws 0) (< bwd-w 0)
                                          (= fwd-zero 0) (= bwd-zero 0)))))))))))"#;
    let expect = expect_test::expect![[r#""OK (3 6 3 0 -3 -3 0 t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: parse identifiers, operators, whitespace using skip-syntax
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_adv_parse_expression() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use skip-syntax-forward with different classes to tokenize an expression
    // into identifier/operator/number/whitespace tokens
    let form = r#"(progn
                    (fset 'neovm--test-syntax-tokenize
                          (lambda ()
                            "Tokenize buffer using syntax classes."
                            (let ((tokens nil)
                                  (st (make-syntax-table)))
                              ;; Configure: letters/digits = word, _- = symbol, operators = punct
                              (modify-syntax-entry '(?a . ?z) "w" st)
                              (modify-syntax-entry '(?A . ?Z) "w" st)
                              (modify-syntax-entry '(?0 . ?9) "w" st)
                              (modify-syntax-entry ?_ "_" st)
                              (modify-syntax-entry ?+ "." st)
                              (modify-syntax-entry ?- "." st)
                              (modify-syntax-entry ?* "." st)
                              (modify-syntax-entry ?/ "." st)
                              (modify-syntax-entry ?= "." st)
                              (modify-syntax-entry ?< "." st)
                              (modify-syntax-entry ?> "." st)
                              (modify-syntax-entry ?\( "()" st)
                              (modify-syntax-entry ?\) ")(" st)
                              (set-syntax-table st)
                              (while (< (point) (point-max))
                                (let ((start (point)))
                                  (cond
                                   ;; Whitespace
                                   ((> (skip-syntax-forward " ") 0)
                                    nil) ;; discard whitespace tokens
                                   ;; Word (identifier or number)
                                   ((> (skip-syntax-forward "w") 0)
                                    (setq tokens
                                          (cons (cons 'word (buffer-substring start (point)))
                                                tokens)))
                                   ;; Symbol constituent
                                   ((> (skip-syntax-forward "_") 0)
                                    (setq tokens
                                          (cons (cons 'sym (buffer-substring start (point)))
                                                tokens)))
                                   ;; Punctuation (operators)
                                   ((> (skip-syntax-forward ".") 0)
                                    (setq tokens
                                          (cons (cons 'punct (buffer-substring start (point)))
                                                tokens)))
                                   ;; Open paren
                                   ((= (char-syntax (char-after (point))) ?\()
                                    (forward-char 1)
                                    (setq tokens (cons (cons 'open "(") tokens)))
                                   ;; Close paren
                                   ((= (char-syntax (char-after (point))) ?\))
                                    (forward-char 1)
                                    (setq tokens (cons (cons 'close ")") tokens)))
                                   ;; Fallback
                                   (t (forward-char 1)
                                      (setq tokens
                                            (cons (cons 'other (buffer-substring start (point)))
                                                  tokens))))))
                              (nreverse tokens))))
                    (unwind-protect
                        (with-temp-buffer
                          (insert "result_val = max(x + 42, y_coord * 3) - offset")
                          (goto-char (point-min))
                          (neovm--test-syntax-tokenize))
                      (fmakunbound 'neovm--test-syntax-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK ((word . \"result\") (sym . \"_\") (word . \"val\") (punct . \"=\") (word . \"max\") (open . \"(\") (word . \"x\") (punct . \"+\") (word . \"42\") (punct . \",\") (word . \"y\") (sym . \"_\") (word . \"coord\") (punct . \"*\") (word . \"3\") (close . \")\") (punct . \"-\") (word . \"offset\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: extract words from buffer using syntax-based scanning
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_syntax_adv_extract_words() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Scan buffer, collecting all word-syntax runs and their positions,
    // then group by first letter and count occurrences
    let form = r#"(progn
                    (defvar neovm--test-syntax-words nil)
                    (unwind-protect
                        (with-temp-buffer
                          (let ((st (make-syntax-table)))
                            (modify-syntax-entry '(?a . ?z) "w" st)
                            (modify-syntax-entry '(?A . ?Z) "w" st)
                            (modify-syntax-entry '(?0 . ?9) "w" st)
                            (modify-syntax-entry ?_ "_" st)
                            (modify-syntax-entry ?- "_" st)
                            (set-syntax-table st)
                            (insert "The quick brown fox jumped over the lazy brown dog. ")
                            (insert "The dog barked quickly at the fox.")
                            (goto-char (point-min))
                            (setq neovm--test-syntax-words nil)
                            ;; Collect all words
                            (while (< (point) (point-max))
                              (skip-syntax-forward "^w")
                              (when (< (point) (point-max))
                                (let ((start (point)))
                                  (skip-syntax-forward "w")
                                  (when (> (point) start)
                                    (setq neovm--test-syntax-words
                                          (cons (downcase (buffer-substring start (point)))
                                                neovm--test-syntax-words))))))
                            (setq neovm--test-syntax-words
                                  (nreverse neovm--test-syntax-words))
                            ;; Build frequency table
                            (let ((freq (make-hash-table :test 'equal)))
                              (dolist (w neovm--test-syntax-words)
                                (puthash w (1+ (gethash w freq 0)) freq))
                              ;; Collect words with count > 1, sorted
                              (let ((repeated nil))
                                (maphash (lambda (k v)
                                           (when (> v 1)
                                             (setq repeated (cons (list k v) repeated))))
                                         freq)
                                (list
                                 (length neovm--test-syntax-words)
                                 (sort repeated
                                       (lambda (a b) (string< (car a) (car b)))))))))
                      (makunbound 'neovm--test-syntax-words)))"#;
    let expect =
        expect_test::expect![[r#""OK (17 ((\"brown\" 2) (\"dog\" 2) (\"fox\" 2) (\"the\" 4)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
