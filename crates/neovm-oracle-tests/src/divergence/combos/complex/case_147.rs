//! Complex combo batch 147 — `cl` (deprecated) / `cl-lib` / `gv` /
//! `pcase` macros for general reference / place semantics, `define-symbol-prop`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx147_gv_setf_expander_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'gv)
      (gv-define-expander neo-cx147-accessor
        (lambda (place do)
          (funcall do (list 'neo-cx147-get place)
                   (lambda (v) (list 'neo-cx147-set place v)))))
      (list (fboundp 'gv-define-expander)
            (fboundp 'gv-letplace)
            (boundp 'gv-dynamically-lexically-macro-expanded)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx147_cl_setf_with_custom_expander() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (require 'gv)
      (defvar neo-cx147-place (list :a :b :c))
      (gv-define-setter nth
        (lambda (store idx list)
          `(setcar (nthcdr ,idx ,list) ,store)))
      (setf (nth 1 neo-cx147-place) :changed)
      neo-cx147-place)
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx147_cl_getf_setf_on_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ cl-getf\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (list :a 1 :b 2 :c 3)))
  (setf (cl-getf p :a) 10)
  (setf (cl-getf p :d) 40)
  (list p (cl-getf p :a) (cl-getf p :b) (cl-getf p :d)))
"##,
        expect,
    );
}

#[test]
fn div_cx147_cl_rotatef_three_places() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((x 1) (y 2) (z 3))
  (cl-rotatef x y z)
  (list x y z))
"##,
        expect,
    );
}

#[test]
fn div_cx147_cl_shiftf_chain_returns_first_old() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-shiftf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((x 1) (y 2) (z 3))
  (let ((result (cl-shiftf x y z 99)))
    (list result x y z)))
"##,
        expect,
    );
}

#[test]
fn div_cx147_cl_letf_with_simple_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((x 1))
  (cl-letf ((x 99))
    (list x))
  x)
"##,
        expect,
    );
}

#[test]
fn div_cx147_cl_letf_with_setf_place() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((vec (vector 1 2 3)))
  (cl-letf (((aref vec 1) 99))
    (list (aref vec 0) (aref vec 1) (aref vec 2))))
"##,
        expect,
    );
}

#[test]
fn div_cx147_define_symbol_prop_usage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:val)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-symbol-prop 'neo-cx147-sym 'neo-cx147-prop :val)
      (list (get 'neo-cx147-sym 'neo-cx147-prop)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx147_cl_defmacro_complex_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (invalid-function (closure (t) ((a b &optional c) &rest body) `(let ((,a 1) (,b 2) (,c (or ,c 3))) ,@body)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(defmacro neo-cx147-let-complex ((a b &optional c) &rest body)
  `(let ((,a 1) (,b 2) (,c (or ,c 3)))
     ,@body))
(list (macroexpand '(neo-cx147-let-complex (x y) (+ x y)))
      (eval '(neo-cx147-let-complex (x y) (+ x y)) t)
      (eval '(neo-cx147-let-complex (x y z) (+ x y z)) t))
"##,
        expect,
    );
}

#[test]
fn div_cx147_pcase_app_pred_and_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:big-int (:string-of-len 5) :cons :other :other :other)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (v)
          (pcase v
            ((and (pred integerp) (pred (> _ 10))) :big-int)
            ((and (pred stringp) s) (list :string-of-len (length s)))
            (`(,a . ,_) :cons)
            (_ :other)))
        '(42 "hello" (1 2 3) [vec] nil :sym))
"##,
        expect,
    );
}

#[test]
fn div_cx147_pcase_with_map_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ht (make-hash-table :test 'equal)))
      (puthash 'name "alpha" ht)
      (puthash 'age 30 ht)
      (pcase ht
        ((map (:name name) (:age age))
         (list :parsed name age))
        (_ :no-match)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx147_gv_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ cl-getf\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (list :a 1 :b 2 :c 3)))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "gv mega test plist: %S" p))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (setf (cl-getf p :b) 99)
      (let ((state (list p (cl-getf p :b)
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
