//! Complex combo batch 171 — `cl-incf`/`cl-decf`/`cl-shiftf`/`cl-rotatef`
//! / `cl-letf` on places, `setf` of plist-get/aref/nth/car/cdr.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx171_cl_incf_decf_with_places() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-decf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (vector 1 2 3 4 5))
      (lst (list 10 20 30 40)))
  (cl-incf (aref v 0))
  (cl-incf (aref v 1) 10)
  (cl-decf (aref v 2))
  (cl-decf (aref v 3) 5)
  (cl-incf (car lst))
  (cl-incf (nth 1 lst) 100)
  (cl-decf (nth 3 lst))
  (list v lst))
"##,
        expect,
    );
}

#[test]
fn div_cx171_cl_rotatef_three_places_in_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [1 2 3 4 5]))
  (cl-rotatef (aref v 0) (aref v 2) (aref v 4))
  v)
"##,
        expect,
    );
}

#[test]
fn div_cx171_cl_shiftf_chain_through_places() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-shiftf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [1 2 3 4 5]))
  (cl-shiftf (aref v 0) (aref v 1) (aref v 2) (aref v 3) 99)
  (list v))
"##,
        expect,
    );
}

#[test]
fn div_cx171_setf_on_plist_getf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ cl-getf\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (list :a 1 :b 2 :c 3)))
  (setf (cl-getf p :a) 99)
  (setf (cl-getf p :d) 40)
  (list p (cl-getf p :a) (cl-getf p :b) (cl-getf p :d)))
"##,
        expect,
    );
}

#[test]
fn div_cx171_setf_on_car_cdr_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (100 200 300 400)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst (list 1 2 3 4)))
  (setf (car lst) 100)
  (setf (cadr lst) 200)
  (setf (cddr lst) (list 300 400))
  lst)
"##,
        expect,
    );
}

#[test]
fn div_cx171_setf_on_nthcdr_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 2 30 4 50)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst (list 1 2 3 4 5)))
  (setf (nth 0 lst) 10)
  (setf (nth 2 lst) 30)
  (setf (nth 4 lst) 50)
  lst)
"##,
        expect,
    );
}

#[test]
fn div_cx171_cl_letf_with_symbol_function_temp_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((result-list nil))
  (cl-letf (((symbol-function 'neo-cx171-temp-fn)
             (lambda (x) (* x 100))))
    (push (neo-cx171-temp-fn 5) result-list))
  (push (condition-case e (neo-cx171-temp-fn 5) (error (car e))) result-list)
  (nreverse result-list))
"##,
        expect,
    );
}

#[test]
fn div_cx171_cl_letf_with_buffer_local_var_temp_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx171-letf*")))
  (with-current-buffer buf
    (set (make-local-variable 'neo-cx171-local) :orig))
  (with-current-buffer buf
    (cl-letf ((neo-cx171-local :temp-override))
      (list neo-cx171-local
            (buffer-local-value 'neo-cx171-local buf))))
  (prog1 (buffer-local-value 'neo-cx171-local buf)
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx171_cl_letf_star_with_dependencies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf*)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [1 2 3 4]))
  (cl-letf* (((aref v 0) 10)
             ((aref v 1) (* (aref v 0) 2))
             ((aref v 2) (* (aref v 1) 2)))
    v))
"##,
        expect,
    );
}

#[test]
fn div_cx171_setf_through_indirect_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defsetf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [1 2 3 4]))
  (cl-defsetf neo-cx171-access (vec idx)
    (store)
    `(aset ,vec ,idx ,store))
  (setf (neo-cx171-access v 0) 100)
  v)
"##,
        expect,
    );
}

#[test]
fn div_cx171_setf_on_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ get-text-property\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello World")
  (setf (get-text-property 1 'face) 'bold)
  (add-text-properties 1 6 '(face bold weight heavy))
  (setf (get-text-property 1 'face) 'italic)
  (list (text-properties-at 1)
        (text-properties-at 5)
        (text-properties-at 7)))
"##,
        expect,
    );
}

#[test]
fn div_cx171_setf_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v [1 2 3 4 5])
      (p (list :a 1 :b 2)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "setf/letf mega test buffer content")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (cl-rotatef (aref v 0) (aref v 2) (aref v 4))
      (setf (cl-getf p :a) 99)
      (let ((state (list v p
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
    );
}
