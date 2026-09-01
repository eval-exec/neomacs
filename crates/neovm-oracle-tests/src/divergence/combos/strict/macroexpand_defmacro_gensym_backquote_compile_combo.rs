//! Strict combo oracle probes, batch 210: macroexpansion + defmacro. macroexpand
//! over built-in macros (when/unless/and/or/push/pop), custom defmacro with
//! gensym hygiene, backquote-splice in macros, and macroexpansion of cl-incf/
//! push to their expansions.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_macroexpand_builtin_macros() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (macroexpand '(when t 'yes))
      (macroexpand '(unless nil 'yes))
      (macroexpand '(and a b c))
      (macroexpand '(or a b c))
      (macroexpand '(push x lst))
      (macroexpand '(pop lst))
      (macroexpand '(setq a 1 b 2))
      (macroexpand '(prog1 a b c)))
"##;
    let expect = expect_test::expect![[
        r#""OK ((if t (progn 'yes)) (if nil nil 'yes) (and a b c) (or a b c) (setq lst (cons x lst)) (car-safe (prog1 lst (setq lst (cdr lst)))) (setq a 1 b 2) (prog1 a b c))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_defmacro_custom_gensym_hygiene() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defmacro probe-swap (a b)
    (let ((tmp (make-symbol "tmp")))
      `(let ((,tmp ,a))
         (setq ,a ,b)
         (setq ,b ,tmp))))
  (let ((x 1) (y 2))
    (probe-swap x y)
    (list x y)))
"##;
    let expect = expect_test::expect![[r#""OK (2 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_defmacro_backquote_splice_and_rest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defmacro probe-unless-verbose (cond &rest body)
    `(if ,cond (progn ,@body) 'skipped))
  (defmacro probe-inc (place &optional (n 1))
    `(setq ,place (+ ,place ,n)))
  (let ((counter 0) (flag nil))
    (probe-unless-verbose flag (setq counter 41) (setq counter (1+ counter)))
    (probe-inc counter 5)
    (probe-inc counter)
    (list counter
          (probe-unless-verbose t 'ran))))
"##;
    let expect = expect_test::expect![[
        r#""ERR (invalid-function (closure (t) (place &optional (n 1)) `(setq ,place (+ ,place ,n))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
