//! Complex combo batch 162 — `cl` / `cl-lib` deep iteration constructs:
//! cl-do, cl-do*, cl-declare, cl-destructuring-bind with &key/&rest,
//! cl-flet with mutual recursion.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx162_cl_do_basic_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-do)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (acc)
  (cl-do ((i 0 (1+ i)))
      ((>= i 5))
    (push i acc))
  (nreverse acc))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_do_with_multiple_vars_and_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-do)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (acc)
  (cl-do ((i 0 (1+ i))
          (j 10 (1- j)))
      ((>= i 5))
    (push (cons i j) acc))
  (nreverse acc))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_do_star_with_dependencies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-do*)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (acc)
  (cl-do* ((i 0 (1+ i))
           (j (* i 2) (* i 2)))
      ((>= i 5))
    (push (cons i j) acc))
  (nreverse acc))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_destructuring_bind_with_key_rest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-destructuring-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-destructuring-bind (a b &key c (d 99)) '(:c 3) (list a b c d)))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_destructuring_bind_with_whole() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-destructuring-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-destructuring-bind (&whole whole a b) '(1 2)
  (list whole a b))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_flet_basic_local_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-flet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-flet ((double (x) (* x 2))
          (triple (x) (* x 3)))
  (list (double 5) (triple 5) (+ (double 1) (triple 1))))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_flet_with_mutual_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-flet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-flet ((even-cx162? (n) (if (= n 0) t (odd-cx162? (1- n))))
          (odd-cx162? (n) (if (= n 0) nil (even-cx162? (1- n)))))
  (list (even-cx162? 10) (odd-cx162? 7) (even-cx162? 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_labels_with_closure_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((counter 0))
    (cl-labels ((inc-and-get () (cl-incf counter)))
      (list (inc-and-get)
            (inc-and-get)
            (inc-and-get)
            counter))))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_macrolet_local_macro_definition() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-macrolet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-macrolet ((swap (a b)
                `(cl-rotatef ,a ,b)))
  (let ((x 1) (y 2))
    (swap x y)
    (list x y)))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_symbol_macrolet_global_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-symbol-macrolet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-symbol-macrolet ((x (list :expanded)))
  (list x x x)))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_macrolet_with_complex_body() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-macrolet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-macrolet ((with-trace (form)
                `(prog1 ,form (message "done"))))
  (let ((x 5))
    (with-trace (cl-incf x))
    x))
"##,
        expect,
    );
}

#[test]
fn div_cx162_cl_do_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-do)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (acc)
  (cl-do ((i 0 (1+ i))) ((>= i 5))
    (push (* i i) acc))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (mapconcat #'number-to-string (nreverse acc) ", "))
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (buffer-string)
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
