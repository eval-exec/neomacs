//! Divergence tests: real control flow behavioral differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_catch_throw_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (99 1 (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (catch 'outer
    (catch 'inner
      (throw 'inner 42))
    99)
  (catch 'outer
    (throw 'outer 1))
  (catch 'tag
    (throw 'tag (list 1 2 3)))) ",
        expect,
    );
}

#[test]
fn divergence_catch_throw_across_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (thrown-value thrown-value)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-throw-fn-xxx ()
    (throw 'done 'thrown-value))
  (list
   (catch 'done (test-throw-fn-xxx))
   (catch 'done
     (unwind-protect
         (test-throw-fn-xxx)
       'cleanup-ran)))) ",
        expect,
    );
}

#[test]
fn divergence_dotimes_dolist_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((4 3 2 1 0) (d c b a) done finished)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (let ((result nil))
    (dotimes (i 5 result)
      (push i result)))
  (let ((result nil))
    (dolist (x '(a b c d) result)
      (push x result)))
  (dotimes (_ 3 'done) nil)
  (dolist (_ '(1 2 3) 'finished) nil)) ",
        expect,
    );
}

#[test]
fn divergence_loop_macro_real() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (cl-loop for i from 1 to 5 collect (* i i))
  (cl-loop for x in '(1 2 3 4 5 6) when (cl-oddp x) collect x)
  (cl-loop for x in '(a b c) for y in '(1 2 3) collect (cons x y))
  (cl-loop with total = 0
           for x in '(1 2 3 4 5)
           do (setq total (+ total x))
           finally return total)
  (cl-loop repeat 3 collect 'x)) ",
        expect,
    );
}

#[test]
fn divergence_cl_block_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-block)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (cl-block done
    (cl-return-from done 42)
    99)
  (cl-block outer
    (cl-block inner
      (cl-return-from outer 'exited))
    'not-reached)) ",
        expect,
    );
}

#[test]
fn divergence_cl_flet_labels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-flet)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (cl-flet ((double (x) (* x 2)))
    (list (double 3) (double 7)))
  (cl-labels ((fact (n) (if (zerop n) 1 (* n (fact (1- n))))))
    (list (fact 0) (fact 1) (fact 5) (fact 10)))) ",
        expect,
    );
}

#[test]
fn divergence_cl_case_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-case)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (cl-case 3
    (1 'one)
    (2 'two)
    (3 'three)
    (otherwise 'other))
  (cl-case 99
    (1 'one)
    (otherwise 'other))
  (cl-case 'banana
    (apple 'fruit-a)
    ((banana cherry) 'fruit-bc))
  (cl-ecase 2
    (1 'one)
    (2 'two))) ",
        expect,
    );
}

#[test]
fn divergence_cl_typecase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-typecase)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (cl-typecase 42
    (string 'str)
    (integer 'int)
    (float 'flt))
  (cl-typecase 3.14
    (string 'str)
    (integer 'int)
    (float 'flt))
  (cl-typecase \"hello\"
    (string 'str)
    (integer 'int)
    (float 'flt))
  (cl-typecase nil
    (null 'null)
    (list 'list))) ",
        expect,
    );
}

#[test]
fn divergence_while_with_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((4 3 2 1 0) 5)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((i 0) (acc nil))
  (while (< i 5)
    (push i acc)
    (setq i (1+ i)))
  (list acc i)) ",
        expect,
    );
}

#[test]
fn divergence_cl_letf_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((x 10))
  (cl-letf (((symbol-value 'x) 99))
    (list x
          (symbol-value 'x)))
  x) ",
        expect,
    );
}
