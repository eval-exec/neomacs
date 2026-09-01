//! Oracle parity tests for `following-char` and `char-after`:
//! positional lookups, boundary behavior, nil argument semantics,
//! narrowed buffers, and character classification loops.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// following-char at various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_following_char_various_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "ABCDEF")
  (let ((results nil))
    (goto-char 1)
    (setq results (cons (following-char) results))  ;; ?A = 65
    (goto-char 3)
    (setq results (cons (following-char) results))  ;; ?C = 67
    (goto-char 6)
    (setq results (cons (following-char) results))  ;; ?F = 70
    ;; Move through buffer collecting each char
    (goto-char 1)
    (let ((all-chars nil))
      (while (not (eobp))
        (setq all-chars (cons (following-char) all-chars))
        (forward-char 1))
      (setq results (cons (nreverse all-chars) results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (65 67 70 (65 66 67 68 69 70))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// following-char at end of buffer returns 0
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_following_char_at_eob() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "xyz")
  ;; After insert, point is at position 4 (past last char)
  (let ((at-eob (following-char)))
    ;; Also check after going to point-max explicitly
    (goto-char (point-max))
    (let ((at-pmax (following-char)))
      ;; And in an empty buffer
      (let ((in-empty (with-temp-buffer (following-char))))
        (list at-eob at-pmax in-empty
              ;; All should be 0
              (= at-eob 0)
              (= at-pmax 0)
              (= in-empty 0))))))"#;
    let expect = expect_test::expect![[r#""OK (0 0 0 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-after with explicit POS argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_after_explicit_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "0123456789")
  (list
   ;; char-after at each digit position
   (char-after 1)   ;; ?0 = 48
   (char-after 5)   ;; ?4 = 52
   (char-after 10)  ;; ?9 = 57
   ;; out of range returns nil
   (char-after 0)
   (char-after 11)
   (char-after -1)
   ;; char-after does NOT move point
   (let ((p (point)))
     (char-after 1)
     (= (point) p))))"#;
    let expect = expect_test::expect![[r#""OK (48 52 57 nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-after with nil (use point)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_after_nil_uses_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "Hello World")
  (goto-char 1)
  (let ((with-nil (char-after nil))
        (without-arg (char-after))
        (with-explicit (char-after 1)))
    ;; All three should be ?H = 72
    (list with-nil without-arg with-explicit
          (eq with-nil without-arg)
          (eq without-arg with-explicit)
          ;; Move point and verify nil tracks
          (progn
            (goto-char 7)
            (list (char-after nil) (char-after) (char-after 7))))))"#;
    let expect = expect_test::expect![[r#""OK (72 72 72 t t (87 87 87))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-after returns nil at end of buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_after_nil_at_eob() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Note: char-after returns nil (not 0) when there is no character.
    // This differs from following-char which returns 0.
    let form = r#"(with-temp-buffer
  (insert "abc")
  (let ((results nil))
    ;; At point-max (position 4, one past last char)
    (setq results (cons (char-after (point-max)) results))  ;; nil
    ;; following-char at same position returns 0
    (goto-char (point-max))
    (setq results (cons (following-char) results))  ;; 0
    ;; In empty buffer
    (setq results
          (cons (with-temp-buffer
                  (list (char-after) (char-after 1) (following-char)))
                results))
    ;; Verify nil vs 0 distinction
    (setq results (cons (null (char-after (point-max))) results))  ;; t
    (setq results (cons (= (progn (goto-char (point-max)) (following-char)) 0) results))  ;; t
    (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (nil 0 (nil nil 0) t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: character classification loop using char-after
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_after_classification_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Scan a string classifying each character as digit/alpha/space/other,
    // collecting runs of same-class characters as tokens
    let form = r#"(with-temp-buffer
  (insert "hello 42 world! 3.14 foo_bar")
  (goto-char (point-min))
  (let ((tokens nil)
        (classify
         (lambda (ch)
           (cond
            ((and (>= ch ?0) (<= ch ?9)) 'digit)
            ((and (>= ch ?a) (<= ch ?z)) 'alpha)
            ((and (>= ch ?A) (<= ch ?Z)) 'alpha)
            ((= ch ?\s) 'space)
            ((= ch ?_) 'alpha)
            ((= ch ?.) 'punct)
            (t 'other)))))
    ;; Tokenize by runs of same class
    (while (not (eobp))
      (let* ((start (point))
             (ch (char-after))
             (cls (funcall classify ch)))
        ;; Skip space runs without emitting token
        (if (eq cls 'space)
            (while (and (not (eobp))
                        (let ((c (char-after)))
                          (and c (= c ?\s))))
              (forward-char 1))
          ;; Collect run of same class
          (while (and (not (eobp))
                      (let ((c (char-after)))
                        (and c (eq (funcall classify c) cls))))
            (forward-char 1))
          (setq tokens
                (cons (list cls (buffer-substring start (point)))
                      tokens)))))
    ;; Return tokens, counts by class, and total non-space chars
    (let ((rev-tokens (nreverse tokens))
          (alpha-count 0) (digit-count 0) (other-count 0))
      (dolist (tok rev-tokens)
        (let ((cls (car tok))
              (len (length (cadr tok))))
          (cond
           ((eq cls 'alpha) (setq alpha-count (+ alpha-count len)))
           ((eq cls 'digit) (setq digit-count (+ digit-count len)))
           (t (setq other-count (+ other-count len))))))
      (list rev-tokens
            (list 'alpha alpha-count 'digit digit-count 'other other-count)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((alpha \"hello\") (digit \"42\") (alpha \"world\") (other \"!\") (digit \"3\") (punct \".\") (digit \"14\") (alpha \"foo_bar\")) (alpha 17 digit 5 other 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// following-char and char-after in narrowed buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_following_char_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (save-restriction
    ;; Narrow to "DEFGH" (positions 4..9)
    (narrow-to-region 4 9)
    (let ((results nil))
      ;; following-char at narrowed point-min
      (goto-char (point-min))
      (setq results (cons (following-char) results))  ;; ?D = 68
      ;; char-after at narrowed point-min
      (setq results (cons (char-after (point-min)) results))  ;; ?D = 68
      ;; char-after at narrowed point-max => nil
      (setq results (cons (char-after (point-max)) results))  ;; nil
      ;; following-char at narrowed point-max => 0
      (goto-char (point-max))
      (setq results (cons (following-char) results))  ;; 0
      ;; char-after outside narrowed range => nil
      (setq results (cons (char-after 2) results))  ;; nil (before narrow start)
      ;; Iterate through narrowed region
      (goto-char (point-min))
      (let ((chars nil))
        (while (not (eobp))
          (setq chars (cons (following-char) chars))
          (forward-char 1))
        (setq results (cons (nreverse chars) results)))
      (nreverse results))))"#;
    let expect = expect_test::expect![[r#""OK (68 68 nil 0 nil (68 69 70 71 72))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: forward scanning using char-after to find balanced parens
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_after_balanced_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "(defun foo (x y) (+ (* x x) (* y y)))")
  (goto-char (point-min))
  ;; Find matching closing paren for opening paren at point
  (let ((find-matching-close
         (lambda (start)
           (goto-char start)
           (let ((depth 0)
                 (found nil))
             (while (and (not (eobp)) (not found))
               (let ((ch (char-after)))
                 (cond
                  ((= ch ?\() (setq depth (1+ depth)))
                  ((= ch ?\))
                   (setq depth (1- depth))
                   (when (= depth 0)
                     (setq found (point))))))
               (forward-char 1))
             found))))
    ;; Find matching close for outermost paren at position 1
    (let ((outer-close (funcall find-matching-close 1)))
      ;; Find matching close for inner "(x y)" starting at position 12
      (let ((inner-close (funcall find-matching-close 12)))
        ;; Find matching close for "(+ (* x x) (* y y))" starting at position 18
        (let ((expr-close (funcall find-matching-close 18)))
          (list
           outer-close
           inner-close
           expr-close
           ;; Extract the substring for each matched pair
           (buffer-substring 1 (1+ outer-close))
           (buffer-substring 12 (1+ inner-close))
           (buffer-substring 18 (1+ expr-close))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (37 16 36 \"(defun foo (x y) (+ (* x x) (* y y)))\" \"(x y)\" \"(+ (* x x) (* y y))\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
