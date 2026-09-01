//! Oracle parity tests for `char-before` and `preceding-char`:
//! positional lookups, boundary conditions, narrowed buffers,
//! optional POS argument, comparison with `char-after`, and
//! backward scanning patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// char-before at various positions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_before_various_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "abcdefg")
  (list
   ;; char-before at position 2 => char at position 1 => ?a = 97
   (char-before 2)
   ;; char-before at position 4 => char at position 3 => ?c = 99
   (char-before 4)
   ;; char-before at position 7 => char at position 6 => ?f = 102
   (char-before 7)
   ;; char-before at end of buffer (point-max + 1) => nil
   (char-before (1+ (point-max)))
   ;; char-before at last valid pos (point-max) => ?g = 103
   (char-before (point-max))))"#;
    let expect = expect_test::expect![r#""OK (97 99 102 nil 103)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-before with optional POS argument (nil means use point)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_before_optional_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "XYZW")
  (goto-char 3)
  (let ((without-arg (char-before))
        (with-nil-arg (char-before nil))
        (with-explicit (char-before 3)))
    ;; All three should agree: char at position 2 = ?Y = 89
    (list without-arg with-nil-arg with-explicit
          (eq without-arg with-nil-arg)
          (eq with-nil-arg with-explicit))))"#;
    let expect = expect_test::expect![r#""OK (89 89 89 t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_char_before_bignum_position_saturates_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs editfns.c:Fchar_before uses buffer.c:fix_position for
    // explicit integer POS, so huge bignums saturate before the range check.
    let form = r#"(with-temp-buffer
  (insert "abc")
  (char-before 1000000000000000000000000000000))"#;
    let expect = expect_test::expect![r#""OK nil""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-before at beginning of buffer returns nil
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_before_at_beginning() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "hello")
  (goto-char (point-min))
  (let ((at-bob-no-arg (char-before))
        (at-bob-explicit (char-before 1))
        (at-bob-pmin (char-before (point-min))))
    ;; All should be nil because there is no character before position 1
    (list at-bob-no-arg at-bob-explicit at-bob-pmin)))"#;
    let expect = expect_test::expect![r#""OK (nil nil nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// preceding-char: returns 0 at beginning of buffer, char code otherwise
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_preceding_char_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "ABCDE")
  (let ((results nil))
    ;; At end of buffer (after insert, point is at end)
    (setq results (cons (preceding-char) results))  ;; ?E = 69
    ;; Move to position 3
    (goto-char 3)
    (setq results (cons (preceding-char) results))  ;; ?B = 66
    ;; Move to position 1 (beginning of buffer)
    (goto-char 1)
    (setq results (cons (preceding-char) results))  ;; 0
    ;; Move to position 2
    (goto-char 2)
    (setq results (cons (preceding-char) results))  ;; ?A = 65
    (nreverse results)))"#;
    let expect = expect_test::expect![r#""OK (69 66 0 65)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-before vs char-after comparison at same position
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_before_vs_char_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // char-before at position N looks at char at N-1
    // char-after at position N looks at char at N
    // So char-before(N) == char-after(N-1) for valid positions
    let form = r#"(with-temp-buffer
  (insert "abcdef")
  (let ((results nil))
    ;; For each position 2..6, verify char-before(pos) == char-after(pos - 1)
    (dolist (pos '(2 3 4 5 6))
      (let ((cb (char-before pos))
            (ca (char-after (1- pos))))
        (setq results (cons (list pos cb ca (eq cb ca)) results))))
    ;; Also check boundary: char-before(1) is nil, char-after(point-max+1) is nil
    (setq results (cons (list 'before-1 (char-before 1)) results))
    (setq results (cons (list 'after-max (char-after (1+ (point-max)))) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![
        r#""OK ((2 97 97 t) (3 98 98 t) (4 99 99 t) (5 100 100 t) (6 101 101 t) (before-1 nil) (after-max nil))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-before in narrowed buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_before_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "0123456789")
  (let ((results nil))
    ;; Narrow to region 4..8 which is "3456" (positions 4,5,6,7)
    (save-restriction
      (narrow-to-region 4 8)
      ;; point-min is now 4, point-max is now 8
      ;; char-before at point-min => nil (no char before narrowed start)
      (setq results (cons (char-before (point-min)) results))
      ;; char-before at (point-min + 1) => char at point-min => ?3 = 51
      (setq results (cons (char-before (1+ (point-min))) results))
      ;; char-before at point-max => char at (point-max - 1) => ?6 = 54
      (setq results (cons (char-before (point-max)) results))
      ;; preceding-char at point-min => 0
      (goto-char (point-min))
      (setq results (cons (preceding-char) results))
      ;; preceding-char at point-max => ?7 = 55
      (goto-char (point-max))
      (setq results (cons (preceding-char) results))
      ;; buffer-string in narrowed region for context
      (setq results (cons (buffer-string) results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (nil 51 54 0 54 \"3456\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: backward scanning loop using char-before
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_before_backward_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Scan backward from point collecting characters until we hit a space
    // or beginning of buffer, building a "word" in reverse
    let form = r#"(with-temp-buffer
  (insert "the quick brown fox jumps over")
  ;; Point is at end of buffer after insert
  ;; Scan backward to collect the last word
  (let ((chars nil)
        (pos (point)))
    (while (and (> pos (point-min))
                (let ((ch (char-before pos)))
                  (and ch (/= ch ?\s))))
      (setq chars (cons (char-before pos) chars))
      (setq pos (1- pos)))
    ;; chars should be the last word "over" as char codes
    (let ((last-word (apply #'string chars)))
      ;; Now do it again for the second-to-last word
      ;; Skip the space
      (when (and (> pos (point-min))
                 (= (char-before pos) ?\s))
        (setq pos (1- pos)))
      (let ((chars2 nil))
        (while (and (> pos (point-min))
                    (let ((ch (char-before pos)))
                      (and ch (/= ch ?\s))))
          (setq chars2 (cons (char-before pos) chars2))
          (setq pos (1- pos)))
        (let ((second-word (apply #'string chars2)))
          (list last-word second-word
                ;; Also verify: scan from position 10 backward
                (let ((chars3 nil)
                      (p 10))
                  (while (and (> p (point-min))
                              (let ((ch (char-before p)))
                                (and ch (/= ch ?\s))))
                    (setq chars3 (cons (char-before p) chars3))
                    (setq p (1- p)))
                  (apply #'string chars3))))))))"#;
    let expect = expect_test::expect![[r#""OK (\"over\" \"jumps\" \"quick\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-before with multibyte characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_before_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "A\u00e9B\u00fcC")
  ;; Buffer contains: A e-acute B u-umlaut C
  ;; Positions: 1=A, 2=e-acute, 3=B, 4=u-umlaut, 5=C
  (list
   (char-before 2)   ;; ?A = 65
   (char-before 3)   ;; e-acute = 233
   (char-before 4)   ;; ?B = 66
   (char-before 5)   ;; u-umlaut = 252
   ;; preceding-char at various positions
   (progn (goto-char 3) (preceding-char))  ;; 233
   (progn (goto-char 5) (preceding-char))  ;; 252
   ;; Compare with char-after for consistency
   (eq (char-before 3) (char-after 2))
   (eq (char-before 5) (char-after 4))))"#;
    let expect = expect_test::expect![r#""OK (65 233 66 252 233 252 t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building a frequency table by scanning backward with char-before
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_before_frequency_scan() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "abracadabra")
  ;; Scan entire buffer backward using char-before, build frequency alist
  (let ((freq nil)
        (pos (point-max)))
    (while (> pos (point-min))
      (let* ((ch (char-before pos))
             (entry (assq ch freq)))
        (if entry
            (setcdr entry (1+ (cdr entry)))
          (setq freq (cons (cons ch 1) freq))))
      (setq pos (1- pos)))
    ;; Sort by character code for deterministic output
    (let ((sorted (sort freq (lambda (a b) (< (car a) (car b))))))
      ;; Convert char codes to strings for readability
      (list
       ;; Raw frequency alist sorted by char code
       sorted
       ;; Total chars counted
       (apply #'+ (mapcar #'cdr sorted))
       ;; Most frequent char
       (let ((max-entry nil))
         (dolist (e sorted)
           (when (or (null max-entry) (> (cdr e) (cdr max-entry)))
             (setq max-entry e)))
         (cons (char-to-string (car max-entry)) (cdr max-entry)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((97 . 5) (98 . 2) (99 . 1) (100 . 1) (114 . 2)) 11 (\"a\" . 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
