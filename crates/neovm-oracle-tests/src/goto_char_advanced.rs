//! Advanced oracle parity tests for point movement and buffer navigation:
//! goto-char boundary conditions, forward-char/backward-char with counts,
//! beginning-of-line/end-of-line with N, forward-line positive/negative/zero,
//! skip-chars-forward/backward complex patterns, word-by-word navigation,
//! paragraph detection, and balanced expression navigation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// goto-char with all boundary conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_goto_char_adv_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test goto-char at, before, and beyond buffer boundaries.
    // Emacs clamps to [point-min, point-max].
    let form = r#"(with-temp-buffer
      (insert "abcdefghij")
      (list
        ;; Normal position
        (progn (goto-char 5) (point))
        ;; Beginning of buffer
        (progn (goto-char 1) (point))
        ;; End of buffer
        (progn (goto-char 11) (point))
        ;; Beyond end -> clamped to point-max
        (progn (goto-char 999) (point))
        ;; Before beginning -> clamped to point-min
        (progn (goto-char -10) (point))
        ;; Zero -> clamped to point-min
        (progn (goto-char 0) (point))
        ;; Exact point-max
        (progn (goto-char (point-max)) (point))
        ;; Exact point-min
        (progn (goto-char (point-min)) (point))
        ;; Return value is the position
        (goto-char 7)))"#;
    let expect = expect_test::expect![[r#""OK (5 1 11 11 1 1 11 1 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-char / backward-char with counts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_goto_char_adv_forward_backward_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // forward-char and backward-char with various counts including
    // boundary overflow behavior.
    let form = r#"(with-temp-buffer
      (insert "Hello, World!")
      (list
        ;; forward-char from beginning
        (progn (goto-char 1)
               (forward-char 5)
               (point))
        ;; backward-char from end
        (progn (goto-char (point-max))
               (backward-char 6)
               (point))
        ;; forward-char 1 (default)
        (progn (goto-char 1)
               (forward-char)
               (point))
        ;; backward-char 1 (default)
        (progn (goto-char (point-max))
               (backward-char)
               (point))
        ;; forward-char with 0
        (progn (goto-char 5)
               (forward-char 0)
               (point))
        ;; negative forward-char = backward
        (progn (goto-char 10)
               (forward-char -3)
               (point))
        ;; negative backward-char = forward
        (progn (goto-char 1)
               (backward-char -5)
               (point))
        ;; forward-char at end -> error caught
        (progn (goto-char 1)
               (condition-case err
                   (forward-char 100)
                 (error (list 'hit-end (point)))))
        ;; backward-char at beginning -> error caught
        (progn (goto-char 1)
               (condition-case err
                   (backward-char 5)
                 (error (list 'hit-begin (point)))))))"#;
    let expect = expect_test::expect![[r#""OK (6 8 2 13 5 7 6 (hit-end 14) (hit-begin 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// beginning-of-line / end-of-line with N parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_goto_char_adv_bol_eol_with_n() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // beginning-of-line and end-of-line accept an optional N parameter
    // that moves to the Nth line first.
    let form = r#"(with-temp-buffer
      (insert "line1\nline2\nline3\nline4\nline5")
      (list
        ;; beginning-of-line with N=1 (current line)
        (progn (goto-char 8) (beginning-of-line 1) (point))
        ;; beginning-of-line with N=2 (next line)
        (progn (goto-char 1) (beginning-of-line 2) (point))
        ;; beginning-of-line with N=3 (two lines forward)
        (progn (goto-char 1) (beginning-of-line 3) (point))
        ;; end-of-line with N=1 (current line)
        (progn (goto-char 1) (end-of-line 1) (point))
        ;; end-of-line with N=2 (next line)
        (progn (goto-char 1) (end-of-line 2) (point))
        ;; beginning-of-line with N=0 (previous line end + bol)
        (progn (goto-char 14) (beginning-of-line 0) (point))
        ;; beginning-of-line default (no arg)
        (progn (goto-char 9) (beginning-of-line) (point))
        ;; end-of-line default (no arg)
        (progn (goto-char 9) (end-of-line) (point))
        ;; Multiple lines: bol from middle of line3
        (progn (goto-char 16) (beginning-of-line 1)
               (buffer-substring (point) (progn (end-of-line) (point))))))"#;
    let expect = expect_test::expect![[r#""OK (7 7 13 6 12 7 7 12 \"line3\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// forward-line with positive/negative/zero
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_goto_char_adv_forward_line_directions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // forward-line returns the number of lines NOT moved (0 = success).
    // Negative N moves backward.
    let form = r#"(with-temp-buffer
      (insert "aaa\nbbb\nccc\nddd\neee")
      (list
        ;; forward-line 0 (move to beginning of current line)
        (progn (goto-char 6) ;; middle of "bbb"
               (let ((ret (forward-line 0)))
                 (list ret (point))))
        ;; forward-line 1
        (progn (goto-char 1)
               (let ((ret (forward-line 1)))
                 (list ret (point)
                       (buffer-substring (point) (progn (end-of-line) (point))))))
        ;; forward-line 3
        (progn (goto-char 1)
               (let ((ret (forward-line 3)))
                 (list ret (point))))
        ;; forward-line beyond end
        (progn (goto-char 1)
               (let ((ret (forward-line 100)))
                 (list ret (point))))
        ;; forward-line -1 (backward)
        (progn (goto-char 16) ;; line 4
               (let ((ret (forward-line -1)))
                 (list ret (point))))
        ;; forward-line -3
        (progn (goto-char 16)
               (let ((ret (forward-line -3)))
                 (list ret (point))))
        ;; forward-line -100 (beyond beginning)
        (progn (goto-char 16)
               (let ((ret (forward-line -100)))
                 (list ret (point))))
        ;; forward-line with no newline at end
        (progn (goto-char 17) ;; last line "eee" (no trailing \n)
               (let ((ret (forward-line 1)))
                 (list ret (point))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 5) (0 5 \"bbb\") (0 13) (95 20) (0 9) (0 1) (-97 1) (0 20))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// skip-chars-forward/backward in complex patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_goto_char_adv_skip_chars_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test skip-chars with ranges, negation, special chars, and limits.
    let form = r#"(with-temp-buffer
      (insert "abc123!@#DEF456_-+xyz")
      (list
        ;; Skip lowercase + digits
        (progn (goto-char 1)
               (let ((n (skip-chars-forward "a-z0-9")))
                 (list n (point) (buffer-substring 1 (point)))))
        ;; Skip everything except uppercase
        (progn (goto-char 1)
               (let ((n (skip-chars-forward "^A-Z")))
                 (list n (point) (char-after (point)))))
        ;; Skip with explicit char list (not range)
        (progn (goto-char 1)
               (let ((n (skip-chars-forward "abc123")))
                 (list n (point))))
        ;; Skip backward from end: skip lowercase
        (progn (goto-char (point-max))
               (let ((n (skip-chars-backward "a-z")))
                 (list n (point) (buffer-substring (point) (point-max)))))
        ;; Skip with limit
        (progn (goto-char 1)
               (let ((n (skip-chars-forward "a-z0-9" 4)))
                 (list n (point))))
        ;; Skip backward with limit
        (progn (goto-char (point-max))
               (let ((n (skip-chars-backward "a-z" 18)))
                 (list n (point))))
        ;; Skip nothing (no match at point)
        (progn (goto-char 7) ;; at '!'
               (let ((n (skip-chars-forward "a-z")))
                 (list n (point))))
        ;; Skip with underscore and hyphen in set
        (progn (goto-char 13) ;; at '_'
               (let ((n (skip-chars-forward "_+---")))
                 (list n (point))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((6 7 \"abc123\") (9 10 68) (6 7) (-3 19 \"xyz\") (3 4) (-3 19) (0 7) (0 13))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: word-by-word navigation simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_goto_char_adv_word_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate extracting each word by navigating forward word-by-word,
    // using skip-chars to skip whitespace and non-word chars.
    let form = r#"(with-temp-buffer
      (insert "  hello   world,  foo-bar  baz_qux  123num  ")
      (goto-char (point-min))
      (let ((words nil))
        ;; Collect words: sequences of [a-zA-Z0-9_-]
        (while (< (point) (point-max))
          ;; Skip non-word chars
          (skip-chars-forward "^a-zA-Z0-9_")
          (when (< (point) (point-max))
            (let ((start (point)))
              (skip-chars-forward "a-zA-Z0-9_-")
              (when (> (point) start)
                (setq words (cons (list (buffer-substring start (point))
                                        start (point))
                                  words))))))
        (let ((forward-words (nreverse words)))
          ;; Now do it backward from end
          (goto-char (point-max))
          (let ((bwords nil))
            (while (> (point) (point-min))
              (skip-chars-backward "^a-zA-Z0-9_")
              (when (> (point) (point-min))
                (let ((end (point)))
                  (skip-chars-backward "a-zA-Z0-9_-")
                  (when (< (point) end)
                    (setq bwords (cons (buffer-substring (point) end)
                                       bwords))))))
            (list forward-words
                  bwords
                  ;; Should match
                  (equal (mapcar #'car forward-words) bwords))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"hello\" 3 8) (\"world\" 11 16) (\"foo-bar\" 19 26) (\"baz_qux\" 28 35) (\"123num\" 37 43)) (\"hello\" \"world\" \"foo-bar\" \"baz_qux\" \"123num\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: paragraph navigation with blank line detection
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_goto_char_adv_paragraph_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Split text into paragraphs (separated by blank lines) using
    // forward-line and point movement.
    let form = r#"(with-temp-buffer
      (insert "First paragraph line one.\nFirst paragraph line two.\n\nSecond paragraph here.\nStill second paragraph.\n\n\nThird after double blank.\n")
      (goto-char (point-min))
      (let ((paragraphs nil)
            (current-para nil))
        (while (< (point) (point-max))
          (let ((line-start (point)))
            (end-of-line)
            (let ((line-end (point))
                  (line-text (buffer-substring line-start (point))))
              ;; Check if line is blank (empty or whitespace only)
              (if (string-match-p "\\`[ \t]*\\'" line-text)
                  ;; Blank line: finalize current paragraph
                  (when current-para
                    (setq paragraphs
                          (cons (mapconcat #'identity
                                           (nreverse current-para) " ")
                                paragraphs))
                    (setq current-para nil))
                ;; Non-blank: accumulate
                (setq current-para (cons line-text current-para)))
              ;; Move to next line
              (when (< (point) (point-max))
                (forward-char 1)))))
        ;; Finalize last paragraph
        (when current-para
          (setq paragraphs
                (cons (mapconcat #'identity
                                 (nreverse current-para) " ")
                      paragraphs)))
        (let ((result (nreverse paragraphs)))
          (list (length result)
                result))))"#;
    let expect = expect_test::expect![[
        r#""OK (3 (\"First paragraph line one. First paragraph line two.\" \"Second paragraph here. Still second paragraph.\" \"Third after double blank.\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: balanced expression navigation (paren matching)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_goto_char_adv_balanced_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Navigate and extract balanced parenthesized expressions from a
    // string using manual character-by-character scanning.
    let form = r#"(with-temp-buffer
      (insert "(a (b c) (d (e f) g) h)")
      (goto-char (point-min))
      ;; Find all top-level balanced groups within the outer parens
      (let ((groups nil))
        ;; Skip opening paren
        (forward-char 1)
        (while (< (point) (1- (point-max)))
          ;; Skip whitespace
          (skip-chars-forward " \t\n")
          (when (and (< (point) (1- (point-max)))
                     (not (= (char-after (point)) ?\))))
            (if (= (char-after (point)) ?\()
                ;; Balanced group: find matching close paren
                (let ((start (point))
                      (depth 0)
                      (found nil))
                  (while (and (< (point) (point-max)) (not found))
                    (let ((c (char-after (point))))
                      (cond
                       ((= c ?\() (setq depth (1+ depth)))
                       ((= c ?\))
                        (setq depth (1- depth))
                        (when (= depth 0)
                          (forward-char 1)
                          (setq groups
                                (cons (buffer-substring start (point))
                                      groups))
                          (setq found t))))
                      (unless found (forward-char 1)))))
              ;; Atom: skip to next space or paren
              (let ((start (point)))
                (skip-chars-forward "^ \t\n()")
                (setq groups
                      (cons (buffer-substring start (point))
                            groups))))))
        (nreverse groups)))"#;
    let expect = expect_test::expect![[r#""OK (\"a\" \"(b c)\" \"(d (e f) g)\" \"h\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
