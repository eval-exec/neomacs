//! Language-feature combo parity: calc-eval/math, pcase (destructure,
//! guards, pcase-let, exhaustive), EIEIO/cl-generic (defclass, inheritance,
//! next-method, eql specializers, struct :include), keyboard macros / kbd /
//! key lookup, and byte-compilation (lambdas, closures, recursion,
//! macroexpand).

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn calc_eval_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"5\" \"70\" \"1024\" \"4\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calc)
(list (calc-eval "2+3") (calc-eval "10*7") (calc-eval "2^10") (calc-eval "sqrt(16)"))"##,
        expect,
    );
}

#[test]
fn calc_eval_frac_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"0.333333\" \"3.14286\" \"6.28318\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calc)
(let ((calc-float-format '(float 6)))
  (list (calc-eval "1/3") (calc-eval "22/7") (calc-eval "3.14159*2")))"##,
        expect,
    );
}

#[test]
fn calc_eval_funcs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"12\" \"720\" \"1 mod 3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calc)
(list (calc-eval "gcd(48,36)") (calc-eval "fact(6)") (calc-eval "10 mod 3"))"##,
        expect,
    );
}

#[test]
fn math_read_number() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function math-integerp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'calc)
(list (math-zerop 0) (math-integerp 5) (calc-eval "deg(pi)" ))"##,
        expect,
    );
}

#[test]
fn cl_case_pcase_mix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (b 49)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(list (pcase-exhaustive 2 (1 'a) (2 'b) (3 'c))
      (cl-flet ((sq (n) (* n n))) (sq 7)))"##,
        expect,
    );
}

#[test]
fn pcase_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (int 3 greet)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (pcase 5 (1 'one) ((pred integerp) 'int) (_ 'other))
        (pcase '(1 2) (`(,a ,b) (+ a b)))
        (pcase "hi" ((or "hello" "hi") 'greet) (_ 'no)))"##,
        expect,
    );
}

#[test]
fn pcase_destructure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase '(add 3 4)
  (`(add ,x ,y) (+ x y))
  (`(sub ,x ,y) (- x y)))"##,
        expect,
    );
}

#[test]
fn pcase_guards_pred() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (big (2 . 1) (3 2 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (pcase 10 ((and n (guard (> n 5))) 'big) (_ 'small))
        (pcase '(1 . 2) (`(,a . ,b) (cons b a)))
        (pcase [1 2 3] (`[,a ,b ,c] (list c b a))))"##,
        expect,
    );
}

#[test]
fn pcase_let_seq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 10 (20 30))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(pcase-let ((`(,a ,b ,c) '(1 2 3))
            (`(,x . ,y) '(10 20 30)))
  (list a b c x y))"##,
        expect,
    );
}

#[test]
fn cl_generic_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"int:5\" \"str:hi\" \"zero\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-defgeneric neo-desc (x))
(cl-defmethod neo-desc ((x integer)) (format "int:%d" x))
(cl-defmethod neo-desc ((x string)) (format "str:%s" x))
(cl-defmethod neo-desc ((x (eql 0))) "zero")
(list (neo-desc 5) (neo-desc "hi") (neo-desc 0))"##,
        expect,
    );
}

#[test]
fn cl_struct_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12 2 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-defstruct neo-shape area)
(cl-defstruct (neo-circle (:include neo-shape)) radius)
(let ((c (make-neo-circle :area 12 :radius 2)))
  (list (neo-shape-area c) (neo-circle-radius c) (neo-shape-p c) (cl-typep c 'neo-shape)))"##,
        expect,
    );
}

#[test]
fn eieio_defclass() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"cat\" \"meow\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'eieio)
(defclass neo-animal () ((name :initarg :name :accessor neo-name)
                          (sound :initarg :sound :initform "..." :accessor neo-sound)))
(let ((a (neo-animal :name "cat" :sound "meow")))
  (list (neo-name a) (neo-sound a) (object-of-class-p a 'neo-animal) (eieio-object-p a)))"##,
        expect,
    );
}

#[test]
fn eieio_inherit_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 10 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'eieio)
(defclass neo-base () ((v :initarg :v :initform 0)))
(defclass neo-derived (neo-base) ((w :initarg :w :initform 1)))
(cl-defmethod neo-total ((o neo-base)) (oref o v))
(cl-defmethod neo-total ((o neo-derived)) (+ (cl-call-next-method) (oref o w)))
(let ((d (neo-derived :v 10 :w 5)))
  (list (neo-total d) (slot-value d 'v) (child-of-class-p 'neo-derived 'neo-base)))"##,
        expect,
    );
}

#[test]
fn kbd_macro_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"\u{18}\" [f5] [134217729] \"\t\" \" \")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((kmacro-counter 0))
  (list (kbd "C-x") (kbd "<f5>") (kbd "C-M-a") (kbd "TAB") (kbd "SPC")))"##,
        expect,
    );
}

#[test]
fn key_binding_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"C-f\" t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (key-description (where-is-internal 'forward-char global-map t))
        (commandp 'forward-char) (commandp 'car))"##,
        expect,
    );
}

#[test]
fn kmacro_define_run() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"abc\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (execute-kbd-macro (kbd "a b c"))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn bytecomp_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((make-adder (lambda (n) (lambda (x) (+ x n))))
        (add5 (byte-compile (funcall make-adder 5))))
  (list (funcall add5 10) (funcall add5 -3)))"##,
        expect,
    );
}

#[test]
fn bytecomp_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (55 610 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(defun neo-fib (n) (if (< n 2) n (+ (neo-fib (- n 1)) (neo-fib (- n 2)))))
(byte-compile 'neo-fib)
(list (neo-fib 10) (neo-fib 15) (byte-code-function-p (symbol-function 'neo-fib)))"##,
        expect,
    );
}

#[test]
fn bytecomp_run_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (49 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (byte-compile (lambda (x) (* x x)))))
  (list (funcall f 7) (byte-code-function-p f)))"##,
        expect,
    );
}

#[test]
fn eval_dynamic_lexical() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((closure ((x . 1)) nil x) 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (let ((lexical-binding t)) (funcall (eval '(lambda () (let ((x 1)) (lambda () x))) t)))
        (funcall (funcall (eval '(lambda (n) (lambda () n)) t) 42)))"##,
        expect,
    );
}

#[test]
fn macroexpand_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((if t (progn 1 2)) (incf x) (and a (or b c)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (macroexpand '(when t 1 2))
        (macroexpand-1 '(cl-incf x))
        (macroexpand-all '(and a (or b c))))"##,
        expect,
    );
}
