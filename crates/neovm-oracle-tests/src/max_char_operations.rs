//! Oracle parity tests for `max-char` and related character boundary operations.
//!
//! Tests max-char return value, unicode argument, characterp boundaries,
//! loop bounds using max-char, and Unicode range classification.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic max-char return value and type
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_max_char_basic_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-analyze-max-char
    (lambda ()
      (let* ((mc (max-char))
             (mc-type (type-of mc))
             (mc-is-int (integerp mc))
             (mc-positive (> mc 0))
             (mc-gt-ascii (> mc 127))
             (mc-gt-unicode (>= mc #x10FFFF)))
        (list mc mc-type mc-is-int mc-positive mc-gt-ascii mc-gt-unicode))))
  (unwind-protect
      (funcall 'neovm--test-analyze-max-char)
    (fmakunbound 'neovm--test-analyze-max-char)))"#;
    let expect = expect_test::expect![[r#""OK (4194303 integer t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// max-char with UNICODE argument
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_max_char_unicode_argument() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-max-char-unicode
    (lambda ()
      (let* ((mc-default (max-char))
             (mc-unicode (max-char t))
             (mc-no-unicode (max-char nil))
             (unicode-is-smaller (<= mc-unicode mc-default))
             (unicode-eq-10ffff (= mc-unicode #x10FFFF))
             (default-eq-no-unicode (= mc-default mc-no-unicode)))
        (list mc-default mc-unicode mc-no-unicode
              unicode-is-smaller unicode-eq-10ffff
              default-eq-no-unicode))))
  (unwind-protect
      (funcall 'neovm--test-max-char-unicode)
    (fmakunbound 'neovm--test-max-char-unicode)))"#;
    let expect = expect_test::expect![[r#""OK (4194303 1114111 4194303 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// characterp boundary testing around max-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_max_char_characterp_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-char-boundary
    (lambda ()
      (let* ((mc (max-char))
             (mc-unicode (max-char t))
             ;; Test characterp at various boundaries
             (zero-is-char (characterp 0))
             (one-is-char (characterp 1))
             (ascii-max-is-char (characterp 127))
             (latin1-max-is-char (characterp 255))
             (bmp-max-is-char (characterp #xFFFF))
             (unicode-max-is-char (characterp #x10FFFF))
             (mc-is-char (characterp mc))
             ;; One past max-char should NOT be a character
             (past-mc-is-char (characterp (1+ mc)))
             ;; Negative should NOT be a character
             (neg-is-char (characterp -1))
             ;; Non-integer should NOT be a character
             (float-is-char (characterp 65.0))
             (string-is-char (characterp "A"))
             (nil-is-char (characterp nil)))
        (list zero-is-char one-is-char ascii-max-is-char
              latin1-max-is-char bmp-max-is-char unicode-max-is-char
              mc-is-char past-mc-is-char neg-is-char
              float-is-char string-is-char nil-is-char))))
  (unwind-protect
      (funcall 'neovm--test-char-boundary)
    (fmakunbound 'neovm--test-char-boundary)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Using max-char as upper bound in counting loops
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_max_char_loop_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Count how many characters in a range satisfy a predicate
  ;; We test near the boundaries of max-char, not the full range
  (fset 'neovm--test-count-chars-in-range
    (lambda (start end pred)
      (let ((count 0) (i start))
        (while (<= i end)
          (when (funcall pred i)
            (setq count (1+ count)))
          (setq i (1+ i)))
        count)))

  ;; Classify characters near max-char boundary
  (fset 'neovm--test-boundary-analysis
    (lambda ()
      (let* ((mc (max-char))
             (mc-uni (max-char t))
             ;; Count valid chars in last 10 before max-char
             (valid-near-end
              (funcall 'neovm--test-count-chars-in-range
                       (- mc 9) mc #'characterp))
             ;; Count valid chars just past max-char (should be 0)
             (valid-past-end
              (funcall 'neovm--test-count-chars-in-range
                       (1+ mc) (+ mc 10) #'characterp))
             ;; Count chars in last 5 before unicode max
             (valid-near-uni-end
              (funcall 'neovm--test-count-chars-in-range
                       (- mc-uni 4) mc-uni #'characterp))
             ;; Build a list of char-or-nil for the boundary region
             (boundary-list
              (let ((result nil) (i (- mc 2)))
                (while (<= i (+ mc 2))
                  (setq result (cons (characterp i) result))
                  (setq i (1+ i)))
                (nreverse result))))
        (list valid-near-end valid-past-end
              valid-near-uni-end boundary-list))))
  (unwind-protect
      (funcall 'neovm--test-boundary-analysis)
    (fmakunbound 'neovm--test-count-chars-in-range)
    (fmakunbound 'neovm--test-boundary-analysis)))"#;
    let expect = expect_test::expect![[r#""OK (10 0 5 (t t t nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Unicode range classification using max-char
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_max_char_unicode_range_classification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Classify a character into its Unicode plane or internal range
  (fset 'neovm--test-classify-char
    (lambda (ch)
      (cond
        ((not (characterp ch)) 'not-a-char)
        ((< ch #x80) 'ascii)
        ((< ch #x100) 'latin-supplement)
        ((< ch #x10000) 'bmp)
        ((<= ch #x10FFFF) 'supplementary)
        ((<= ch (max-char)) 'emacs-internal)
        (t 'out-of-range))))

  ;; Build a char-table mapping from sample chars to their classification
  (fset 'neovm--test-build-classification-table
    (lambda (chars)
      (let ((tbl (make-char-table 'neovm--test-cls nil)))
        (dolist (ch chars)
          (when (characterp ch)
            (set-char-table-range tbl ch
              (funcall 'neovm--test-classify-char ch))))
        tbl)))

  ;; Extract classifications for a set of sample characters
  (fset 'neovm--test-run-classification
    (lambda ()
      (let* ((mc (max-char))
             (mc-uni (max-char t))
             (samples (list 0 ?A ?z #x7F #x80 #xFF #x100
                            #xFFFF #x10000 #x10FFFF))
             ;; Only include mc if it's different from #x10FFFF
             (samples (if (> mc #x10FFFF)
                          (append samples (list mc))
                        samples))
             (tbl (funcall 'neovm--test-build-classification-table samples))
             ;; Read back from table
             (results (mapcar
                       (lambda (ch)
                         (if (characterp ch)
                             (cons ch (char-table-range tbl ch))
                           (cons ch 'invalid)))
                       samples))
             ;; Also verify consistency: classify directly vs table lookup
             (consistent
              (let ((ok t))
                (dolist (ch samples)
                  (when (characterp ch)
                    (unless (eq (funcall 'neovm--test-classify-char ch)
                                (char-table-range tbl ch))
                      (setq ok nil))))
                ok)))
        (list results consistent
              (funcall 'neovm--test-classify-char -1)
              (funcall 'neovm--test-classify-char (1+ mc))))))
  (unwind-protect
      (funcall 'neovm--test-run-classification)
    (fmakunbound 'neovm--test-classify-char)
    (fmakunbound 'neovm--test-build-classification-table)
    (fmakunbound 'neovm--test-run-classification)))"#;
    let expect = expect_test::expect![[
        r#""OK (((0 . ascii) (65 . ascii) (122 . ascii) (127 . ascii) (128 . latin-supplement) (255 . latin-supplement) (256 . bmp) (65535 . bmp) (65536 . supplementary) (1114111 . supplementary) (4194303 . emacs-internal)) t not-a-char not-a-char)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// max-char arithmetic and comparisons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_max_char_arithmetic_comparisons() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  (fset 'neovm--test-max-char-math
    (lambda ()
      (let* ((mc (max-char))
             (mc-uni (max-char t))
             ;; Arithmetic with max-char
             (doubled (* mc 2))
             (halved (/ mc 2))
             (diff (- mc mc-uni))
             ;; Comparison chain
             (chain-valid (and (< 0 mc-uni)
                               (<= mc-uni mc)
                               (> mc 0)))
             ;; Use max-char in min/max
             (m1 (min mc 100))
             (m2 (max mc 100))
             ;; mod and remainder
             (mc-mod-256 (mod mc 256))
             (mc-mod-65536 (mod mc 65536))
             ;; logand with max-char to extract bits
             (low-byte (logand mc #xFF))
             (high-bits (ash mc -16)))
        (list doubled halved diff chain-valid
              m1 m2 mc-mod-256 mc-mod-65536
              low-byte high-bits))))
  (unwind-protect
      (funcall 'neovm--test-max-char-math)
    (fmakunbound 'neovm--test-max-char-math)))"#;
    let expect =
        expect_test::expect![[r#""OK (8388606 2097151 3080192 t 100 4194303 255 65535 255 63)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// max-char in char-table operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_max_char_char_table_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(progn
  ;; Use max-char to set up char-table ranges that cover the full space
  (fset 'neovm--test-char-table-full-range
    (lambda ()
      (let* ((mc (max-char))
             (mc-uni (max-char t))
             (tbl (make-char-table 'neovm--test-ctr 'default))
             ;; Set specific ranges
             (_ (set-char-table-range tbl '(0 . 127) 'ascii-range))
             (_ (set-char-table-range tbl '(128 . 255) 'latin-range))
             ;; Query at specific points
             (at-zero (char-table-range tbl 0))
             (at-65 (char-table-range tbl 65))
             (at-128 (char-table-range tbl 128))
             (at-255 (char-table-range tbl 255))
             (at-256 (char-table-range tbl 256))
             ;; Set at max unicode char
             (_ (when (characterp mc-uni)
                  (set-char-table-range tbl mc-uni 'at-max-unicode)))
             (at-mc-uni (when (characterp mc-uni)
                          (char-table-range tbl mc-uni)))
             ;; Check that out-of-ascii-range returns default
             (at-1000 (char-table-range tbl 1000)))
        (list at-zero at-65 at-128 at-255 at-256
              at-mc-uni at-1000))))
  (unwind-protect
      (funcall 'neovm--test-char-table-full-range)
    (fmakunbound 'neovm--test-char-table-full-range)))"#;
    let expect = expect_test::expect![[
        r#""OK (ascii-range ascii-range latin-range latin-range default at-max-unicode default)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
