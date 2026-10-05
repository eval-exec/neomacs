use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn oracle_gdl_integer_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(let ((x (expt 2 200)))
  (list
   (condition-case e (expt 2 65536) (error e))
   (let ((integer-width 64))
     (list
      (condition-case e (ash 1 128) (error e))
      (condition-case e (truncate 1e40) (error e))
      (condition-case e (format "%x" 1e100) (error e))
      (format "%d" 1e100)
      (condition-case e (* (ash 1 100) (ash 1 100)) (error e))
      (condition-case e (funcall (byte-compile (lambda (a b) (* a b))) (ash 1 100) (ash 1 100)) (error e))
      (logb (ash 1 127))
      (eq (ash x 0) x)
      (eq (truncate x) x)
      (eq (+ x) x)
      (= (read (number-to-string x)) x)
      (= (string-to-number (number-to-string x)) x)
      (let ((r (random (expt 2 127)))) (and (integerp r) (>= r 0) (< r (expt 2 127))))
      (condition-case e (random x) (error e))
      (condition-case e (- x) (error e))
      (condition-case e (ash x -1) (error e))
      (condition-case e (1+ x) (error e))
      (condition-case e (lognot x) (error e))))))
"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK ((overflow-error) ((overflow-error) (overflow-error) (overflow-error) \"10000000000000000159028911097599180468360808563945281389781327557747838772170381060813469985856815104\" (overflow-error) (overflow-error) 127 t t t t t t (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error)))""#
        ]],
    );
}

#[test]
fn oracle_gdl_integer_width_compiled_signal_hook_collects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((hits nil)
      (fn (byte-compile (lambda (a b) (* a b)))))
  (let ((integer-width 128)
        (signal-hook-function
         (lambda (symbol data)
           (when (eq symbol 'overflow-error)
             (push symbol hits)
             (garbage-collect)))))
    (list (condition-case e (funcall fn (expt 2 100) (expt 2 100)) (error e))
          hits)))
"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK ((overflow-error) (overflow-error))""#]],
    );
}
