use crate::common::{assert_oracle_parity_expect, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn oracle_gdl_integer_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"
(progn (require 'bytecomp) (list

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


(let* ((x (expt 2 200))
       (negative-x (- x))
       (y (1+ (expt 2 199)))
       (multiply (byte-compile (lambda (a b) (* a b))))
       (add (byte-compile (lambda (a b) (+ a b))))
       (subtract (byte-compile (lambda (a b) (- a b))))
       (negate (byte-compile (lambda (a) (- a)))))
  (let ((integer-width 128))
    (list
      (list 'fresh
        (condition-case e (+ x 1) (error e))
        (condition-case e (- x 1) (error e))
        (condition-case e (* x 1) (error e))
        (condition-case e (/ x 3) (error e))
        (condition-case e (% x y) (error e))
        (condition-case e (mod x y) (error e))
        (condition-case e (1+ x) (error e))
        (condition-case e (1- x) (error e))
        (condition-case e (abs negative-x) (error e))
        (condition-case e (logand x x) (error e))
        (condition-case e (logior x 0) (error e))
        (condition-case e (logxor x 1) (error e))
        (condition-case e (lognot x) (error e))
        (condition-case e (ash x -1) (error e))
        (condition-case e (expt x 1) (error e))
        (condition-case e (random x) (error e))
        (condition-case e (truncate x 1) (error e))
        (condition-case e (floor x 1) (error e))
        (condition-case e (ceiling x 1) (error e))
        (condition-case e (round x 1) (error e))
        (condition-case e (truncate 1e100) (error e))
        (condition-case e (floor 1e100) (error e))
        (condition-case e (ceiling 1e100) (error e))
        (condition-case e (round 1e100) (error e)))
      (list 'identities
        (eq (+ x) x) (eq (* x) x)
        (eq (logand x) x) (eq (logior x) x) (eq (logxor x) x)
        (eq (abs x) x) (eq (ash x 0) x)
        (eq (truncate x) x) (eq (floor x) x)
        (eq (ceiling x) x) (eq (round x) x)
        (eq (truncate x nil) x) (eq (floor x nil) x)
        (eq (ceiling x nil) x) (eq (round x nil) x))
      (list 'compiled
        (condition-case e (funcall multiply x 1) (error e))
        (condition-case e (funcall multiply x 1) (error e))
        (condition-case e (funcall add x 1) (error e))
        (condition-case e (funcall add x 1) (error e))
        (condition-case e (funcall subtract x 1) (error e))
        (condition-case e (funcall subtract x 1) (error e))
        (condition-case e (funcall negate x) (error e))
        (condition-case e (funcall negate x) (error e))))))
))
"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[
            r#""OK (((overflow-error) ((overflow-error) (overflow-error) (overflow-error) \"10000000000000000159028911097599180468360808563945281389781327557747838772170381060813469985856815104\" (overflow-error) (overflow-error) 127 t t t t t t (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error))) ((fresh (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error)) (identities t t t t t t t t t t t t t t t) (compiled (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error) (overflow-error))))""#
        ]],
    );
}

#[test]
fn oracle_gdl_integer_width_compiled_signal_hook_collects() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((hits nil)
      (part (expt 2 100))
      (fn (byte-compile (lambda (a b) (* a b)))))
  ;; Establish bignum operand feedback and enter the generic compiled path
  ;; before narrowing the result limit and collecting from its signal hook.
  (dotimes (_ 32) (funcall fn part part))
  (let ((integer-width 128)
        (signal-hook-function
         (lambda (symbol data)
           (when (eq symbol 'overflow-error)
             (push symbol hits)
             (garbage-collect)))))
    (list (condition-case e (funcall fn part part) (error e))
          hits)))
"#;
    assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK ((overflow-error) (overflow-error))""#]],
    );
}
