//! Oracle parity tests for `skip-chars-forward` and `skip-chars-backward`
//! with ALL parameter combinations: character set specification (ranges, individual
//! chars, negated ^), LIM parameter (limit position), return value (number of
//! characters skipped), skipping at buffer boundaries, and complex patterns
//! like tokenizers and whitespace normalization.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Character set specification: ranges, individual chars, negated
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_patterns_charset_varieties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "aAbBcC123!@#xXyYzZ")
  (let ((results nil))
    ;; Single range: lowercase
    (goto-char (point-min))
    (push (list 'lower (skip-chars-forward "a-z") (point)) results)
    ;; Single range: uppercase
    (goto-char (point-min))
    (push (list 'upper (skip-chars-forward "A-Z") (point)) results)
    ;; Combined: alpha
    (goto-char (point-min))
    (push (list 'alpha (skip-chars-forward "a-zA-Z") (point)) results)
    ;; Combined: alnum
    (goto-char (point-min))
    (push (list 'alnum (skip-chars-forward "a-zA-Z0-9") (point)) results)
    ;; Individual chars only
    (goto-char (point-min))
    (push (list 'indiv (skip-chars-forward "aAbB") (point)) results)
    ;; Negated: skip until digit
    (goto-char (point-min))
    (push (list 'neg-digit (skip-chars-forward "^0-9") (point)) results)
    ;; Negated: skip until punctuation
    (goto-char (point-min))
    (push (list 'neg-punct (skip-chars-forward "^!@#") (point)) results)
    ;; Empty string: skip nothing
    (goto-char (point-min))
    (push (list 'empty-set (skip-chars-forward "") (point)) results)
    ;; Range with hyphen as literal: include hyphen in set
    (erase-buffer)
    (insert "a-b-c-d efg")
    (goto-char (point-min))
    (push (list 'hyphen-literal (skip-chars-forward "a-d\\-") (point)
                (buffer-substring (point-min) (point)))
          results)
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((lower 1 2) (upper 0 1) (alpha 6 7) (alnum 9 10) (indiv 4 5) (neg-digit 6 7) (neg-punct 9 10) (empty-set 0 1) (hyphen-literal 7 8 \"a-b-c-d\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_skip_chars_patterns_iso_character_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU syntax.c parses ISO C character classes in skip_chars via
    // re_wctype_parse, e.g. [:alpha:] and [:digit:], using the same syntax as
    // regexp bracket contents.
    let form = r#"(with-temp-buffer
  (insert "abc123 DEF_456!")
  (let ((results nil))
    (goto-char (point-min))
    (push (list 'alpha (skip-chars-forward "[:alpha:]") (point)) results)
    (push (list 'digit (skip-chars-forward "[:digit:]") (point)) results)
    (push (list 'blank-stop (skip-chars-forward "[:alpha:]") (point)) results)
    (skip-chars-forward " ")
    (push (list 'upper-alpha (skip-chars-forward "[:alpha:]") (point)) results)
    (push (list 'symbol-stop (skip-chars-forward "[:alnum:]") (point)) results)
    (goto-char (point-min))
    (push (list 'neg-alpha (skip-chars-forward "^[:alpha:]") (point)) results)
    (goto-char 4)
    (push (list 'neg-digit (skip-chars-forward "^[:digit:]") (point)) results)
    (goto-char (point-max))
    (push (list 'back-punct (skip-chars-backward "^[:alnum:]") (point)) results)
    (push (list 'back-alnum (skip-chars-backward "[:alnum:]") (point)) results)
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((alpha 3 4) (digit 3 7) (blank-stop 0 7) (upper-alpha 3 11) (symbol-stop 0 11) (neg-alpha 0 1) (neg-digit 0 4) (back-punct -1 15) (back-alnum -3 12))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LIM parameter: various limit positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_patterns_lim_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "aaabbbcccdddeee")
  (let ((results nil))
    ;; Forward: LIM before natural stop
    (goto-char (point-min))
    (push (list 'fwd-lim-2 (skip-chars-forward "a-z" 2) (point)) results)
    ;; Forward: LIM at natural stop (no effect)
    (goto-char (point-min))
    (push (list 'fwd-lim-exact (skip-chars-forward "a" 4) (point)) results)
    ;; Forward: LIM beyond natural stop (no effect)
    (goto-char (point-min))
    (push (list 'fwd-lim-beyond (skip-chars-forward "a" 100) (point)) results)
    ;; Forward: LIM at current point (skip 0)
    (goto-char 5)
    (push (list 'fwd-lim-at-point (skip-chars-forward "a-z" 5) (point)) results)
    ;; Forward: LIM behind current point (skip 0)
    (goto-char 5)
    (push (list 'fwd-lim-behind (skip-chars-forward "a-z" 2) (point)) results)
    ;; Backward: LIM after natural stop
    (goto-char (point-max))
    (push (list 'bwd-lim (skip-chars-backward "a-z" 10) (point)) results)
    ;; Backward: LIM before natural stop
    (goto-char (point-max))
    (push (list 'bwd-lim-before (skip-chars-backward "a-z" 14) (point)) results)
    ;; Backward: LIM at current point
    (goto-char 8)
    (push (list 'bwd-lim-at-point (skip-chars-backward "a-z" 8) (point)) results)
    ;; Backward: LIM ahead of current point (skip 0)
    (goto-char 8)
    (push (list 'bwd-lim-ahead (skip-chars-backward "a-z" 12) (point)) results)
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((fwd-lim-2 1 2) (fwd-lim-exact 3 4) (fwd-lim-beyond 3 4) (fwd-lim-at-point 0 5) (fwd-lim-behind 0 5) (bwd-lim -6 10) (bwd-lim-before -2 14) (bwd-lim-at-point 0 8) (bwd-lim-ahead 0 8))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Return value: exact count of characters skipped
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_patterns_return_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "   \t\t  hello world  \t\n")
  (let ((results nil))
    ;; Forward: count whitespace
    (goto-char (point-min))
    (let ((n (skip-chars-forward " \t")))
      (push (list 'fwd-ws n (point)) results))
    ;; Forward: count word chars
    (let ((n (skip-chars-forward "a-z")))
      (push (list 'fwd-word n (point)) results))
    ;; Forward: skip one space
    (let ((n (skip-chars-forward " ")))
      (push (list 'fwd-space n (point)) results))
    ;; Backward from end: count newline+whitespace
    (goto-char (point-max))
    (let ((n (skip-chars-backward " \t\n")))
      (push (list 'bwd-ws n (point)) results))
    ;; Backward: count word chars
    (let ((n (skip-chars-backward "a-z")))
      (push (list 'bwd-word n (point)) results))
    ;; No match: returns 0
    (goto-char (point-min))
    (let ((n (skip-chars-forward "0-9")))
      (push (list 'no-match n (point)) results))
    ;; Forward returns positive, backward returns negative
    (goto-char 10)
    (let ((fwd (skip-chars-forward "a-z"))
          (bwd (progn (goto-char 10) (skip-chars-backward "a-z"))))
      (push (list 'signs fwd bwd (> fwd 0) (< bwd 0)) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((fwd-ws 7 8) (fwd-word 5 13) (fwd-space 1 14) (bwd-ws -4 19) (bwd-word -5 14) (no-match 0 1) (signs 3 -2 t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Skipping at buffer boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_patterns_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((results nil))
  ;; Forward at point-max
  (with-temp-buffer
    (insert "abc")
    (goto-char (point-max))
    (push (list 'fwd-at-max (skip-chars-forward "a-z") (point)) results))
  ;; Backward at point-min
  (with-temp-buffer
    (insert "abc")
    (goto-char (point-min))
    (push (list 'bwd-at-min (skip-chars-backward "a-z") (point)) results))
  ;; Forward skips entire buffer
  (with-temp-buffer
    (insert "abcdef")
    (goto-char (point-min))
    (push (list 'fwd-all (skip-chars-forward "a-z") (point) (= (point) (point-max))) results))
  ;; Backward skips entire buffer
  (with-temp-buffer
    (insert "abcdef")
    (goto-char (point-max))
    (push (list 'bwd-all (skip-chars-backward "a-z") (point) (= (point) (point-min))) results))
  ;; Empty buffer
  (with-temp-buffer
    (push (list 'empty-fwd (skip-chars-forward "a-z") (point)) results)
    (push (list 'empty-bwd (skip-chars-backward "a-z") (point)) results))
  ;; With narrowing: skipping stops at narrow boundaries
  (with-temp-buffer
    (insert "aaa---bbb")
    (save-restriction
      (narrow-to-region 4 7)
      (goto-char (point-min))
      (push (list 'narrow-fwd (skip-chars-forward "\\-a-z") (point) (point-max)) results)
      (goto-char (point-max))
      (push (list 'narrow-bwd (skip-chars-backward "\\-a-z") (point) (point-min)) results)))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((fwd-at-max 0 4) (bwd-at-min 0 1) (fwd-all 6 7 t) (bwd-all -6 1 t) (empty-fwd 0 1) (empty-bwd 0 1) (narrow-fwd 3 7 7) (narrow-bwd -3 4 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-pass tokenizer using skip-chars-forward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_patterns_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A complete expression tokenizer that handles identifiers, numbers,
    // string literals, operators, and whitespace
    let form = r#"(with-temp-buffer
  (insert "result = count + 42 * (total - 7)")
  (goto-char (point-min))
  (let ((tokens nil))
    (while (< (point) (point-max))
      ;; Skip whitespace
      (skip-chars-forward " \t")
      (when (< (point) (point-max))
        (let ((start (point))
              (ch (char-after)))
          (cond
           ;; Identifier: starts with alpha or underscore
           ((or (and (>= ch ?a) (<= ch ?z))
                (and (>= ch ?A) (<= ch ?Z))
                (= ch ?_))
            (skip-chars-forward "a-zA-Z0-9_")
            (push (list 'id (buffer-substring start (point))) tokens))
           ;; Number: digits possibly with dot
           ((and (>= ch ?0) (<= ch ?9))
            (skip-chars-forward "0-9")
            (when (and (< (point) (point-max)) (= (char-after) ?.))
              (forward-char 1)
              (skip-chars-forward "0-9"))
            (push (list 'num (buffer-substring start (point))) tokens))
           ;; Operators: single or double character
           ((memq ch '(?= ?+ ?- ?* ?/ ?< ?> ?!))
            (forward-char 1)
            (when (and (< (point) (point-max))
                       (= (char-after) ?=))
              (forward-char 1))
            (push (list 'op (buffer-substring start (point))) tokens))
           ;; Parentheses
           ((memq ch '(?\( ?\)))
            (forward-char 1)
            (push (list 'paren (buffer-substring start (point))) tokens))
           ;; Unknown: skip one char
           (t
            (forward-char 1)
            (push (list 'unknown (buffer-substring start (point))) tokens))))))
    (nreverse tokens)))"#;
    let expect = expect_test::expect![[
        r#""OK ((id \"result\") (op \"=\") (id \"count\") (op \"+\") (num \"42\") (op \"*\") (paren \"(\") (id \"total\") (op \"-\") (num \"7\") (paren \")\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: whitespace normalization using skip-chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_patterns_whitespace_normalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Normalize whitespace: collapse multiple spaces/tabs to single space,
    // trim leading/trailing whitespace from each line
    let form = r#"(with-temp-buffer
  (insert "  hello   world  \n\tfoo\t\tbar\tbaz\n   leading   trailing   \n")
  ;; Process line by line
  (goto-char (point-min))
  (let ((output-lines nil))
    (while (not (eobp))
      (let ((bol (line-beginning-position))
            (eol (line-end-position)))
        ;; Extract and normalize the line
        (goto-char bol)
        ;; Skip leading whitespace
        (skip-chars-forward " \t" eol)
        (let ((content-start (point))
              (words nil))
          ;; Collect words
          (while (< (point) eol)
            (let ((wstart (point)))
              (skip-chars-forward "^ \t" eol)
              (when (> (point) wstart)
                (push (buffer-substring wstart (point)) words)))
            (skip-chars-forward " \t" eol))
          (push (mapconcat #'identity (nreverse words) " ") output-lines)))
      (forward-line 1))
    (nreverse output-lines)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"hello world\" \"foo bar baz\" \"leading trailing\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: S-expression boundary detection using skip-chars
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_skip_chars_patterns_sexp_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Find atom boundaries in a Lisp-like expression using skip-chars
    let form = r#"(with-temp-buffer
  (insert "(defun add (x y) (+ x y))")
  (goto-char (point-min))
  (let ((atoms nil))
    (while (< (point) (point-max))
      ;; Skip delimiters and whitespace
      (skip-chars-forward " \t\n()")
      (when (< (point) (point-max))
        (let ((start (point)))
          ;; An atom is anything that's not whitespace or a paren
          (skip-chars-forward "^ \t\n()")
          (when (> (point) start)
            (push (list (buffer-substring start (point))
                        start (point)
                        (- (point) start))
                  atoms)))))
    ;; Return atoms with their positions and lengths
    (let* ((atom-list (nreverse atoms))
           (names (mapcar #'car atom-list))
           (total-chars (apply #'+ (mapcar (lambda (a) (nth 3 a)) atom-list))))
      (list names total-chars (length atom-list)))))"#;
    let expect =
        expect_test::expect![[r#""OK ((\"defun\" \"add\" \"x\" \"y\" \"+\" \"x\" \"y\") 13 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
