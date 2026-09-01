//! Comprehensive oracle parity tests for thing-at-point patterns:
//! word boundaries via skip-chars/forward-word, sentence navigation,
//! symbol-at-point via skip-syntax, sexp parsing with forward-sexp/backward-sexp,
//! line extraction via line-beginning-position/line-end-position,
//! paragraph boundaries, defun boundaries, URL-like pattern matching,
//! and number-at-point extraction. All implemented using buffer operations
//! in pure Elisp.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Word boundaries: forward-word, backward-word, word extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_word_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract word at various positions using forward-word/backward-word
    let form = r#"(with-temp-buffer
      (insert "hello world foo-bar baz_qux")
      ;; Extract word at beginning
      (goto-char (point-min))
      (let ((start (point)))
        (forward-word 1)
        (let ((w1 (buffer-substring start (point))))
          ;; Extract second word
          (skip-chars-forward " ")
          (let ((s2 (point)))
            (forward-word 1)
            (let ((w2 (buffer-substring s2 (point))))
              ;; Extract word containing hyphen (forward-word stops at hyphen)
              (skip-chars-forward " ")
              (let ((s3 (point)))
                (forward-word 1)
                (let ((w3a (buffer-substring s3 (point))))
                  ;; Skip hyphen and get second part
                  (forward-char 1)
                  (let ((s4 (point)))
                    (forward-word 1)
                    (let ((w3b (buffer-substring s4 (point))))
                      ;; Now backward-word from end
                      (goto-char (point-max))
                      (backward-word 1)
                      (let ((bw-start (point)))
                        (forward-word 1)
                        (let ((last-word (buffer-substring bw-start (point))))
                          (list w1 w2 w3a w3b last-word))))))))))))"#;
    let expect = expect_test::expect![[r#""OK (\"hello\" \"world\" \"foo\" \"bar\" \"qux\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Skip-chars for symbol extraction (Elisp symbol chars: word + hyphens + underscores)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_symbol_via_skip_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "  my-variable-name  another_symbol  third.one  ")
      ;; Skip to first non-space, then grab symbol chars
      (goto-char (point-min))
      (skip-chars-forward " \t")
      (let ((sym-start (point)))
        (skip-chars-forward "a-zA-Z0-9_-")
        (let ((sym1 (buffer-substring sym-start (point))))
          ;; Next symbol
          (skip-chars-forward " \t")
          (let ((s2 (point)))
            (skip-chars-forward "a-zA-Z0-9_-")
            (let ((sym2 (buffer-substring s2 (point))))
              ;; Third: skip-chars stops at period
              (skip-chars-forward " \t")
              (let ((s3 (point)))
                (skip-chars-forward "a-zA-Z0-9_\\-.")
                (let ((sym3 (buffer-substring s3 (point))))
                  (list sym1 sym2 sym3))))))))"#;
    let expect =
        expect_test::expect![[r#""OK (\"my-variable-name\" \"another_symbol\" \"third.one\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Skip-syntax for word/symbol boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_skip_syntax_word_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "hello, world! (test) [array] {block}")
      (goto-char (point-min))
      ;; skip-syntax-forward "w" skips word constituents
      (let ((results nil))
        ;; First word
        (skip-syntax-forward "^ w")
        (let ((s1 (point)))
          (skip-syntax-forward "w")
          (setq results (cons (buffer-substring s1 (point)) results)))
        ;; Skip non-word to next word
        (skip-syntax-forward "^w")
        (let ((s2 (point)))
          (skip-syntax-forward "w")
          (setq results (cons (buffer-substring s2 (point)) results)))
        ;; Next word
        (skip-syntax-forward "^w")
        (let ((s3 (point)))
          (skip-syntax-forward "w")
          (setq results (cons (buffer-substring s3 (point)) results)))
        ;; Next word
        (skip-syntax-forward "^w")
        (let ((s4 (point)))
          (skip-syntax-forward "w")
          (setq results (cons (buffer-substring s4 (point)) results)))
        ;; Next word
        (skip-syntax-forward "^w")
        (let ((s5 (point)))
          (skip-syntax-forward "w")
          (setq results (cons (buffer-substring s5 (point)) results)))
        (nreverse results)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"hello\" \"world\" \"test\" \"array\" \"block\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Line extraction: line-beginning-position, line-end-position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_line_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "first line\nsecond line\nthird line\nfourth line")
      ;; Extract each line by navigating to it
      (goto-char (point-min))
      (let ((line1 (buffer-substring (line-beginning-position)
                                      (line-end-position))))
        (forward-line 1)
        (let ((line2 (buffer-substring (line-beginning-position)
                                        (line-end-position))))
          (forward-line 1)
          (let ((line3 (buffer-substring (line-beginning-position)
                                          (line-end-position))))
            (forward-line 1)
            (let ((line4 (buffer-substring (line-beginning-position)
                                            (line-end-position))))
              ;; Also test line-beginning-position with ARG
              (goto-char (point-min))
              (forward-line 2)
              (let ((prev-line-begin (line-beginning-position 0))
                    (next-line-begin (line-beginning-position 2))
                    (cur-line-begin (line-beginning-position)))
                (list line1 line2 line3 line4
                      prev-line-begin cur-line-begin next-line-begin)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"first line\" \"second line\" \"third line\" \"fourth line\" 12 24 35)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sexp parsing: forward-sexp, backward-sexp for balanced expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_sexp_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "(defun foo (x y) (+ x y))")
      ;; forward-sexp from beginning should skip the whole form
      (goto-char (point-min))
      (let ((start (point)))
        (forward-sexp 1)
        (let ((whole-sexp (buffer-substring start (point))))
          ;; Go inside the sexp: after "(defun "
          (goto-char (1+ (point-min)))
          (forward-sexp 1)  ;; skip "defun"
          (let ((p1 (point)))
            (skip-chars-forward " ")
            (let ((p2 (point)))
              (forward-sexp 1)  ;; skip "foo"
              (let ((name (buffer-substring p2 (point))))
                (skip-chars-forward " ")
                (let ((p3 (point)))
                  (forward-sexp 1)  ;; skip "(x y)"
                  (let ((params (buffer-substring p3 (point))))
                    (skip-chars-forward " ")
                    (let ((p4 (point)))
                      (forward-sexp 1)  ;; skip "(+ x y)"
                      (let ((body (buffer-substring p4 (point))))
                        (list whole-sexp name params body)))))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"(defun foo (x y) (+ x y))\" \"foo\" \"(x y)\" \"(+ x y)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Backward-sexp: navigate backwards through expressions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_backward_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "(a (b c) (d (e f)) g)")
      ;; Go to end, then backward-sexp to navigate
      (goto-char (1- (point-max)))  ;; before closing paren
      (let ((results nil))
        ;; backward-sexp: skip "g"
        (backward-sexp 1)
        (let ((p (point)))
          (forward-sexp 1)
          (setq results (cons (buffer-substring p (point)) results))
          (goto-char p))
        ;; backward-sexp: skip "(d (e f))"
        (backward-sexp 1)
        (let ((p (point)))
          (forward-sexp 1)
          (setq results (cons (buffer-substring p (point)) results))
          (goto-char p))
        ;; backward-sexp: skip "(b c)"
        (backward-sexp 1)
        (let ((p (point)))
          (forward-sexp 1)
          (setq results (cons (buffer-substring p (point)) results))
          (goto-char p))
        ;; backward-sexp: skip "a"
        (backward-sexp 1)
        (let ((p (point)))
          (forward-sexp 1)
          (setq results (cons (buffer-substring p (point)) results))
          (goto-char p))
        results))"#;
    let expect = expect_test::expect![[r#""OK (\"a\" \"(b c)\" \"(d (e f))\" \"g\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Number extraction: skip-chars to find and extract numbers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_number_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "x = 42, y = -17, z = 3.14, w = 1e5")
      (goto-char (point-min))
      (let ((numbers nil))
        ;; Find each number: skip to digit or minus-before-digit
        (while (re-search-forward "[-+]?[0-9]+\\.?[0-9]*\\(?:e[0-9]+\\)?" nil t)
          (setq numbers (cons (match-string 0) numbers)))
        (nreverse numbers)))"#;
    let expect = expect_test::expect![[r#""OK (\"42\" \"-17\" \"3.14\" \"1e5\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sentence boundaries using forward-sentence / backward-sentence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_sentence_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "This is sentence one.  This is sentence two.  And three.")
      (goto-char (point-min))
      ;; forward-sentence moves to end of sentence
      (let ((positions nil))
        (forward-sentence 1)
        (setq positions (cons (point) positions))
        (forward-sentence 1)
        (setq positions (cons (point) positions))
        (forward-sentence 1)
        (setq positions (cons (point) positions))
        ;; Now backward
        (backward-sentence 1)
        (setq positions (cons (point) positions))
        (backward-sentence 1)
        (setq positions (cons (point) positions))
        (nreverse positions)))"#;
    let expect = expect_test::expect![[r#""OK (22 45 57 47 24)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// URL-like pattern matching with regexp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_url_pattern_matching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "Visit https://example.com/path?q=1 or http://foo.bar/baz#frag for info.")
      (goto-char (point-min))
      (let ((urls nil))
        (while (re-search-forward "https?://[^ \t\n,;)]*" nil t)
          (setq urls (cons (match-string 0) urls)))
        (nreverse urls)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"https://example.com/path?q=1\" \"http://foo.bar/baz#frag\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Paragraph boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_paragraph_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "First paragraph line one.\nFirst paragraph line two.\n\nSecond paragraph.\n\nThird paragraph\nwith multiple lines\nhere.")
      (goto-char (point-min))
      (let ((results nil))
        ;; forward-paragraph
        (forward-paragraph 1)
        (setq results (cons (list 'after-para-1 (point)) results))
        (forward-paragraph 1)
        (setq results (cons (list 'after-para-2 (point)) results))
        (forward-paragraph 1)
        (setq results (cons (list 'after-para-3 (point)) results))
        ;; backward-paragraph from end
        (goto-char (point-max))
        (backward-paragraph 1)
        (setq results (cons (list 'back-para-1 (point)) results))
        (backward-paragraph 1)
        (setq results (cons (list 'back-para-2 (point)) results))
        (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((after-para-1 53) (after-para-2 72) (after-para-3 114) (back-para-1 72) (back-para-2 53))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Defun boundaries using beginning-of-defun / end-of-defun
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_defun_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (emacs-lisp-mode)
      (insert "(defun foo ()\n  (message \"foo\"))\n\n(defun bar (x)\n  (1+ x))\n\n(defun baz ()\n  nil)\n")
      ;; Navigate defun boundaries
      (goto-char (point-max))
      (let ((results nil))
        (beginning-of-defun 1)
        (setq results (cons (list 'baz-start (point)) results))
        (end-of-defun 1)
        (setq results (cons (list 'baz-end (point)) results))
        (beginning-of-defun 2)
        (setq results (cons (list 'foo-start (point)) results))
        (end-of-defun 1)
        (setq results (cons (list 'foo-end (point)) results))
        (nreverse results)))"#;
    let expect =
        expect_test::expect![[r#""OK ((baz-start 61) (baz-end 82) (foo-start 35) (foo-end 60))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multi-thing extraction from one buffer: words, lines, sexps
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_multi_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "(let ((x 42)\n      (y \"hello world\"))\n  (+ x (length y)))")
      (goto-char (point-min))
      ;; Extract whole sexp
      (let ((whole-start (point)))
        (forward-sexp 1)
        (let ((whole (buffer-substring whole-start (point))))
          ;; Extract first line
          (goto-char (point-min))
          (let ((first-line (buffer-substring (line-beginning-position)
                                               (line-end-position))))
            ;; Count lines
            (goto-char (point-min))
            (let ((line-count 0))
              (while (not (eobp))
                (setq line-count (1+ line-count))
                (forward-line 1))
              ;; Forward-word count
              (goto-char (point-min))
              (let ((word-count 0))
                (while (forward-word 1)
                  (setq word-count (1+ word-count)))
                (list (length whole)
                      first-line
                      line-count
                      word-count)))))))"#;
    let expect = expect_test::expect![[r#""OK (57 \"(let ((x 42)\" 3 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Email-like pattern matching with regexp in buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_email_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "Contact us at foo@example.com or bar.baz@test.org for help.")
      (goto-char (point-min))
      (let ((emails nil))
        (while (re-search-forward "[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\\.[a-zA-Z]\\{2,\\}" nil t)
          (setq emails (cons (match-string 0) emails)))
        (list (nreverse emails) (length emails))))"#;
    let expect = expect_test::expect![[r#""OK ((\"foo@example.com\" \"bar.baz@test.org\") 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Whitespace-bounded thing extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_whitespace_bounded() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "  alpha   beta\tgamma\n  delta   ")
      (goto-char (point-min))
      (let ((things nil))
        (while (not (eobp))
          (skip-chars-forward " \t\n")
          (unless (eobp)
            (let ((start (point)))
              (skip-chars-forward "^ \t\n")
              (setq things (cons (buffer-substring start (point)) things)))))
        (list (nreverse things) (length things))))"#;
    let expect = expect_test::expect![[r#""OK ((\"alpha\" \"beta\" \"gamma\" \"delta\") 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Balanced bracket extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_thing_balanced_brackets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
      (insert "a[1] + b[2][3] + c(x, y)")
      (goto-char (point-min))
      (let ((brackets nil))
        (while (search-forward "[" nil t)
          (let ((start (1- (point))))
            (backward-char 1)
            (forward-sexp 1)
            (setq brackets (cons (buffer-substring start (point)) brackets))))
        (nreverse brackets)))"#;
    let expect = expect_test::expect![[r#""OK (\"[1]\" \"[2]\" \"[3]\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
