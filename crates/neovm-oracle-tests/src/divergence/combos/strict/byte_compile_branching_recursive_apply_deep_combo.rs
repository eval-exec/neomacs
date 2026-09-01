//! Strict combo oracle probes, batch 315: byte-compile branching / recursion /
//! apply deep. Conditional dispatch, recursive compiled factorial, apply on
//! compiled function, and compiled-function-p + funcall identity.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_byte_compile_conditional_dispatch_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((classify (byte-compile
                 (lambda (x)
                   (cond ((null x) 'nil)
                         ((numberp x) 'num)
                         ((stringp x) 'str)
                         ((consp x) 'cons)
                         (t 'other))))))
  (list (compiled-function-p classify)
        (mapcar classify '(nil 42 "hi" (a b) foo))
        (apply classify '(99))
        (funcall classify "test")))
"##;
    let expect = expect_test::expect![[r#""OK (t (nil num str cons other) num str)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_byte_compile_recursive_closure_lexical_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((counter 0)
       (inc (byte-compile (lambda () (setq counter (1+ counter))))))
  (list (funcall inc)
        (funcall inc)
        (funcall inc)
        counter
        (let ((f (byte-compile (lambda (n acc) (if (= n 0) acc (funcall f (1- n) (* acc n)))))))
          (funcall f 5 1))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable f)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_byte_compile_loop_mapcar_let_dynamic_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((f (byte-compile (lambda (lst)
                         (let ((total 0))
                           (dolist (x lst total)
                             (setq total (+ total x))))))))
  (list (funcall f '(1 2 3 4 5))
        (funcall f '(10 20 30))
        (compiled-function-p f)
        (funcall (byte-compile (lambda (s) (upcase s))) "hello")))
"##;
    let expect = expect_test::expect![[r#""OK (15 60 t \"HELLO\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
