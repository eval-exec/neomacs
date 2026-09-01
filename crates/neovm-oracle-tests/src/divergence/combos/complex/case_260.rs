//! Complex combo batch 260 — `cl-incf`/`cl-decf`/`cl-rotatef` on
//! hash-table/cons/vector/aref places, `cl-psetf` parallel
//! assignment, `cl-letf` with multiple places simultaneously.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx260_cl_incf_on_hash_table_via_letf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-decf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "counter" 0 ht)
  (cl-incf (gethash "counter" ht 0))
  (cl-incf (gethash "counter" ht 0) 10)
  (cl-decf (gethash "counter" ht 0) 3)
  (list (gethash "counter" ht)))
"##,
        expect,
    )
}

#[test]
fn div_cx260_cl_psetf_parallel_assignment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-psetf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a 1) (b 2) (c 3))
  (cl-psetf a b b c c a)
  (list a b c))
"##,
        expect,
    )
}

#[test]
fn div_cx260_cl_rotatef_on_vector_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [1 2 3 4 5]))
  (cl-rotatef (aref v 0) (aref v 2) (aref v 4))
  (list v))
"##,
        expect,
    )
}

#[test]
fn div_cx260_cl_shiftf_chain_on_list_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-shiftf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst (list 1 2 3 4 5)))
  (cl-shiftf (car lst) (cadr lst) (caddr lst) 99)
  lst)
"##,
        expect,
    )
}

#[test]
fn div_cx260_cl_letf_with_multiple_places() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v1 (vector 0))
      (v2 (vector 0))
      (sym-val 0))
  (cl-letf (((aref v1 0) 10)
            ((aref v2 0) 20)
            (sym-val 30))
    (list (aref v1 0) (aref v2 0) sym-val))
  (list (aref v1 0) (aref v2 0) sym-val))
"##,
        expect,
    )
}

#[test]
fn div_cx260_cl_setf_on_plist_with_getf_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ cl-getf\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (list :a 1 :b 2 :c 3)))
  (setf (cl-getf p :a) 100)
  (setf (cl-getf p :d) 400)
  (cl-remf p :b)
  (list p (cl-getf p :a) (cl-getf p :b :missing) (cl-getf p :d)))
"##,
        expect,
    )
}

#[test]
fn div_cx260_cl_pushnew_with_test_and_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-pushnew)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst '((1 . "a") (2 . "b"))))
  (cl-pushnew '(1 . "x") lst :key #'car)
  (cl-pushnew '(3 . "c") lst :key #'car)
  (cl-pushnew 5 lst :test (lambda (a b) nil))
  lst)
"##,
        expect,
    )
}

#[test]
fn div_cx260_cl_letf_star_dependency_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf*)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [0 0 0 0]))
  (cl-letf* (((aref v 0) 1)
             ((aref v 1) (1+ (aref v 0)))
             ((aref v 2) (+ (aref v 0) (aref v 1)))
             ((aref v 3) (* (aref v 0) (aref v 1) (aref v 2))))
    (append v nil)))
"##,
        expect,
    )
}

#[test]
fn div_cx260_cl_setf_on_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"BYE!o World\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "Hello World")
      (setf (buffer-substring 1 5) "BYE!")
      (buffer-string))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx260_cl_setf_places_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [1 2 3 4 5])
      (p (list :a 1 :b 2))
      (ht (make-hash-table :test 'equal)))
  (puthash "counter" 0 ht)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "setf/letf/incf mega test buffer content")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (cl-rotatef (aref v 0) (aref v 2) (aref v 4))
      (cl-incf (gethash "counter" ht 0) 5)
      (setf (cl-getf p :a) 99)
      (let ((state (list v p ht
                         (gethash "counter" ht)
                         (cl-getf p :a)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
