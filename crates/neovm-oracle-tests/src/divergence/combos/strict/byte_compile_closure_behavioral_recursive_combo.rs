//! Strict combo oracle probes, batch 172: byte-compiler (behavioral). byte-
//! compile a lambda and funcall it, recursive compiled function, closure over
//! lexical let, compiled-function-p, and byte-compile a conditional/closure
//! then exercise it. (We do NOT prin1 the byte-code object #^[...] since that
//! is implementation-specific representation.)
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_byte_compile_lambda_funcall_compiled_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((f (byte-compile (lambda (x) (* x 2)))))
  (list (compiled-function-p f)
        (funcall f 21)
        (funcall f 0)
        (funcall (byte-compile (lambda (a b) (- (* a 3) b))) 4 1)
        (funcall (byte-compile (lambda () (+ 1 2))))))
"##;
    let expect = expect_test::expect![[r#""OK (t 42 0 11 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_byte_compile_recursive_closure_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((fact-src (lambda (n) (if (<= n 1) 1 (* n (funcall fact-src (1- n))))))
       (fact (byte-compile fact-src)))
  (list (funcall fact 5)
        (funcall fact 1)
        (funcall fact 10)
        (let ((counter 0))
          (let ((inc (byte-compile (lambda () (setq counter (1+ counter))))))
            (funcall inc)
            (funcall inc)
            (funcall inc)
            counter))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable fact-src)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_byte_compile_conditional_mapcar_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((classify (byte-compile
                 (lambda (x)
                   (cond ((numberp x) 'num)
                         ((stringp x) 'str)
                         ((listp x) 'list)
                         (t 'other))))))
  (list (mapcar classify '(42 "hi" (a b) foo))
        (funcall (byte-compile (lambda (lst) (apply #'+ lst))) '(1 2 3 4))
        (funcall (byte-compile (lambda (s) (upcase s))) "hello")
        (funcall (byte-compile (lambda (n) (cl-loop for i below n sum i))) 5)
        (compiled-function-p (byte-compile (lambda (x) x)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable for)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
