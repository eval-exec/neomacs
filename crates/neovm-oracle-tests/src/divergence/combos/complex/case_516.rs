/// Batch 516: pcase deep pattern matching, cl-loop more complex clauses.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx516_pcase_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"string: hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase "hello"
  ((and (pred stringp) s) (format "string: %s" s))
  (_ "other"))
"##,
        expect,
    );
}

#[test]
fn div_cx516_pcase_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase [1 2 3]
  (`[,a ,b ,c] (+ a b c))
  (_ 0))
"##,
        expect,
    );
}

#[test]
fn div_cx516_pcase_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (c b a)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase '(a b c)
  (`(,a ,b ,c) (list c b a))
  (_ nil))
"##,
        expect,
    );
}

#[test]
fn div_cx516_pcase_or() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :answer""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase 42
  ((or 0 1 2) :small)
  ((or 42 43) :answer)
  (_ :other))
"##,
        expect,
    );
}

#[test]
fn div_cx516_pcase_app() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"sqrt: 5.0\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase 25
  ((and (pred numberp)
        (app sqrt (and (pred numberp) val)))
   (format "sqrt: %.1f" val))
  (_ "none"))
"##,
        expect,
    );
}

#[test]
fn div_cx516_pcase_cl_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'cl-lib)
  (cl-defstruct cx516-point x y)
  (let ((p (make-cx516-point :x 3 :y 4)))
    (pcase p
      ((cl-struct cx516-point x y) (+ x y))
      (_ nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx516_cl_loop_finish() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-loop for i from 1 to 10
           when (> i 5) return i
           finally return 0)
"##,
        expect,
    );
}

#[test]
fn div_cx516_cl_loop_counting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-loop for i in '(1 2 3 4 5)
           counting (oddp i))
"##,
        expect,
    );
}

#[test]
fn div_cx516_pcase_guard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :high""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((x 5))
  (pcase x
    ((guard (> x 3)) :high)
    (_ :low)))
"##,
        expect,
    );
}

#[test]
fn div_cx516_pcase_pred() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :matched""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase '(2 . 4)
  ((and (pred consp)
        (app car a)
        (app cdr b)
        (guard (= a 2)))
   :matched)
  (_ :no))
"##,
        expect,
    );
}

#[test]
fn div_cx516_cl_loop_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-loop for i from 0 to 10 by 3 collect i)
"##,
        expect,
    );
}

#[test]
fn div_cx516_cl_loop_unless() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-loop for i in '(1 2 3 4 5)
           unless (oddp i) collect i)
"##,
        expect,
    );
}

#[test]
fn div_cx516_cl_loop_thereis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-loop for i in '(1 3 5 2 4)
           thereis (< i 3))
"##,
        expect,
    );
}

#[test]
fn div_cx516_cl_loop_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-loop named cx516-loop
           for i from 1 to 10
           do (when (> i 3) (return-from cx516-loop i)))
"##,
        expect,
    );
}

#[test]
fn div_cx516_cl_loop_multiple_for() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-loop for i in '(a b c)
           for j from 1
           collect (cons j i))
"##,
        expect,
    );
}
