//! Oracle parity tests for cross-feature combinations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

const COMBINATION_ORACLE_PROP_CASES: u32 = 4;

#[test]
fn oracle_prop_combination_macro_advice_apply_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--combo-target)
                      (around 'neovm--combo-around))
                  (progn
                    (defmacro neovm--combo-call-twice (f x)
                      (list 'list
                            (list 'funcall f x)
                            (list 'apply f (list 'list x))))
                    (fset target (lambda (x) (+ x 1)))
                    (fset around (lambda (orig x) (* 2 (funcall orig x))))
                    (unwind-protect
                        (progn
                          (advice-add target :around around)
                          (neovm--combo-call-twice target 5))
                      (condition-case nil (advice-remove target around) (error nil))
                      (fmakunbound target)
                      (fmakunbound around)
                      (fmakunbound 'neovm--combo-call-twice))))";
    let expect = expect_test::expect![[r#""OK (12 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_from_advised_function_keeps_log_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--combo-throw-target)
                      (before 'neovm--combo-throw-before)
                      (log nil))
                  (fset target (lambda (x)
                                 (setq log (cons (list 'orig x) log))
                                 (throw 'neovm--combo-tag (+ x 1))))
                  (fset before (lambda (&rest args)
                                 (setq log (cons (cons 'before args) log))))
                  (unwind-protect
                      (progn
                        (advice-add target :before before)
                        (list (catch 'neovm--combo-tag (funcall target 4))
                              (nreverse log)))
                    (condition-case nil (advice-remove target before) (error nil))
                    (fmakunbound target)
                    (fmakunbound before)))";
    let expect = expect_test::expect![[r#""OK (5 ((before 4) (orig 4)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_cleanup_error_overrides_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(condition-case err
                  (catch 'neovm--combo-tag
                    (unwind-protect
                        (throw 'neovm--combo-tag 'ok)
                      (car 1)))
                (wrong-type-argument (car err)))";
    let expect = expect_test::expect![[r#""OK wrong-type-argument""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("wrong-type-argument", &oracle, &neovm);
}

#[test]
fn oracle_prop_combination_macro_expansion_side_effect_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((expands 0))
                  (defmacro neovm--combo-expander (x)
                    (setq expands (1+ expands))
                    `(condition-case nil (car ,x) (wrong-type-argument 'bad)))
                  (unwind-protect
                      (list (neovm--combo-expander '(1 2))
                            (neovm--combo-expander 1)
                            expands)
                    (fmakunbound 'neovm--combo-expander)))";
    let expect = expect_test::expect![[r#""OK (1 bad 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_eval_defmacro_then_expand_and_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (eval '(defmacro neovm--combo-eval-m (op x y) (list op x y)))
                  (unwind-protect
                      (list
                        (macroexpand '(neovm--combo-eval-m + 2 3))
                        (neovm--combo-eval-m + 2 3)
                        (funcall (lambda (f) (neovm--combo-eval-m f 10 4)) '-))
                    (fmakunbound 'neovm--combo-eval-m)))";
    let expect = expect_test::expect![[r#""ERR (void-function f)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_nested_condition_case_throw_and_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((state nil))
                  (list
                    (condition-case err
                        (catch 'neovm--combo-done
                          (unwind-protect
                              (progn
                                (setq state (cons 'enter state))
                                (condition-case nil
                                    (car 1)
                                  (wrong-type-argument
                                   (setq state (cons 'handled state))
                                   (throw 'neovm--combo-done 'thrown)))
                                'tail)
                            (setq state (cons 'cleanup state))))
                      (error (list 'err (car err))))
                    (nreverse state)))";
    let expect = expect_test::expect![[r#""OK (thrown (enter handled cleanup))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macroexpand_env_override_then_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-mx (x) (list '+ x 1))
                  (unwind-protect
                      (list
                        (macroexpand '(neovm--combo-mx 7))
                        (eval (macroexpand '(neovm--combo-mx 7)
                                           '((neovm--combo-mx . (lambda (x) (list '- x 1))))))
                        (eval (macroexpand '(neovm--combo-mx 7))))
                    (fmakunbound 'neovm--combo-mx)))";
    let expect = expect_test::expect![[r#""OK ((+ 7 1) 6 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_apply_with_symbol_function_mutation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((sym 'neovm--combo-fn))
                  (fset sym (lambda (x) (+ x 1)))
                  (unwind-protect
                      (let ((orig (symbol-function sym)))
                        (list
                          (apply sym '(2))
                          (progn
                            (fset sym (lambda (x) (* x 10)))
                            (apply sym '(2)))
                          (apply orig '(2))))
                    (fmakunbound sym)))";
    let expect = expect_test::expect![[r#""OK (3 20 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_nested_unwind_cleanup_stack_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((log nil))
                  (list
                    (catch 'neovm--combo-tag
                      (unwind-protect
                          (unwind-protect
                              (progn
                                (setq log (cons 'body log))
                                (throw 'neovm--combo-tag 'done))
                            (setq log (cons 'inner-clean log)))
                        (setq log (cons 'outer-clean log))))
                    log))";
    let expect = expect_test::expect![[r#""OK (done (outer-clean inner-clean body))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_guards_apply_with_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-guarded-call (fn args)
                    `(condition-case err
                         (apply ,fn ,args)
                       (wrong-type-argument (list 'wta (car err)))
                       (error (list 'err (car err)))))
                  (unwind-protect
                      (list
                        (neovm--combo-guarded-call '+ '(1 2 3))
                        (neovm--combo-guarded-call 'car '(1)))
                    (fmakunbound 'neovm--combo-guarded-call)))";
    let expect = expect_test::expect![[r#""OK (6 (wta wrong-type-argument))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macroexpand_and_filter_return_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-call-target (x) `(neovm--combo-target ,x))
                  (fset 'neovm--combo-target (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-filter-ret (lambda (ret) (* ret 3)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-target :filter-return 'neovm--combo-filter-ret)
                        (list
                          (macroexpand '(neovm--combo-call-target 4))
                          (neovm--combo-call-target 4)))
                    (condition-case nil
                        (advice-remove 'neovm--combo-target 'neovm--combo-filter-ret)
                      (error nil))
                    (fmakunbound 'neovm--combo-target)
                    (fmakunbound 'neovm--combo-filter-ret)
                    (fmakunbound 'neovm--combo-call-target)))";
    let expect = expect_test::expect![[r#""OK ((neovm--combo-target 4) 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_eval_macro_with_lexenv_shadowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-eval-env (sym) sym)
                  (unwind-protect
                      (let ((x 7))
                        (list
                          (eval '(neovm--combo-eval-env x))
                          (eval '(neovm--combo-eval-env x) '((x . 11)))))
                    (fmakunbound 'neovm--combo-eval-env)))";
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_apply_with_filter_chain_and_log() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--combo-chain-target)
                      (fargs 'neovm--combo-chain-fargs)
                      (fret 'neovm--combo-chain-fret)
                      (log nil))
                  (fset target (lambda (a b) (setq log (cons (list 'orig a b) log)) (+ a b)))
                  (fset fargs (lambda (args)
                                (list (1+ (car args))
                                      (1+ (car (cdr args))))))
                  (fset fret (lambda (ret)
                               (setq log (cons (list 'ret ret) log))
                               (* ret 2)))
                  (unwind-protect
                      (progn
                        (advice-add target :filter-args fargs)
                        (advice-add target :filter-return fret)
                        (list (apply target '(2 5)) (nreverse log)))
                    (condition-case nil (advice-remove target fargs) (error nil))
                    (condition-case nil (advice-remove target fret) (error nil))
                    (fmakunbound target)
                    (fmakunbound fargs)
                    (fmakunbound fret)))";
    let expect = expect_test::expect![[r#""OK (18 ((orig 3 6) (ret 9)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_generated_unwind_with_nonlocal_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-wrap (body cleanup)
                    `(unwind-protect ,body ,cleanup))
                  (let ((x 0))
                    (unwind-protect
                        (list
                          (catch 'neovm--combo-tag
                            (funcall
                              (lambda ()
                                (neovm--combo-wrap
                                  (progn
                                    (setq x 1)
                                    (throw 'neovm--combo-tag 'stop))
                                  (setq x 2)))))
                          x)
                      (fmakunbound 'neovm--combo-wrap))))";
    let expect = expect_test::expect![[r#""OK (stop 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_filter_return_advice_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-path-target (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-path-filter (lambda (ret) (* ret 3)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-path-target :filter-return 'neovm--combo-path-filter)
                        (list
                          (funcall 'neovm--combo-path-target 4)
                          (apply 'neovm--combo-path-target '(4))
                          (neovm--combo-path-target 4)
                          (eval '(neovm--combo-path-target 4))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-path-target 'neovm--combo-path-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-path-target)
                    (fmakunbound 'neovm--combo-path-filter)))";
    let expect = expect_test::expect![[r#""OK (15 15 15 15)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_before_advice_call_path_logging_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((log nil))
                  (fset 'neovm--combo-before-path-target
                        (lambda (x) (setq log (cons (list 'orig x) log)) x))
                  (fset 'neovm--combo-before-path
                        (lambda (&rest args)
                          (setq log (cons (cons 'before args) log))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-before-path-target :before 'neovm--combo-before-path)
                        (list
                          (funcall 'neovm--combo-before-path-target 1)
                          (apply 'neovm--combo-before-path-target '(2))
                          (neovm--combo-before-path-target 3)
                          (eval '(neovm--combo-before-path-target 4))
                          (nreverse log)))
                    (condition-case nil
                        (advice-remove 'neovm--combo-before-path-target 'neovm--combo-before-path)
                      (error nil))
                    (fmakunbound 'neovm--combo-before-path-target)
                    (fmakunbound 'neovm--combo-before-path)))";
    let expect = expect_test::expect![[
        r#""OK (1 2 3 4 ((before 1) (orig 1) (before 2) (orig 2) (before 3) (orig 3) (before 4) (orig 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_direct_vs_funcall_under_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-call-direct (x) `(neovm--combo-macro-path-target ,x))
                  (defmacro neovm--combo-call-funcall (x) `(funcall 'neovm--combo-macro-path-target ,x))
                  (fset 'neovm--combo-macro-path-target (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-macro-path-filter (lambda (ret) (* ret 3)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-macro-path-target :filter-return 'neovm--combo-macro-path-filter)
                        (list
                          (neovm--combo-call-direct 5)
                          (neovm--combo-call-funcall 5)
                          (macroexpand '(neovm--combo-call-direct 5))
                          (macroexpand '(neovm--combo-call-funcall 5))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-macro-path-target 'neovm--combo-macro-path-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-call-direct)
                    (fmakunbound 'neovm--combo-call-funcall)
                    (fmakunbound 'neovm--combo-macro-path-target)
                    (fmakunbound 'neovm--combo-macro-path-filter)))";
    let expect = expect_test::expect![[
        r#""OK (18 18 (neovm--combo-macro-path-target 5) (funcall 'neovm--combo-macro-path-target 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_filter_args_advice_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-fargs-path-target (lambda (a b) (+ a b)))
                  (fset 'neovm--combo-fargs-path
                        (lambda (args)
                          (list (+ 10 (car args))
                                (+ 20 (car (cdr args))))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-fargs-path-target :filter-args 'neovm--combo-fargs-path)
                        (list
                          (funcall 'neovm--combo-fargs-path-target 1 2)
                          (apply 'neovm--combo-fargs-path-target '(1 2))
                          (neovm--combo-fargs-path-target 1 2)
                          (eval '(neovm--combo-fargs-path-target 1 2))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-fargs-path-target 'neovm--combo-fargs-path)
                      (error nil))
                    (fmakunbound 'neovm--combo-fargs-path-target)
                    (fmakunbound 'neovm--combo-fargs-path)))";
    let expect = expect_test::expect![[r#""OK (33 33 33 33)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_override_advice_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-override-path-target (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-override-path (lambda (&rest _args) 99))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-override-path-target :override 'neovm--combo-override-path)
                        (list
                          (funcall 'neovm--combo-override-path-target 7)
                          (apply 'neovm--combo-override-path-target '(7))
                          (neovm--combo-override-path-target 7)
                          (eval '(neovm--combo-override-path-target 7))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-override-path-target 'neovm--combo-override-path)
                      (error nil))
                    (fmakunbound 'neovm--combo-override-path-target)
                    (fmakunbound 'neovm--combo-override-path)))";
    let expect = expect_test::expect![[r#""OK (99 99 99 99)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_after_advice_call_path_matrix_logging() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((log nil))
                  (fset 'neovm--combo-after-path-target
                        (lambda (x) (setq log (cons (list 'orig x) log)) x))
                  (fset 'neovm--combo-after-path
                        (lambda (&rest args)
                          (setq log (cons (cons 'after args) log))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-after-path-target :after 'neovm--combo-after-path)
                        (list
                          (funcall 'neovm--combo-after-path-target 1)
                          (apply 'neovm--combo-after-path-target '(2))
                          (neovm--combo-after-path-target 3)
                          (eval '(neovm--combo-after-path-target 4))
                          (nreverse log)))
                    (condition-case nil
                        (advice-remove 'neovm--combo-after-path-target 'neovm--combo-after-path)
                      (error nil))
                    (fmakunbound 'neovm--combo-after-path-target)
                    (fmakunbound 'neovm--combo-after-path)))";
    let expect = expect_test::expect![[
        r#""OK (1 2 3 4 ((orig 1) (after 1) (orig 2) (after 2) (orig 3) (after 3) (orig 4) (after 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_eval_macroexpand_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-bad-macro (x) (list 'car x))
                  (unwind-protect
                      (list
                        (condition-case err
                            (eval (macroexpand '(neovm--combo-bad-macro 1)))
                          (wrong-type-argument (car err)))
                        (condition-case err
                            (eval (macroexpand '(neovm--combo-bad-macro '(9 8))))
                          (error (car err))))
                    (fmakunbound 'neovm--combo-bad-macro)))";
    let expect = expect_test::expect![[r#""OK (wrong-type-argument 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_unwind_cleanup_with_mutating_closure_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 0)
                      (f (let ((cell 0))
                           (lambda (delta)
                             (setq cell (+ cell delta))
                             cell))))
                  (list
                    (unwind-protect
                        (progn
                          (funcall f 3)
                          (funcall f 4))
                      (setq x (funcall f 10)))
                    x
                    (funcall f 1)))";
    let expect = expect_test::expect![[r#""OK (7 17 18)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_generated_tags_and_nonlocal_exit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-with-tag (tag value)
                    `(catch ,tag
                       (condition-case err
                           (throw ,tag ,value)
                         (error (list 'err (car err))))))
                  (unwind-protect
                      (list
                        (neovm--combo-with-tag 'neovm--t1 11)
                        (neovm--combo-with-tag 'neovm--t2 22))
                    (fmakunbound 'neovm--combo-with-tag)))";
    let expect = expect_test::expect![[r#""OK (11 22)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_runtime_macro_definition_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (eval '(defmacro neovm--combo-runtime (x) (list 'list x x)))
                  (unwind-protect
                      (list
                        (eval '(neovm--combo-runtime 5))
                        (macroexpand '(neovm--combo-runtime 9)))
                    (fmakunbound 'neovm--combo-runtime)))";
    let expect = expect_test::expect![[r#""OK ((5 5) (list 9 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_direct_dynamic_tag_catch_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-dyn-tag))
                  (list
                    (catch 'neovm--combo-lit-tag (throw 'neovm--combo-lit-tag 10))
                    (catch tag (throw tag 20))
                    (condition-case err
                        (catch tag (throw 'neovm--combo-other 1))
                      (no-catch (car err)))))";
    let expect = expect_test::expect![[r#""OK (10 20 no-catch)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_parameterized_tag_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-catch-throw-param (tag value)
                    `(catch ,tag (throw ,tag ,value)))
                  (unwind-protect
                      (list
                        (neovm--combo-catch-throw-param 'neovm--combo-a 3)
                        (let ((tg 'neovm--combo-b))
                          (neovm--combo-catch-throw-param tg 4)))
                    (fmakunbound 'neovm--combo-catch-throw-param)))";
    let expect = expect_test::expect![[r#""OK (3 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macroexpanded_tag_form_eval_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-catch-throw-param (tag value)
                    `(catch ,tag (throw ,tag ,value)))
                  (unwind-protect
                      (let ((expanded
                              (macroexpand
                                '(neovm--combo-catch-throw-param 'neovm--combo-c 9))))
                        (list expanded (eval expanded)))
                    (fmakunbound 'neovm--combo-catch-throw-param)))";
    let expect =
        expect_test::expect![[r#""OK ((catch 'neovm--combo-c (throw 'neovm--combo-c 9)) 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_eval_constructed_catch_throw_runtime_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let* ((tag 'neovm--combo-rt-tag)
                       (form (list 'catch
                                   (list 'quote tag)
                                   (list 'throw (list 'quote tag) 77))))
                  (eval form))";
    let expect = expect_test::expect![[r#""OK 77""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_apply_eval_macro_generated_throw() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-build-throw (tag val)
                    `(throw ,tag ,val))
                  (unwind-protect
                      (catch 'neovm--combo-ap
                        (apply (lambda (frm) (eval frm))
                               (list (macroexpand
                                       '(neovm--combo-build-throw
                                          'neovm--combo-ap
                                          13)))))
                    (fmakunbound 'neovm--combo-build-throw)))";
    let expect = expect_test::expect![[r#""OK 13""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_dynamic_tag_with_condition_case_without_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-cond-tag))
                  (catch tag
                    (condition-case err
                        (throw tag 31)
                      (error (list 'err (car err))))))";
    let expect = expect_test::expect![[r#""OK 31""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_tag_with_condition_case_expansion_and_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-with-tag-cc (tag value)
                    `(catch ,tag
                       (condition-case err
                           (throw ,tag ,value)
                         (error (list 'err (car err))))))
                  (unwind-protect
                      (let ((a (macroexpand '(neovm--combo-with-tag-cc 'neovm--combo-c1 41)))
                            (b (macroexpand '(neovm--combo-with-tag-cc 'neovm--combo-c2 42))))
                        (list a b (eval a) (eval b)))
                    (fmakunbound 'neovm--combo-with-tag-cc)))";
    let expect = expect_test::expect![[
        r#""OK ((catch 'neovm--combo-c1 (condition-case err (throw 'neovm--combo-c1 41) (error (list 'err (car err))))) (catch 'neovm--combo-c2 (condition-case err (throw 'neovm--combo-c2 42) (error (list 'err (car err))))) 41 42)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_eval_macroexpanded_lambda_with_lexenv() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-make-adder (x)
                    `(lambda (y) (+ ,x y)))
                  (unwind-protect
                      (let ((f (eval '(neovm--combo-make-adder x) '((x . 9)))))
                        (list (funcall f 1) (apply f '(2))))
                    (fmakunbound 'neovm--combo-make-adder)))";
    let expect = expect_test::expect![[r#""OK (10 11)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_unwind_cleanup_rebinds_function_seen_by_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((sym 'neovm--combo-rebind))
                  (fset sym (lambda (x) (+ x 1)))
                  (unwind-protect
                      (list
                        (funcall sym 3)
                        (unwind-protect
                            (catch 'neovm--combo-rb-tag
                              (throw 'neovm--combo-rb-tag (funcall sym 4)))
                          (fset sym (lambda (x) (* x 10))))
                        (funcall sym 3)
                        (eval (list sym 3)))
                    (fmakunbound sym)))";
    let expect = expect_test::expect![[r#""OK (4 5 30 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_runtime_redefinition_changes_future_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-rdef (x) (list '+ x 1))
                  (unwind-protect
                      (let ((first (macroexpand '(neovm--combo-rdef 7))))
                        (fset 'neovm--combo-rdef (cons 'macro (lambda (x) (list '- x 1))))
                        (list
                          first
                          (macroexpand '(neovm--combo-rdef 7))
                          (neovm--combo-rdef 7)))
                    (fmakunbound 'neovm--combo-rdef)))";
    let expect = expect_test::expect![[r#""OK ((+ 7 1) (- 7 1) 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_dynamic_tag_throw_inside_unwind_protect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-tag-u))
                  (catch tag
                    (unwind-protect
                        (throw tag 55)
                      'cleanup)))";
    let expect = expect_test::expect![[r#""OK 55""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_dynamic_tag_throw_in_condition_case_inside_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-tag-uc))
                  (catch tag
                    (unwind-protect
                        (condition-case err
                            (throw tag 66)
                          (error (list 'err (car err))))
                      'cleanup)))";
    let expect = expect_test::expect![[r#""OK 66""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_no_catch_handler_rethrows_to_outer_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-tag-r))
                  (catch tag
                    (condition-case err
                        (throw 'neovm--combo-other 1)
                      (no-catch
                       (throw tag (list 'rescued (car err)))))))";
    let expect = expect_test::expect![[r#""OK (rescued no-catch)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_wrapped_condition_case_throw_with_dynamic_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-cc-throw (tag v)
                    `(condition-case err
                         (throw ,tag ,v)
                       (error (list 'err (car err)))))
                  (unwind-protect
                      (let ((tag 'neovm--combo-mtag))
                        (catch tag
                          (neovm--combo-cc-throw tag 77)))
                    (fmakunbound 'neovm--combo-cc-throw)))";
    let expect = expect_test::expect![[r#""OK 77""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_apply_throw_with_dynamic_tag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-apply-tag))
                  (catch tag
                    (apply #'throw (list tag 88))))";
    let expect = expect_test::expect![[r#""OK 88""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_through_condition_case_unrelated_error_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-cc-pass-tag))
                  (catch tag
                    (condition-case nil
                        (progn (throw tag 91) 'tail)
                      (arith-error 'arith)
                      (wrong-type-argument 'wta))))";
    let expect = expect_test::expect![[r#""OK 91""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_through_multiple_condition_case_layers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-cc-nest-tag))
                  (catch tag
                    (condition-case nil
                        (condition-case nil
                            (throw tag 92)
                          (wrong-type-argument 'inner))
                      (arith-error 'outer))))";
    let expect = expect_test::expect![[r#""OK 92""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_through_condition_case_and_unwind_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-cc-unwind-tag)
                      (x 0))
                  (list
                    (catch tag
                      (condition-case nil
                          (unwind-protect
                              (throw tag 93)
                            (setq x 1))
                        (error 'caught)))
                    x))";
    let expect = expect_test::expect![[r#""OK (93 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_apply_throw_inside_condition_case_to_outer_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag 'neovm--combo-cc-apply-tag))
                  (catch tag
                    (condition-case nil
                        (apply #'throw (list tag 94))
                      (error 'caught))))";
    let expect = expect_test::expect![[r#""OK 94""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_not_caught_by_condition_case_error_clause() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(catch 'neovm--combo-cc-basic-tag
                  (condition-case nil
                      (throw 'neovm--combo-cc-basic-tag 95)
                    (error 'caught)))";
    let expect = expect_test::expect![[r#""OK 95""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_symbol_function_after_advice_call_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-sf-target (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-sf-filter (lambda (ret) (* ret 3)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-sf-target :filter-return 'neovm--combo-sf-filter)
                        (let ((f (symbol-function 'neovm--combo-sf-target)))
                          (list
                            (funcall f 5)
                            (apply f '(5))
                            (funcall 'neovm--combo-sf-target 5)
                            (neovm--combo-sf-target 5))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-sf-target 'neovm--combo-sf-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-sf-target)
                    (fmakunbound 'neovm--combo-sf-filter)))";
    let expect = expect_test::expect![[r#""OK (18 18 18 18)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_fset_after_advice_add_keeps_wrapping_behavior() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-fset-target (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-fset-filter (lambda (ret) (* ret 2)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-fset-target :filter-return 'neovm--combo-fset-filter)
                        (fset 'neovm--combo-fset-target (lambda (x) (+ x 10)))
                        (list
                          (funcall 'neovm--combo-fset-target 3)
                          (neovm--combo-fset-target 3)))
                    (condition-case nil
                        (advice-remove 'neovm--combo-fset-target 'neovm--combo-fset-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-fset-target)
                    (fmakunbound 'neovm--combo-fset-filter)))";
    let expect = expect_test::expect![[r#""OK (13 13)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_defalias_to_advised_symbol_call_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-alias-target (lambda (x) (+ x 1)))
                  (defalias 'neovm--combo-alias 'neovm--combo-alias-target)
                  (fset 'neovm--combo-alias-filter (lambda (ret) (* ret 3)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-alias-target :filter-return 'neovm--combo-alias-filter)
                        (list
                          (funcall 'neovm--combo-alias-target 2)
                          (neovm--combo-alias-target 2)
                          (funcall 'neovm--combo-alias 2)
                          (neovm--combo-alias 2)))
                    (condition-case nil
                        (advice-remove 'neovm--combo-alias-target 'neovm--combo-alias-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-alias-target)
                    (fmakunbound 'neovm--combo-alias)
                    (fmakunbound 'neovm--combo-alias-filter)))";
    let expect = expect_test::expect![[r#""OK (9 9 9 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_catch_throw_non_symbol_tag_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list
                  (catch 1 (throw 1 'int-tag))
                  (let ((tag (list 'a)))
                    (catch tag (throw tag 'cons-tag))))";
    let expect = expect_test::expect![[r#""OK (int-tag cons-tag)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_catch_throw_tag_identity_uses_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(condition-case err
                  (let ((tag (list 'a)))
                    (catch tag (throw (list 'a) 'mismatch)))
                (no-catch (car err)))";
    let expect = expect_test::expect![[r#""OK no-catch""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_catch_tag_expression_evaluated_once() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((n 0))
                  (list
                    (catch (progn (setq n (1+ n)) 'neovm--combo-once-tag)
                      (throw 'neovm--combo-once-tag n))
                    n))";
    let expect = expect_test::expect![[r#""OK (1 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_non_symbol_tag_throw_through_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag (list 'neovm--combo-nsym)))
                  (catch tag
                    (condition-case nil
                        (throw tag 96)
                      (error 'caught))))";
    let expect = expect_test::expect![[r#""OK 96""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_through_condition_case_with_no_catch_clause() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(catch 'neovm--combo-nc-tag
                  (condition-case err
                      (throw 'neovm--combo-nc-tag 97)
                    (no-catch (list 'handled (car err)))
                    (error 'caught)))";
    let expect = expect_test::expect![[r#""OK 97""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_non_symbol_throw_with_no_catch_clause() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag (list 'neovm--combo-nc-nsym)))
                  (catch tag
                    (condition-case err
                        (throw tag 98)
                      (no-catch (list 'handled (car err)))
                      (error 'caught))))";
    let expect = expect_test::expect![[r#""OK 98""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_from_funcall_inside_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(catch 'neovm--combo-funcall-tag
                  (condition-case nil
                      (funcall (lambda () (throw 'neovm--combo-funcall-tag 99)))
                    (error 'caught)))";
    let expect = expect_test::expect![[r#""OK 99""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_from_while_inside_condition_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((i 0))
                  (catch 'neovm--combo-while-tag
                    (condition-case nil
                        (progn
                          (while t
                            (setq i (1+ i))
                            (if (= i 3)
                                (throw 'neovm--combo-while-tag i))))
                      (error 'caught))))";
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_from_condition_case_handler_to_outer_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((log nil))
                  (list
                    (catch 'neovm--combo-handler-tag
                      (condition-case err
                          (progn
                            (setq log (cons 'body log))
                            (/ 1 0))
                        (arith-error
                         (unwind-protect
                             (throw 'neovm--combo-handler-tag 'handled)
                           (setq log (cons 'cleanup log))))))
                    (nreverse log)))";
    let expect = expect_test::expect![[r#""OK (handled (body cleanup))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_no_catch_payload_shape_outside_catch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(condition-case err
                    (throw 'neovm--combo-no-catch-tag 77)
                  (no-catch
                   (list
                     (car err)
                     (car (cdr err))
                     (car (cdr (cdr err)))))
                  (error 'caught))";
    let expect = expect_test::expect![[r#""OK (no-catch neovm--combo-no-catch-tag 77)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_prog1_unwind_throw_cleanup_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((x 1) (log nil))
                  (list
                    (catch 'neovm--combo-prog1-tag
                      (prog1
                          (unwind-protect
                              (progn
                                (setq log (cons 'body log))
                                (throw 'neovm--combo-prog1-tag (+ x 1)))
                            (setq x (+ x 10))
                            (setq log (cons 'cleanup log)))
                        (setq log (cons 'tail log))))
                    x
                    (nreverse log)))";
    let expect = expect_test::expect![[r#""OK (2 11 (body cleanup))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_from_error_handler_lambda() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(catch 'neovm--combo-handler-lambda-tag
                  (condition-case nil
                      (progn
                        (signal 'error nil)
                        'unreachable)
                    (error
                     (funcall
                      (lambda ()
                        (throw 'neovm--combo-handler-lambda-tag 123))))))";
    let expect = expect_test::expect![[r#""OK 123""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_around_advice_throw_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((target 'neovm--combo-around-throw-target)
                      (around 'neovm--combo-around-throw)
                      (log nil))
                  (fset target (lambda (x)
                                 (setq log (cons (list 'orig x) log))
                                 (throw 'neovm--combo-around-throw-tag (+ x 1))))
                  (fset around (lambda (orig x)
                                 (setq log (cons (list 'around-enter x) log))
                                 (unwind-protect
                                     (funcall orig x)
                                   (setq log (cons (list 'around-cleanup x) log)))))
                  (unwind-protect
                      (progn
                        (advice-add target :around around)
                        (list
                          (catch 'neovm--combo-around-throw-tag (funcall target 5))
                          (catch 'neovm--combo-around-throw-tag (apply target '(5)))
                          (catch 'neovm--combo-around-throw-tag (target 5))
                          (catch 'neovm--combo-around-throw-tag (eval '(target 5)))
                          (nreverse log)))
                    (condition-case nil (advice-remove target around) (error nil))
                    (fmakunbound target)
                    (fmakunbound around)))";
    let expect = expect_test::expect![[r#""ERR (void-function target)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_advice_condition_case_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-call-cc (f x)
                    `(condition-case err
                         (,f ,x)
                       (error (list 'err (car err)))))
                  (fset 'neovm--combo-macro-advice-target (lambda (x) (* 3 x)))
                  (fset 'neovm--combo-macro-advice-around
                        (lambda (orig x) (+ 1 (funcall orig x))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-macro-advice-target :around 'neovm--combo-macro-advice-around)
                        (list
                          (macroexpand '(neovm--combo-m-call-cc neovm--combo-macro-advice-target 7))
                          (neovm--combo-m-call-cc neovm--combo-macro-advice-target 7)
                          (eval '(neovm--combo-m-call-cc neovm--combo-macro-advice-target 7))
                          (funcall 'neovm--combo-macro-advice-target 7)
                          (apply 'neovm--combo-macro-advice-target '(7))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-macro-advice-target 'neovm--combo-macro-advice-around)
                      (error nil))
                    (fmakunbound 'neovm--combo-macro-advice-target)
                    (fmakunbound 'neovm--combo-macro-advice-around)
                    (fmakunbound 'neovm--combo-m-call-cc)))";
    let expect = expect_test::expect![[
        r#""OK ((condition-case err (neovm--combo-macro-advice-target 7) (error (list 'err (car err)))) 22 22 22 22)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_filter_return_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-fr-call (x)
                    `(neovm--combo-m-fr-target ,x))
                  (fset 'neovm--combo-m-fr-target (lambda (x) (* 2 x)))
                  (fset 'neovm--combo-m-fr-filter (lambda (ret) (+ ret 9)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-fr-target :filter-return 'neovm--combo-m-fr-filter)
                        (list
                          (neovm--combo-m-fr-call 8)
                          (eval '(neovm--combo-m-fr-call 8))
                          (funcall 'neovm--combo-m-fr-target 8)
                          (apply 'neovm--combo-m-fr-target '(8))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-fr-target 'neovm--combo-m-fr-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-fr-target)
                    (fmakunbound 'neovm--combo-m-fr-filter)
                    (fmakunbound 'neovm--combo-m-fr-call)))";
    let expect = expect_test::expect![[r#""OK (25 25 25 25)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_filter_args_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-fa-call (x y)
                    `(neovm--combo-m-fa-target ,x ,y))
                  (fset 'neovm--combo-m-fa-target (lambda (x y) (+ x y)))
                  (fset 'neovm--combo-m-fa-filter
                        (lambda (args)
                          (list (+ 10 (car args))
                                (+ 20 (car (cdr args))))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-fa-target :filter-args 'neovm--combo-m-fa-filter)
                        (list
                          (neovm--combo-m-fa-call 1 2)
                          (eval '(neovm--combo-m-fa-call 1 2))
                          (funcall 'neovm--combo-m-fa-target 1 2)
                          (apply 'neovm--combo-m-fa-target '(1 2))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-fa-target 'neovm--combo-m-fa-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-fa-target)
                    (fmakunbound 'neovm--combo-m-fa-filter)
                    (fmakunbound 'neovm--combo-m-fa-call)))";
    let expect = expect_test::expect![[r#""OK (33 33 33 33)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_before_advice_throw_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-before-throw-call (x)
                    `(neovm--combo-m-before-throw-target ,x))
                  (fset 'neovm--combo-m-before-throw-target (lambda (x) (* 10 x)))
                  (fset 'neovm--combo-m-before-throw
                        (lambda (&rest args)
                          (throw 'neovm--combo-m-before-throw-tag
                                 (list 'thrown (car args)))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-before-throw-target :before 'neovm--combo-m-before-throw)
                        (list
                          (catch 'neovm--combo-m-before-throw-tag
                            (neovm--combo-m-before-throw-call 3))
                          (catch 'neovm--combo-m-before-throw-tag
                            (eval '(neovm--combo-m-before-throw-call 3)))
                          (catch 'neovm--combo-m-before-throw-tag
                            (funcall 'neovm--combo-m-before-throw-target 3))
                          (catch 'neovm--combo-m-before-throw-tag
                            (apply 'neovm--combo-m-before-throw-target '(3)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-before-throw-target 'neovm--combo-m-before-throw)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-before-throw-target)
                    (fmakunbound 'neovm--combo-m-before-throw)
                    (fmakunbound 'neovm--combo-m-before-throw-call)))";
    let expect = expect_test::expect![[r#""OK ((thrown 3) (thrown 3) (thrown 3) (thrown 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_override_advice_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-override-call (x)
                    `(neovm--combo-m-override-target ,x))
                  (fset 'neovm--combo-m-override-target (lambda (x) (* 4 x)))
                  (fset 'neovm--combo-m-override
                        (lambda (x) (+ x 100)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-override-target :override 'neovm--combo-m-override)
                        (list
                          (neovm--combo-m-override-call 3)
                          (eval '(neovm--combo-m-override-call 3))
                          (funcall 'neovm--combo-m-override-target 3)
                          (apply 'neovm--combo-m-override-target '(3))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-override-target 'neovm--combo-m-override)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-override-target)
                    (fmakunbound 'neovm--combo-m-override)
                    (fmakunbound 'neovm--combo-m-override-call)))";
    let expect = expect_test::expect![[r#""OK (103 103 103 103)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_after_advice_side_effect_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-after-call (x)
                    `(neovm--combo-m-after-target ,x))
                  (fset 'neovm--combo-m-after-target (lambda (x) x))
                  (let ((log nil))
                    (fset 'neovm--combo-m-after
                          (lambda (&rest args)
                            (setq log (cons (car args) log))))
                    (unwind-protect
                        (progn
                          (advice-add 'neovm--combo-m-after-target :after 'neovm--combo-m-after)
                          (list
                            (neovm--combo-m-after-call 6)
                            (eval '(neovm--combo-m-after-call 6))
                            (funcall 'neovm--combo-m-after-target 6)
                            (apply 'neovm--combo-m-after-target '(6))
                            (length log)
                            (nreverse log)))
                      (condition-case nil
                          (advice-remove 'neovm--combo-m-after-target 'neovm--combo-m-after)
                        (error nil))
                      (fmakunbound 'neovm--combo-m-after-target)
                      (fmakunbound 'neovm--combo-m-after)
                      (fmakunbound 'neovm--combo-m-after-call))))";
    let expect = expect_test::expect![[r#""OK (6 6 6 6 4 (6 6 6 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_condition_case_throw_before_advice_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-cc-throw-call (x)
                    `(condition-case err
                         (neovm--combo-m-cc-throw-target ,x)
                       (error (list 'err (car err)))))
                  (fset 'neovm--combo-m-cc-throw-target
                        (lambda (x)
                          (throw 'neovm--combo-m-cc-throw-tag x)))
                  (let ((log nil))
                    (fset 'neovm--combo-m-cc-throw-before
                          (lambda (&rest _args)
                            (setq log (cons 'before log))))
                    (unwind-protect
                        (progn
                          (advice-add 'neovm--combo-m-cc-throw-target :before 'neovm--combo-m-cc-throw-before)
                          (list
                            (catch 'neovm--combo-m-cc-throw-tag
                              (neovm--combo-m-cc-throw-call 4))
                            (catch 'neovm--combo-m-cc-throw-tag
                              (eval '(neovm--combo-m-cc-throw-call 4)))
                            (catch 'neovm--combo-m-cc-throw-tag
                              (condition-case err
                                  (funcall 'neovm--combo-m-cc-throw-target 4)
                                (error (list 'err (car err)))))
                            (catch 'neovm--combo-m-cc-throw-tag
                              (condition-case err
                                  (apply 'neovm--combo-m-cc-throw-target '(4))
                                (error (list 'err (car err)))))
                            (length log)))
                      (condition-case nil
                          (advice-remove 'neovm--combo-m-cc-throw-target 'neovm--combo-m-cc-throw-before)
                        (error nil))
                      (fmakunbound 'neovm--combo-m-cc-throw-target)
                      (fmakunbound 'neovm--combo-m-cc-throw-before)
                      (fmakunbound 'neovm--combo-m-cc-throw-call))))";
    let expect = expect_test::expect![[r#""OK (4 4 4 4 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_symbol_function_under_advice_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-sf-call (sym x)
                    `((symbol-function ',sym) ,x))
                  (fset 'neovm--combo-m-sf-target (lambda (x) (* 2 x)))
                  (fset 'neovm--combo-m-sf-around
                        (lambda (orig x) (+ 1 (funcall orig x))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-sf-target :around 'neovm--combo-m-sf-around)
                        (list
                          (neovm--combo-m-sf-call neovm--combo-m-sf-target 10)
                          (eval '(neovm--combo-m-sf-call neovm--combo-m-sf-target 10))
                          (funcall (symbol-function 'neovm--combo-m-sf-target) 10)
                          (apply (symbol-function 'neovm--combo-m-sf-target) '(10))
                          (neovm--combo-m-sf-target 10)
                          (funcall 'neovm--combo-m-sf-target 10)))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-sf-target 'neovm--combo-m-sf-around)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-sf-target)
                    (fmakunbound 'neovm--combo-m-sf-around)
                    (fmakunbound 'neovm--combo-m-sf-call)))";
    let expect = expect_test::expect![[
        r#""ERR (invalid-function (symbol-function 'neovm--combo-m-sf-target))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_stacked_around_and_filter_return_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-stack-call (x)
                    `(neovm--combo-m-stack-target ,x))
                  (fset 'neovm--combo-m-stack-target (lambda (x) (+ x 2)))
                  (fset 'neovm--combo-m-stack-around
                        (lambda (orig x) (* 2 (funcall orig x))))
                  (fset 'neovm--combo-m-stack-fr
                        (lambda (ret) (+ ret 5)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-stack-target :around 'neovm--combo-m-stack-around)
                        (advice-add 'neovm--combo-m-stack-target :filter-return 'neovm--combo-m-stack-fr)
                        (list
                          (macroexpand '(neovm--combo-m-stack-call 4))
                          (neovm--combo-m-stack-call 4)
                          (eval '(neovm--combo-m-stack-call 4))
                          (funcall 'neovm--combo-m-stack-target 4)
                          (apply 'neovm--combo-m-stack-target '(4))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-stack-target 'neovm--combo-m-stack-fr)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-stack-target 'neovm--combo-m-stack-around)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-stack-target)
                    (fmakunbound 'neovm--combo-m-stack-around)
                    (fmakunbound 'neovm--combo-m-stack-fr)
                    (fmakunbound 'neovm--combo-m-stack-call)))";
    let expect = expect_test::expect![[r#""OK ((neovm--combo-m-stack-target 4) 17 17 17 17)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_fset_after_advice_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-fset-call (x)
                    `(neovm--combo-m-fset-target ,x))
                  (fset 'neovm--combo-m-fset-target (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-m-fset-around
                        (lambda (orig x) (+ 100 (funcall orig x))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-fset-target :around 'neovm--combo-m-fset-around)
                        (fset 'neovm--combo-m-fset-target (lambda (x) (* 2 x)))
                        (list
                          (neovm--combo-m-fset-call 3)
                          (eval '(neovm--combo-m-fset-call 3))
                          (funcall 'neovm--combo-m-fset-target 3)
                          (apply 'neovm--combo-m-fset-target '(3))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-fset-target 'neovm--combo-m-fset-around)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-fset-target)
                    (fmakunbound 'neovm--combo-m-fset-around)
                    (fmakunbound 'neovm--combo-m-fset-call)))";
    let expect = expect_test::expect![[r#""OK (6 6 6 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_non_symbol_throw_from_around_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((tag (list 'neovm--combo-m-nsym-tag)))
                  (progn
                    (defmacro neovm--combo-m-nsym-call (x)
                      `(neovm--combo-m-nsym-target ,x))
                    (fset 'neovm--combo-m-nsym-target (lambda (x) x))
                    (fset 'neovm--combo-m-nsym-around
                          (lambda (_orig x)
                            (throw tag (+ x 9))))
                    (unwind-protect
                        (progn
                          (advice-add 'neovm--combo-m-nsym-target :around 'neovm--combo-m-nsym-around)
                          (list
                            (catch tag (neovm--combo-m-nsym-call 5))
                            (catch tag (eval '(neovm--combo-m-nsym-call 5)))
                            (catch tag (funcall 'neovm--combo-m-nsym-target 5))
                            (catch tag (apply 'neovm--combo-m-nsym-target '(5)))))
                      (condition-case nil
                          (advice-remove 'neovm--combo-m-nsym-target 'neovm--combo-m-nsym-around)
                        (error nil))
                      (fmakunbound 'neovm--combo-m-nsym-target)
                      (fmakunbound 'neovm--combo-m-nsym-around)
                      (fmakunbound 'neovm--combo-m-nsym-call))))";
    let expect = expect_test::expect![[r#""OK (14 14 14 14)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_defalias_under_advice_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-alias-call (x)
                    `(neovm--combo-m-alias ,x))
                  (fset 'neovm--combo-m-alias-target (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-m-alias-around
                        (lambda (orig x) (* 2 (funcall orig x))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-alias-target :around 'neovm--combo-m-alias-around)
                        (defalias 'neovm--combo-m-alias 'neovm--combo-m-alias-target)
                        (list
                          (neovm--combo-m-alias-call 5)
                          (eval '(neovm--combo-m-alias-call 5))
                          (funcall 'neovm--combo-m-alias 5)
                          (apply 'neovm--combo-m-alias '(5))
                          (funcall 'neovm--combo-m-alias-target 5)
                          (apply 'neovm--combo-m-alias-target '(5))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-alias-target 'neovm--combo-m-alias-around)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-alias)
                    (fmakunbound 'neovm--combo-m-alias-target)
                    (fmakunbound 'neovm--combo-m-alias-around)
                    (fmakunbound 'neovm--combo-m-alias-call)))";
    let expect = expect_test::expect![[r#""OK (12 12 12 12 12 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_after_advice_throw_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((log nil))
                  (progn
                    (defmacro neovm--combo-m-after-throw-call (x)
                      `(neovm--combo-m-after-throw-target ,x))
                    (fset 'neovm--combo-m-after-throw-target
                          (lambda (x)
                            (setq log (cons (list 'orig x) log))
                            x))
                    (fset 'neovm--combo-m-after-throw
                          (lambda (&rest args)
                            (setq log (cons (list 'after (car args)) log))
                            (throw 'neovm--combo-m-after-throw-tag (+ 50 (car args)))))
                    (unwind-protect
                        (progn
                          (advice-add 'neovm--combo-m-after-throw-target :after 'neovm--combo-m-after-throw)
                          (list
                            (catch 'neovm--combo-m-after-throw-tag
                              (neovm--combo-m-after-throw-call 2))
                            (catch 'neovm--combo-m-after-throw-tag
                              (eval '(neovm--combo-m-after-throw-call 2)))
                            (catch 'neovm--combo-m-after-throw-tag
                              (funcall 'neovm--combo-m-after-throw-target 2))
                            (catch 'neovm--combo-m-after-throw-tag
                              (apply 'neovm--combo-m-after-throw-target '(2)))
                            (nreverse log)))
                      (condition-case nil
                          (advice-remove 'neovm--combo-m-after-throw-target 'neovm--combo-m-after-throw)
                        (error nil))
                      (fmakunbound 'neovm--combo-m-after-throw-target)
                      (fmakunbound 'neovm--combo-m-after-throw)
                      (fmakunbound 'neovm--combo-m-after-throw-call))))";
    let expect = expect_test::expect![[
        r#""OK (52 52 52 52 ((orig 2) (after 2) (orig 2) (after 2) (orig 2) (after 2) (orig 2) (after 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_filter_return_advice_toggle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-toggle-call (x)
                    `(neovm--combo-m-toggle-target ,x))
                  (fset 'neovm--combo-m-toggle-target (lambda (x) x))
                  (fset 'neovm--combo-m-toggle-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (list
                        (neovm--combo-m-toggle-call 3)
                        (funcall 'neovm--combo-m-toggle-target 3)
                        (progn
                          (advice-add 'neovm--combo-m-toggle-target :filter-return 'neovm--combo-m-toggle-filter)
                          (list
                            (neovm--combo-m-toggle-call 3)
                            (eval '(neovm--combo-m-toggle-call 3))
                            (funcall 'neovm--combo-m-toggle-target 3)
                            (apply 'neovm--combo-m-toggle-target '(3))))
                        (progn
                          (advice-remove 'neovm--combo-m-toggle-target 'neovm--combo-m-toggle-filter)
                          (list
                            (neovm--combo-m-toggle-call 3)
                            (eval '(neovm--combo-m-toggle-call 3))
                            (funcall 'neovm--combo-m-toggle-target 3)
                            (apply 'neovm--combo-m-toggle-target '(3)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-toggle-target 'neovm--combo-m-toggle-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-toggle-target)
                    (fmakunbound 'neovm--combo-m-toggle-filter)
                    (fmakunbound 'neovm--combo-m-toggle-call)))";
    let expect = expect_test::expect![[r#""OK (3 3 (10 10 10 10) (3 3 3 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_around_translates_error_to_throw_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-err-throw-call (x)
                    `(neovm--combo-m-err-throw-target ,x))
                  (fset 'neovm--combo-m-err-throw-target
                        (lambda (_x) (/ 1 0)))
                  (fset 'neovm--combo-m-err-throw-around
                        (lambda (orig x)
                          (condition-case nil
                              (funcall orig x)
                            (arith-error
                             (throw 'neovm--combo-m-err-throw-tag (+ 50 x))))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-err-throw-target :around 'neovm--combo-m-err-throw-around)
                        (list
                          (catch 'neovm--combo-m-err-throw-tag
                            (condition-case nil
                                (neovm--combo-m-err-throw-call 4)
                              (arith-error 'arith)))
                          (catch 'neovm--combo-m-err-throw-tag
                            (condition-case nil
                                (eval '(neovm--combo-m-err-throw-call 4))
                              (arith-error 'arith)))
                          (catch 'neovm--combo-m-err-throw-tag
                            (condition-case nil
                                (funcall 'neovm--combo-m-err-throw-target 4)
                              (arith-error 'arith)))
                          (catch 'neovm--combo-m-err-throw-tag
                            (condition-case nil
                                (apply 'neovm--combo-m-err-throw-target '(4))
                              (arith-error 'arith)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-err-throw-target 'neovm--combo-m-err-throw-around)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-err-throw-target)
                    (fmakunbound 'neovm--combo-m-err-throw-around)
                    (fmakunbound 'neovm--combo-m-err-throw-call)))";
    let expect = expect_test::expect![[r#""OK (54 54 54 54)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_advice_member_state_and_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-member-call (x)
                    `(neovm--combo-m-member-target ,x))
                  (fset 'neovm--combo-m-member-target (lambda (x) x))
                  (fset 'neovm--combo-m-member-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (list
                        (if (advice-member-p 'neovm--combo-m-member-filter 'neovm--combo-m-member-target) t nil)
                        (progn
                          (advice-add 'neovm--combo-m-member-target :filter-return 'neovm--combo-m-member-filter)
                          (list
                            (if (advice-member-p 'neovm--combo-m-member-filter 'neovm--combo-m-member-target) t nil)
                            (neovm--combo-m-member-call 2)
                            (eval '(neovm--combo-m-member-call 2))
                            (funcall 'neovm--combo-m-member-target 2)
                            (apply 'neovm--combo-m-member-target '(2))))
                        (progn
                          (advice-remove 'neovm--combo-m-member-target 'neovm--combo-m-member-filter)
                          (list
                            (if (advice-member-p 'neovm--combo-m-member-filter 'neovm--combo-m-member-target) t nil)
                            (neovm--combo-m-member-call 2)
                            (eval '(neovm--combo-m-member-call 2))
                            (funcall 'neovm--combo-m-member-target 2)
                            (apply 'neovm--combo-m-member-target '(2)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-member-target 'neovm--combo-m-member-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-member-target)
                    (fmakunbound 'neovm--combo-m-member-filter)
                    (fmakunbound 'neovm--combo-m-member-call)))";
    let expect = expect_test::expect![[r#""OK (nil (t 9 9 9 9) (nil 2 2 2 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_expansion_shape_under_around_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-shape-direct (x)
                    `(neovm--combo-m-shape-target ,x))
                  (defmacro neovm--combo-m-shape-funcall (x)
                    `(funcall 'neovm--combo-m-shape-target ,x))
                  (defmacro neovm--combo-m-shape-apply (x)
                    `(apply 'neovm--combo-m-shape-target (list ,x)))
                  (fset 'neovm--combo-m-shape-target (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-m-shape-around
                        (lambda (orig x) (+ 100 (funcall orig x))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-shape-target :around 'neovm--combo-m-shape-around)
                        (list
                          (macroexpand '(neovm--combo-m-shape-direct 4))
                          (macroexpand '(neovm--combo-m-shape-funcall 4))
                          (macroexpand '(neovm--combo-m-shape-apply 4))
                          (neovm--combo-m-shape-direct 4)
                          (eval '(neovm--combo-m-shape-direct 4))
                          (neovm--combo-m-shape-funcall 4)
                          (eval '(neovm--combo-m-shape-funcall 4))
                          (neovm--combo-m-shape-apply 4)
                          (eval '(neovm--combo-m-shape-apply 4))
                          (funcall 'neovm--combo-m-shape-target 4)
                          (apply 'neovm--combo-m-shape-target '(4))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-shape-target 'neovm--combo-m-shape-around)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-shape-target)
                    (fmakunbound 'neovm--combo-m-shape-around)
                    (fmakunbound 'neovm--combo-m-shape-direct)
                    (fmakunbound 'neovm--combo-m-shape-funcall)
                    (fmakunbound 'neovm--combo-m-shape-apply)))";
    let expect = expect_test::expect![[
        r#""OK ((neovm--combo-m-shape-target 4) (funcall 'neovm--combo-m-shape-target 4) (apply 'neovm--combo-m-shape-target (list 4)) 105 105 105 105 105 105 105 105)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_float_eq_call_shape_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-float-eq (a b)
                    `(eq ,a ,b))
                  (defmacro neovm--combo-m-float-feq (a b)
                    `(funcall 'eq ,a ,b))
                  (defmacro neovm--combo-m-float-aeq (a b)
                    `(apply 'eq (list ,a ,b)))
                  (let ((x 1.0))
                    (list
                      (eq 1.0 1.0)
                      (funcall 'eq 1.0 1.0)
                      (apply 'eq '(1.0 1.0))
                      (neovm--combo-m-float-eq 1.0 1.0)
                      (eval '(neovm--combo-m-float-eq 1.0 1.0))
                      (neovm--combo-m-float-feq 1.0 1.0)
                      (eval '(neovm--combo-m-float-feq 1.0 1.0))
                      (neovm--combo-m-float-aeq 1.0 1.0)
                      (eval '(neovm--combo-m-float-aeq 1.0 1.0))
                      (let ((y x)) (eq x y))
                      (memq 1.0 '(1.0 2.0)))))";
    let expect = expect_test::expect![[r#""OK (nil nil nil nil nil nil nil nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_float_eq_funcall_apply_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-feq-funcall (a b)
                    `(funcall 'eq ,a ,b))
                  (defmacro neovm--combo-m-feq-apply (a b)
                    `(apply 'eq (list ,a ,b)))
                  (let ((f 1.0))
                    (list
                      (neovm--combo-m-feq-funcall 1.0 1.0)
                      (eval '(neovm--combo-m-feq-funcall 1.0 1.0))
                      (neovm--combo-m-feq-apply 1.0 1.0)
                      (eval '(neovm--combo-m-feq-apply 1.0 1.0))
                      (funcall 'eq f f)
                      (apply 'eq (list f f))
                      (let ((g (+ 0.5 0.5))
                            (h (- 2.0 1.0)))
                        (list
                          (funcall 'eq g h)
                          (apply 'eq (list g h))
                          (eq g h))))))";
    let expect = expect_test::expect![[r#""OK (nil nil nil nil t t (nil nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_float_eq_hash_table_key_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-ht-put (k v ht)
                    `(puthash ,k ,v ,ht))
                  (defmacro neovm--combo-m-ht-get (k ht)
                    `(gethash ,k ,ht 'missing))
                  (let* ((k1 (car (read-from-string \"1.0\")))
                         (k2 (car (read-from-string \"1.0\")))
                         (ht (make-hash-table :test 'eq)))
                    (list
                      (eq k1 k2)
                      (neovm--combo-m-ht-put k1 'v ht)
                      (neovm--combo-m-ht-get k1 ht)
                      (eval '(neovm--combo-m-ht-get k1 ht))
                      (neovm--combo-m-ht-get k2 ht)
                      (funcall 'gethash k2 ht 'missing)
                      (apply 'gethash (list k2 ht 'missing))
                      (progn
                        (neovm--combo-m-ht-put k2 'w ht)
                        (hash-table-count ht))
                      (list
                        (neovm--combo-m-ht-get k1 ht)
                        (neovm--combo-m-ht-get k2 ht)))))";
    let expect = expect_test::expect![[r#""ERR (void-variable k1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_generated_lambda_call_shape_under_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-make-caller (mode)
                    (cond
                      ((eq mode 'direct) '(lambda (x) (neovm--combo-m-lambda-target x)))
                      ((eq mode 'funcall) '(lambda (x) (funcall 'neovm--combo-m-lambda-target x)))
                      (t '(lambda (x) (apply 'neovm--combo-m-lambda-target (list x))))))
                  (fset 'neovm--combo-m-lambda-target (lambda (x) x))
                  (fset 'neovm--combo-m-lambda-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-lambda-target :filter-return 'neovm--combo-m-lambda-filter)
                        (let ((d (neovm--combo-m-make-caller 'direct))
                              (f (neovm--combo-m-make-caller 'funcall))
                              (a (neovm--combo-m-make-caller 'apply)))
                          (list
                            (funcall d 3)
                            (funcall f 3)
                            (funcall a 3)
                            (eval '(funcall (neovm--combo-m-make-caller 'direct) 3))
                            (eval '(funcall (neovm--combo-m-make-caller 'funcall) 3))
                            (eval '(funcall (neovm--combo-m-make-caller 'apply) 3)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-lambda-target 'neovm--combo-m-lambda-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-lambda-target)
                    (fmakunbound 'neovm--combo-m-lambda-filter)
                    (fmakunbound 'neovm--combo-m-make-caller)))";
    let expect = expect_test::expect![[r#""OK (10 10 10 10 10 10)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_eval_quoted_symbol_arg_lambda_call() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((n 4))
                  (progn
                    (defmacro neovm--combo-m-min-caller (mode)
                      (cond
                        ((eq mode 'direct) '(lambda (x) (1+ x)))
                        ((eq mode 'funcall) '(lambda (x) (funcall '+ x 1)))
                        (t '(lambda (x) (apply '+ (list x 1))))))
                    (unwind-protect
                        (list
                          (eval '(funcall (neovm--combo-m-min-caller 'direct) n))
                          (eval '(funcall (neovm--combo-m-min-caller 'funcall) n))
                          (eval '(funcall (neovm--combo-m-min-caller 'apply) n)))
                      (fmakunbound 'neovm--combo-m-min-caller))))";
    let expect = expect_test::expect![[r#""ERR (void-variable n)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_generated_lambda_advice_toggle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-toggle-make-caller (mode)
                    (cond
                      ((eq mode 'direct) '(lambda (x) (neovm--combo-m-toggle-lambda-target x)))
                      (t '(lambda (x) (funcall 'neovm--combo-m-toggle-lambda-target x)))))
                  (fset 'neovm--combo-m-toggle-lambda-target (lambda (x) x))
                  (fset 'neovm--combo-m-toggle-lambda-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (let ((d (neovm--combo-m-toggle-make-caller 'direct))
                            (f (neovm--combo-m-toggle-make-caller 'funcall)))
                        (list
                          (funcall d 2)
                          (funcall f 2)
                          (progn
                            (advice-add 'neovm--combo-m-toggle-lambda-target :filter-return 'neovm--combo-m-toggle-lambda-filter)
                            (list (funcall d 2) (funcall f 2)))
                          (progn
                            (advice-remove 'neovm--combo-m-toggle-lambda-target 'neovm--combo-m-toggle-lambda-filter)
                            (list (funcall d 2) (funcall f 2)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-toggle-lambda-target 'neovm--combo-m-toggle-lambda-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-toggle-lambda-target)
                    (fmakunbound 'neovm--combo-m-toggle-lambda-filter)
                    (fmakunbound 'neovm--combo-m-toggle-make-caller)))";
    let expect = expect_test::expect![[r#""OK (2 2 (9 9) (2 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_advice_member_alias_visibility_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-m-member-alias-target (lambda (x) x))
                  (defalias 'neovm--combo-m-member-alias 'neovm--combo-m-member-alias-target)
                  (fset 'neovm--combo-m-member-alias-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-member-alias :filter-return 'neovm--combo-m-member-alias-filter)
                        (list
                          (if (advice-member-p 'neovm--combo-m-member-alias-filter 'neovm--combo-m-member-alias) t nil)
                          (if (advice-member-p 'neovm--combo-m-member-alias-filter 'neovm--combo-m-member-alias-target) t nil)
                          (funcall 'neovm--combo-m-member-alias 2)
                          (funcall 'neovm--combo-m-member-alias-target 2)))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-member-alias 'neovm--combo-m-member-alias-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-member-alias)
                    (fmakunbound 'neovm--combo-m-member-alias-target)
                    (fmakunbound 'neovm--combo-m-member-alias-filter)))";
    let expect = expect_test::expect![[r#""OK (t nil 9 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_stacked_advice_order_call_path_logs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((log nil))
                  (fset 'neovm--combo-m-order-target
                        (lambda (x)
                          (setq log (cons 'orig log))
                          x))
                  (fset 'neovm--combo-m-order-before
                        (lambda (&rest _args)
                          (setq log (cons 'before log))))
                  (fset 'neovm--combo-m-order-around
                        (lambda (orig x)
                          (setq log (cons 'around-enter log))
                          (unwind-protect
                              (funcall orig x)
                            (setq log (cons 'around-exit log)))))
                  (fset 'neovm--combo-m-order-after
                        (lambda (&rest _args)
                          (setq log (cons 'after log))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-order-target :before 'neovm--combo-m-order-before)
                        (advice-add 'neovm--combo-m-order-target :around 'neovm--combo-m-order-around)
                        (advice-add 'neovm--combo-m-order-target :after 'neovm--combo-m-order-after)
                        (list
                          (progn
                            (setq log nil)
                            (list (neovm--combo-m-order-target 1) (nreverse log)))
                          (progn
                            (setq log nil)
                            (list (eval '(neovm--combo-m-order-target 1)) (nreverse log)))
                          (progn
                            (setq log nil)
                            (list (funcall 'neovm--combo-m-order-target 1) (nreverse log)))
                          (progn
                            (setq log nil)
                            (list (apply 'neovm--combo-m-order-target '(1)) (nreverse log)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-order-target 'neovm--combo-m-order-after)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-order-target 'neovm--combo-m-order-around)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-order-target 'neovm--combo-m-order-before)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-order-target)
                    (fmakunbound 'neovm--combo-m-order-before)
                    (fmakunbound 'neovm--combo-m-order-around)
                    (fmakunbound 'neovm--combo-m-order-after)))";
    let expect = expect_test::expect![[
        r#""OK ((1 (around-enter before orig around-exit after)) (1 (around-enter before orig around-exit after)) (1 (around-enter before orig around-exit after)) (1 (around-enter before orig around-exit after)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_stacked_advice_throw_order_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((log nil))
                  (fset 'neovm--combo-m-order-throw-target
                        (lambda (x)
                          (setq log (cons 'orig log))
                          (throw 'neovm--combo-m-order-throw-tag x)))
                  (fset 'neovm--combo-m-order-throw-before
                        (lambda (&rest _args)
                          (setq log (cons 'before log))))
                  (fset 'neovm--combo-m-order-throw-around
                        (lambda (orig x)
                          (setq log (cons 'around-enter log))
                          (unwind-protect
                              (funcall orig x)
                            (setq log (cons 'around-exit log)))))
                  (fset 'neovm--combo-m-order-throw-after
                        (lambda (&rest _args)
                          (setq log (cons 'after log))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-order-throw-target :before 'neovm--combo-m-order-throw-before)
                        (advice-add 'neovm--combo-m-order-throw-target :around 'neovm--combo-m-order-throw-around)
                        (advice-add 'neovm--combo-m-order-throw-target :after 'neovm--combo-m-order-throw-after)
                        (list
                          (progn
                            (setq log nil)
                            (list
                              (catch 'neovm--combo-m-order-throw-tag
                                (neovm--combo-m-order-throw-target 1))
                              (nreverse log)))
                          (progn
                            (setq log nil)
                            (list
                              (catch 'neovm--combo-m-order-throw-tag
                                (eval '(neovm--combo-m-order-throw-target 1)))
                              (nreverse log)))
                          (progn
                            (setq log nil)
                            (list
                              (catch 'neovm--combo-m-order-throw-tag
                                (funcall 'neovm--combo-m-order-throw-target 1))
                              (nreverse log)))
                          (progn
                            (setq log nil)
                            (list
                              (catch 'neovm--combo-m-order-throw-tag
                                (apply 'neovm--combo-m-order-throw-target '(1)))
                              (nreverse log)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-order-throw-target 'neovm--combo-m-order-throw-after)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-order-throw-target 'neovm--combo-m-order-throw-around)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-order-throw-target 'neovm--combo-m-order-throw-before)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-order-throw-target)
                    (fmakunbound 'neovm--combo-m-order-throw-before)
                    (fmakunbound 'neovm--combo-m-order-throw-around)
                    (fmakunbound 'neovm--combo-m-order-throw-after)))";
    let expect = expect_test::expect![[
        r#""OK ((1 (around-enter before orig around-exit)) (1 (around-enter before orig around-exit)) (1 (around-enter before orig around-exit)) (1 (around-enter before orig around-exit)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_alias_stacked_advice_order_visibility_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((log nil))
                  (fset 'neovm--combo-alias-order-target
                        (lambda (x)
                          (setq log (cons 'orig log))
                          x))
                  (defalias 'neovm--combo-alias-order 'neovm--combo-alias-order-target)
                  (fset 'neovm--combo-alias-order-before
                        (lambda (&rest _args)
                          (setq log (cons 'before log))))
                  (fset 'neovm--combo-alias-order-around
                        (lambda (orig x)
                          (setq log (cons 'around-enter log))
                          (unwind-protect
                              (funcall orig x)
                            (setq log (cons 'around-exit log)))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-alias-order :before 'neovm--combo-alias-order-before)
                        (advice-add 'neovm--combo-alias-order :around 'neovm--combo-alias-order-around)
                        (list
                          (progn
                            (setq log nil)
                            (list (funcall 'neovm--combo-alias-order 1) (nreverse log)))
                          (progn
                            (setq log nil)
                            (list (funcall 'neovm--combo-alias-order-target 1) (nreverse log)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-alias-order 'neovm--combo-alias-order-around)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-alias-order 'neovm--combo-alias-order-before)
                      (error nil))
                    (fmakunbound 'neovm--combo-alias-order)
                    (fmakunbound 'neovm--combo-alias-order-target)
                    (fmakunbound 'neovm--combo-alias-order-before)
                    (fmakunbound 'neovm--combo-alias-order-around)))";
    let expect =
        expect_test::expect![[r#""OK ((1 (around-enter before orig around-exit)) (1 (orig)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_advice_mapc_order_and_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((seen nil))
                  (fset 'neovm--combo-mapc-target (lambda (x) x))
                  (fset 'neovm--combo-mapc-before (lambda (&rest _args) nil))
                  (fset 'neovm--combo-mapc-around (lambda (orig x) (funcall orig (+ x 1))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-mapc-target :before 'neovm--combo-mapc-before)
                        (advice-add 'neovm--combo-mapc-target :around 'neovm--combo-mapc-around)
                        (advice-mapc
                         (lambda (ad props)
                           (setq seen (cons (list ad (plist-get props :where)) seen)))
                         'neovm--combo-mapc-target)
                        (list
                          (nreverse seen)
                          (neovm--combo-mapc-target 3)
                          (eval '(neovm--combo-mapc-target 3))
                          (funcall 'neovm--combo-mapc-target 3)
                          (apply 'neovm--combo-mapc-target '(3))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-mapc-target 'neovm--combo-mapc-around)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-mapc-target 'neovm--combo-mapc-before)
                      (error nil))
                    (fmakunbound 'neovm--combo-mapc-target)
                    (fmakunbound 'neovm--combo-mapc-before)
                    (fmakunbound 'neovm--combo-mapc-around)))";
    let expect = expect_test::expect![[
        r#""OK (((neovm--combo-mapc-around nil) (neovm--combo-mapc-before nil)) 4 4 4 4)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_symbol_function_identity_during_advice_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(let ((orig nil))
                  (fset 'neovm--combo-sf-toggle-target (lambda (x) x))
                  (setq orig (symbol-function 'neovm--combo-sf-toggle-target))
                  (fset 'neovm--combo-sf-toggle-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (list
                        (eq (symbol-function 'neovm--combo-sf-toggle-target) orig)
                        (progn
                          (advice-add 'neovm--combo-sf-toggle-target :filter-return 'neovm--combo-sf-toggle-filter)
                          (list
                            (eq (symbol-function 'neovm--combo-sf-toggle-target) orig)
                            (funcall 'neovm--combo-sf-toggle-target 1)
                            (apply 'neovm--combo-sf-toggle-target '(1))))
                        (progn
                          (advice-remove 'neovm--combo-sf-toggle-target 'neovm--combo-sf-toggle-filter)
                          (list
                            (eq (symbol-function 'neovm--combo-sf-toggle-target) orig)
                            (funcall 'neovm--combo-sf-toggle-target 1)
                            (apply 'neovm--combo-sf-toggle-target '(1)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-sf-toggle-target 'neovm--combo-sf-toggle-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-sf-toggle-target)
                    (fmakunbound 'neovm--combo-sf-toggle-filter)))";
    let expect = expect_test::expect![[r#""OK (t (nil 8 8) (t 1 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_defalias_rebind_under_active_advice_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-alias-rebind-target-a (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-alias-rebind-target-b (lambda (x) (* 2 x)))
                  (defalias 'neovm--combo-alias-rebind 'neovm--combo-alias-rebind-target-a)
                  (fset 'neovm--combo-alias-rebind-filter (lambda (ret) (+ ret 100)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-alias-rebind :filter-return 'neovm--combo-alias-rebind-filter)
                        (list
                          (funcall 'neovm--combo-alias-rebind 3)
                          (funcall 'neovm--combo-alias-rebind-target-a 3)
                          (progn
                            (defalias 'neovm--combo-alias-rebind 'neovm--combo-alias-rebind-target-b)
                            (list
                              (funcall 'neovm--combo-alias-rebind 3)
                              (funcall 'neovm--combo-alias-rebind-target-b 3)
                              (apply 'neovm--combo-alias-rebind '(3))
                              (eval '(neovm--combo-alias-rebind 3))))
                          (progn
                            (advice-remove 'neovm--combo-alias-rebind 'neovm--combo-alias-rebind-filter)
                            (list
                              (funcall 'neovm--combo-alias-rebind 3)
                              (funcall 'neovm--combo-alias-rebind-target-b 3)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-alias-rebind 'neovm--combo-alias-rebind-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-alias-rebind)
                    (fmakunbound 'neovm--combo-alias-rebind-target-a)
                    (fmakunbound 'neovm--combo-alias-rebind-target-b)
                    (fmakunbound 'neovm--combo-alias-rebind-filter)))";
    let expect = expect_test::expect![[r#""OK (104 4 (106 6 106 106) (6 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throw_caught_by_around_toggle_call_paths() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-throw-around-call (x)
                    `(neovm--combo-m-throw-around-target ,x))
                  (fset 'neovm--combo-m-throw-around-target
                        (lambda (x)
                          (throw 'neovm--combo-m-throw-around-tag x)))
                  (fset 'neovm--combo-m-throw-around
                        (lambda (orig x)
                          (+ 100
                             (catch 'neovm--combo-m-throw-around-tag
                               (funcall orig x)))))
                  (unwind-protect
                      (list
                        (progn
                          (advice-add 'neovm--combo-m-throw-around-target :around 'neovm--combo-m-throw-around)
                          (list
                            (catch 'neovm--combo-m-throw-around-tag
                              (neovm--combo-m-throw-around-call 5))
                            (catch 'neovm--combo-m-throw-around-tag
                              (eval '(neovm--combo-m-throw-around-call 5)))
                            (catch 'neovm--combo-m-throw-around-tag
                              (funcall 'neovm--combo-m-throw-around-target 5))
                            (catch 'neovm--combo-m-throw-around-tag
                              (apply 'neovm--combo-m-throw-around-target '(5)))))
                        (progn
                          (advice-remove 'neovm--combo-m-throw-around-target 'neovm--combo-m-throw-around)
                          (list
                            (catch 'neovm--combo-m-throw-around-tag
                              (neovm--combo-m-throw-around-call 5))
                            (catch 'neovm--combo-m-throw-around-tag
                              (eval '(neovm--combo-m-throw-around-call 5)))
                            (catch 'neovm--combo-m-throw-around-tag
                              (funcall 'neovm--combo-m-throw-around-target 5))
                            (catch 'neovm--combo-m-throw-around-tag
                              (apply 'neovm--combo-m-throw-around-target '(5))))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-throw-around-target 'neovm--combo-m-throw-around)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-throw-around-target)
                    (fmakunbound 'neovm--combo-m-throw-around)
                    (fmakunbound 'neovm--combo-m-throw-around-call)))";
    let expect = expect_test::expect![[r#""OK ((105 105 105 105) (5 5 5 5))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_defalias_rebind_filter_args_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-alias-fargs-target-a (lambda (x y) (+ x y)))
                  (fset 'neovm--combo-alias-fargs-target-b (lambda (x y) (* x y)))
                  (defalias 'neovm--combo-alias-fargs 'neovm--combo-alias-fargs-target-a)
                  (fset 'neovm--combo-alias-fargs-filter
                        (lambda (args)
                          (list (+ 10 (car args))
                                (+ 20 (car (cdr args))))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-alias-fargs :filter-args 'neovm--combo-alias-fargs-filter)
                        (list
                          (funcall 'neovm--combo-alias-fargs 1 2)
                          (apply 'neovm--combo-alias-fargs '(1 2))
                          (progn
                            (defalias 'neovm--combo-alias-fargs 'neovm--combo-alias-fargs-target-b)
                            (list
                              (neovm--combo-alias-fargs 1 2)
                              (eval '(neovm--combo-alias-fargs 1 2))
                              (funcall 'neovm--combo-alias-fargs 1 2)
                              (apply 'neovm--combo-alias-fargs '(1 2))
                              (funcall 'neovm--combo-alias-fargs-target-b 1 2)))
                          (progn
                            (advice-remove 'neovm--combo-alias-fargs 'neovm--combo-alias-fargs-filter)
                            (list
                              (funcall 'neovm--combo-alias-fargs 1 2)
                              (apply 'neovm--combo-alias-fargs '(1 2))
                              (funcall 'neovm--combo-alias-fargs-target-b 1 2)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-alias-fargs 'neovm--combo-alias-fargs-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-alias-fargs)
                    (fmakunbound 'neovm--combo-alias-fargs-target-a)
                    (fmakunbound 'neovm--combo-alias-fargs-target-b)
                    (fmakunbound 'neovm--combo-alias-fargs-filter)))";
    let expect = expect_test::expect![[r#""OK (33 33 (242 242 242 242 2) (2 2 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_before_advice_error_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-before-err-call (x)
                    `(neovm--combo-m-before-err-target ,x))
                  (fset 'neovm--combo-m-before-err-target (lambda (x) x))
                  (fset 'neovm--combo-m-before-err
                        (lambda (&rest _args) (/ 1 0)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-before-err-target :before 'neovm--combo-m-before-err)
                        (list
                          (condition-case nil
                              (neovm--combo-m-before-err-call 1)
                            (arith-error 'arith))
                          (condition-case nil
                              (eval '(neovm--combo-m-before-err-call 1))
                            (arith-error 'arith))
                          (condition-case nil
                              (funcall 'neovm--combo-m-before-err-target 1)
                            (arith-error 'arith))
                          (condition-case nil
                              (apply 'neovm--combo-m-before-err-target '(1))
                            (arith-error 'arith))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-before-err-target 'neovm--combo-m-before-err)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-before-err-target)
                    (fmakunbound 'neovm--combo-m-before-err)
                    (fmakunbound 'neovm--combo-m-before-err-call)))";
    let expect = expect_test::expect![[r#""OK (arith arith arith arith)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_multi_stage_advice_removal_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-stage-call (x)
                    `(neovm--combo-m-stage-target ,x))
                  (fset 'neovm--combo-m-stage-target (lambda (x) x))
                  (fset 'neovm--combo-m-stage-before (lambda (&rest _args) nil))
                  (fset 'neovm--combo-m-stage-around
                        (lambda (orig x) (* 2 (funcall orig x))))
                  (fset 'neovm--combo-m-stage-filter (lambda (ret) (+ ret 10)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-stage-target :before 'neovm--combo-m-stage-before)
                        (advice-add 'neovm--combo-m-stage-target :around 'neovm--combo-m-stage-around)
                        (advice-add 'neovm--combo-m-stage-target :filter-return 'neovm--combo-m-stage-filter)
                        (list
                          (list
                            (neovm--combo-m-stage-call 3)
                            (eval '(neovm--combo-m-stage-call 3))
                            (funcall 'neovm--combo-m-stage-target 3)
                            (apply 'neovm--combo-m-stage-target '(3)))
                          (progn
                            (advice-remove 'neovm--combo-m-stage-target 'neovm--combo-m-stage-around)
                            (list
                              (neovm--combo-m-stage-call 3)
                              (eval '(neovm--combo-m-stage-call 3))
                              (funcall 'neovm--combo-m-stage-target 3)
                              (apply 'neovm--combo-m-stage-target '(3))))
                          (progn
                            (advice-remove 'neovm--combo-m-stage-target 'neovm--combo-m-stage-filter)
                            (list
                              (neovm--combo-m-stage-call 3)
                              (eval '(neovm--combo-m-stage-call 3))
                              (funcall 'neovm--combo-m-stage-target 3)
                              (apply 'neovm--combo-m-stage-target '(3))))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-stage-target 'neovm--combo-m-stage-filter)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-stage-target 'neovm--combo-m-stage-around)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-stage-target 'neovm--combo-m-stage-before)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-stage-target)
                    (fmakunbound 'neovm--combo-m-stage-before)
                    (fmakunbound 'neovm--combo-m-stage-around)
                    (fmakunbound 'neovm--combo-m-stage-filter)
                    (fmakunbound 'neovm--combo-m-stage-call)))";
    let expect = expect_test::expect![[r#""OK ((16 16 16 16) (13 13 13 13) (3 3 3 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_symbol_function_capture_across_advice_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-sf-cap-target (lambda (x) x))
                  (fset 'neovm--combo-sf-cap-filter (lambda (ret) (+ ret 7)))
                  (let ((f0 (symbol-function 'neovm--combo-sf-cap-target)))
                    (unwind-protect
                        (list
                          (progn
                            (advice-add 'neovm--combo-sf-cap-target :filter-return 'neovm--combo-sf-cap-filter)
                            (let ((f1 (symbol-function 'neovm--combo-sf-cap-target)))
                              (list
                                (eq f0 f1)
                                (funcall f0 1)
                                (funcall f1 1)
                                (funcall 'neovm--combo-sf-cap-target 1)
                                (apply f1 '(1))
                                (apply 'neovm--combo-sf-cap-target '(1)))))
                          (progn
                            (advice-remove 'neovm--combo-sf-cap-target 'neovm--combo-sf-cap-filter)
                            (let ((f2 (symbol-function 'neovm--combo-sf-cap-target)))
                              (list
                                (eq f0 f2)
                                (funcall f2 1)
                                (funcall 'neovm--combo-sf-cap-target 1)
                                (apply f2 '(1))
                                (apply 'neovm--combo-sf-cap-target '(1))))))
                      (condition-case nil
                          (advice-remove 'neovm--combo-sf-cap-target 'neovm--combo-sf-cap-filter)
                        (error nil))
                      (fmakunbound 'neovm--combo-sf-cap-target)
                      (fmakunbound 'neovm--combo-sf-cap-filter))))";
    let expect = expect_test::expect![[r#""OK ((nil 1 8 8 8 8) (t 1 1 1 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_recursive_around_advice_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-rec-around-call (x)
                    `(neovm--combo-m-rec-around-target ,x))
                  (fset 'neovm--combo-m-rec-around-target
                        (lambda (x) (* 10 x)))
                  (fset 'neovm--combo-m-rec-around
                        (lambda (orig x)
                          (if (= x 0)
                              (funcall orig x)
                            (+ 1 (funcall 'neovm--combo-m-rec-around-target (1- x))))))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-m-rec-around-target :around 'neovm--combo-m-rec-around)
                        (list
                          (neovm--combo-m-rec-around-call 3)
                          (eval '(neovm--combo-m-rec-around-call 3))
                          (funcall 'neovm--combo-m-rec-around-target 3)
                          (apply 'neovm--combo-m-rec-around-target '(3))
                          (funcall (symbol-function 'neovm--combo-m-rec-around-target) 3)))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-rec-around-target 'neovm--combo-m-rec-around)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-rec-around-target)
                    (fmakunbound 'neovm--combo-m-rec-around)
                    (fmakunbound 'neovm--combo-m-rec-around-call)))";
    let expect = expect_test::expect![[r#""OK (3 3 3 3 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_advice_added_on_alias_removed_on_target_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-alias-cross-target (lambda (x) x))
                  (defalias 'neovm--combo-alias-cross 'neovm--combo-alias-cross-target)
                  (fset 'neovm--combo-alias-cross-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (list
                        (progn
                          (advice-add 'neovm--combo-alias-cross :filter-return 'neovm--combo-alias-cross-filter)
                          (list
                            (funcall 'neovm--combo-alias-cross 2)
                            (funcall 'neovm--combo-alias-cross-target 2)
                            (neovm--combo-alias-cross 2)
                            (eval '(neovm--combo-alias-cross 2))
                            (if (advice-member-p 'neovm--combo-alias-cross-filter 'neovm--combo-alias-cross) t nil)
                            (if (advice-member-p 'neovm--combo-alias-cross-filter 'neovm--combo-alias-cross-target) t nil)))
                        (progn
                          (advice-remove 'neovm--combo-alias-cross-target 'neovm--combo-alias-cross-filter)
                          (list
                            (funcall 'neovm--combo-alias-cross 2)
                            (funcall 'neovm--combo-alias-cross-target 2)
                            (neovm--combo-alias-cross 2)
                            (eval '(neovm--combo-alias-cross 2))
                            (if (advice-member-p 'neovm--combo-alias-cross-filter 'neovm--combo-alias-cross) t nil)
                            (if (advice-member-p 'neovm--combo-alias-cross-filter 'neovm--combo-alias-cross-target) t nil))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-alias-cross 'neovm--combo-alias-cross-filter)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-alias-cross-target 'neovm--combo-alias-cross-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-alias-cross)
                    (fmakunbound 'neovm--combo-alias-cross-target)
                    (fmakunbound 'neovm--combo-alias-cross-filter)))";
    let expect = expect_test::expect![[r#""OK ((9 2 9 9 t nil) (9 2 9 9 t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_alias_symbol_function_snapshot_across_rebind_and_advice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-alias-snap-target-a (lambda (x) (+ x 1)))
                  (fset 'neovm--combo-alias-snap-target-b (lambda (x) (* 2 x)))
                  (defalias 'neovm--combo-alias-snap 'neovm--combo-alias-snap-target-a)
                  (fset 'neovm--combo-alias-snap-filter (lambda (ret) (+ ret 100)))
                  (let ((f0 (symbol-function 'neovm--combo-alias-snap)))
                    (unwind-protect
                        (progn
                          (advice-add 'neovm--combo-alias-snap :filter-return 'neovm--combo-alias-snap-filter)
                          (let ((f1 (symbol-function 'neovm--combo-alias-snap)))
                            (list
                              (eq f0 f1)
                              (funcall f0 3)
                              (funcall f1 3)
                              (funcall 'neovm--combo-alias-snap 3)
                              (progn
                                (defalias 'neovm--combo-alias-snap 'neovm--combo-alias-snap-target-b)
                                (list
                                  (funcall f0 3)
                                  (funcall f1 3)
                                  (funcall (symbol-function 'neovm--combo-alias-snap) 3)
                                  (funcall 'neovm--combo-alias-snap 3)
                                  (apply 'neovm--combo-alias-snap '(3))))
                              (progn
                                (advice-remove 'neovm--combo-alias-snap 'neovm--combo-alias-snap-filter)
                                (list
                                  (funcall 'neovm--combo-alias-snap 3)
                                  (funcall (symbol-function 'neovm--combo-alias-snap) 3))))))
                      (condition-case nil
                          (advice-remove 'neovm--combo-alias-snap 'neovm--combo-alias-snap-filter)
                        (error nil))
                      (fmakunbound 'neovm--combo-alias-snap)
                      (fmakunbound 'neovm--combo-alias-snap-target-a)
                      (fmakunbound 'neovm--combo-alias-snap-target-b)
                      (fmakunbound 'neovm--combo-alias-snap-filter))))";
    let expect = expect_test::expect![[r#""OK (nil 4 104 104 (4 104 106 106 106) (6 6))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_advice_added_on_target_removed_on_alias_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-target-cross-target (lambda (x) x))
                  (defalias 'neovm--combo-target-cross 'neovm--combo-target-cross-target)
                  (fset 'neovm--combo-target-cross-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (list
                        (progn
                          (advice-add 'neovm--combo-target-cross-target :filter-return 'neovm--combo-target-cross-filter)
                          (list
                            (funcall 'neovm--combo-target-cross 2)
                            (funcall 'neovm--combo-target-cross-target 2)
                            (neovm--combo-target-cross 2)
                            (eval '(neovm--combo-target-cross 2))
                            (if (advice-member-p 'neovm--combo-target-cross-filter 'neovm--combo-target-cross) t nil)
                            (if (advice-member-p 'neovm--combo-target-cross-filter 'neovm--combo-target-cross-target) t nil)))
                        (progn
                          (advice-remove 'neovm--combo-target-cross 'neovm--combo-target-cross-filter)
                          (list
                            (funcall 'neovm--combo-target-cross 2)
                            (funcall 'neovm--combo-target-cross-target 2)
                            (neovm--combo-target-cross 2)
                            (eval '(neovm--combo-target-cross 2))
                            (if (advice-member-p 'neovm--combo-target-cross-filter 'neovm--combo-target-cross) t nil)
                            (if (advice-member-p 'neovm--combo-target-cross-filter 'neovm--combo-target-cross-target) t nil))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-target-cross-target 'neovm--combo-target-cross-filter)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-target-cross 'neovm--combo-target-cross-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-target-cross)
                    (fmakunbound 'neovm--combo-target-cross-target)
                    (fmakunbound 'neovm--combo-target-cross-filter)))";
    let expect = expect_test::expect![[r#""OK ((9 9 9 9 nil t) (9 9 9 9 nil t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_duplicate_advice_add_remove_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-dup-target (lambda (x) x))
                  (fset 'neovm--combo-dup-filter (lambda (ret) (+ ret 1)))
                  (unwind-protect
                      (list
                        (progn
                          (advice-add 'neovm--combo-dup-target :filter-return 'neovm--combo-dup-filter)
                          (list
                            (funcall 'neovm--combo-dup-target 3)
                            (if (advice-member-p 'neovm--combo-dup-filter 'neovm--combo-dup-target) t nil)))
                        (progn
                          (advice-add 'neovm--combo-dup-target :filter-return 'neovm--combo-dup-filter)
                          (list
                            (funcall 'neovm--combo-dup-target 3)
                            (if (advice-member-p 'neovm--combo-dup-filter 'neovm--combo-dup-target) t nil)))
                        (progn
                          (advice-remove 'neovm--combo-dup-target 'neovm--combo-dup-filter)
                          (list
                            (funcall 'neovm--combo-dup-target 3)
                            (if (advice-member-p 'neovm--combo-dup-filter 'neovm--combo-dup-target) t nil)))
                        (progn
                          (advice-remove 'neovm--combo-dup-target 'neovm--combo-dup-filter)
                          (list
                            (funcall 'neovm--combo-dup-target 3)
                            (if (advice-member-p 'neovm--combo-dup-filter 'neovm--combo-dup-target) t nil))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-dup-target 'neovm--combo-dup-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-dup-target)
                    (fmakunbound 'neovm--combo-dup-filter)))";
    let expect = expect_test::expect![[r#""OK ((4 t) (4 t) (3 nil) (3 nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_captured_advised_function_after_remove_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-cap-remove-target (lambda (x) x))
                  (fset 'neovm--combo-cap-remove-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (progn
                        (advice-add 'neovm--combo-cap-remove-target :filter-return 'neovm--combo-cap-remove-filter)
                        (let ((f1 (symbol-function 'neovm--combo-cap-remove-target)))
                          (list
                            (funcall f1 2)
                            (funcall 'neovm--combo-cap-remove-target 2)
                            (progn
                              (advice-remove 'neovm--combo-cap-remove-target 'neovm--combo-cap-remove-filter)
                              (list
                                (funcall f1 2)
                                (funcall 'neovm--combo-cap-remove-target 2)
                                (apply f1 '(2))
                                (apply 'neovm--combo-cap-remove-target '(2))
                                (eq (symbol-function 'neovm--combo-cap-remove-target) f1))))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-cap-remove-target 'neovm--combo-cap-remove-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-cap-remove-target)
                    (fmakunbound 'neovm--combo-cap-remove-filter)))";
    let expect = expect_test::expect![[r#""OK (9 9 (9 2 9 2 nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_macro_eval_advice_toggle_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-m-eval-toggle-call (x)
                    `(neovm--combo-m-eval-toggle-target ,x))
                  (fset 'neovm--combo-m-eval-toggle-target (lambda (x) x))
                  (fset 'neovm--combo-m-eval-toggle-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (list
                        (eval '(neovm--combo-m-eval-toggle-call 2))
                        (progn
                          (advice-add 'neovm--combo-m-eval-toggle-target :filter-return 'neovm--combo-m-eval-toggle-filter)
                          (list
                            (eval '(neovm--combo-m-eval-toggle-call 2))
                            (neovm--combo-m-eval-toggle-call 2)
                            (funcall 'neovm--combo-m-eval-toggle-target 2)
                            (apply 'neovm--combo-m-eval-toggle-target '(2))))
                        (progn
                          (advice-remove 'neovm--combo-m-eval-toggle-target 'neovm--combo-m-eval-toggle-filter)
                          (list
                            (eval '(neovm--combo-m-eval-toggle-call 2))
                            (neovm--combo-m-eval-toggle-call 2)
                            (funcall 'neovm--combo-m-eval-toggle-target 2)
                            (apply 'neovm--combo-m-eval-toggle-target '(2)))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-m-eval-toggle-target 'neovm--combo-m-eval-toggle-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-m-eval-toggle-target)
                    (fmakunbound 'neovm--combo-m-eval-toggle-filter)
                    (fmakunbound 'neovm--combo-m-eval-toggle-call)))";
    let expect = expect_test::expect![[r#""OK (2 (9 9 9 9) (2 2 2 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_two_aliases_cross_advice_remove_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-two-alias-target (lambda (x) x))
                  (defalias 'neovm--combo-two-alias-a 'neovm--combo-two-alias-target)
                  (defalias 'neovm--combo-two-alias-b 'neovm--combo-two-alias-target)
                  (fset 'neovm--combo-two-alias-filter (lambda (ret) (+ ret 7)))
                  (unwind-protect
                      (list
                        (progn
                          (advice-add 'neovm--combo-two-alias-a :filter-return 'neovm--combo-two-alias-filter)
                          (list
                            (funcall 'neovm--combo-two-alias-a 2)
                            (funcall 'neovm--combo-two-alias-b 2)
                            (funcall 'neovm--combo-two-alias-target 2)
                            (if (advice-member-p 'neovm--combo-two-alias-filter 'neovm--combo-two-alias-a) t nil)
                            (if (advice-member-p 'neovm--combo-two-alias-filter 'neovm--combo-two-alias-b) t nil)
                            (if (advice-member-p 'neovm--combo-two-alias-filter 'neovm--combo-two-alias-target) t nil)))
                        (progn
                          (advice-remove 'neovm--combo-two-alias-b 'neovm--combo-two-alias-filter)
                          (list
                            (funcall 'neovm--combo-two-alias-a 2)
                            (funcall 'neovm--combo-two-alias-b 2)
                            (funcall 'neovm--combo-two-alias-target 2)
                            (if (advice-member-p 'neovm--combo-two-alias-filter 'neovm--combo-two-alias-a) t nil)
                            (if (advice-member-p 'neovm--combo-two-alias-filter 'neovm--combo-two-alias-b) t nil)
                            (if (advice-member-p 'neovm--combo-two-alias-filter 'neovm--combo-two-alias-target) t nil))))
                    (condition-case nil
                        (advice-remove 'neovm--combo-two-alias-a 'neovm--combo-two-alias-filter)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-two-alias-b 'neovm--combo-two-alias-filter)
                      (error nil))
                    (condition-case nil
                        (advice-remove 'neovm--combo-two-alias-target 'neovm--combo-two-alias-filter)
                      (error nil))
                    (fmakunbound 'neovm--combo-two-alias-target)
                    (fmakunbound 'neovm--combo-two-alias-a)
                    (fmakunbound 'neovm--combo-two-alias-b)
                    (fmakunbound 'neovm--combo-two-alias-filter)))";
    let expect = expect_test::expect![[r#""OK ((9 2 2 t nil nil) (9 2 2 t nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_throwing_before_advice_cleanup_removal_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (fset 'neovm--combo-clean-target (lambda (x) x))
                  (fset 'neovm--combo-clean-before
                        (lambda (&rest _args)
                          (throw 'neovm--combo-clean-tag 99)))
                  (list
                    (let ((after nil))
                      (list
                        (catch 'neovm--combo-clean-tag
                          (unwind-protect
                              (progn
                                (advice-add 'neovm--combo-clean-target :before 'neovm--combo-clean-before)
                                (neovm--combo-clean-target 1))
                            (condition-case nil
                                (advice-remove 'neovm--combo-clean-target 'neovm--combo-clean-before)
                              (error nil))
                            (setq after (neovm--combo-clean-target 1))))
                        after))
                    (let ((after nil))
                      (list
                        (catch 'neovm--combo-clean-tag
                          (unwind-protect
                              (progn
                                (advice-add 'neovm--combo-clean-target :before 'neovm--combo-clean-before)
                                (eval '(neovm--combo-clean-target 1)))
                            (condition-case nil
                                (advice-remove 'neovm--combo-clean-target 'neovm--combo-clean-before)
                              (error nil))
                            (setq after (neovm--combo-clean-target 1))))
                        after))
                    (let ((after nil))
                      (list
                        (catch 'neovm--combo-clean-tag
                          (unwind-protect
                              (progn
                                (advice-add 'neovm--combo-clean-target :before 'neovm--combo-clean-before)
                                (funcall 'neovm--combo-clean-target 1))
                            (condition-case nil
                                (advice-remove 'neovm--combo-clean-target 'neovm--combo-clean-before)
                              (error nil))
                            (setq after (neovm--combo-clean-target 1))))
                        after))
                    (let ((after nil))
                      (list
                        (catch 'neovm--combo-clean-tag
                          (unwind-protect
                              (progn
                                (advice-add 'neovm--combo-clean-target :before 'neovm--combo-clean-before)
                                (apply 'neovm--combo-clean-target '(1)))
                            (condition-case nil
                                (advice-remove 'neovm--combo-clean-target 'neovm--combo-clean-before)
                              (error nil))
                            (setq after (neovm--combo-clean-target 1))))
                        after)))
                  )";
    let expect = expect_test::expect![[r#""OK ((99 1) (99 1) (99 1) (99 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_advice_depth_order_call_path_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-depth-call (x)
                    `(neovm--combo-depth-target ,x))
                  (let ((log nil))
                    (fset 'neovm--combo-depth-target
                          (lambda (x)
                            (setq log (cons (list 'orig x) log))
                            (+ x 3)))
                    (fset 'neovm--combo-depth-before-low
                          (lambda (&rest args)
                            (setq log (cons (cons 'before-low args) log))))
                    (fset 'neovm--combo-depth-before-high
                          (lambda (&rest args)
                            (setq log (cons (cons 'before-high args) log))))
                    (fset 'neovm--combo-depth-around-low
                          (lambda (orig x)
                            (setq log (cons (list 'around-low-enter x) log))
                            (let ((ret (funcall orig (+ x 1))))
                              (setq log (cons (list 'around-low-exit ret) log))
                              (+ ret 10))))
                    (fset 'neovm--combo-depth-around-high
                          (lambda (orig x)
                            (setq log (cons (list 'around-high-enter x) log))
                            (let ((ret (funcall orig (* x 2))))
                              (setq log (cons (list 'around-high-exit ret) log))
                              (* ret 2))))
                    (fset 'neovm--combo-depth-after-low
                          (lambda (&rest args)
                            (setq log (cons (cons 'after-low args) log))))
                    (fset 'neovm--combo-depth-after-high
                          (lambda (&rest args)
                            (setq log (cons (cons 'after-high args) log))))
                    (unwind-protect
                        (progn
                          (advice-add 'neovm--combo-depth-target :before 'neovm--combo-depth-before-low '((depth . -90)))
                          (advice-add 'neovm--combo-depth-target :before 'neovm--combo-depth-before-high '((depth . 90)))
                          (advice-add 'neovm--combo-depth-target :around 'neovm--combo-depth-around-low '((depth . -30)))
                          (advice-add 'neovm--combo-depth-target :around 'neovm--combo-depth-around-high '((depth . 30)))
                          (advice-add 'neovm--combo-depth-target :after 'neovm--combo-depth-after-low '((depth . -70)))
                          (advice-add 'neovm--combo-depth-target :after 'neovm--combo-depth-after-high '((depth . 70)))
                          (list
                            (let ((log nil))
                              (list
                                (neovm--combo-depth-call 4)
                                (nreverse log)))
                            (let ((log nil))
                              (list
                                (eval '(neovm--combo-depth-call 4))
                                (nreverse log)))
                            (let ((log nil))
                              (list
                                (funcall 'neovm--combo-depth-target 4)
                                (nreverse log)))
                            (let ((log nil))
                              (list
                                (apply 'neovm--combo-depth-target '(4))
                                (nreverse log)))))
                      (condition-case nil
                          (advice-remove 'neovm--combo-depth-target 'neovm--combo-depth-before-low)
                        (error nil))
                      (condition-case nil
                          (advice-remove 'neovm--combo-depth-target 'neovm--combo-depth-before-high)
                        (error nil))
                      (condition-case nil
                          (advice-remove 'neovm--combo-depth-target 'neovm--combo-depth-around-low)
                        (error nil))
                      (condition-case nil
                          (advice-remove 'neovm--combo-depth-target 'neovm--combo-depth-around-high)
                        (error nil))
                      (condition-case nil
                          (advice-remove 'neovm--combo-depth-target 'neovm--combo-depth-after-low)
                        (error nil))
                      (condition-case nil
                          (advice-remove 'neovm--combo-depth-target 'neovm--combo-depth-after-high)
                        (error nil))
                      (fmakunbound 'neovm--combo-depth-target)
                      (fmakunbound 'neovm--combo-depth-before-low)
                      (fmakunbound 'neovm--combo-depth-before-high)
                      (fmakunbound 'neovm--combo-depth-around-low)
                      (fmakunbound 'neovm--combo-depth-around-high)
                      (fmakunbound 'neovm--combo-depth-after-low)
                      (fmakunbound 'neovm--combo-depth-after-high)
                      (fmakunbound 'neovm--combo-depth-call))))";
    let expect = expect_test::expect![[r#""OK ((36 nil) (36 nil) (36 nil) (36 nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_anonymous_around_advice_alias_remove_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(progn
                  (defmacro neovm--combo-anon-call (x)
                    `(neovm--combo-anon-target ,x))
                  (let ((log nil)
                        (adv (let ((delta 11))
                               (lambda (orig x)
                                 (+ delta (funcall orig x))))))
                    (fset 'neovm--combo-anon-target
                          (lambda (x)
                            (setq log (cons x log))
                            (+ x 1)))
                    (defalias 'neovm--combo-anon-alias 'neovm--combo-anon-target)
                    (unwind-protect
                        (list
                          (progn
                            (advice-add 'neovm--combo-anon-target :around adv '((name . neovm--combo-anon-name)))
                            (list
                              (neovm--combo-anon-call 3)
                              (eval '(neovm--combo-anon-call 3))
                              (funcall 'neovm--combo-anon-target 3)
                              (apply 'neovm--combo-anon-target '(3))
                              (funcall (symbol-function 'neovm--combo-anon-target) 3)
                              (if (advice-member-p adv 'neovm--combo-anon-target) t nil)
                              (if (advice-member-p adv 'neovm--combo-anon-alias) t nil)
                              (nreverse log)))
                          (progn
                            (advice-remove 'neovm--combo-anon-alias adv)
                            (setq log nil)
                            (list
                              (neovm--combo-anon-call 3)
                              (eval '(neovm--combo-anon-call 3))
                              (funcall 'neovm--combo-anon-target 3)
                              (apply 'neovm--combo-anon-target '(3))
                              (funcall (symbol-function 'neovm--combo-anon-target) 3)
                              (if (advice-member-p adv 'neovm--combo-anon-target) t nil)
                              (if (advice-member-p adv 'neovm--combo-anon-alias) t nil)
                              (nreverse log))))
                      (condition-case nil
                          (advice-remove 'neovm--combo-anon-target adv)
                        (error nil))
                      (condition-case nil
                          (advice-remove 'neovm--combo-anon-alias adv)
                        (error nil))
                      (fmakunbound 'neovm--combo-anon-target)
                      (fmakunbound 'neovm--combo-anon-alias)
                      (fmakunbound 'neovm--combo-anon-call))))";
    let expect = expect_test::expect![[
        r#""OK ((15 15 15 15 15 t nil (3 3 3 3 3)) (15 15 15 15 15 t nil (3 3 3 3 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_combination_anonymous_advice_symbol_function_capture_rebind_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-anon-rebind-call (x)
             `(neovm--combo-anon-rebind-target ,x))
           (fset 'neovm--combo-anon-rebind-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-anon-rebind-alias 'neovm--combo-anon-rebind-target)
           (let* ((adv (let ((d {delta}))
                         (lambda (orig x)
                           (+ d (funcall orig x)))))
                  (f0 nil))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-anon-rebind-target :around adv)
                     (setq f0 (symbol-function 'neovm--combo-anon-rebind-target))
                     (list
                       (neovm--combo-anon-rebind-call {n})
                       (eval '(neovm--combo-anon-rebind-call {n}))
                       (funcall 'neovm--combo-anon-rebind-target {n})
                       (funcall 'neovm--combo-anon-rebind-alias {n})
                       (funcall f0 {n})
                       (progn
                         (fset 'neovm--combo-anon-rebind-target (lambda (x) (* x {mul})))
                         (list
                           (neovm--combo-anon-rebind-call {n})
                           (eval '(neovm--combo-anon-rebind-call {n}))
                           (funcall 'neovm--combo-anon-rebind-target {n})
                           (funcall 'neovm--combo-anon-rebind-alias {n})
                           (funcall f0 {n})
                           (apply f0 (list {n}))
                           (apply 'neovm--combo-anon-rebind-target (list {n}))))))
                   (progn
                     (advice-remove 'neovm--combo-anon-rebind-alias adv)
                     (list
                       (neovm--combo-anon-rebind-call {n})
                       (eval '(neovm--combo-anon-rebind-call {n}))
                       (funcall 'neovm--combo-anon-rebind-target {n})
                       (funcall 'neovm--combo-anon-rebind-alias {n})
                       (funcall f0 {n})
                       (apply f0 (list {n}))
                       (if (advice-member-p adv 'neovm--combo-anon-rebind-target) t nil)
                       (if (advice-member-p adv 'neovm--combo-anon-rebind-alias) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-anon-rebind-target adv)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-anon-rebind-alias adv)
                 (error nil))
               (fmakunbound 'neovm--combo-anon-rebind-target)
               (fmakunbound 'neovm--combo-anon-rebind-alias)
               (fmakunbound 'neovm--combo-anon-rebind-call))))",
        n = 2i64,
        delta = 9i64,
        mul = 10i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_distinct_anonymous_around_chain_remove_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-anon-chain-call (x)
             `(neovm--combo-anon-chain-target ,x))
           (fset 'neovm--combo-anon-chain-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-anon-chain-alias 'neovm--combo-anon-chain-target)
           (let* ((a1 (let ((d {d1}))
                        (lambda (orig x)
                          (+ d (funcall orig x)))))
                  (a2 (let ((m {m2}))
                        (lambda (orig x)
                          (* m (funcall orig x))))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-anon-chain-target :around a1 '((name . neovm--combo-anon-chain-a1) (depth . -20)))
                     (advice-add 'neovm--combo-anon-chain-target :around a2 '((name . neovm--combo-anon-chain-a2) (depth . 20)))
                     (list
                       (neovm--combo-anon-chain-call {n})
                       (eval '(neovm--combo-anon-chain-call {n}))
                       (funcall 'neovm--combo-anon-chain-target {n})
                       (apply 'neovm--combo-anon-chain-target (list {n}))
                       (if (advice-member-p a1 'neovm--combo-anon-chain-target) t nil)
                       (if (advice-member-p a2 'neovm--combo-anon-chain-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-anon-chain-target a1)
                     (list
                       (neovm--combo-anon-chain-call {n})
                       (eval '(neovm--combo-anon-chain-call {n}))
                       (funcall 'neovm--combo-anon-chain-target {n})
                       (apply 'neovm--combo-anon-chain-target (list {n}))
                       (if (advice-member-p a1 'neovm--combo-anon-chain-target) t nil)
                       (if (advice-member-p a2 'neovm--combo-anon-chain-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-anon-chain-alias a2)
                     (list
                       (neovm--combo-anon-chain-call {n})
                       (eval '(neovm--combo-anon-chain-call {n}))
                       (funcall 'neovm--combo-anon-chain-target {n})
                       (apply 'neovm--combo-anon-chain-target (list {n}))
                       (if (advice-member-p a1 'neovm--combo-anon-chain-target) t nil)
                       (if (advice-member-p a2 'neovm--combo-anon-chain-target) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-anon-chain-target a1)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-anon-chain-target a2)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-anon-chain-alias a1)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-anon-chain-alias a2)
                 (error nil))
               (fmakunbound 'neovm--combo-anon-chain-target)
               (fmakunbound 'neovm--combo-anon-chain-alias)
               (fmakunbound 'neovm--combo-anon-chain-call))))",
        n = 3i64,
        d1 = 7i64,
        m2 = 5i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_fmakunbound_rebind_under_anonymous_advice_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-fm-call (x)
             `(neovm--combo-fm-target ,x))
           (fset 'neovm--combo-fm-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-fm-alias 'neovm--combo-fm-target)
           (let* ((adv (let ((d {delta}))
                         (lambda (orig x)
                           (+ d (funcall orig x)))))
                  (f0 nil))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-fm-target :around adv)
                     (setq f0 (symbol-function 'neovm--combo-fm-target))
                     (list
                       (neovm--combo-fm-call {n})
                       (eval '(neovm--combo-fm-call {n}))
                       (funcall 'neovm--combo-fm-target {n})
                       (funcall 'neovm--combo-fm-alias {n})
                       (funcall f0 {n})
                       (progn
                         (fmakunbound 'neovm--combo-fm-target)
                         (list
                           (condition-case err
                               (funcall 'neovm--combo-fm-alias {n})
                             (error (list 'err (car err))))
                           (progn
                             (fset 'neovm--combo-fm-target (lambda (x) (* x {mul})))
                             (list
                               (neovm--combo-fm-call {n})
                               (eval '(neovm--combo-fm-call {n}))
                               (funcall 'neovm--combo-fm-target {n})
                               (funcall 'neovm--combo-fm-alias {n})
                               (funcall f0 {n})
                               (apply f0 (list {n}))
                               (if (advice-member-p adv 'neovm--combo-fm-target) t nil)
                               (if (advice-member-p adv 'neovm--combo-fm-alias) t nil)))))))
                   (progn
                     (condition-case err
                         (progn
                           (advice-remove 'neovm--combo-fm-alias adv)
                           'removed)
                       (error (list 'remove-err (car err))))
                     (list
                       (neovm--combo-fm-call {n})
                       (eval '(neovm--combo-fm-call {n}))
                       (funcall 'neovm--combo-fm-target {n})
                       (funcall 'neovm--combo-fm-alias {n})
                       (funcall f0 {n})
                       (apply f0 (list {n}))
                       (if (advice-member-p adv 'neovm--combo-fm-target) t nil)
                       (if (advice-member-p adv 'neovm--combo-fm-alias) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-fm-target adv)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-fm-alias adv)
                 (error nil))
               (fmakunbound 'neovm--combo-fm-target)
               (fmakunbound 'neovm--combo-fm-alias)
               (fmakunbound 'neovm--combo-fm-call))))",
        n = 4i64,
        delta = 8i64,
        mul = 6i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_duplicate_same_anonymous_advice_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-dup-anon-call (x)
             `(neovm--combo-dup-anon-target ,x))
           (fset 'neovm--combo-dup-anon-target (lambda (x) (+ x 1)))
           (let ((adv (let ((d {delta}))
                        (lambda (orig x)
                          (+ d (funcall orig x))))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-dup-anon-target :around adv '((name . neovm--combo-dup-anon-a1) (depth . -10)))
                     (advice-add 'neovm--combo-dup-anon-target :around adv '((name . neovm--combo-dup-anon-a2) (depth . 10)))
                     (list
                       (neovm--combo-dup-anon-call {n})
                       (eval '(neovm--combo-dup-anon-call {n}))
                       (funcall 'neovm--combo-dup-anon-target {n})
                       (apply 'neovm--combo-dup-anon-target (list {n}))
                       (if (advice-member-p adv 'neovm--combo-dup-anon-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-dup-anon-target adv)
                     (list
                       (neovm--combo-dup-anon-call {n})
                       (eval '(neovm--combo-dup-anon-call {n}))
                       (funcall 'neovm--combo-dup-anon-target {n})
                       (apply 'neovm--combo-dup-anon-target (list {n}))
                       (if (advice-member-p adv 'neovm--combo-dup-anon-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-dup-anon-target adv)
                     (list
                       (neovm--combo-dup-anon-call {n})
                       (eval '(neovm--combo-dup-anon-call {n}))
                       (funcall 'neovm--combo-dup-anon-target {n})
                       (apply 'neovm--combo-dup-anon-target (list {n}))
                       (if (advice-member-p adv 'neovm--combo-dup-anon-target) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-dup-anon-target adv)
                 (error nil))
               (fmakunbound 'neovm--combo-dup-anon-target)
               (fmakunbound 'neovm--combo-dup-anon-call))))",
        n = 5i64,
        delta = 4i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_around_filter_return_rebind_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-arf-call (x)
             `(neovm--combo-arf-target ,x))
           (fset 'neovm--combo-arf-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-arf-alias 'neovm--combo-arf-target)
           (fset 'neovm--combo-arf-around
                 (lambda (orig x)
                   (+ 10 (funcall orig x))))
           (fset 'neovm--combo-arf-filter-ret
                 (lambda (ret)
                   (* ret 3)))
           (unwind-protect
               (list
                 (progn
                   (advice-add 'neovm--combo-arf-target :around 'neovm--combo-arf-around)
                   (advice-add 'neovm--combo-arf-target :filter-return 'neovm--combo-arf-filter-ret)
                   (list
                     (neovm--combo-arf-call {n})
                     (eval '(neovm--combo-arf-call {n}))
                     (funcall 'neovm--combo-arf-target {n})
                     (apply 'neovm--combo-arf-target (list {n}))
                     (funcall 'neovm--combo-arf-alias {n})
                     (if (advice-member-p 'neovm--combo-arf-around 'neovm--combo-arf-target) t nil)
                     (if (advice-member-p 'neovm--combo-arf-filter-ret 'neovm--combo-arf-target) t nil)))
                 (progn
                   (fset 'neovm--combo-arf-target (lambda (x) (* x {mul})))
                   (list
                     (neovm--combo-arf-call {n})
                     (eval '(neovm--combo-arf-call {n}))
                     (funcall 'neovm--combo-arf-target {n})
                     (apply 'neovm--combo-arf-target (list {n}))
                     (funcall 'neovm--combo-arf-alias {n})
                     (if (advice-member-p 'neovm--combo-arf-around 'neovm--combo-arf-target) t nil)
                     (if (advice-member-p 'neovm--combo-arf-filter-ret 'neovm--combo-arf-target) t nil)))
                 (progn
                   (advice-remove 'neovm--combo-arf-alias 'neovm--combo-arf-filter-ret)
                   (advice-remove 'neovm--combo-arf-target 'neovm--combo-arf-around)
                   (list
                     (neovm--combo-arf-call {n})
                     (eval '(neovm--combo-arf-call {n}))
                     (funcall 'neovm--combo-arf-target {n})
                     (apply 'neovm--combo-arf-target (list {n}))
                     (funcall 'neovm--combo-arf-alias {n})
                     (if (advice-member-p 'neovm--combo-arf-around 'neovm--combo-arf-target) t nil)
                     (if (advice-member-p 'neovm--combo-arf-filter-ret 'neovm--combo-arf-target) t nil))))
             (condition-case nil
                 (advice-remove 'neovm--combo-arf-target 'neovm--combo-arf-around)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-arf-target 'neovm--combo-arf-filter-ret)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-arf-alias 'neovm--combo-arf-around)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-arf-alias 'neovm--combo-arf-filter-ret)
               (error nil))
             (fmakunbound 'neovm--combo-arf-target)
             (fmakunbound 'neovm--combo-arf-alias)
             (fmakunbound 'neovm--combo-arf-around)
             (fmakunbound 'neovm--combo-arf-filter-ret)
             (fmakunbound 'neovm--combo-arf-call)))",
        n = 4i64,
        mul = 6i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_capture_combined_advice_then_rebind_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-cap-combined-call (x)
             `(neovm--combo-cap-combined-target ,x))
           (fset 'neovm--combo-cap-combined-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-cap-combined-alias 'neovm--combo-cap-combined-target)
           (fset 'neovm--combo-cap-combined-around
                 (lambda (orig x)
                   (+ 10 (funcall orig x))))
           (fset 'neovm--combo-cap-combined-ret
                 (lambda (ret)
                   (* ret 3)))
           (let ((f0 nil))
             (unwind-protect
                 (progn
                   (advice-add 'neovm--combo-cap-combined-target :around 'neovm--combo-cap-combined-around)
                   (advice-add 'neovm--combo-cap-combined-target :filter-return 'neovm--combo-cap-combined-ret)
                   (setq f0 (symbol-function 'neovm--combo-cap-combined-target))
                   (list
                     (list
                       (neovm--combo-cap-combined-call {n})
                       (eval '(neovm--combo-cap-combined-call {n}))
                       (funcall 'neovm--combo-cap-combined-target {n})
                       (apply 'neovm--combo-cap-combined-target (list {n}))
                       (funcall 'neovm--combo-cap-combined-alias {n})
                       (funcall f0 {n})
                       (funcall (symbol-function 'neovm--combo-cap-combined-target) {n})
                       (if (advice-member-p 'neovm--combo-cap-combined-around 'neovm--combo-cap-combined-target) t nil)
                       (if (advice-member-p 'neovm--combo-cap-combined-ret 'neovm--combo-cap-combined-target) t nil))
                     (progn
                       (fset 'neovm--combo-cap-combined-target (lambda (x) (* x {mul})))
                       (list
                         (neovm--combo-cap-combined-call {n})
                         (eval '(neovm--combo-cap-combined-call {n}))
                         (funcall 'neovm--combo-cap-combined-target {n})
                         (apply 'neovm--combo-cap-combined-target (list {n}))
                         (funcall 'neovm--combo-cap-combined-alias {n})
                         (funcall f0 {n})
                         (funcall (symbol-function 'neovm--combo-cap-combined-target) {n})
                         (if (advice-member-p 'neovm--combo-cap-combined-around 'neovm--combo-cap-combined-target) t nil)
                         (if (advice-member-p 'neovm--combo-cap-combined-ret 'neovm--combo-cap-combined-target) t nil)))))
               (condition-case nil
                   (advice-remove 'neovm--combo-cap-combined-target 'neovm--combo-cap-combined-around)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-cap-combined-target 'neovm--combo-cap-combined-ret)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-cap-combined-alias 'neovm--combo-cap-combined-around)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-cap-combined-alias 'neovm--combo-cap-combined-ret)
                 (error nil))
               (fmakunbound 'neovm--combo-cap-combined-target)
               (fmakunbound 'neovm--combo-cap-combined-alias)
               (fmakunbound 'neovm--combo-cap-combined-around)
               (fmakunbound 'neovm--combo-cap-combined-ret)
               (fmakunbound 'neovm--combo-cap-combined-call))))",
        n = 4i64,
        mul = 6i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_alias_rebind_with_split_advice_and_captured_cells_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-split-call-target (x)
             `(neovm--combo-split-target ,x))
           (defmacro neovm--combo-split-call-alias (x)
             `(neovm--combo-split-alias ,x))
           (fset 'neovm--combo-split-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-split-alias 'neovm--combo-split-target)
           (fset 'neovm--combo-split-around
                 (lambda (orig x)
                   (+ 10 (funcall orig x))))
           (fset 'neovm--combo-split-ret
                 (lambda (ret)
                   (* ret 3)))
           (let ((fa nil) (ft nil))
             (unwind-protect
                 (progn
                   (advice-add 'neovm--combo-split-alias :around 'neovm--combo-split-around)
                   (advice-add 'neovm--combo-split-target :filter-return 'neovm--combo-split-ret)
                   (setq fa (symbol-function 'neovm--combo-split-alias))
                   (setq ft (symbol-function 'neovm--combo-split-target))
                   (list
                     (list
                       (neovm--combo-split-call-target {n})
                       (eval '(neovm--combo-split-call-target {n}))
                       (neovm--combo-split-call-alias {n})
                       (eval '(neovm--combo-split-call-alias {n}))
                       (funcall 'neovm--combo-split-target {n})
                       (funcall 'neovm--combo-split-alias {n})
                       (funcall fa {n})
                       (funcall ft {n})
                       (if (advice-member-p 'neovm--combo-split-around 'neovm--combo-split-target) t nil)
                       (if (advice-member-p 'neovm--combo-split-around 'neovm--combo-split-alias) t nil)
                       (if (advice-member-p 'neovm--combo-split-ret 'neovm--combo-split-target) t nil)
                       (if (advice-member-p 'neovm--combo-split-ret 'neovm--combo-split-alias) t nil))
                     (progn
                       (defalias 'neovm--combo-split-alias (lambda (x) (* x {mul})))
                       (list
                         (neovm--combo-split-call-target {n})
                         (eval '(neovm--combo-split-call-target {n}))
                         (neovm--combo-split-call-alias {n})
                         (eval '(neovm--combo-split-call-alias {n}))
                         (funcall 'neovm--combo-split-target {n})
                         (funcall 'neovm--combo-split-alias {n})
                         (funcall fa {n})
                         (funcall ft {n})
                         (if (advice-member-p 'neovm--combo-split-around 'neovm--combo-split-target) t nil)
                         (if (advice-member-p 'neovm--combo-split-around 'neovm--combo-split-alias) t nil)
                         (if (advice-member-p 'neovm--combo-split-ret 'neovm--combo-split-target) t nil)
                         (if (advice-member-p 'neovm--combo-split-ret 'neovm--combo-split-alias) t nil)))))
               (condition-case nil
                   (advice-remove 'neovm--combo-split-target 'neovm--combo-split-around)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-split-target 'neovm--combo-split-ret)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-split-alias 'neovm--combo-split-around)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-split-alias 'neovm--combo-split-ret)
                 (error nil))
               (fmakunbound 'neovm--combo-split-target)
               (fmakunbound 'neovm--combo-split-alias)
               (fmakunbound 'neovm--combo-split-around)
               (fmakunbound 'neovm--combo-split-ret)
               (fmakunbound 'neovm--combo-split-call-target)
               (fmakunbound 'neovm--combo-split-call-alias))))",
        n = 4i64,
        mul = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_fset_alias_unlink_under_stacked_advice_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-fset-alias-call-a (x)
             `(neovm--combo-fset-alias-a ,x))
           (defmacro neovm--combo-fset-alias-call-t (x)
             `(neovm--combo-fset-alias-target ,x))
           (fset 'neovm--combo-fset-alias-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-fset-alias-a 'neovm--combo-fset-alias-target)
           (fset 'neovm--combo-fset-alias-around
                 (lambda (orig x)
                   (+ 10 (funcall orig x))))
           (fset 'neovm--combo-fset-alias-ret
                 (lambda (ret)
                   (* ret 2)))
           (let ((fa0 nil) (ft0 nil))
             (unwind-protect
                 (progn
                   (advice-add 'neovm--combo-fset-alias-a :around 'neovm--combo-fset-alias-around)
                   (advice-add 'neovm--combo-fset-alias-a :filter-return 'neovm--combo-fset-alias-ret)
                   (setq fa0 (symbol-function 'neovm--combo-fset-alias-a))
                   (setq ft0 (symbol-function 'neovm--combo-fset-alias-target))
                   (list
                     (list
                       (neovm--combo-fset-alias-call-a {n})
                       (eval '(neovm--combo-fset-alias-call-a {n}))
                       (neovm--combo-fset-alias-call-t {n})
                       (eval '(neovm--combo-fset-alias-call-t {n}))
                       (funcall 'neovm--combo-fset-alias-a {n})
                       (funcall 'neovm--combo-fset-alias-target {n})
                       (funcall fa0 {n})
                       (funcall ft0 {n})
                       (if (advice-member-p 'neovm--combo-fset-alias-around 'neovm--combo-fset-alias-a) t nil)
                       (if (advice-member-p 'neovm--combo-fset-alias-ret 'neovm--combo-fset-alias-a) t nil)
                       (if (advice-member-p 'neovm--combo-fset-alias-around 'neovm--combo-fset-alias-target) t nil)
                       (if (advice-member-p 'neovm--combo-fset-alias-ret 'neovm--combo-fset-alias-target) t nil))
                     (progn
                       (fset 'neovm--combo-fset-alias-a (lambda (x) (* x {mul})))
                       (list
                         (neovm--combo-fset-alias-call-a {n})
                         (eval '(neovm--combo-fset-alias-call-a {n}))
                         (neovm--combo-fset-alias-call-t {n})
                         (eval '(neovm--combo-fset-alias-call-t {n}))
                         (funcall 'neovm--combo-fset-alias-a {n})
                         (funcall 'neovm--combo-fset-alias-target {n})
                         (funcall fa0 {n})
                         (funcall ft0 {n})
                         (if (advice-member-p 'neovm--combo-fset-alias-around 'neovm--combo-fset-alias-a) t nil)
                         (if (advice-member-p 'neovm--combo-fset-alias-ret 'neovm--combo-fset-alias-a) t nil)
                         (if (advice-member-p 'neovm--combo-fset-alias-around 'neovm--combo-fset-alias-target) t nil)
                         (if (advice-member-p 'neovm--combo-fset-alias-ret 'neovm--combo-fset-alias-target) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-fset-alias-a 'neovm--combo-fset-alias-around)
                       (advice-remove 'neovm--combo-fset-alias-a 'neovm--combo-fset-alias-ret)
                       (list
                         (neovm--combo-fset-alias-call-a {n})
                         (eval '(neovm--combo-fset-alias-call-a {n}))
                         (neovm--combo-fset-alias-call-t {n})
                         (eval '(neovm--combo-fset-alias-call-t {n}))
                         (funcall 'neovm--combo-fset-alias-a {n})
                         (funcall 'neovm--combo-fset-alias-target {n})
                         (funcall fa0 {n})
                         (funcall ft0 {n})
                         (if (advice-member-p 'neovm--combo-fset-alias-around 'neovm--combo-fset-alias-a) t nil)
                         (if (advice-member-p 'neovm--combo-fset-alias-ret 'neovm--combo-fset-alias-a) t nil)
                         (if (advice-member-p 'neovm--combo-fset-alias-around 'neovm--combo-fset-alias-target) t nil)
                         (if (advice-member-p 'neovm--combo-fset-alias-ret 'neovm--combo-fset-alias-target) t nil)))))
               (condition-case nil
                   (advice-remove 'neovm--combo-fset-alias-a 'neovm--combo-fset-alias-around)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-fset-alias-a 'neovm--combo-fset-alias-ret)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-fset-alias-target 'neovm--combo-fset-alias-around)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-fset-alias-target 'neovm--combo-fset-alias-ret)
                 (error nil))
               (fmakunbound 'neovm--combo-fset-alias-target)
               (fmakunbound 'neovm--combo-fset-alias-a)
               (fmakunbound 'neovm--combo-fset-alias-around)
               (fmakunbound 'neovm--combo-fset-alias-ret)
               (fmakunbound 'neovm--combo-fset-alias-call-a)
               (fmakunbound 'neovm--combo-fset-alias-call-t))))",
        n = 3i64,
        mul = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_before_while_after_until_alias_rebind_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-bwau-call-a (x)
             `(neovm--combo-bwau-a ,x))
           (defmacro neovm--combo-bwau-call-t (x)
             `(neovm--combo-bwau-target ,x))
           (fset 'neovm--combo-bwau-target
                 (lambda (x)
                   (if (= (% x 2) 0) nil (+ x 100))))
           (defalias 'neovm--combo-bwau-a 'neovm--combo-bwau-target)
           (fset 'neovm--combo-bwau-guard
                 (lambda (&rest args)
                   (> (car args) -1)))
           (fset 'neovm--combo-bwau-fallback
                 (lambda (&rest args)
                   (list 'fb (car args))))
           (let ((fa0 nil) (ft0 nil))
             (unwind-protect
                 (progn
                   (advice-add 'neovm--combo-bwau-a :before-while 'neovm--combo-bwau-guard)
                   (advice-add 'neovm--combo-bwau-a :after-until 'neovm--combo-bwau-fallback)
                   (setq fa0 (symbol-function 'neovm--combo-bwau-a))
                   (setq ft0 (symbol-function 'neovm--combo-bwau-target))
                   (list
                     (list
                       (neovm--combo-bwau-call-a -1)
                       (eval '(neovm--combo-bwau-call-a 0))
                       (funcall 'neovm--combo-bwau-a 1)
                       (apply 'neovm--combo-bwau-a '(2))
                       (neovm--combo-bwau-call-t -1)
                       (eval '(neovm--combo-bwau-call-t 0))
                       (funcall 'neovm--combo-bwau-target 1)
                       (apply 'neovm--combo-bwau-target '(2))
                       (funcall fa0 0)
                       (funcall ft0 0)
                       (if (advice-member-p 'neovm--combo-bwau-guard 'neovm--combo-bwau-a) t nil)
                       (if (advice-member-p 'neovm--combo-bwau-fallback 'neovm--combo-bwau-a) t nil)
                       (if (advice-member-p 'neovm--combo-bwau-guard 'neovm--combo-bwau-target) t nil)
                       (if (advice-member-p 'neovm--combo-bwau-fallback 'neovm--combo-bwau-target) t nil))
                     (progn
                       (fset 'neovm--combo-bwau-a (lambda (_x) nil))
                       (list
                         (neovm--combo-bwau-call-a -1)
                         (eval '(neovm--combo-bwau-call-a 0))
                         (funcall 'neovm--combo-bwau-a 1)
                         (apply 'neovm--combo-bwau-a '(2))
                         (neovm--combo-bwau-call-t -1)
                         (eval '(neovm--combo-bwau-call-t 0))
                         (funcall 'neovm--combo-bwau-target 1)
                         (apply 'neovm--combo-bwau-target '(2))
                         (funcall fa0 0)
                         (funcall ft0 0)
                         (if (advice-member-p 'neovm--combo-bwau-guard 'neovm--combo-bwau-a) t nil)
                         (if (advice-member-p 'neovm--combo-bwau-fallback 'neovm--combo-bwau-a) t nil)
                         (if (advice-member-p 'neovm--combo-bwau-guard 'neovm--combo-bwau-target) t nil)
                         (if (advice-member-p 'neovm--combo-bwau-fallback 'neovm--combo-bwau-target) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-bwau-a 'neovm--combo-bwau-guard)
                       (advice-remove 'neovm--combo-bwau-a 'neovm--combo-bwau-fallback)
                       (list
                         (neovm--combo-bwau-call-a -1)
                         (eval '(neovm--combo-bwau-call-a 0))
                         (funcall 'neovm--combo-bwau-a 1)
                         (apply 'neovm--combo-bwau-a '(2))
                         (neovm--combo-bwau-call-t -1)
                         (eval '(neovm--combo-bwau-call-t 0))
                         (funcall 'neovm--combo-bwau-target 1)
                         (apply 'neovm--combo-bwau-target '(2))
                         (funcall fa0 0)
                         (funcall ft0 0)
                         (if (advice-member-p 'neovm--combo-bwau-guard 'neovm--combo-bwau-a) t nil)
                         (if (advice-member-p 'neovm--combo-bwau-fallback 'neovm--combo-bwau-a) t nil)
                         (if (advice-member-p 'neovm--combo-bwau-guard 'neovm--combo-bwau-target) t nil)
                         (if (advice-member-p 'neovm--combo-bwau-fallback 'neovm--combo-bwau-target) t nil)))))
               (condition-case nil
                   (advice-remove 'neovm--combo-bwau-a 'neovm--combo-bwau-guard)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-bwau-a 'neovm--combo-bwau-fallback)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-bwau-target 'neovm--combo-bwau-guard)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-bwau-target 'neovm--combo-bwau-fallback)
                 (error nil))
               (fmakunbound 'neovm--combo-bwau-target)
               (fmakunbound 'neovm--combo-bwau-a)
               (fmakunbound 'neovm--combo-bwau-guard)
               (fmakunbound 'neovm--combo-bwau-fallback)
               (fmakunbound 'neovm--combo-bwau-call-a)
               (fmakunbound 'neovm--combo-bwau-call-t))))",
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_before_until_after_while_alias_switch_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-buaw-call-a (x)
             `(neovm--combo-buaw-a ,x))
           (defmacro neovm--combo-buaw-call-t (x)
             `(neovm--combo-buaw-target-a ,x))
           (fset 'neovm--combo-buaw-target-a
                 (lambda (x)
                   (if (> x 0) (+ x 10) nil)))
           (fset 'neovm--combo-buaw-target-b
                 (lambda (x)
                   (if (< x 0) (- x 10) nil)))
           (defalias 'neovm--combo-buaw-a 'neovm--combo-buaw-target-a)
           (fset 'neovm--combo-buaw-before-until
                 (lambda (&rest args)
                   (let ((x (car args)))
                     (if (< x 0) (list 'short x) nil))))
           (fset 'neovm--combo-buaw-after-while
                 (lambda (&rest args)
                   (let ((x (car args)))
                     (if (< x 3) (list 'post x) nil))))
           (let ((fa0 nil) (ft0 nil))
             (unwind-protect
                 (progn
                   (advice-add 'neovm--combo-buaw-a :before-until 'neovm--combo-buaw-before-until)
                   (advice-add 'neovm--combo-buaw-a :after-while 'neovm--combo-buaw-after-while)
                   (setq fa0 (symbol-function 'neovm--combo-buaw-a))
                   (setq ft0 (symbol-function 'neovm--combo-buaw-target-a))
                   (list
                     (list
                       (neovm--combo-buaw-call-a -1)
                       (eval '(neovm--combo-buaw-call-a 0))
                       (funcall 'neovm--combo-buaw-a 1)
                       (apply 'neovm--combo-buaw-a '(2))
                       (neovm--combo-buaw-call-t -1)
                       (eval '(neovm--combo-buaw-call-t 0))
                       (funcall 'neovm--combo-buaw-target-a 1)
                       (apply 'neovm--combo-buaw-target-a '(2))
                       (funcall fa0 0)
                       (funcall ft0 0)
                       (if (advice-member-p 'neovm--combo-buaw-before-until 'neovm--combo-buaw-a) t nil)
                       (if (advice-member-p 'neovm--combo-buaw-after-while 'neovm--combo-buaw-a) t nil))
                     (progn
                       (defalias 'neovm--combo-buaw-a 'neovm--combo-buaw-target-b)
                       (list
                         (neovm--combo-buaw-call-a -1)
                         (eval '(neovm--combo-buaw-call-a 0))
                         (funcall 'neovm--combo-buaw-a 1)
                         (apply 'neovm--combo-buaw-a '(2))
                         (neovm--combo-buaw-call-t -1)
                         (eval '(neovm--combo-buaw-call-t 0))
                         (funcall 'neovm--combo-buaw-target-a 1)
                         (apply 'neovm--combo-buaw-target-a '(2))
                         (funcall fa0 0)
                         (funcall ft0 0)
                         (if (advice-member-p 'neovm--combo-buaw-before-until 'neovm--combo-buaw-a) t nil)
                         (if (advice-member-p 'neovm--combo-buaw-after-while 'neovm--combo-buaw-a) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-buaw-a 'neovm--combo-buaw-before-until)
                       (advice-remove 'neovm--combo-buaw-a 'neovm--combo-buaw-after-while)
                       (list
                         (neovm--combo-buaw-call-a -1)
                         (eval '(neovm--combo-buaw-call-a 0))
                         (funcall 'neovm--combo-buaw-a 1)
                         (apply 'neovm--combo-buaw-a '(2))
                         (neovm--combo-buaw-call-t -1)
                         (eval '(neovm--combo-buaw-call-t 0))
                         (funcall 'neovm--combo-buaw-target-a 1)
                         (apply 'neovm--combo-buaw-target-a '(2))
                         (funcall fa0 0)
                         (funcall ft0 0)
                         (if (advice-member-p 'neovm--combo-buaw-before-until 'neovm--combo-buaw-a) t nil)
                         (if (advice-member-p 'neovm--combo-buaw-after-while 'neovm--combo-buaw-a) t nil)))))
               (condition-case nil
                   (advice-remove 'neovm--combo-buaw-a 'neovm--combo-buaw-before-until)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-buaw-a 'neovm--combo-buaw-after-while)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-buaw-target-a 'neovm--combo-buaw-before-until)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-buaw-target-a 'neovm--combo-buaw-after-while)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-buaw-target-b 'neovm--combo-buaw-before-until)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-buaw-target-b 'neovm--combo-buaw-after-while)
                 (error nil))
               (fmakunbound 'neovm--combo-buaw-target-a)
               (fmakunbound 'neovm--combo-buaw-target-b)
               (fmakunbound 'neovm--combo-buaw-a)
               (fmakunbound 'neovm--combo-buaw-before-until)
               (fmakunbound 'neovm--combo-buaw-after-while)
               (fmakunbound 'neovm--combo-buaw-call-a)
               (fmakunbound 'neovm--combo-buaw-call-t))))",
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_add_function_rebind_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-addf-call (x)
             `(neovm--combo-addf-target ,x))
           (fset 'neovm--combo-addf-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-addf-alias 'neovm--combo-addf-target)
           (fset 'neovm--combo-addf-around
                 (lambda (orig x)
                   (+ 10 (funcall orig x))))
           (fset 'neovm--combo-addf-filter-ret
                 (lambda (ret)
                   (* ret 3)))
           (let ((f0 nil))
             (unwind-protect
                 (progn
                   (add-function :around (symbol-function 'neovm--combo-addf-target) 'neovm--combo-addf-around)
                   (add-function :filter-return (symbol-function 'neovm--combo-addf-target) 'neovm--combo-addf-filter-ret)
                   (setq f0 (symbol-function 'neovm--combo-addf-target))
                   (list
                     (list
                       (neovm--combo-addf-call {n})
                       (eval '(neovm--combo-addf-call {n}))
                       (funcall 'neovm--combo-addf-target {n})
                       (apply 'neovm--combo-addf-target (list {n}))
                       (funcall 'neovm--combo-addf-alias {n})
                       (funcall f0 {n}))
                     (progn
                       (fset 'neovm--combo-addf-target (lambda (x) (* x {mul})))
                       (list
                         (neovm--combo-addf-call {n})
                         (eval '(neovm--combo-addf-call {n}))
                         (funcall 'neovm--combo-addf-target {n})
                         (apply 'neovm--combo-addf-target (list {n}))
                         (funcall 'neovm--combo-addf-alias {n})
                         (funcall f0 {n})))
                     (progn
                       (condition-case nil
                           (remove-function (symbol-function 'neovm--combo-addf-target) 'neovm--combo-addf-around)
                         (error nil))
                       (condition-case nil
                           (remove-function (symbol-function 'neovm--combo-addf-target) 'neovm--combo-addf-filter-ret)
                         (error nil))
                       (list
                         (neovm--combo-addf-call {n})
                         (eval '(neovm--combo-addf-call {n}))
                         (funcall 'neovm--combo-addf-target {n})
                         (apply 'neovm--combo-addf-target (list {n}))
                         (funcall 'neovm--combo-addf-alias {n})
                         (funcall f0 {n})))))
               (condition-case nil
                   (remove-function (symbol-function 'neovm--combo-addf-target) 'neovm--combo-addf-around)
                 (error nil))
               (condition-case nil
                   (remove-function (symbol-function 'neovm--combo-addf-target) 'neovm--combo-addf-filter-ret)
                 (error nil))
               (fmakunbound 'neovm--combo-addf-target)
               (fmakunbound 'neovm--combo-addf-alias)
               (fmakunbound 'neovm--combo-addf-around)
               (fmakunbound 'neovm--combo-addf-filter-ret)
               (fmakunbound 'neovm--combo-addf-call))))",
        n = 4i64,
        mul = 6i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_same_name_around_replacement_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-name-repl-call (x)
             `(neovm--combo-name-repl-target ,x))
           (fset 'neovm--combo-name-repl-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-name-repl-alias 'neovm--combo-name-repl-target)
           (fset 'neovm--combo-name-repl-a1
                 (lambda (orig x)
                   (+ 1 (funcall orig x))))
           (fset 'neovm--combo-name-repl-a2
                 (lambda (orig x)
                   (+ 10 (funcall orig x))))
           (unwind-protect
               (list
                 (progn
                   (advice-add 'neovm--combo-name-repl-target :around 'neovm--combo-name-repl-a1 '((name . neovm--combo-name-repl-shared) (depth . -10)))
                   (list
                     (neovm--combo-name-repl-call {n})
                     (eval '(neovm--combo-name-repl-call {n}))
                     (funcall 'neovm--combo-name-repl-target {n})
                     (funcall 'neovm--combo-name-repl-alias {n})
                     (if (advice-member-p 'neovm--combo-name-repl-a1 'neovm--combo-name-repl-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-repl-a2 'neovm--combo-name-repl-target) t nil)))
                 (progn
                   (advice-add 'neovm--combo-name-repl-target :around 'neovm--combo-name-repl-a2 '((name . neovm--combo-name-repl-shared) (depth . 10)))
                   (list
                     (neovm--combo-name-repl-call {n})
                     (eval '(neovm--combo-name-repl-call {n}))
                     (funcall 'neovm--combo-name-repl-target {n})
                     (funcall 'neovm--combo-name-repl-alias {n})
                     (if (advice-member-p 'neovm--combo-name-repl-a1 'neovm--combo-name-repl-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-repl-a2 'neovm--combo-name-repl-target) t nil)))
                 (progn
                   (advice-remove 'neovm--combo-name-repl-alias 'neovm--combo-name-repl-a1)
                   (list
                     (neovm--combo-name-repl-call {n})
                     (eval '(neovm--combo-name-repl-call {n}))
                     (funcall 'neovm--combo-name-repl-target {n})
                     (funcall 'neovm--combo-name-repl-alias {n})
                     (if (advice-member-p 'neovm--combo-name-repl-a1 'neovm--combo-name-repl-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-repl-a2 'neovm--combo-name-repl-target) t nil)))
                 (progn
                   (advice-remove 'neovm--combo-name-repl-target 'neovm--combo-name-repl-a2)
                   (list
                     (neovm--combo-name-repl-call {n})
                     (eval '(neovm--combo-name-repl-call {n}))
                     (funcall 'neovm--combo-name-repl-target {n})
                     (funcall 'neovm--combo-name-repl-alias {n})
                     (if (advice-member-p 'neovm--combo-name-repl-a1 'neovm--combo-name-repl-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-repl-a2 'neovm--combo-name-repl-target) t nil))))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-repl-target 'neovm--combo-name-repl-a1)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-repl-target 'neovm--combo-name-repl-a2)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-repl-alias 'neovm--combo-name-repl-a1)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-repl-alias 'neovm--combo-name-repl-a2)
               (error nil))
             (fmakunbound 'neovm--combo-name-repl-target)
             (fmakunbound 'neovm--combo-name-repl-alias)
             (fmakunbound 'neovm--combo-name-repl-a1)
             (fmakunbound 'neovm--combo-name-repl-a2)
             (fmakunbound 'neovm--combo-name-repl-call)))",
        n = 5i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_same_name_filter_return_replacement_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-name-fr-call (x)
             `(neovm--combo-name-fr-target ,x))
           (fset 'neovm--combo-name-fr-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-name-fr-alias 'neovm--combo-name-fr-target)
           (fset 'neovm--combo-name-fr-f1
                 (lambda (ret)
                   (+ ret 1)))
           (fset 'neovm--combo-name-fr-f2
                 (lambda (ret)
                   (* ret 10)))
           (unwind-protect
               (list
                 (progn
                   (advice-add 'neovm--combo-name-fr-target :filter-return 'neovm--combo-name-fr-f1 '((name . neovm--combo-name-fr-shared) (depth . -10)))
                   (list
                     (neovm--combo-name-fr-call {n})
                     (eval '(neovm--combo-name-fr-call {n}))
                     (funcall 'neovm--combo-name-fr-target {n})
                     (funcall 'neovm--combo-name-fr-alias {n})
                     (if (advice-member-p 'neovm--combo-name-fr-f1 'neovm--combo-name-fr-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-fr-f2 'neovm--combo-name-fr-target) t nil)))
                 (progn
                   (advice-add 'neovm--combo-name-fr-target :filter-return 'neovm--combo-name-fr-f2 '((name . neovm--combo-name-fr-shared) (depth . 10)))
                   (list
                     (neovm--combo-name-fr-call {n})
                     (eval '(neovm--combo-name-fr-call {n}))
                     (funcall 'neovm--combo-name-fr-target {n})
                     (funcall 'neovm--combo-name-fr-alias {n})
                     (if (advice-member-p 'neovm--combo-name-fr-f1 'neovm--combo-name-fr-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-fr-f2 'neovm--combo-name-fr-target) t nil)))
                 (progn
                   (advice-remove 'neovm--combo-name-fr-alias 'neovm--combo-name-fr-f1)
                   (list
                     (neovm--combo-name-fr-call {n})
                     (eval '(neovm--combo-name-fr-call {n}))
                     (funcall 'neovm--combo-name-fr-target {n})
                     (funcall 'neovm--combo-name-fr-alias {n})
                     (if (advice-member-p 'neovm--combo-name-fr-f1 'neovm--combo-name-fr-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-fr-f2 'neovm--combo-name-fr-target) t nil)))
                 (progn
                   (advice-remove 'neovm--combo-name-fr-target 'neovm--combo-name-fr-f2)
                   (list
                     (neovm--combo-name-fr-call {n})
                     (eval '(neovm--combo-name-fr-call {n}))
                     (funcall 'neovm--combo-name-fr-target {n})
                     (funcall 'neovm--combo-name-fr-alias {n})
                     (if (advice-member-p 'neovm--combo-name-fr-f1 'neovm--combo-name-fr-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-fr-f2 'neovm--combo-name-fr-target) t nil))))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-fr-target 'neovm--combo-name-fr-f1)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-fr-target 'neovm--combo-name-fr-f2)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-fr-alias 'neovm--combo-name-fr-f1)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-fr-alias 'neovm--combo-name-fr-f2)
               (error nil))
             (fmakunbound 'neovm--combo-name-fr-target)
             (fmakunbound 'neovm--combo-name-fr-alias)
             (fmakunbound 'neovm--combo-name-fr-f1)
             (fmakunbound 'neovm--combo-name-fr-f2)
             (fmakunbound 'neovm--combo-name-fr-call)))",
        n = 5i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_distinct_equal_lambda_remove_semantics_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-lid-call (x)
             `(neovm--combo-lid-target ,x))
           (fset 'neovm--combo-lid-target (lambda (x) (+ x 1)))
           (let* ((adv1 (lambda (orig x) (+ 10 (funcall orig x))))
                  (adv2 (lambda (orig x) (+ 10 (funcall orig x)))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-lid-target :around adv1)
                     (list
                       (neovm--combo-lid-call {n})
                       (eval '(neovm--combo-lid-call {n}))
                       (funcall 'neovm--combo-lid-target {n})
                       (apply 'neovm--combo-lid-target (list {n}))
                       (eq adv1 adv2)
                       (if (advice-member-p adv1 'neovm--combo-lid-target) t nil)
                       (if (advice-member-p adv2 'neovm--combo-lid-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-lid-target adv2)
                     (list
                       (neovm--combo-lid-call {n})
                       (eval '(neovm--combo-lid-call {n}))
                       (funcall 'neovm--combo-lid-target {n})
                       (apply 'neovm--combo-lid-target (list {n}))
                       (eq adv1 adv2)
                       (if (advice-member-p adv1 'neovm--combo-lid-target) t nil)
                       (if (advice-member-p adv2 'neovm--combo-lid-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-lid-target adv1)
                     (list
                       (neovm--combo-lid-call {n})
                       (eval '(neovm--combo-lid-call {n}))
                       (funcall 'neovm--combo-lid-target {n})
                       (apply 'neovm--combo-lid-target (list {n}))
                       (eq adv1 adv2)
                       (if (advice-member-p adv1 'neovm--combo-lid-target) t nil)
                       (if (advice-member-p adv2 'neovm--combo-lid-target) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-lid-target adv1)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-lid-target adv2)
                 (error nil))
               (fmakunbound 'neovm--combo-lid-target)
               (fmakunbound 'neovm--combo-lid-call))))",
        n = 5i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_lambda_before_and_filter_return_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-lbf-call (x)
             `(neovm--combo-lbf-target ,x))
           (fset 'neovm--combo-lbf-target (lambda (x) (+ x 1)))
           (let* ((log nil)
                  (before1 (lambda (&rest args) (setq log (cons (cons 'b1 args) log))))
                  (before2 (lambda (&rest args) (setq log (cons (cons 'b2 args) log))))
                  (fret1 (lambda (ret) (+ ret 10)))
                  (fret2 (lambda (ret) (+ ret 100))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-lbf-target :before before1)
                     (advice-add 'neovm--combo-lbf-target :filter-return fret1)
                     (setq log nil)
                     (list
                       (neovm--combo-lbf-call {n})
                       (eval '(neovm--combo-lbf-call {n}))
                       (funcall 'neovm--combo-lbf-target {n})
                       (apply 'neovm--combo-lbf-target (list {n}))
                       (nreverse log)
                       (if (advice-member-p before1 'neovm--combo-lbf-target) t nil)
                       (if (advice-member-p fret1 'neovm--combo-lbf-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-lbf-target before2)
                     (advice-remove 'neovm--combo-lbf-target fret2)
                     (setq log nil)
                     (list
                       (neovm--combo-lbf-call {n})
                       (eval '(neovm--combo-lbf-call {n}))
                       (funcall 'neovm--combo-lbf-target {n})
                       (apply 'neovm--combo-lbf-target (list {n}))
                       (nreverse log)
                       (if (advice-member-p before1 'neovm--combo-lbf-target) t nil)
                       (if (advice-member-p fret1 'neovm--combo-lbf-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-lbf-target before1)
                     (advice-remove 'neovm--combo-lbf-target fret1)
                     (setq log nil)
                     (list
                       (neovm--combo-lbf-call {n})
                       (eval '(neovm--combo-lbf-call {n}))
                       (funcall 'neovm--combo-lbf-target {n})
                       (apply 'neovm--combo-lbf-target (list {n}))
                       (nreverse log)
                       (if (advice-member-p before1 'neovm--combo-lbf-target) t nil)
                       (if (advice-member-p fret1 'neovm--combo-lbf-target) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-lbf-target before1)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-lbf-target before2)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-lbf-target fret1)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-lbf-target fret2)
                 (error nil))
               (fmakunbound 'neovm--combo-lbf-target)
               (fmakunbound 'neovm--combo-lbf-call))))",
        n = 4i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_same_name_override_replacement_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-name-ov-call (x)
             `(neovm--combo-name-ov-target ,x))
           (fset 'neovm--combo-name-ov-target (lambda (x) (+ x 1)))
           (defalias 'neovm--combo-name-ov-alias 'neovm--combo-name-ov-target)
           (fset 'neovm--combo-name-ov-o1 (lambda (x) (+ x 10)))
           (fset 'neovm--combo-name-ov-o2 (lambda (x) (+ x 100)))
           (unwind-protect
               (list
                 (progn
                   (advice-add 'neovm--combo-name-ov-target :override 'neovm--combo-name-ov-o1 '((name . neovm--combo-name-ov-shared) (depth . -10)))
                   (list
                     (neovm--combo-name-ov-call {n})
                     (eval '(neovm--combo-name-ov-call {n}))
                     (funcall 'neovm--combo-name-ov-target {n})
                     (funcall 'neovm--combo-name-ov-alias {n})
                     (if (advice-member-p 'neovm--combo-name-ov-o1 'neovm--combo-name-ov-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-ov-o2 'neovm--combo-name-ov-target) t nil)))
                 (progn
                   (advice-add 'neovm--combo-name-ov-target :override 'neovm--combo-name-ov-o2 '((name . neovm--combo-name-ov-shared) (depth . 10)))
                   (list
                     (neovm--combo-name-ov-call {n})
                     (eval '(neovm--combo-name-ov-call {n}))
                     (funcall 'neovm--combo-name-ov-target {n})
                     (funcall 'neovm--combo-name-ov-alias {n})
                     (if (advice-member-p 'neovm--combo-name-ov-o1 'neovm--combo-name-ov-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-ov-o2 'neovm--combo-name-ov-target) t nil)))
                 (progn
                   (advice-remove 'neovm--combo-name-ov-alias 'neovm--combo-name-ov-o1)
                   (list
                     (neovm--combo-name-ov-call {n})
                     (eval '(neovm--combo-name-ov-call {n}))
                     (funcall 'neovm--combo-name-ov-target {n})
                     (funcall 'neovm--combo-name-ov-alias {n})
                     (if (advice-member-p 'neovm--combo-name-ov-o1 'neovm--combo-name-ov-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-ov-o2 'neovm--combo-name-ov-target) t nil)))
                 (progn
                   (advice-remove 'neovm--combo-name-ov-target 'neovm--combo-name-ov-o2)
                   (list
                     (neovm--combo-name-ov-call {n})
                     (eval '(neovm--combo-name-ov-call {n}))
                     (funcall 'neovm--combo-name-ov-target {n})
                     (funcall 'neovm--combo-name-ov-alias {n})
                     (if (advice-member-p 'neovm--combo-name-ov-o1 'neovm--combo-name-ov-target) t nil)
                     (if (advice-member-p 'neovm--combo-name-ov-o2 'neovm--combo-name-ov-target) t nil))))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-ov-target 'neovm--combo-name-ov-o1)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-ov-target 'neovm--combo-name-ov-o2)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-ov-alias 'neovm--combo-name-ov-o1)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-name-ov-alias 'neovm--combo-name-ov-o2)
               (error nil))
             (fmakunbound 'neovm--combo-name-ov-target)
             (fmakunbound 'neovm--combo-name-ov-alias)
             (fmakunbound 'neovm--combo-name-ov-o1)
             (fmakunbound 'neovm--combo-name-ov-o2)
             (fmakunbound 'neovm--combo-name-ov-call)))",
        n = 5i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_lambda_override_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defmacro neovm--combo-lov-call (x)
             `(neovm--combo-lov-target ,x))
           (fset 'neovm--combo-lov-target (lambda (x) (+ x 1)))
           (let* ((ov1 (lambda (x) (+ x 10)))
                  (ov2 (lambda (x) (+ x 100))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-lov-target :override ov1)
                     (list
                       (neovm--combo-lov-call {n})
                       (eval '(neovm--combo-lov-call {n}))
                       (funcall 'neovm--combo-lov-target {n})
                       (apply 'neovm--combo-lov-target (list {n}))
                       (if (advice-member-p ov1 'neovm--combo-lov-target) t nil)
                       (if (advice-member-p ov2 'neovm--combo-lov-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-lov-target ov2)
                     (list
                       (neovm--combo-lov-call {n})
                       (eval '(neovm--combo-lov-call {n}))
                       (funcall 'neovm--combo-lov-target {n})
                       (apply 'neovm--combo-lov-target (list {n}))
                       (if (advice-member-p ov1 'neovm--combo-lov-target) t nil)
                       (if (advice-member-p ov2 'neovm--combo-lov-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-lov-target ov1)
                     (list
                       (neovm--combo-lov-call {n})
                       (eval '(neovm--combo-lov-call {n}))
                       (funcall 'neovm--combo-lov-target {n})
                       (apply 'neovm--combo-lov-target (list {n}))
                       (if (advice-member-p ov1 'neovm--combo-lov-target) t nil)
                       (if (advice-member-p ov2 'neovm--combo-lov-target) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-lov-target ov1)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-lov-target ov2)
                 (error nil))
               (fmakunbound 'neovm--combo-lov-target)
               (fmakunbound 'neovm--combo-lov-call))))",
        n = 5i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_same_name_cross_location_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((log nil))
             (fset 'neovm--combo-name-cross-target
                   (lambda (x)
                     (setq log (cons (list 'orig x) log))
                     x))
             (fset 'neovm--combo-name-cross-before
                   (lambda (&rest args)
                     (setq log (cons (cons 'before args) log))))
             (fset 'neovm--combo-name-cross-after
                   (lambda (&rest args)
                     (setq log (cons (cons 'after args) log))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-name-cross-target :before 'neovm--combo-name-cross-before '((name . neovm--combo-name-cross-shared)))
                     (setq log nil)
                     (list
                       (funcall 'neovm--combo-name-cross-target {n})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-name-cross-before 'neovm--combo-name-cross-target) t nil)
                       (if (advice-member-p 'neovm--combo-name-cross-after 'neovm--combo-name-cross-target) t nil)))
                   (progn
                     (advice-add 'neovm--combo-name-cross-target :after 'neovm--combo-name-cross-after '((name . neovm--combo-name-cross-shared)))
                     (setq log nil)
                     (list
                       (funcall 'neovm--combo-name-cross-target {n})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-name-cross-before 'neovm--combo-name-cross-target) t nil)
                       (if (advice-member-p 'neovm--combo-name-cross-after 'neovm--combo-name-cross-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-name-cross-target 'neovm--combo-name-cross-before)
                     (setq log nil)
                     (list
                       (funcall 'neovm--combo-name-cross-target {n})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-name-cross-before 'neovm--combo-name-cross-target) t nil)
                       (if (advice-member-p 'neovm--combo-name-cross-after 'neovm--combo-name-cross-target) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-name-cross-target 'neovm--combo-name-cross-after)
                     (setq log nil)
                     (list
                       (funcall 'neovm--combo-name-cross-target {n})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-name-cross-before 'neovm--combo-name-cross-target) t nil)
                       (if (advice-member-p 'neovm--combo-name-cross-after 'neovm--combo-name-cross-target) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-name-cross-target 'neovm--combo-name-cross-before)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-name-cross-target 'neovm--combo-name-cross-after)
                 (error nil))
               (fmakunbound 'neovm--combo-name-cross-target)
               (fmakunbound 'neovm--combo-name-cross-before)
               (fmakunbound 'neovm--combo-name-cross-after))))",
        n = 3i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_before_advice_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((log nil))
             (fset 'neovm--combo-plus-before
                   (lambda (&rest args)
                     (setq log (cons args log))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add '+ :before 'neovm--combo-plus-before)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-before '+) t nil)))
                   (progn
                     (advice-remove '+ 'neovm--combo-plus-before)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-before '+) t nil))))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-before)
                 (error nil))
               (fmakunbound 'neovm--combo-plus-before))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_before_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((log nil))
             (fset 'neovm--combo-plus-name-b1
                   (lambda (&rest args)
                     (setq log (cons (cons 'b1 args) log))))
             (fset 'neovm--combo-plus-name-b2
                   (lambda (&rest args)
                     (setq log (cons (cons 'b2 args) log))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add '+ :before 'neovm--combo-plus-name-b1 '((name . neovm--combo-plus-name-shared)))
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-b1 '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-b2 '+) t nil)))
                   (progn
                     (advice-add '+ :before 'neovm--combo-plus-name-b2 '((name . neovm--combo-plus-name-shared)))
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-b1 '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-b2 '+) t nil)))
                   (progn
                     (advice-remove '+ 'neovm--combo-plus-name-b1)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-b1 '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-b2 '+) t nil)))
                   (progn
                     (advice-remove '+ 'neovm--combo-plus-name-b2)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-b1 '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-b2 '+) t nil))))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-name-b1)
                 (error nil))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-name-b2)
                 (error nil))
               (fmakunbound 'neovm--combo-plus-name-b1)
               (fmakunbound 'neovm--combo-plus-name-b2))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_after_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((log nil))
             (fset 'neovm--combo-plus-name-a1
                   (lambda (&rest args)
                     (setq log (cons (cons 'a1 args) log))))
             (fset 'neovm--combo-plus-name-a2
                   (lambda (&rest args)
                     (setq log (cons (cons 'a2 args) log))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add '+ :after 'neovm--combo-plus-name-a1 '((name . neovm--combo-plus-name-after-shared)))
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-a1 '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-a2 '+) t nil)))
                   (progn
                     (advice-add '+ :after 'neovm--combo-plus-name-a2 '((name . neovm--combo-plus-name-after-shared)))
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-a1 '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-a2 '+) t nil)))
                   (progn
                     (advice-remove '+ 'neovm--combo-plus-name-a1)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-a1 '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-a2 '+) t nil)))
                   (progn
                     (advice-remove '+ 'neovm--combo-plus-name-a2)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-a1 '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-a2 '+) t nil))))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-name-a1)
                 (error nil))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-name-a2)
                 (error nil))
               (fmakunbound 'neovm--combo-plus-name-a1)
               (fmakunbound 'neovm--combo-plus-name-a2))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_around_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (fset 'neovm--combo-plus-name-ar1 (lambda (orig x y) x))
           (fset 'neovm--combo-plus-name-ar2 (lambda (orig x y) y))
           (unwind-protect
               (list
                 (progn
                   (advice-add '+ :around 'neovm--combo-plus-name-ar1 '((name . neovm--combo-plus-name-around-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ar1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ar2 '+) t nil)))
                 (progn
                   (advice-add '+ :around 'neovm--combo-plus-name-ar2 '((name . neovm--combo-plus-name-around-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ar1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ar2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-ar1)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ar1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ar2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-ar2)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ar1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ar2 '+) t nil))))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-ar1)
               (error nil))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-ar2)
               (error nil))
             (fmakunbound 'neovm--combo-plus-name-ar1)
             (fmakunbound 'neovm--combo-plus-name-ar2)))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_around_depth_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (fset 'neovm--combo-plus-name-ard1 (lambda (orig x y) x))
           (fset 'neovm--combo-plus-name-ard2 (lambda (orig x y) y))
           (unwind-protect
               (list
                 (progn
                   (advice-add '+ :around 'neovm--combo-plus-name-ard1 '((name . neovm--combo-plus-name-ard-shared) (depth . -50)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ard1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ard2 '+) t nil)))
                 (progn
                   (advice-add '+ :around 'neovm--combo-plus-name-ard2 '((name . neovm--combo-plus-name-ard-shared) (depth . 50)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ard1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ard2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-ard1)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ard1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ard2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-ard2)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ard1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ard2 '+) t nil))))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-ard1)
               (error nil))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-ard2)
               (error nil))
             (fmakunbound 'neovm--combo-plus-name-ard1)
             (fmakunbound 'neovm--combo-plus-name-ard2)))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_filter_return_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (fset 'neovm--combo-plus-name-fr1 (lambda (ret) ret))
           (fset 'neovm--combo-plus-name-fr2 (lambda (ret) (- ret)))
           (unwind-protect
               (list
                 (progn
                   (advice-add '+ :filter-return 'neovm--combo-plus-name-fr1 '((name . neovm--combo-plus-name-fr-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-fr1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-fr2 '+) t nil)))
                 (progn
                   (advice-add '+ :filter-return 'neovm--combo-plus-name-fr2 '((name . neovm--combo-plus-name-fr-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-fr1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-fr2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-fr1)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-fr1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-fr2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-fr2)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-fr1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-fr2 '+) t nil))))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-fr1)
               (error nil))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-fr2)
               (error nil))
             (fmakunbound 'neovm--combo-plus-name-fr1)
             (fmakunbound 'neovm--combo-plus-name-fr2)))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_filter_args_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (fset 'neovm--combo-plus-name-fa1 (lambda (args) args))
           (fset 'neovm--combo-plus-name-fa2 (lambda (args) (list 0 0)))
           (unwind-protect
               (list
                 (progn
                   (advice-add '+ :filter-args 'neovm--combo-plus-name-fa1 '((name . neovm--combo-plus-name-fa-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-fa1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-fa2 '+) t nil)))
                 (progn
                   (advice-add '+ :filter-args 'neovm--combo-plus-name-fa2 '((name . neovm--combo-plus-name-fa-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-fa1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-fa2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-fa1)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-fa1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-fa2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-fa2)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-fa1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-fa2 '+) t nil))))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-fa1)
               (error nil))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-fa2)
               (error nil))
             (fmakunbound 'neovm--combo-plus-name-fa1)
             (fmakunbound 'neovm--combo-plus-name-fa2)))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_around_to_filter_return_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (fset 'neovm--combo-plus-name-arfr-around (lambda (orig x y) x))
           (fset 'neovm--combo-plus-name-arfr-fr (lambda (ret) (- ret)))
           (unwind-protect
               (list
                 (progn
                   (advice-add '+ :around 'neovm--combo-plus-name-arfr-around '((name . neovm--combo-plus-name-arfr-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-arfr-around '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-arfr-fr '+) t nil)))
                 (progn
                   (advice-add '+ :filter-return 'neovm--combo-plus-name-arfr-fr '((name . neovm--combo-plus-name-arfr-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-arfr-around '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-arfr-fr '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-arfr-around)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-arfr-around '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-arfr-fr '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-arfr-fr)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-arfr-around '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-arfr-fr '+) t nil))))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-arfr-around)
               (error nil))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-arfr-fr)
               (error nil))
             (fmakunbound 'neovm--combo-plus-name-arfr-around)
             (fmakunbound 'neovm--combo-plus-name-arfr-fr)))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_override_to_after_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((log nil))
             (fset 'neovm--combo-plus-name-ovaf-ov (lambda (x y) x))
             (fset 'neovm--combo-plus-name-ovaf-af
                   (lambda (&rest args)
                     (setq log (cons args log))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add '+ :override 'neovm--combo-plus-name-ovaf-ov '((name . neovm--combo-plus-name-ovaf-shared)))
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-ovaf-ov '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-ovaf-af '+) t nil)))
                   (progn
                     (advice-add '+ :after 'neovm--combo-plus-name-ovaf-af '((name . neovm--combo-plus-name-ovaf-shared)))
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-ovaf-ov '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-ovaf-af '+) t nil)))
                   (progn
                     (advice-remove '+ 'neovm--combo-plus-name-ovaf-ov)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-ovaf-ov '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-ovaf-af '+) t nil)))
                   (progn
                     (advice-remove '+ 'neovm--combo-plus-name-ovaf-af)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-ovaf-ov '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-ovaf-af '+) t nil))))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-name-ovaf-ov)
                 (error nil))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-name-ovaf-af)
                 (error nil))
               (fmakunbound 'neovm--combo-plus-name-ovaf-ov)
               (fmakunbound 'neovm--combo-plus-name-ovaf-af))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_before_to_after_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((log nil))
             (fset 'neovm--combo-plus-name-btaf-b
                   (lambda (&rest args)
                     (setq log (cons (cons 'b args) log))))
             (fset 'neovm--combo-plus-name-btaf-a
                   (lambda (&rest args)
                     (setq log (cons (cons 'a args) log))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add '+ :before 'neovm--combo-plus-name-btaf-b '((name . neovm--combo-plus-name-btaf-shared)))
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-btaf-b '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-btaf-a '+) t nil)))
                   (progn
                     (advice-add '+ :after 'neovm--combo-plus-name-btaf-a '((name . neovm--combo-plus-name-btaf-shared)))
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-btaf-b '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-btaf-a '+) t nil)))
                   (progn
                     (advice-remove '+ 'neovm--combo-plus-name-btaf-b)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-btaf-b '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-btaf-a '+) t nil)))
                   (progn
                     (advice-remove '+ 'neovm--combo-plus-name-btaf-a)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-name-btaf-b '+) t nil)
                       (if (advice-member-p 'neovm--combo-plus-name-btaf-a '+) t nil))))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-name-btaf-b)
                 (error nil))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-name-btaf-a)
                 (error nil))
               (fmakunbound 'neovm--combo-plus-name-btaf-b)
               (fmakunbound 'neovm--combo-plus-name-btaf-a))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_alias_same_name_override_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (defalias 'neovm--combo-plus-alias '+)
           (fset 'neovm--combo-plus-alias-name-ov1 (lambda (x y) x))
           (fset 'neovm--combo-plus-alias-name-ov2 (lambda (x y) y))
           (unwind-protect
               (list
                 (progn
                   (advice-add 'neovm--combo-plus-alias :override 'neovm--combo-plus-alias-name-ov1 '((name . neovm--combo-plus-alias-name-ov-shared)))
                   (list
                     (neovm--combo-plus-alias {a} {b})
                     (funcall 'neovm--combo-plus-alias {a} {b})
                     (apply 'neovm--combo-plus-alias (list {a} {b}))
                     (+ {a} {b})
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov1 'neovm--combo-plus-alias) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov2 'neovm--combo-plus-alias) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov2 '+) t nil)))
                 (progn
                   (advice-add 'neovm--combo-plus-alias :override 'neovm--combo-plus-alias-name-ov2 '((name . neovm--combo-plus-alias-name-ov-shared)))
                   (list
                     (neovm--combo-plus-alias {a} {b})
                     (funcall 'neovm--combo-plus-alias {a} {b})
                     (apply 'neovm--combo-plus-alias (list {a} {b}))
                     (+ {a} {b})
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov1 'neovm--combo-plus-alias) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov2 'neovm--combo-plus-alias) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov2 '+) t nil)))
                 (progn
                   (advice-remove 'neovm--combo-plus-alias 'neovm--combo-plus-alias-name-ov1)
                   (list
                     (neovm--combo-plus-alias {a} {b})
                     (funcall 'neovm--combo-plus-alias {a} {b})
                     (apply 'neovm--combo-plus-alias (list {a} {b}))
                     (+ {a} {b})
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov1 'neovm--combo-plus-alias) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov2 'neovm--combo-plus-alias) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov2 '+) t nil)))
                 (progn
                   (advice-remove 'neovm--combo-plus-alias 'neovm--combo-plus-alias-name-ov2)
                   (list
                     (neovm--combo-plus-alias {a} {b})
                     (funcall 'neovm--combo-plus-alias {a} {b})
                     (apply 'neovm--combo-plus-alias (list {a} {b}))
                     (+ {a} {b})
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov1 'neovm--combo-plus-alias) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov2 'neovm--combo-plus-alias) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-alias-name-ov2 '+) t nil))))
             (condition-case nil
                 (advice-remove 'neovm--combo-plus-alias 'neovm--combo-plus-alias-name-ov1)
               (error nil))
             (condition-case nil
                 (advice-remove 'neovm--combo-plus-alias 'neovm--combo-plus-alias-name-ov2)
               (error nil))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-alias-name-ov1)
               (error nil))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-alias-name-ov2)
               (error nil))
             (fmakunbound 'neovm--combo-plus-alias)
             (fmakunbound 'neovm--combo-plus-alias-name-ov1)
             (fmakunbound 'neovm--combo-plus-alias-name-ov2)))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_alias_same_name_override_to_after_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((log nil))
             (defalias 'neovm--combo-plus-alias-ovaf '+)
             (fset 'neovm--combo-plus-alias-ovaf-ov (lambda (x y) x))
             (fset 'neovm--combo-plus-alias-ovaf-af
                   (lambda (&rest args)
                     (setq log (cons args log))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-plus-alias-ovaf :override 'neovm--combo-plus-alias-ovaf-ov '((name . neovm--combo-plus-alias-ovaf-shared)))
                     (setq log nil)
                     (list
                       (neovm--combo-plus-alias-ovaf {a} {b})
                       (funcall 'neovm--combo-plus-alias-ovaf {a} {b})
                       (apply 'neovm--combo-plus-alias-ovaf (list {a} {b}))
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-alias-ovaf-ov 'neovm--combo-plus-alias-ovaf) t nil)
                       (if (advice-member-p 'neovm--combo-plus-alias-ovaf-af 'neovm--combo-plus-alias-ovaf) t nil)))
                   (progn
                     (advice-add 'neovm--combo-plus-alias-ovaf :after 'neovm--combo-plus-alias-ovaf-af '((name . neovm--combo-plus-alias-ovaf-shared)))
                     (setq log nil)
                     (list
                       (neovm--combo-plus-alias-ovaf {a} {b})
                       (funcall 'neovm--combo-plus-alias-ovaf {a} {b})
                       (apply 'neovm--combo-plus-alias-ovaf (list {a} {b}))
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-alias-ovaf-ov 'neovm--combo-plus-alias-ovaf) t nil)
                       (if (advice-member-p 'neovm--combo-plus-alias-ovaf-af 'neovm--combo-plus-alias-ovaf) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-plus-alias-ovaf 'neovm--combo-plus-alias-ovaf-ov)
                     (setq log nil)
                     (list
                       (neovm--combo-plus-alias-ovaf {a} {b})
                       (funcall 'neovm--combo-plus-alias-ovaf {a} {b})
                       (apply 'neovm--combo-plus-alias-ovaf (list {a} {b}))
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-alias-ovaf-ov 'neovm--combo-plus-alias-ovaf) t nil)
                       (if (advice-member-p 'neovm--combo-plus-alias-ovaf-af 'neovm--combo-plus-alias-ovaf) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-plus-alias-ovaf 'neovm--combo-plus-alias-ovaf-af)
                     (setq log nil)
                     (list
                       (neovm--combo-plus-alias-ovaf {a} {b})
                       (funcall 'neovm--combo-plus-alias-ovaf {a} {b})
                       (apply 'neovm--combo-plus-alias-ovaf (list {a} {b}))
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-alias-ovaf-ov 'neovm--combo-plus-alias-ovaf) t nil)
                       (if (advice-member-p 'neovm--combo-plus-alias-ovaf-af 'neovm--combo-plus-alias-ovaf) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-plus-alias-ovaf 'neovm--combo-plus-alias-ovaf-ov)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-plus-alias-ovaf 'neovm--combo-plus-alias-ovaf-af)
                 (error nil))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-alias-ovaf-ov)
                 (error nil))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-alias-ovaf-af)
                 (error nil))
               (fmakunbound 'neovm--combo-plus-alias-ovaf)
               (fmakunbound 'neovm--combo-plus-alias-ovaf-ov)
               (fmakunbound 'neovm--combo-plus-alias-ovaf-af))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_alias_same_name_before_to_after_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((log nil))
             (defalias 'neovm--combo-plus-alias-btaf '+)
             (fset 'neovm--combo-plus-alias-btaf-b
                   (lambda (&rest args)
                     (setq log (cons (cons 'b args) log))))
             (fset 'neovm--combo-plus-alias-btaf-a
                   (lambda (&rest args)
                     (setq log (cons (cons 'a args) log))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add 'neovm--combo-plus-alias-btaf :before 'neovm--combo-plus-alias-btaf-b '((name . neovm--combo-plus-alias-btaf-shared)))
                     (setq log nil)
                     (list
                       (neovm--combo-plus-alias-btaf {a} {b})
                       (funcall 'neovm--combo-plus-alias-btaf {a} {b})
                       (apply 'neovm--combo-plus-alias-btaf (list {a} {b}))
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-alias-btaf-b 'neovm--combo-plus-alias-btaf) t nil)
                       (if (advice-member-p 'neovm--combo-plus-alias-btaf-a 'neovm--combo-plus-alias-btaf) t nil)))
                   (progn
                     (advice-add 'neovm--combo-plus-alias-btaf :after 'neovm--combo-plus-alias-btaf-a '((name . neovm--combo-plus-alias-btaf-shared)))
                     (setq log nil)
                     (list
                       (neovm--combo-plus-alias-btaf {a} {b})
                       (funcall 'neovm--combo-plus-alias-btaf {a} {b})
                       (apply 'neovm--combo-plus-alias-btaf (list {a} {b}))
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-alias-btaf-b 'neovm--combo-plus-alias-btaf) t nil)
                       (if (advice-member-p 'neovm--combo-plus-alias-btaf-a 'neovm--combo-plus-alias-btaf) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-plus-alias-btaf 'neovm--combo-plus-alias-btaf-b)
                     (setq log nil)
                     (list
                       (neovm--combo-plus-alias-btaf {a} {b})
                       (funcall 'neovm--combo-plus-alias-btaf {a} {b})
                       (apply 'neovm--combo-plus-alias-btaf (list {a} {b}))
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-alias-btaf-b 'neovm--combo-plus-alias-btaf) t nil)
                       (if (advice-member-p 'neovm--combo-plus-alias-btaf-a 'neovm--combo-plus-alias-btaf) t nil)))
                   (progn
                     (advice-remove 'neovm--combo-plus-alias-btaf 'neovm--combo-plus-alias-btaf-a)
                     (setq log nil)
                     (list
                       (neovm--combo-plus-alias-btaf {a} {b})
                       (funcall 'neovm--combo-plus-alias-btaf {a} {b})
                       (apply 'neovm--combo-plus-alias-btaf (list {a} {b}))
                       (+ {a} {b})
                       (nreverse log)
                       (if (advice-member-p 'neovm--combo-plus-alias-btaf-b 'neovm--combo-plus-alias-btaf) t nil)
                       (if (advice-member-p 'neovm--combo-plus-alias-btaf-a 'neovm--combo-plus-alias-btaf) t nil))))
               (condition-case nil
                   (advice-remove 'neovm--combo-plus-alias-btaf 'neovm--combo-plus-alias-btaf-b)
                 (error nil))
               (condition-case nil
                   (advice-remove 'neovm--combo-plus-alias-btaf 'neovm--combo-plus-alias-btaf-a)
                 (error nil))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-alias-btaf-b)
                 (error nil))
               (condition-case nil
                   (advice-remove '+ 'neovm--combo-plus-alias-btaf-a)
                 (error nil))
               (fmakunbound 'neovm--combo-plus-alias-btaf)
               (fmakunbound 'neovm--combo-plus-alias-btaf-b)
               (fmakunbound 'neovm--combo-plus-alias-btaf-a))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_anonymous_same_name_override_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((ov1 (lambda (x y) x))
                 (ov2 (lambda (x y) y)))
             (unwind-protect
                 (list
                   (progn
                     (advice-add '+ :override ov1 '((name . neovm--combo-plus-anon-ov-shared)))
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (if (advice-member-p ov1 '+) t nil)
                       (if (advice-member-p ov2 '+) t nil)))
                   (progn
                     (advice-add '+ :override ov2 '((name . neovm--combo-plus-anon-ov-shared)))
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (if (advice-member-p ov1 '+) t nil)
                       (if (advice-member-p ov2 '+) t nil)))
                   (progn
                     (advice-remove '+ ov1)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (if (advice-member-p ov1 '+) t nil)
                       (if (advice-member-p ov2 '+) t nil)))
                   (progn
                     (advice-remove '+ ov2)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (if (advice-member-p ov1 '+) t nil)
                       (if (advice-member-p ov2 '+) t nil))))
               (condition-case nil
                   (advice-remove '+ ov1)
                 (error nil))
               (condition-case nil
                   (advice-remove '+ ov2)
                 (error nil)))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_anonymous_before_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((log nil)
                 (adv (lambda (&rest args)
                        (setq log (cons args log)))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add '+ :before adv)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p adv '+) t nil)))
                   (progn
                     (advice-remove '+ adv)
                     (setq log nil)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (nreverse log)
                       (if (advice-member-p adv '+) t nil))))
               (condition-case nil
                   (advice-remove '+ adv)
                 (error nil)))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_anonymous_around_lifecycle_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (let ((adv (lambda (orig x y)
                        (funcall orig (+ x 1) y))))
             (unwind-protect
                 (list
                   (progn
                     (advice-add '+ :around adv)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (if (advice-member-p adv '+) t nil)))
                   (progn
                     (advice-remove '+ adv)
                     (list
                       (+ {a} {b})
                       (funcall '+ {a} {b})
                       (apply '+ (list {a} {b}))
                       (if (advice-member-p adv '+) t nil))))
               (condition-case nil
                   (advice-remove '+ adv)
                 (error nil)))))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_cross_target_same_name_override_isolation_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (fset 'neovm--combo-ct-name-ov-plus (lambda (x y) x))
           (fset 'neovm--combo-ct-name-ov-minus (lambda (x y) y))
           (unwind-protect
               (list
                 (progn
                   (advice-add '+ :override 'neovm--combo-ct-name-ov-plus '((name . neovm--combo-ct-name-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (- {a} {b})
                     (funcall '- {a} {b})
                     (apply '- (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-ct-name-ov-plus '+) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-minus '+) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-plus '-) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-minus '-) t nil)))
                 (progn
                   (advice-add '- :override 'neovm--combo-ct-name-ov-minus '((name . neovm--combo-ct-name-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (- {a} {b})
                     (funcall '- {a} {b})
                     (apply '- (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-ct-name-ov-plus '+) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-minus '+) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-plus '-) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-minus '-) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-ct-name-ov-plus)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (- {a} {b})
                     (funcall '- {a} {b})
                     (apply '- (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-ct-name-ov-plus '+) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-minus '+) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-plus '-) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-minus '-) t nil)))
                 (progn
                   (advice-remove '- 'neovm--combo-ct-name-ov-minus)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (- {a} {b})
                     (funcall '- {a} {b})
                     (apply '- (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-ct-name-ov-plus '+) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-minus '+) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-plus '-) t nil)
                     (if (advice-member-p 'neovm--combo-ct-name-ov-minus '-) t nil))))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-ct-name-ov-plus)
               (error nil))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-ct-name-ov-minus)
               (error nil))
             (condition-case nil
                 (advice-remove '- 'neovm--combo-ct-name-ov-plus)
               (error nil))
             (condition-case nil
                 (advice-remove '- 'neovm--combo-ct-name-ov-minus)
               (error nil))
             (fmakunbound 'neovm--combo-ct-name-ov-plus)
             (fmakunbound 'neovm--combo-ct-name-ov-minus)))",
        a = 8i64,
        b = 3i64,
    );
    crate::common::assert_oracle_parity(&form);
}

#[test]
fn oracle_prop_combination_subr_plus_same_name_override_replacement_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = format!(
        "(progn
           (fset 'neovm--combo-plus-name-ov1 (lambda (x y) x))
           (fset 'neovm--combo-plus-name-ov2 (lambda (x y) y))
           (unwind-protect
               (list
                 (progn
                   (advice-add '+ :override 'neovm--combo-plus-name-ov1 '((name . neovm--combo-plus-name-ov-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ov1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ov2 '+) t nil)))
                 (progn
                   (advice-add '+ :override 'neovm--combo-plus-name-ov2 '((name . neovm--combo-plus-name-ov-shared)))
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ov1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ov2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-ov1)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ov1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ov2 '+) t nil)))
                 (progn
                   (advice-remove '+ 'neovm--combo-plus-name-ov2)
                   (list
                     (+ {a} {b})
                     (funcall '+ {a} {b})
                     (apply '+ (list {a} {b}))
                     (if (advice-member-p 'neovm--combo-plus-name-ov1 '+) t nil)
                     (if (advice-member-p 'neovm--combo-plus-name-ov2 '+) t nil))))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-ov1)
               (error nil))
             (condition-case nil
                 (advice-remove '+ 'neovm--combo-plus-name-ov2)
               (error nil))
             (fmakunbound 'neovm--combo-plus-name-ov1)
             (fmakunbound 'neovm--combo-plus-name-ov2)))",
        a = 4i64,
        b = 7i64,
    );
    crate::common::assert_oracle_parity(&form);
}

proptest! {
    #![proptest_config({
        let mut config = proptest::test_runner::Config::with_cases(
            ORACLE_PROP_CASES.min(COMBINATION_ORACLE_PROP_CASES),
        );
        // This module contains the heaviest cross-runtime oracle properties.
        // Replaying accumulated local failure seeds before every property run
        // makes nextest timeout results depend on untracked workspace state.
        config.failure_persistence = None;
        config
    })]

    #[test]
    fn oracle_prop_combination_eval_macro_apply_arithmetic(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
        c in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop (x y z)
                 (list '+ x (list 'apply (list 'quote '+) (list 'list y z))))
               (unwind-protect
                   (eval '(neovm--combo-prop {} {} {}))
                 (fmakunbound 'neovm--combo-prop)))",
            a, b, c
        );
        let expected = (a + b + c).to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_nonlocal_exit_cleanup_state(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
        c in -10_000i64..10_000i64,
        throw_now in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let flow = if throw_now {
            "(throw 'neovm--combo-tag x)"
        } else {
            "(+ x C)"
        };
        let form = format!(
            "(let ((x {a}))
               (list
                 (catch 'neovm--combo-tag
                   (unwind-protect
                       (progn
                         (setq x (+ x {b}))
                         {flow})
                     (setq x (+ x 1))))
                 x))",
            a = a,
            b = b,
            flow = flow.replace("C", &c.to_string()),
        );

        let protected = if throw_now { a + b } else { a + b + c };
        let x_after_cleanup = a + b + 1;
        let expected = format!("({protected} {x_after_cleanup})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_throw_cleanup_updates_multiple_cells(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
        c in -10_000i64..10_000i64,
        d in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((x {a}) (y {b}))
               (list
                 (catch 'neovm--combo-tag
                   (unwind-protect
                       (progn
                         (setq x (+ x y))
                         (throw 'neovm--combo-tag (+ x {c})))
                     (setq y (- y {d}))))
                 x
                 y))",
            a = a,
            b = b,
            c = c,
            d = d,
        );

        let x_after = a + b;
        let y_after = b - d;
        let caught = x_after + c;
        let expected = format!("({caught} {x_after} {y_after})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_around_advice_call_path_matrix_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-around-path-target (lambda (x) (* 2 x)))
               (fset 'neovm--combo-around-path
                     (lambda (orig x) (+ 1 (funcall orig x))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-around-path-target :around 'neovm--combo-around-path)
                     (list
                       (funcall 'neovm--combo-around-path-target {n})
                       (apply 'neovm--combo-around-path-target (list {n}))
                       (neovm--combo-around-path-target {n})
                       (eval '(neovm--combo-around-path-target {n}))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-around-path-target 'neovm--combo-around-path)
                   (error nil))
                 (fmakunbound 'neovm--combo-around-path-target)
                 (fmakunbound 'neovm--combo-around-path)))",
            n = n,
        );

        let expected = 2 * n + 1;
        let expected_payload = format!("({expected} {expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_around_advice_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-mcall (x)
                 `(neovm--combo-prop-mtarget ,x))
               (fset 'neovm--combo-prop-mtarget (lambda (x) (* 3 x)))
               (fset 'neovm--combo-prop-mar
                     (lambda (orig x) (+ 1 (funcall orig x))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-mtarget :around 'neovm--combo-prop-mar)
                     (list
                       (neovm--combo-prop-mcall {n})
                       (eval '(neovm--combo-prop-mcall {n}))
                       (funcall 'neovm--combo-prop-mtarget {n})
                       (apply 'neovm--combo-prop-mtarget (list {n}))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-mtarget 'neovm--combo-prop-mar)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-mtarget)
                 (fmakunbound 'neovm--combo-prop-mar)
                 (fmakunbound 'neovm--combo-prop-mcall)))",
            n = n,
        );

        let expected = 3 * n + 1;
        let expected_payload = format!("({expected} {expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_advice_depth_order_call_path_consistency(
        n in -1_000i64..1_000i64,
        before_a in -100i32..100i32,
        before_b in -100i32..100i32,
        around_a in -100i32..100i32,
        around_b in -100i32..100i32,
        after_a in -100i32..100i32,
        after_b in -100i32..100i32,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-depth-call (x)
                 `(neovm--combo-prop-depth-target ,x))
               (let ((log nil))
                 (fset 'neovm--combo-prop-depth-target
                       (lambda (x)
                         (setq log (cons (list 'orig x) log))
                         (+ x 3)))
                 (fset 'neovm--combo-prop-depth-before-a
                       (lambda (&rest args)
                         (setq log (cons (cons 'before-a args) log))))
                 (fset 'neovm--combo-prop-depth-before-b
                       (lambda (&rest args)
                         (setq log (cons (cons 'before-b args) log))))
                 (fset 'neovm--combo-prop-depth-around-a
                       (lambda (orig x)
                         (setq log (cons (list 'around-a-enter x) log))
                         (let ((ret (funcall orig (+ x 1))))
                           (setq log (cons (list 'around-a-exit ret) log))
                           (+ ret 10))))
                 (fset 'neovm--combo-prop-depth-around-b
                       (lambda (orig x)
                         (setq log (cons (list 'around-b-enter x) log))
                         (let ((ret (funcall orig (* x 2))))
                           (setq log (cons (list 'around-b-exit ret) log))
                           (* ret 2))))
                 (fset 'neovm--combo-prop-depth-after-a
                       (lambda (&rest args)
                         (setq log (cons (cons 'after-a args) log))))
                 (fset 'neovm--combo-prop-depth-after-b
                       (lambda (&rest args)
                         (setq log (cons (cons 'after-b args) log))))
                 (unwind-protect
                     (progn
                       (advice-add 'neovm--combo-prop-depth-target :before 'neovm--combo-prop-depth-before-a '((depth . {before_a})))
                       (advice-add 'neovm--combo-prop-depth-target :before 'neovm--combo-prop-depth-before-b '((depth . {before_b})))
                       (advice-add 'neovm--combo-prop-depth-target :around 'neovm--combo-prop-depth-around-a '((depth . {around_a})))
                       (advice-add 'neovm--combo-prop-depth-target :around 'neovm--combo-prop-depth-around-b '((depth . {around_b})))
                       (advice-add 'neovm--combo-prop-depth-target :after 'neovm--combo-prop-depth-after-a '((depth . {after_a})))
                       (advice-add 'neovm--combo-prop-depth-target :after 'neovm--combo-prop-depth-after-b '((depth . {after_b})))
                       (list
                         (let ((log nil))
                           (list
                             (neovm--combo-prop-depth-call {n})
                             (nreverse log)))
                         (let ((log nil))
                           (list
                             (eval '(neovm--combo-prop-depth-call {n}))
                             (nreverse log)))
                         (let ((log nil))
                           (list
                             (funcall 'neovm--combo-prop-depth-target {n})
                             (nreverse log)))
                         (let ((log nil))
                           (list
                             (apply 'neovm--combo-prop-depth-target (list {n}))
                             (nreverse log)))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-depth-target 'neovm--combo-prop-depth-before-a)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-depth-target 'neovm--combo-prop-depth-before-b)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-depth-target 'neovm--combo-prop-depth-around-a)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-depth-target 'neovm--combo-prop-depth-around-b)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-depth-target 'neovm--combo-prop-depth-after-a)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-depth-target 'neovm--combo-prop-depth-after-b)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-depth-target)
                   (fmakunbound 'neovm--combo-prop-depth-before-a)
                   (fmakunbound 'neovm--combo-prop-depth-before-b)
                   (fmakunbound 'neovm--combo-prop-depth-around-a)
                   (fmakunbound 'neovm--combo-prop-depth-around-b)
                   (fmakunbound 'neovm--combo-prop-depth-after-a)
                   (fmakunbound 'neovm--combo-prop-depth-after-b)
                   (fmakunbound 'neovm--combo-prop-depth-call))))",
            n = n,
            before_a = before_a,
            before_b = before_b,
            around_a = around_a,
            around_b = around_b,
            after_a = after_a,
            after_b = after_b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_anonymous_around_advice_alias_remove_consistency(
        n in -1_000i64..1_000i64,
        delta in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-anon-call (x)
                 `(neovm--combo-prop-anon-target ,x))
               (let ((adv (let ((d {delta}))
                            (lambda (orig x)
                              (+ d (funcall orig x))))))
                 (fset 'neovm--combo-prop-anon-target (lambda (x) (+ x 1)))
                 (defalias 'neovm--combo-prop-anon-alias 'neovm--combo-prop-anon-target)
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-anon-target :around adv '((name . neovm--combo-prop-anon-name)))
                         (list
                           (neovm--combo-prop-anon-call {n})
                           (eval '(neovm--combo-prop-anon-call {n}))
                           (funcall 'neovm--combo-prop-anon-target {n})
                           (apply 'neovm--combo-prop-anon-target (list {n}))
                           (funcall (symbol-function 'neovm--combo-prop-anon-target) {n})
                           (if (advice-member-p adv 'neovm--combo-prop-anon-target) t nil)
                           (if (advice-member-p adv 'neovm--combo-prop-anon-alias) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-anon-alias adv)
                         (list
                           (neovm--combo-prop-anon-call {n})
                           (eval '(neovm--combo-prop-anon-call {n}))
                           (funcall 'neovm--combo-prop-anon-target {n})
                           (apply 'neovm--combo-prop-anon-target (list {n}))
                           (funcall (symbol-function 'neovm--combo-prop-anon-target) {n})
                           (if (advice-member-p adv 'neovm--combo-prop-anon-target) t nil)
                           (if (advice-member-p adv 'neovm--combo-prop-anon-alias) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-anon-target adv)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-anon-alias adv)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-anon-target)
                   (fmakunbound 'neovm--combo-prop-anon-alias)
                   (fmakunbound 'neovm--combo-prop-anon-call))))",
            n = n,
            delta = delta,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_anonymous_advice_symbol_function_capture_rebind_consistency(
        n in -1_000i64..1_000i64,
        delta in -1_000i64..1_000i64,
        mul in -20i64..20i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-anon-rebind-call (x)
                 `(neovm--combo-prop-anon-rebind-target ,x))
               (fset 'neovm--combo-prop-anon-rebind-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-anon-rebind-alias 'neovm--combo-prop-anon-rebind-target)
               (let* ((adv (let ((d {delta}))
                             (lambda (orig x)
                               (+ d (funcall orig x)))))
                      (f0 nil))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-anon-rebind-target :around adv)
                         (setq f0 (symbol-function 'neovm--combo-prop-anon-rebind-target))
                         (list
                           (neovm--combo-prop-anon-rebind-call {n})
                           (eval '(neovm--combo-prop-anon-rebind-call {n}))
                           (funcall 'neovm--combo-prop-anon-rebind-target {n})
                           (funcall 'neovm--combo-prop-anon-rebind-alias {n})
                           (funcall f0 {n})
                           (progn
                             (fset 'neovm--combo-prop-anon-rebind-target (lambda (x) (* x {mul})))
                             (list
                               (neovm--combo-prop-anon-rebind-call {n})
                               (eval '(neovm--combo-prop-anon-rebind-call {n}))
                               (funcall 'neovm--combo-prop-anon-rebind-target {n})
                               (funcall 'neovm--combo-prop-anon-rebind-alias {n})
                               (funcall f0 {n})
                               (apply f0 (list {n}))
                               (apply 'neovm--combo-prop-anon-rebind-target (list {n}))))))
                       (progn
                         (advice-remove 'neovm--combo-prop-anon-rebind-alias adv)
                         (list
                           (neovm--combo-prop-anon-rebind-call {n})
                           (eval '(neovm--combo-prop-anon-rebind-call {n}))
                           (funcall 'neovm--combo-prop-anon-rebind-target {n})
                           (funcall 'neovm--combo-prop-anon-rebind-alias {n})
                           (funcall f0 {n})
                           (apply f0 (list {n}))
                           (if (advice-member-p adv 'neovm--combo-prop-anon-rebind-target) t nil)
                           (if (advice-member-p adv 'neovm--combo-prop-anon-rebind-alias) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-anon-rebind-target adv)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-anon-rebind-alias adv)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-anon-rebind-target)
                   (fmakunbound 'neovm--combo-prop-anon-rebind-alias)
                   (fmakunbound 'neovm--combo-prop-anon-rebind-call))))",
            n = n,
            delta = delta,
            mul = mul,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_distinct_anonymous_around_chain_remove_consistency(
        n in -1_000i64..1_000i64,
        d1 in -100i64..100i64,
        m2 in -20i64..20i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-anon-chain-call (x)
                 `(neovm--combo-prop-anon-chain-target ,x))
               (fset 'neovm--combo-prop-anon-chain-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-anon-chain-alias 'neovm--combo-prop-anon-chain-target)
               (let* ((a1 (let ((d {d1}))
                            (lambda (orig x)
                              (+ d (funcall orig x)))))
                      (a2 (let ((m {m2}))
                            (lambda (orig x)
                              (* m (funcall orig x))))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-anon-chain-target :around a1 '((name . neovm--combo-prop-anon-chain-a1) (depth . -20)))
                         (advice-add 'neovm--combo-prop-anon-chain-target :around a2 '((name . neovm--combo-prop-anon-chain-a2) (depth . 20)))
                         (list
                           (neovm--combo-prop-anon-chain-call {n})
                           (eval '(neovm--combo-prop-anon-chain-call {n}))
                           (funcall 'neovm--combo-prop-anon-chain-target {n})
                           (apply 'neovm--combo-prop-anon-chain-target (list {n}))
                           (if (advice-member-p a1 'neovm--combo-prop-anon-chain-target) t nil)
                           (if (advice-member-p a2 'neovm--combo-prop-anon-chain-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-anon-chain-target a1)
                         (list
                           (neovm--combo-prop-anon-chain-call {n})
                           (eval '(neovm--combo-prop-anon-chain-call {n}))
                           (funcall 'neovm--combo-prop-anon-chain-target {n})
                           (apply 'neovm--combo-prop-anon-chain-target (list {n}))
                           (if (advice-member-p a1 'neovm--combo-prop-anon-chain-target) t nil)
                           (if (advice-member-p a2 'neovm--combo-prop-anon-chain-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-anon-chain-alias a2)
                         (list
                           (neovm--combo-prop-anon-chain-call {n})
                           (eval '(neovm--combo-prop-anon-chain-call {n}))
                           (funcall 'neovm--combo-prop-anon-chain-target {n})
                           (apply 'neovm--combo-prop-anon-chain-target (list {n}))
                           (if (advice-member-p a1 'neovm--combo-prop-anon-chain-target) t nil)
                           (if (advice-member-p a2 'neovm--combo-prop-anon-chain-target) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-anon-chain-target a1)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-anon-chain-target a2)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-anon-chain-alias a1)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-anon-chain-alias a2)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-anon-chain-target)
                   (fmakunbound 'neovm--combo-prop-anon-chain-alias)
                   (fmakunbound 'neovm--combo-prop-anon-chain-call))))",
            n = n,
            d1 = d1,
            m2 = m2,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_fmakunbound_rebind_under_anonymous_advice_consistency(
        n in -1_000i64..1_000i64,
        delta in -1_000i64..1_000i64,
        mul in -20i64..20i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-fm-call (x)
                 `(neovm--combo-prop-fm-target ,x))
               (fset 'neovm--combo-prop-fm-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-fm-alias 'neovm--combo-prop-fm-target)
               (let* ((adv (let ((d {delta}))
                             (lambda (orig x)
                               (+ d (funcall orig x)))))
                      (f0 nil))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-fm-target :around adv)
                         (setq f0 (symbol-function 'neovm--combo-prop-fm-target))
                         (list
                           (neovm--combo-prop-fm-call {n})
                           (eval '(neovm--combo-prop-fm-call {n}))
                           (funcall 'neovm--combo-prop-fm-target {n})
                           (funcall 'neovm--combo-prop-fm-alias {n})
                           (funcall f0 {n})
                           (progn
                             (fmakunbound 'neovm--combo-prop-fm-target)
                             (list
                               (condition-case err
                                   (funcall 'neovm--combo-prop-fm-alias {n})
                                 (error (list 'err (car err))))
                               (progn
                                 (fset 'neovm--combo-prop-fm-target (lambda (x) (* x {mul})))
                                 (list
                                   (neovm--combo-prop-fm-call {n})
                                   (eval '(neovm--combo-prop-fm-call {n}))
                                   (funcall 'neovm--combo-prop-fm-target {n})
                                   (funcall 'neovm--combo-prop-fm-alias {n})
                                   (funcall f0 {n})
                                   (apply f0 (list {n}))
                                   (if (advice-member-p adv 'neovm--combo-prop-fm-target) t nil)
                                   (if (advice-member-p adv 'neovm--combo-prop-fm-alias) t nil)))))))
                       (progn
                         (condition-case err
                             (progn
                               (advice-remove 'neovm--combo-prop-fm-alias adv)
                               'removed)
                           (error (list 'remove-err (car err))))
                         (list
                           (neovm--combo-prop-fm-call {n})
                           (eval '(neovm--combo-prop-fm-call {n}))
                           (funcall 'neovm--combo-prop-fm-target {n})
                           (funcall 'neovm--combo-prop-fm-alias {n})
                           (funcall f0 {n})
                           (apply f0 (list {n}))
                           (if (advice-member-p adv 'neovm--combo-prop-fm-target) t nil)
                           (if (advice-member-p adv 'neovm--combo-prop-fm-alias) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-fm-target adv)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-fm-alias adv)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-fm-target)
                   (fmakunbound 'neovm--combo-prop-fm-alias)
                   (fmakunbound 'neovm--combo-prop-fm-call))))",
            n = n,
            delta = delta,
            mul = mul,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_duplicate_same_anonymous_advice_lifecycle_consistency(
        n in -1_000i64..1_000i64,
        delta in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-dup-anon-call (x)
                 `(neovm--combo-prop-dup-anon-target ,x))
               (fset 'neovm--combo-prop-dup-anon-target (lambda (x) (+ x 1)))
               (let ((adv (let ((d {delta}))
                            (lambda (orig x)
                              (+ d (funcall orig x))))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-dup-anon-target :around adv '((name . neovm--combo-prop-dup-anon-a1) (depth . -10)))
                         (advice-add 'neovm--combo-prop-dup-anon-target :around adv '((name . neovm--combo-prop-dup-anon-a2) (depth . 10)))
                         (list
                           (neovm--combo-prop-dup-anon-call {n})
                           (eval '(neovm--combo-prop-dup-anon-call {n}))
                           (funcall 'neovm--combo-prop-dup-anon-target {n})
                           (apply 'neovm--combo-prop-dup-anon-target (list {n}))
                           (if (advice-member-p adv 'neovm--combo-prop-dup-anon-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-dup-anon-target adv)
                         (list
                           (neovm--combo-prop-dup-anon-call {n})
                           (eval '(neovm--combo-prop-dup-anon-call {n}))
                           (funcall 'neovm--combo-prop-dup-anon-target {n})
                           (apply 'neovm--combo-prop-dup-anon-target (list {n}))
                           (if (advice-member-p adv 'neovm--combo-prop-dup-anon-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-dup-anon-target adv)
                         (list
                           (neovm--combo-prop-dup-anon-call {n})
                           (eval '(neovm--combo-prop-dup-anon-call {n}))
                           (funcall 'neovm--combo-prop-dup-anon-target {n})
                           (apply 'neovm--combo-prop-dup-anon-target (list {n}))
                           (if (advice-member-p adv 'neovm--combo-prop-dup-anon-target) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-dup-anon-target adv)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-dup-anon-target)
                   (fmakunbound 'neovm--combo-prop-dup-anon-call))))",
            n = n,
            delta = delta,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_around_filter_return_rebind_lifecycle_consistency(
        n in -1_000i64..1_000i64,
        mul in -20i64..20i64,
        add_filter_first in any::<bool>(),
        remove_on_alias in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let add_order = if add_filter_first {
            "(progn
               (advice-add 'neovm--combo-prop-arf-target :filter-return 'neovm--combo-prop-arf-filter-ret)
               (advice-add 'neovm--combo-prop-arf-target :around 'neovm--combo-prop-arf-around))"
        } else {
            "(progn
               (advice-add 'neovm--combo-prop-arf-target :around 'neovm--combo-prop-arf-around)
               (advice-add 'neovm--combo-prop-arf-target :filter-return 'neovm--combo-prop-arf-filter-ret))"
        };

        let remove_sym = if remove_on_alias {
            "neovm--combo-prop-arf-alias"
        } else {
            "neovm--combo-prop-arf-target"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-arf-call (x)
                 `(neovm--combo-prop-arf-target ,x))
               (fset 'neovm--combo-prop-arf-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-arf-alias 'neovm--combo-prop-arf-target)
               (fset 'neovm--combo-prop-arf-around
                     (lambda (orig x)
                       (+ 10 (funcall orig x))))
               (fset 'neovm--combo-prop-arf-filter-ret
                     (lambda (ret)
                       (* ret 3)))
               (unwind-protect
                   (list
                     (progn
                       {add_order}
                       (list
                         (neovm--combo-prop-arf-call {n})
                         (eval '(neovm--combo-prop-arf-call {n}))
                         (funcall 'neovm--combo-prop-arf-target {n})
                         (apply 'neovm--combo-prop-arf-target (list {n}))
                         (funcall 'neovm--combo-prop-arf-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-arf-around 'neovm--combo-prop-arf-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-arf-filter-ret 'neovm--combo-prop-arf-target) t nil)))
                     (progn
                       (fset 'neovm--combo-prop-arf-target (lambda (x) (* x {mul})))
                       (list
                         (neovm--combo-prop-arf-call {n})
                         (eval '(neovm--combo-prop-arf-call {n}))
                         (funcall 'neovm--combo-prop-arf-target {n})
                         (apply 'neovm--combo-prop-arf-target (list {n}))
                         (funcall 'neovm--combo-prop-arf-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-arf-around 'neovm--combo-prop-arf-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-arf-filter-ret 'neovm--combo-prop-arf-target) t nil)))
                     (progn
                       (advice-remove '{remove_sym} 'neovm--combo-prop-arf-filter-ret)
                       (advice-remove '{remove_sym} 'neovm--combo-prop-arf-around)
                       (list
                         (neovm--combo-prop-arf-call {n})
                         (eval '(neovm--combo-prop-arf-call {n}))
                         (funcall 'neovm--combo-prop-arf-target {n})
                         (apply 'neovm--combo-prop-arf-target (list {n}))
                         (funcall 'neovm--combo-prop-arf-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-arf-around 'neovm--combo-prop-arf-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-arf-filter-ret 'neovm--combo-prop-arf-target) t nil))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-arf-target 'neovm--combo-prop-arf-around)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-arf-target 'neovm--combo-prop-arf-filter-ret)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-arf-alias 'neovm--combo-prop-arf-around)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-arf-alias 'neovm--combo-prop-arf-filter-ret)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-arf-target)
                 (fmakunbound 'neovm--combo-prop-arf-alias)
                 (fmakunbound 'neovm--combo-prop-arf-around)
                 (fmakunbound 'neovm--combo-prop-arf-filter-ret)
                 (fmakunbound 'neovm--combo-prop-arf-call)))",
            add_order = add_order,
            remove_sym = remove_sym,
            n = n,
            mul = mul,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_capture_combined_advice_then_rebind_consistency(
        n in -1_000i64..1_000i64,
        mul in -20i64..20i64,
        add_filter_first in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let add_order = if add_filter_first {
            "(progn
               (advice-add 'neovm--combo-prop-cap-combined-target :filter-return 'neovm--combo-prop-cap-combined-ret)
               (advice-add 'neovm--combo-prop-cap-combined-target :around 'neovm--combo-prop-cap-combined-around))"
        } else {
            "(progn
               (advice-add 'neovm--combo-prop-cap-combined-target :around 'neovm--combo-prop-cap-combined-around)
               (advice-add 'neovm--combo-prop-cap-combined-target :filter-return 'neovm--combo-prop-cap-combined-ret))"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-cap-combined-call (x)
                 `(neovm--combo-prop-cap-combined-target ,x))
               (fset 'neovm--combo-prop-cap-combined-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-cap-combined-alias 'neovm--combo-prop-cap-combined-target)
               (fset 'neovm--combo-prop-cap-combined-around
                     (lambda (orig x)
                       (+ 10 (funcall orig x))))
               (fset 'neovm--combo-prop-cap-combined-ret
                     (lambda (ret)
                       (* ret 3)))
               (let ((f0 nil))
                 (unwind-protect
                     (progn
                       {add_order}
                       (setq f0 (symbol-function 'neovm--combo-prop-cap-combined-target))
                       (list
                         (list
                           (neovm--combo-prop-cap-combined-call {n})
                           (eval '(neovm--combo-prop-cap-combined-call {n}))
                           (funcall 'neovm--combo-prop-cap-combined-target {n})
                           (apply 'neovm--combo-prop-cap-combined-target (list {n}))
                           (funcall 'neovm--combo-prop-cap-combined-alias {n})
                           (funcall f0 {n})
                           (funcall (symbol-function 'neovm--combo-prop-cap-combined-target) {n})
                           (if (advice-member-p 'neovm--combo-prop-cap-combined-around 'neovm--combo-prop-cap-combined-target) t nil)
                           (if (advice-member-p 'neovm--combo-prop-cap-combined-ret 'neovm--combo-prop-cap-combined-target) t nil))
                         (progn
                           (fset 'neovm--combo-prop-cap-combined-target (lambda (x) (* x {mul})))
                           (list
                             (neovm--combo-prop-cap-combined-call {n})
                             (eval '(neovm--combo-prop-cap-combined-call {n}))
                             (funcall 'neovm--combo-prop-cap-combined-target {n})
                             (apply 'neovm--combo-prop-cap-combined-target (list {n}))
                             (funcall 'neovm--combo-prop-cap-combined-alias {n})
                             (funcall f0 {n})
                             (funcall (symbol-function 'neovm--combo-prop-cap-combined-target) {n})
                             (if (advice-member-p 'neovm--combo-prop-cap-combined-around 'neovm--combo-prop-cap-combined-target) t nil)
                             (if (advice-member-p 'neovm--combo-prop-cap-combined-ret 'neovm--combo-prop-cap-combined-target) t nil)))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-cap-combined-target 'neovm--combo-prop-cap-combined-around)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-cap-combined-target 'neovm--combo-prop-cap-combined-ret)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-cap-combined-alias 'neovm--combo-prop-cap-combined-around)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-cap-combined-alias 'neovm--combo-prop-cap-combined-ret)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-cap-combined-target)
                   (fmakunbound 'neovm--combo-prop-cap-combined-alias)
                   (fmakunbound 'neovm--combo-prop-cap-combined-around)
                   (fmakunbound 'neovm--combo-prop-cap-combined-ret)
                   (fmakunbound 'neovm--combo-prop-cap-combined-call))))",
            add_order = add_order,
            n = n,
            mul = mul,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_alias_rebind_with_split_advice_and_captured_cells_consistency(
        n in -1_000i64..1_000i64,
        mul in -20i64..20i64,
        add_ret_first in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let add_order = if add_ret_first {
            "(progn
               (advice-add 'neovm--combo-prop-split-target :filter-return 'neovm--combo-prop-split-ret)
               (advice-add 'neovm--combo-prop-split-alias :around 'neovm--combo-prop-split-around))"
        } else {
            "(progn
               (advice-add 'neovm--combo-prop-split-alias :around 'neovm--combo-prop-split-around)
               (advice-add 'neovm--combo-prop-split-target :filter-return 'neovm--combo-prop-split-ret))"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-split-call-target (x)
                 `(neovm--combo-prop-split-target ,x))
               (defmacro neovm--combo-prop-split-call-alias (x)
                 `(neovm--combo-prop-split-alias ,x))
               (fset 'neovm--combo-prop-split-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-split-alias 'neovm--combo-prop-split-target)
               (fset 'neovm--combo-prop-split-around
                     (lambda (orig x)
                       (+ 10 (funcall orig x))))
               (fset 'neovm--combo-prop-split-ret
                     (lambda (ret)
                       (* ret 3)))
               (let ((fa nil) (ft nil))
                 (unwind-protect
                     (progn
                       {add_order}
                       (setq fa (symbol-function 'neovm--combo-prop-split-alias))
                       (setq ft (symbol-function 'neovm--combo-prop-split-target))
                       (list
                         (list
                           (neovm--combo-prop-split-call-target {n})
                           (eval '(neovm--combo-prop-split-call-target {n}))
                           (neovm--combo-prop-split-call-alias {n})
                           (eval '(neovm--combo-prop-split-call-alias {n}))
                           (funcall 'neovm--combo-prop-split-target {n})
                           (funcall 'neovm--combo-prop-split-alias {n})
                           (funcall fa {n})
                           (funcall ft {n})
                           (if (advice-member-p 'neovm--combo-prop-split-around 'neovm--combo-prop-split-target) t nil)
                           (if (advice-member-p 'neovm--combo-prop-split-around 'neovm--combo-prop-split-alias) t nil)
                           (if (advice-member-p 'neovm--combo-prop-split-ret 'neovm--combo-prop-split-target) t nil)
                           (if (advice-member-p 'neovm--combo-prop-split-ret 'neovm--combo-prop-split-alias) t nil))
                         (progn
                           (defalias 'neovm--combo-prop-split-alias (lambda (x) (* x {mul})))
                           (list
                             (neovm--combo-prop-split-call-target {n})
                             (eval '(neovm--combo-prop-split-call-target {n}))
                             (neovm--combo-prop-split-call-alias {n})
                             (eval '(neovm--combo-prop-split-call-alias {n}))
                             (funcall 'neovm--combo-prop-split-target {n})
                             (funcall 'neovm--combo-prop-split-alias {n})
                             (funcall fa {n})
                             (funcall ft {n})
                             (if (advice-member-p 'neovm--combo-prop-split-around 'neovm--combo-prop-split-target) t nil)
                             (if (advice-member-p 'neovm--combo-prop-split-around 'neovm--combo-prop-split-alias) t nil)
                             (if (advice-member-p 'neovm--combo-prop-split-ret 'neovm--combo-prop-split-target) t nil)
                             (if (advice-member-p 'neovm--combo-prop-split-ret 'neovm--combo-prop-split-alias) t nil)))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-split-target 'neovm--combo-prop-split-around)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-split-target 'neovm--combo-prop-split-ret)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-split-alias 'neovm--combo-prop-split-around)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-split-alias 'neovm--combo-prop-split-ret)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-split-target)
                   (fmakunbound 'neovm--combo-prop-split-alias)
                   (fmakunbound 'neovm--combo-prop-split-around)
                   (fmakunbound 'neovm--combo-prop-split-ret)
                   (fmakunbound 'neovm--combo-prop-split-call-target)
                   (fmakunbound 'neovm--combo-prop-split-call-alias))))",
            add_order = add_order,
            n = n,
            mul = mul,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_fset_alias_unlink_under_stacked_advice_consistency(
        n in -1_000i64..1_000i64,
        mul in -20i64..20i64,
        add_filter_first in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let add_order = if add_filter_first {
            "(progn
               (advice-add 'neovm--combo-prop-fset-alias-a :filter-return 'neovm--combo-prop-fset-alias-ret)
               (advice-add 'neovm--combo-prop-fset-alias-a :around 'neovm--combo-prop-fset-alias-around))"
        } else {
            "(progn
               (advice-add 'neovm--combo-prop-fset-alias-a :around 'neovm--combo-prop-fset-alias-around)
               (advice-add 'neovm--combo-prop-fset-alias-a :filter-return 'neovm--combo-prop-fset-alias-ret))"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-fset-alias-call-a (x)
                 `(neovm--combo-prop-fset-alias-a ,x))
               (defmacro neovm--combo-prop-fset-alias-call-t (x)
                 `(neovm--combo-prop-fset-alias-target ,x))
               (fset 'neovm--combo-prop-fset-alias-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-fset-alias-a 'neovm--combo-prop-fset-alias-target)
               (fset 'neovm--combo-prop-fset-alias-around
                     (lambda (orig x)
                       (+ 10 (funcall orig x))))
               (fset 'neovm--combo-prop-fset-alias-ret
                     (lambda (ret)
                       (* ret 2)))
               (let ((fa0 nil) (ft0 nil))
                 (unwind-protect
                     (progn
                       {add_order}
                       (setq fa0 (symbol-function 'neovm--combo-prop-fset-alias-a))
                       (setq ft0 (symbol-function 'neovm--combo-prop-fset-alias-target))
                       (list
                         (list
                           (neovm--combo-prop-fset-alias-call-a {n})
                           (eval '(neovm--combo-prop-fset-alias-call-a {n}))
                           (neovm--combo-prop-fset-alias-call-t {n})
                           (eval '(neovm--combo-prop-fset-alias-call-t {n}))
                           (funcall 'neovm--combo-prop-fset-alias-a {n})
                           (funcall 'neovm--combo-prop-fset-alias-target {n})
                           (funcall fa0 {n})
                           (funcall ft0 {n})
                           (if (advice-member-p 'neovm--combo-prop-fset-alias-around 'neovm--combo-prop-fset-alias-a) t nil)
                           (if (advice-member-p 'neovm--combo-prop-fset-alias-ret 'neovm--combo-prop-fset-alias-a) t nil)
                           (if (advice-member-p 'neovm--combo-prop-fset-alias-around 'neovm--combo-prop-fset-alias-target) t nil)
                           (if (advice-member-p 'neovm--combo-prop-fset-alias-ret 'neovm--combo-prop-fset-alias-target) t nil))
                         (progn
                           (fset 'neovm--combo-prop-fset-alias-a (lambda (x) (* x {mul})))
                           (list
                             (neovm--combo-prop-fset-alias-call-a {n})
                             (eval '(neovm--combo-prop-fset-alias-call-a {n}))
                             (neovm--combo-prop-fset-alias-call-t {n})
                             (eval '(neovm--combo-prop-fset-alias-call-t {n}))
                             (funcall 'neovm--combo-prop-fset-alias-a {n})
                             (funcall 'neovm--combo-prop-fset-alias-target {n})
                             (funcall fa0 {n})
                             (funcall ft0 {n})
                             (if (advice-member-p 'neovm--combo-prop-fset-alias-around 'neovm--combo-prop-fset-alias-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-fset-alias-ret 'neovm--combo-prop-fset-alias-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-fset-alias-around 'neovm--combo-prop-fset-alias-target) t nil)
                             (if (advice-member-p 'neovm--combo-prop-fset-alias-ret 'neovm--combo-prop-fset-alias-target) t nil)))
                         (progn
                           (advice-remove 'neovm--combo-prop-fset-alias-a 'neovm--combo-prop-fset-alias-around)
                           (advice-remove 'neovm--combo-prop-fset-alias-a 'neovm--combo-prop-fset-alias-ret)
                           (list
                             (neovm--combo-prop-fset-alias-call-a {n})
                             (eval '(neovm--combo-prop-fset-alias-call-a {n}))
                             (neovm--combo-prop-fset-alias-call-t {n})
                             (eval '(neovm--combo-prop-fset-alias-call-t {n}))
                             (funcall 'neovm--combo-prop-fset-alias-a {n})
                             (funcall 'neovm--combo-prop-fset-alias-target {n})
                             (funcall fa0 {n})
                             (funcall ft0 {n})
                             (if (advice-member-p 'neovm--combo-prop-fset-alias-around 'neovm--combo-prop-fset-alias-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-fset-alias-ret 'neovm--combo-prop-fset-alias-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-fset-alias-around 'neovm--combo-prop-fset-alias-target) t nil)
                             (if (advice-member-p 'neovm--combo-prop-fset-alias-ret 'neovm--combo-prop-fset-alias-target) t nil)))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-fset-alias-a 'neovm--combo-prop-fset-alias-around)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-fset-alias-a 'neovm--combo-prop-fset-alias-ret)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-fset-alias-target 'neovm--combo-prop-fset-alias-around)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-fset-alias-target 'neovm--combo-prop-fset-alias-ret)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-fset-alias-target)
                   (fmakunbound 'neovm--combo-prop-fset-alias-a)
                   (fmakunbound 'neovm--combo-prop-fset-alias-around)
                   (fmakunbound 'neovm--combo-prop-fset-alias-ret)
                   (fmakunbound 'neovm--combo-prop-fset-alias-call-a)
                   (fmakunbound 'neovm--combo-prop-fset-alias-call-t))))",
            add_order = add_order,
            n = n,
            mul = mul,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_before_while_after_until_alias_rebind_consistency(
        n in -100i64..100i64,
        mul in -20i64..20i64,
        add_after_first in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let add_order = if add_after_first {
            "(progn
               (advice-add 'neovm--combo-prop-bwau-a :after-until 'neovm--combo-prop-bwau-fallback)
               (advice-add 'neovm--combo-prop-bwau-a :before-while 'neovm--combo-prop-bwau-guard))"
        } else {
            "(progn
               (advice-add 'neovm--combo-prop-bwau-a :before-while 'neovm--combo-prop-bwau-guard)
               (advice-add 'neovm--combo-prop-bwau-a :after-until 'neovm--combo-prop-bwau-fallback))"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-bwau-call-a (x)
                 `(neovm--combo-prop-bwau-a ,x))
               (defmacro neovm--combo-prop-bwau-call-t (x)
                 `(neovm--combo-prop-bwau-target ,x))
               (fset 'neovm--combo-prop-bwau-target
                     (lambda (x)
                       (if (= (% x 2) 0) nil (+ x 100))))
               (defalias 'neovm--combo-prop-bwau-a 'neovm--combo-prop-bwau-target)
               (fset 'neovm--combo-prop-bwau-guard
                     (lambda (&rest args)
                       (> (car args) -1)))
               (fset 'neovm--combo-prop-bwau-fallback
                     (lambda (&rest args)
                       (list 'fb (car args))))
               (let ((fa0 nil) (ft0 nil))
                 (unwind-protect
                     (progn
                       {add_order}
                       (setq fa0 (symbol-function 'neovm--combo-prop-bwau-a))
                       (setq ft0 (symbol-function 'neovm--combo-prop-bwau-target))
                       (list
                         (list
                           (neovm--combo-prop-bwau-call-a {n})
                           (eval '(neovm--combo-prop-bwau-call-a {n_plus_one}))
                           (funcall 'neovm--combo-prop-bwau-a {n_minus_one})
                           (apply 'neovm--combo-prop-bwau-a (list {n_plus_two}))
                           (neovm--combo-prop-bwau-call-t {n})
                           (eval '(neovm--combo-prop-bwau-call-t {n_plus_one}))
                           (funcall 'neovm--combo-prop-bwau-target {n_minus_one})
                           (apply 'neovm--combo-prop-bwau-target (list {n_plus_two}))
                           (funcall fa0 {n_plus_one})
                           (funcall ft0 {n_plus_one})
                           (if (advice-member-p 'neovm--combo-prop-bwau-guard 'neovm--combo-prop-bwau-a) t nil)
                           (if (advice-member-p 'neovm--combo-prop-bwau-fallback 'neovm--combo-prop-bwau-a) t nil)
                           (if (advice-member-p 'neovm--combo-prop-bwau-guard 'neovm--combo-prop-bwau-target) t nil)
                           (if (advice-member-p 'neovm--combo-prop-bwau-fallback 'neovm--combo-prop-bwau-target) t nil))
                         (progn
                           (fset 'neovm--combo-prop-bwau-a (lambda (_x) (* {mul} 0)))
                           (list
                             (neovm--combo-prop-bwau-call-a {n})
                             (eval '(neovm--combo-prop-bwau-call-a {n_plus_one}))
                             (funcall 'neovm--combo-prop-bwau-a {n_minus_one})
                             (apply 'neovm--combo-prop-bwau-a (list {n_plus_two}))
                             (neovm--combo-prop-bwau-call-t {n})
                             (eval '(neovm--combo-prop-bwau-call-t {n_plus_one}))
                             (funcall 'neovm--combo-prop-bwau-target {n_minus_one})
                             (apply 'neovm--combo-prop-bwau-target (list {n_plus_two}))
                             (funcall fa0 {n_plus_one})
                             (funcall ft0 {n_plus_one})
                             (if (advice-member-p 'neovm--combo-prop-bwau-guard 'neovm--combo-prop-bwau-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-bwau-fallback 'neovm--combo-prop-bwau-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-bwau-guard 'neovm--combo-prop-bwau-target) t nil)
                             (if (advice-member-p 'neovm--combo-prop-bwau-fallback 'neovm--combo-prop-bwau-target) t nil)))
                         (progn
                           (advice-remove 'neovm--combo-prop-bwau-a 'neovm--combo-prop-bwau-guard)
                           (advice-remove 'neovm--combo-prop-bwau-a 'neovm--combo-prop-bwau-fallback)
                           (list
                             (neovm--combo-prop-bwau-call-a {n})
                             (eval '(neovm--combo-prop-bwau-call-a {n_plus_one}))
                             (funcall 'neovm--combo-prop-bwau-a {n_minus_one})
                             (apply 'neovm--combo-prop-bwau-a (list {n_plus_two}))
                             (neovm--combo-prop-bwau-call-t {n})
                             (eval '(neovm--combo-prop-bwau-call-t {n_plus_one}))
                             (funcall 'neovm--combo-prop-bwau-target {n_minus_one})
                             (apply 'neovm--combo-prop-bwau-target (list {n_plus_two}))
                             (funcall fa0 {n_plus_one})
                             (funcall ft0 {n_plus_one})
                             (if (advice-member-p 'neovm--combo-prop-bwau-guard 'neovm--combo-prop-bwau-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-bwau-fallback 'neovm--combo-prop-bwau-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-bwau-guard 'neovm--combo-prop-bwau-target) t nil)
                             (if (advice-member-p 'neovm--combo-prop-bwau-fallback 'neovm--combo-prop-bwau-target) t nil)))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-bwau-a 'neovm--combo-prop-bwau-guard)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-bwau-a 'neovm--combo-prop-bwau-fallback)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-bwau-target 'neovm--combo-prop-bwau-guard)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-bwau-target 'neovm--combo-prop-bwau-fallback)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-bwau-target)
                   (fmakunbound 'neovm--combo-prop-bwau-a)
                   (fmakunbound 'neovm--combo-prop-bwau-guard)
                   (fmakunbound 'neovm--combo-prop-bwau-fallback)
                   (fmakunbound 'neovm--combo-prop-bwau-call-a)
                   (fmakunbound 'neovm--combo-prop-bwau-call-t))))",
            add_order = add_order,
            n = n,
            n_minus_one = n - 1,
            n_plus_one = n + 1,
            n_plus_two = n + 2,
            mul = mul,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_before_until_after_while_alias_switch_consistency(
        n in -100i64..100i64,
        add_after_first in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let add_order = if add_after_first {
            "(progn
               (advice-add 'neovm--combo-prop-buaw-a :after-while 'neovm--combo-prop-buaw-after-while)
               (advice-add 'neovm--combo-prop-buaw-a :before-until 'neovm--combo-prop-buaw-before-until))"
        } else {
            "(progn
               (advice-add 'neovm--combo-prop-buaw-a :before-until 'neovm--combo-prop-buaw-before-until)
               (advice-add 'neovm--combo-prop-buaw-a :after-while 'neovm--combo-prop-buaw-after-while))"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-buaw-call-a (x)
                 `(neovm--combo-prop-buaw-a ,x))
               (defmacro neovm--combo-prop-buaw-call-t (x)
                 `(neovm--combo-prop-buaw-target-a ,x))
               (fset 'neovm--combo-prop-buaw-target-a
                     (lambda (x)
                       (if (> x 0) (+ x 10) nil)))
               (fset 'neovm--combo-prop-buaw-target-b
                     (lambda (x)
                       (if (< x 0) (- x 10) nil)))
               (defalias 'neovm--combo-prop-buaw-a 'neovm--combo-prop-buaw-target-a)
               (fset 'neovm--combo-prop-buaw-before-until
                     (lambda (&rest args)
                       (let ((x (car args)))
                         (if (< x 0) (list 'short x) nil))))
               (fset 'neovm--combo-prop-buaw-after-while
                     (lambda (&rest args)
                       (let ((x (car args)))
                         (if (< x 3) (list 'post x) nil))))
               (let ((fa0 nil) (ft0 nil))
                 (unwind-protect
                     (progn
                       {add_order}
                       (setq fa0 (symbol-function 'neovm--combo-prop-buaw-a))
                       (setq ft0 (symbol-function 'neovm--combo-prop-buaw-target-a))
                       (list
                         (list
                           (neovm--combo-prop-buaw-call-a {n})
                           (eval '(neovm--combo-prop-buaw-call-a {n_plus_one}))
                           (funcall 'neovm--combo-prop-buaw-a {n_minus_one})
                           (apply 'neovm--combo-prop-buaw-a (list {n_plus_two}))
                           (neovm--combo-prop-buaw-call-t {n})
                           (eval '(neovm--combo-prop-buaw-call-t {n_plus_one}))
                           (funcall 'neovm--combo-prop-buaw-target-a {n_minus_one})
                           (apply 'neovm--combo-prop-buaw-target-a (list {n_plus_two}))
                           (funcall fa0 {n_plus_one})
                           (funcall ft0 {n_plus_one})
                           (if (advice-member-p 'neovm--combo-prop-buaw-before-until 'neovm--combo-prop-buaw-a) t nil)
                           (if (advice-member-p 'neovm--combo-prop-buaw-after-while 'neovm--combo-prop-buaw-a) t nil))
                         (progn
                           (defalias 'neovm--combo-prop-buaw-a 'neovm--combo-prop-buaw-target-b)
                           (list
                             (neovm--combo-prop-buaw-call-a {n})
                             (eval '(neovm--combo-prop-buaw-call-a {n_plus_one}))
                             (funcall 'neovm--combo-prop-buaw-a {n_minus_one})
                             (apply 'neovm--combo-prop-buaw-a (list {n_plus_two}))
                             (neovm--combo-prop-buaw-call-t {n})
                             (eval '(neovm--combo-prop-buaw-call-t {n_plus_one}))
                             (funcall 'neovm--combo-prop-buaw-target-a {n_minus_one})
                             (apply 'neovm--combo-prop-buaw-target-a (list {n_plus_two}))
                             (funcall fa0 {n_plus_one})
                             (funcall ft0 {n_plus_one})
                             (if (advice-member-p 'neovm--combo-prop-buaw-before-until 'neovm--combo-prop-buaw-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-buaw-after-while 'neovm--combo-prop-buaw-a) t nil)))
                         (progn
                           (advice-remove 'neovm--combo-prop-buaw-a 'neovm--combo-prop-buaw-before-until)
                           (advice-remove 'neovm--combo-prop-buaw-a 'neovm--combo-prop-buaw-after-while)
                           (list
                             (neovm--combo-prop-buaw-call-a {n})
                             (eval '(neovm--combo-prop-buaw-call-a {n_plus_one}))
                             (funcall 'neovm--combo-prop-buaw-a {n_minus_one})
                             (apply 'neovm--combo-prop-buaw-a (list {n_plus_two}))
                             (neovm--combo-prop-buaw-call-t {n})
                             (eval '(neovm--combo-prop-buaw-call-t {n_plus_one}))
                             (funcall 'neovm--combo-prop-buaw-target-a {n_minus_one})
                             (apply 'neovm--combo-prop-buaw-target-a (list {n_plus_two}))
                             (funcall fa0 {n_plus_one})
                             (funcall ft0 {n_plus_one})
                             (if (advice-member-p 'neovm--combo-prop-buaw-before-until 'neovm--combo-prop-buaw-a) t nil)
                             (if (advice-member-p 'neovm--combo-prop-buaw-after-while 'neovm--combo-prop-buaw-a) t nil)))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-buaw-a 'neovm--combo-prop-buaw-before-until)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-buaw-a 'neovm--combo-prop-buaw-after-while)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-buaw-target-a 'neovm--combo-prop-buaw-before-until)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-buaw-target-a 'neovm--combo-prop-buaw-after-while)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-buaw-target-b 'neovm--combo-prop-buaw-before-until)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-buaw-target-b 'neovm--combo-prop-buaw-after-while)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-buaw-target-a)
                   (fmakunbound 'neovm--combo-prop-buaw-target-b)
                   (fmakunbound 'neovm--combo-prop-buaw-a)
                   (fmakunbound 'neovm--combo-prop-buaw-before-until)
                   (fmakunbound 'neovm--combo-prop-buaw-after-while)
                   (fmakunbound 'neovm--combo-prop-buaw-call-a)
                   (fmakunbound 'neovm--combo-prop-buaw-call-t))))",
            add_order = add_order,
            n = n,
            n_minus_one = n - 1,
            n_plus_one = n + 1,
            n_plus_two = n + 2,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_add_function_rebind_lifecycle_consistency(
        n in -1_000i64..1_000i64,
        mul in -20i64..20i64,
        add_filter_first in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let add_order = if add_filter_first {
            "(progn
               (add-function :filter-return (symbol-function 'neovm--combo-prop-addf-target) 'neovm--combo-prop-addf-filter-ret)
               (add-function :around (symbol-function 'neovm--combo-prop-addf-target) 'neovm--combo-prop-addf-around))"
        } else {
            "(progn
               (add-function :around (symbol-function 'neovm--combo-prop-addf-target) 'neovm--combo-prop-addf-around)
               (add-function :filter-return (symbol-function 'neovm--combo-prop-addf-target) 'neovm--combo-prop-addf-filter-ret))"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-addf-call (x)
                 `(neovm--combo-prop-addf-target ,x))
               (fset 'neovm--combo-prop-addf-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-addf-alias 'neovm--combo-prop-addf-target)
               (fset 'neovm--combo-prop-addf-around
                     (lambda (orig x)
                       (+ 10 (funcall orig x))))
               (fset 'neovm--combo-prop-addf-filter-ret
                     (lambda (ret)
                       (* ret 3)))
               (let ((f0 nil))
                 (unwind-protect
                     (progn
                       {add_order}
                       (setq f0 (symbol-function 'neovm--combo-prop-addf-target))
                       (list
                         (list
                           (neovm--combo-prop-addf-call {n})
                           (eval '(neovm--combo-prop-addf-call {n}))
                           (funcall 'neovm--combo-prop-addf-target {n})
                           (apply 'neovm--combo-prop-addf-target (list {n}))
                           (funcall 'neovm--combo-prop-addf-alias {n})
                           (funcall f0 {n}))
                         (progn
                           (fset 'neovm--combo-prop-addf-target (lambda (x) (* x {mul})))
                           (list
                             (neovm--combo-prop-addf-call {n})
                             (eval '(neovm--combo-prop-addf-call {n}))
                             (funcall 'neovm--combo-prop-addf-target {n})
                             (apply 'neovm--combo-prop-addf-target (list {n}))
                             (funcall 'neovm--combo-prop-addf-alias {n})
                             (funcall f0 {n})))
                         (progn
                           (condition-case nil
                               (remove-function (symbol-function 'neovm--combo-prop-addf-target) 'neovm--combo-prop-addf-around)
                             (error nil))
                           (condition-case nil
                               (remove-function (symbol-function 'neovm--combo-prop-addf-target) 'neovm--combo-prop-addf-filter-ret)
                             (error nil))
                           (list
                             (neovm--combo-prop-addf-call {n})
                             (eval '(neovm--combo-prop-addf-call {n}))
                             (funcall 'neovm--combo-prop-addf-target {n})
                             (apply 'neovm--combo-prop-addf-target (list {n}))
                             (funcall 'neovm--combo-prop-addf-alias {n})
                             (funcall f0 {n})))))
                   (condition-case nil
                       (remove-function (symbol-function 'neovm--combo-prop-addf-target) 'neovm--combo-prop-addf-around)
                     (error nil))
                   (condition-case nil
                       (remove-function (symbol-function 'neovm--combo-prop-addf-target) 'neovm--combo-prop-addf-filter-ret)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-addf-target)
                   (fmakunbound 'neovm--combo-prop-addf-alias)
                   (fmakunbound 'neovm--combo-prop-addf-around)
                   (fmakunbound 'neovm--combo-prop-addf-filter-ret)
                   (fmakunbound 'neovm--combo-prop-addf-call))))",
            add_order = add_order,
            n = n,
            mul = mul,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_same_name_around_replacement_lifecycle_consistency(
        n in -1_000i64..1_000i64,
        remove_on_alias in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let remove_sym = if remove_on_alias {
            "neovm--combo-prop-name-repl-alias"
        } else {
            "neovm--combo-prop-name-repl-target"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-name-repl-call (x)
                 `(neovm--combo-prop-name-repl-target ,x))
               (fset 'neovm--combo-prop-name-repl-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-name-repl-alias 'neovm--combo-prop-name-repl-target)
               (fset 'neovm--combo-prop-name-repl-a1
                     (lambda (orig x)
                       (+ 1 (funcall orig x))))
               (fset 'neovm--combo-prop-name-repl-a2
                     (lambda (orig x)
                       (+ 10 (funcall orig x))))
               (unwind-protect
                   (list
                     (progn
                       (advice-add 'neovm--combo-prop-name-repl-target :around 'neovm--combo-prop-name-repl-a1 '((name . neovm--combo-prop-name-repl-shared) (depth . -10)))
                       (list
                         (neovm--combo-prop-name-repl-call {n})
                         (eval '(neovm--combo-prop-name-repl-call {n}))
                         (funcall 'neovm--combo-prop-name-repl-target {n})
                         (funcall 'neovm--combo-prop-name-repl-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-repl-a1 'neovm--combo-prop-name-repl-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-repl-a2 'neovm--combo-prop-name-repl-target) t nil)))
                     (progn
                       (advice-add 'neovm--combo-prop-name-repl-target :around 'neovm--combo-prop-name-repl-a2 '((name . neovm--combo-prop-name-repl-shared) (depth . 10)))
                       (list
                         (neovm--combo-prop-name-repl-call {n})
                         (eval '(neovm--combo-prop-name-repl-call {n}))
                         (funcall 'neovm--combo-prop-name-repl-target {n})
                         (funcall 'neovm--combo-prop-name-repl-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-repl-a1 'neovm--combo-prop-name-repl-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-repl-a2 'neovm--combo-prop-name-repl-target) t nil)))
                     (progn
                       (advice-remove '{remove_sym} 'neovm--combo-prop-name-repl-a1)
                       (list
                         (neovm--combo-prop-name-repl-call {n})
                         (eval '(neovm--combo-prop-name-repl-call {n}))
                         (funcall 'neovm--combo-prop-name-repl-target {n})
                         (funcall 'neovm--combo-prop-name-repl-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-repl-a1 'neovm--combo-prop-name-repl-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-repl-a2 'neovm--combo-prop-name-repl-target) t nil)))
                     (progn
                       (advice-remove '{remove_sym} 'neovm--combo-prop-name-repl-a2)
                       (list
                         (neovm--combo-prop-name-repl-call {n})
                         (eval '(neovm--combo-prop-name-repl-call {n}))
                         (funcall 'neovm--combo-prop-name-repl-target {n})
                         (funcall 'neovm--combo-prop-name-repl-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-repl-a1 'neovm--combo-prop-name-repl-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-repl-a2 'neovm--combo-prop-name-repl-target) t nil))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-repl-target 'neovm--combo-prop-name-repl-a1)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-repl-target 'neovm--combo-prop-name-repl-a2)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-repl-alias 'neovm--combo-prop-name-repl-a1)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-repl-alias 'neovm--combo-prop-name-repl-a2)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-name-repl-target)
                 (fmakunbound 'neovm--combo-prop-name-repl-alias)
                 (fmakunbound 'neovm--combo-prop-name-repl-a1)
                 (fmakunbound 'neovm--combo-prop-name-repl-a2)
                 (fmakunbound 'neovm--combo-prop-name-repl-call)))",
            n = n,
            remove_sym = remove_sym,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_same_name_filter_return_replacement_lifecycle_consistency(
        n in -1_000i64..1_000i64,
        remove_on_alias in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let remove_sym = if remove_on_alias {
            "neovm--combo-prop-name-fr-alias"
        } else {
            "neovm--combo-prop-name-fr-target"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-name-fr-call (x)
                 `(neovm--combo-prop-name-fr-target ,x))
               (fset 'neovm--combo-prop-name-fr-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-name-fr-alias 'neovm--combo-prop-name-fr-target)
               (fset 'neovm--combo-prop-name-fr-f1
                     (lambda (ret)
                       (+ ret 1)))
               (fset 'neovm--combo-prop-name-fr-f2
                     (lambda (ret)
                       (* ret 10)))
               (unwind-protect
                   (list
                     (progn
                       (advice-add 'neovm--combo-prop-name-fr-target :filter-return 'neovm--combo-prop-name-fr-f1 '((name . neovm--combo-prop-name-fr-shared) (depth . -10)))
                       (list
                         (neovm--combo-prop-name-fr-call {n})
                         (eval '(neovm--combo-prop-name-fr-call {n}))
                         (funcall 'neovm--combo-prop-name-fr-target {n})
                         (funcall 'neovm--combo-prop-name-fr-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-fr-f1 'neovm--combo-prop-name-fr-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-fr-f2 'neovm--combo-prop-name-fr-target) t nil)))
                     (progn
                       (advice-add 'neovm--combo-prop-name-fr-target :filter-return 'neovm--combo-prop-name-fr-f2 '((name . neovm--combo-prop-name-fr-shared) (depth . 10)))
                       (list
                         (neovm--combo-prop-name-fr-call {n})
                         (eval '(neovm--combo-prop-name-fr-call {n}))
                         (funcall 'neovm--combo-prop-name-fr-target {n})
                         (funcall 'neovm--combo-prop-name-fr-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-fr-f1 'neovm--combo-prop-name-fr-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-fr-f2 'neovm--combo-prop-name-fr-target) t nil)))
                     (progn
                       (advice-remove '{remove_sym} 'neovm--combo-prop-name-fr-f1)
                       (list
                         (neovm--combo-prop-name-fr-call {n})
                         (eval '(neovm--combo-prop-name-fr-call {n}))
                         (funcall 'neovm--combo-prop-name-fr-target {n})
                         (funcall 'neovm--combo-prop-name-fr-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-fr-f1 'neovm--combo-prop-name-fr-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-fr-f2 'neovm--combo-prop-name-fr-target) t nil)))
                     (progn
                       (advice-remove '{remove_sym} 'neovm--combo-prop-name-fr-f2)
                       (list
                         (neovm--combo-prop-name-fr-call {n})
                         (eval '(neovm--combo-prop-name-fr-call {n}))
                         (funcall 'neovm--combo-prop-name-fr-target {n})
                         (funcall 'neovm--combo-prop-name-fr-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-fr-f1 'neovm--combo-prop-name-fr-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-fr-f2 'neovm--combo-prop-name-fr-target) t nil))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-fr-target 'neovm--combo-prop-name-fr-f1)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-fr-target 'neovm--combo-prop-name-fr-f2)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-fr-alias 'neovm--combo-prop-name-fr-f1)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-fr-alias 'neovm--combo-prop-name-fr-f2)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-name-fr-target)
                 (fmakunbound 'neovm--combo-prop-name-fr-alias)
                 (fmakunbound 'neovm--combo-prop-name-fr-f1)
                 (fmakunbound 'neovm--combo-prop-name-fr-f2)
                 (fmakunbound 'neovm--combo-prop-name-fr-call)))",
            n = n,
            remove_sym = remove_sym,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_distinct_equal_lambda_remove_semantics_consistency(
        n in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-lid-call (x)
                 `(neovm--combo-prop-lid-target ,x))
               (fset 'neovm--combo-prop-lid-target (lambda (x) (+ x 1)))
               (let* ((adv1 (lambda (orig x) (+ 10 (funcall orig x))))
                      (adv2 (lambda (orig x) (+ 10 (funcall orig x)))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-lid-target :around adv1)
                         (list
                           (neovm--combo-prop-lid-call {n})
                           (eval '(neovm--combo-prop-lid-call {n}))
                           (funcall 'neovm--combo-prop-lid-target {n})
                           (apply 'neovm--combo-prop-lid-target (list {n}))
                           (eq adv1 adv2)
                           (if (advice-member-p adv1 'neovm--combo-prop-lid-target) t nil)
                           (if (advice-member-p adv2 'neovm--combo-prop-lid-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-lid-target adv2)
                         (list
                           (neovm--combo-prop-lid-call {n})
                           (eval '(neovm--combo-prop-lid-call {n}))
                           (funcall 'neovm--combo-prop-lid-target {n})
                           (apply 'neovm--combo-prop-lid-target (list {n}))
                           (eq adv1 adv2)
                           (if (advice-member-p adv1 'neovm--combo-prop-lid-target) t nil)
                           (if (advice-member-p adv2 'neovm--combo-prop-lid-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-lid-target adv1)
                         (list
                           (neovm--combo-prop-lid-call {n})
                           (eval '(neovm--combo-prop-lid-call {n}))
                           (funcall 'neovm--combo-prop-lid-target {n})
                           (apply 'neovm--combo-prop-lid-target (list {n}))
                           (eq adv1 adv2)
                           (if (advice-member-p adv1 'neovm--combo-prop-lid-target) t nil)
                           (if (advice-member-p adv2 'neovm--combo-prop-lid-target) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-lid-target adv1)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-lid-target adv2)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-lid-target)
                   (fmakunbound 'neovm--combo-prop-lid-call))))",
            n = n,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_lambda_before_and_filter_return_lifecycle_consistency(
        n in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-lbf-call (x)
                 `(neovm--combo-prop-lbf-target ,x))
               (fset 'neovm--combo-prop-lbf-target (lambda (x) (+ x 1)))
               (let* ((log nil)
                      (before1 (lambda (&rest args) (setq log (cons (cons 'b1 args) log))))
                      (before2 (lambda (&rest args) (setq log (cons (cons 'b2 args) log))))
                      (fret1 (lambda (ret) (+ ret 10)))
                      (fret2 (lambda (ret) (+ ret 100))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-lbf-target :before before1)
                         (advice-add 'neovm--combo-prop-lbf-target :filter-return fret1)
                         (setq log nil)
                         (list
                           (neovm--combo-prop-lbf-call {n})
                           (eval '(neovm--combo-prop-lbf-call {n}))
                           (funcall 'neovm--combo-prop-lbf-target {n})
                           (apply 'neovm--combo-prop-lbf-target (list {n}))
                           (nreverse log)
                           (if (advice-member-p before1 'neovm--combo-prop-lbf-target) t nil)
                           (if (advice-member-p fret1 'neovm--combo-prop-lbf-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-lbf-target before2)
                         (advice-remove 'neovm--combo-prop-lbf-target fret2)
                         (setq log nil)
                         (list
                           (neovm--combo-prop-lbf-call {n})
                           (eval '(neovm--combo-prop-lbf-call {n}))
                           (funcall 'neovm--combo-prop-lbf-target {n})
                           (apply 'neovm--combo-prop-lbf-target (list {n}))
                           (nreverse log)
                           (if (advice-member-p before1 'neovm--combo-prop-lbf-target) t nil)
                           (if (advice-member-p fret1 'neovm--combo-prop-lbf-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-lbf-target before1)
                         (advice-remove 'neovm--combo-prop-lbf-target fret1)
                         (setq log nil)
                         (list
                           (neovm--combo-prop-lbf-call {n})
                           (eval '(neovm--combo-prop-lbf-call {n}))
                           (funcall 'neovm--combo-prop-lbf-target {n})
                           (apply 'neovm--combo-prop-lbf-target (list {n}))
                           (nreverse log)
                           (if (advice-member-p before1 'neovm--combo-prop-lbf-target) t nil)
                           (if (advice-member-p fret1 'neovm--combo-prop-lbf-target) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-lbf-target before1)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-lbf-target before2)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-lbf-target fret1)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-lbf-target fret2)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-lbf-target)
                   (fmakunbound 'neovm--combo-prop-lbf-call))))",
            n = n,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_same_name_override_replacement_lifecycle_consistency(
        n in -1_000i64..1_000i64,
        remove_on_alias in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let remove_sym = if remove_on_alias {
            "neovm--combo-prop-name-ov-alias"
        } else {
            "neovm--combo-prop-name-ov-target"
        };

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-name-ov-call (x)
                 `(neovm--combo-prop-name-ov-target ,x))
               (fset 'neovm--combo-prop-name-ov-target (lambda (x) (+ x 1)))
               (defalias 'neovm--combo-prop-name-ov-alias 'neovm--combo-prop-name-ov-target)
               (fset 'neovm--combo-prop-name-ov-o1 (lambda (x) (+ x 10)))
               (fset 'neovm--combo-prop-name-ov-o2 (lambda (x) (+ x 100)))
               (unwind-protect
                   (list
                     (progn
                       (advice-add 'neovm--combo-prop-name-ov-target :override 'neovm--combo-prop-name-ov-o1 '((name . neovm--combo-prop-name-ov-shared) (depth . -10)))
                       (list
                         (neovm--combo-prop-name-ov-call {n})
                         (eval '(neovm--combo-prop-name-ov-call {n}))
                         (funcall 'neovm--combo-prop-name-ov-target {n})
                         (funcall 'neovm--combo-prop-name-ov-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-ov-o1 'neovm--combo-prop-name-ov-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-ov-o2 'neovm--combo-prop-name-ov-target) t nil)))
                     (progn
                       (advice-add 'neovm--combo-prop-name-ov-target :override 'neovm--combo-prop-name-ov-o2 '((name . neovm--combo-prop-name-ov-shared) (depth . 10)))
                       (list
                         (neovm--combo-prop-name-ov-call {n})
                         (eval '(neovm--combo-prop-name-ov-call {n}))
                         (funcall 'neovm--combo-prop-name-ov-target {n})
                         (funcall 'neovm--combo-prop-name-ov-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-ov-o1 'neovm--combo-prop-name-ov-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-ov-o2 'neovm--combo-prop-name-ov-target) t nil)))
                     (progn
                       (advice-remove '{remove_sym} 'neovm--combo-prop-name-ov-o1)
                       (list
                         (neovm--combo-prop-name-ov-call {n})
                         (eval '(neovm--combo-prop-name-ov-call {n}))
                         (funcall 'neovm--combo-prop-name-ov-target {n})
                         (funcall 'neovm--combo-prop-name-ov-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-ov-o1 'neovm--combo-prop-name-ov-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-ov-o2 'neovm--combo-prop-name-ov-target) t nil)))
                     (progn
                       (advice-remove '{remove_sym} 'neovm--combo-prop-name-ov-o2)
                       (list
                         (neovm--combo-prop-name-ov-call {n})
                         (eval '(neovm--combo-prop-name-ov-call {n}))
                         (funcall 'neovm--combo-prop-name-ov-target {n})
                         (funcall 'neovm--combo-prop-name-ov-alias {n})
                         (if (advice-member-p 'neovm--combo-prop-name-ov-o1 'neovm--combo-prop-name-ov-target) t nil)
                         (if (advice-member-p 'neovm--combo-prop-name-ov-o2 'neovm--combo-prop-name-ov-target) t nil))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-ov-target 'neovm--combo-prop-name-ov-o1)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-ov-target 'neovm--combo-prop-name-ov-o2)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-ov-alias 'neovm--combo-prop-name-ov-o1)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-name-ov-alias 'neovm--combo-prop-name-ov-o2)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-name-ov-target)
                 (fmakunbound 'neovm--combo-prop-name-ov-alias)
                 (fmakunbound 'neovm--combo-prop-name-ov-o1)
                 (fmakunbound 'neovm--combo-prop-name-ov-o2)
                 (fmakunbound 'neovm--combo-prop-name-ov-call)))",
            n = n,
            remove_sym = remove_sym,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_lambda_override_lifecycle_consistency(
        n in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-lov-call (x)
                 `(neovm--combo-prop-lov-target ,x))
               (fset 'neovm--combo-prop-lov-target (lambda (x) (+ x 1)))
               (let* ((ov1 (lambda (x) (+ x 10)))
                      (ov2 (lambda (x) (+ x 100))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-lov-target :override ov1)
                         (list
                           (neovm--combo-prop-lov-call {n})
                           (eval '(neovm--combo-prop-lov-call {n}))
                           (funcall 'neovm--combo-prop-lov-target {n})
                           (apply 'neovm--combo-prop-lov-target (list {n}))
                           (if (advice-member-p ov1 'neovm--combo-prop-lov-target) t nil)
                           (if (advice-member-p ov2 'neovm--combo-prop-lov-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-lov-target ov2)
                         (list
                           (neovm--combo-prop-lov-call {n})
                           (eval '(neovm--combo-prop-lov-call {n}))
                           (funcall 'neovm--combo-prop-lov-target {n})
                           (apply 'neovm--combo-prop-lov-target (list {n}))
                           (if (advice-member-p ov1 'neovm--combo-prop-lov-target) t nil)
                           (if (advice-member-p ov2 'neovm--combo-prop-lov-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-lov-target ov1)
                         (list
                           (neovm--combo-prop-lov-call {n})
                           (eval '(neovm--combo-prop-lov-call {n}))
                           (funcall 'neovm--combo-prop-lov-target {n})
                           (apply 'neovm--combo-prop-lov-target (list {n}))
                           (if (advice-member-p ov1 'neovm--combo-prop-lov-target) t nil)
                           (if (advice-member-p ov2 'neovm--combo-prop-lov-target) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-lov-target ov1)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-lov-target ov2)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-lov-target)
                   (fmakunbound 'neovm--combo-prop-lov-call))))",
            n = n,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_same_name_cross_location_replacement_consistency(
        n in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((log nil))
                 (fset 'neovm--combo-prop-name-cross-target
                       (lambda (x)
                         (setq log (cons (list 'orig x) log))
                         x))
                 (fset 'neovm--combo-prop-name-cross-before
                       (lambda (&rest args)
                         (setq log (cons (cons 'before args) log))))
                 (fset 'neovm--combo-prop-name-cross-after
                       (lambda (&rest args)
                         (setq log (cons (cons 'after args) log))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-name-cross-target :before 'neovm--combo-prop-name-cross-before '((name . neovm--combo-prop-name-cross-shared)))
                         (setq log nil)
                         (list
                           (funcall 'neovm--combo-prop-name-cross-target {n})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-name-cross-before 'neovm--combo-prop-name-cross-target) t nil)
                           (if (advice-member-p 'neovm--combo-prop-name-cross-after 'neovm--combo-prop-name-cross-target) t nil)))
                       (progn
                         (advice-add 'neovm--combo-prop-name-cross-target :after 'neovm--combo-prop-name-cross-after '((name . neovm--combo-prop-name-cross-shared)))
                         (setq log nil)
                         (list
                           (funcall 'neovm--combo-prop-name-cross-target {n})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-name-cross-before 'neovm--combo-prop-name-cross-target) t nil)
                           (if (advice-member-p 'neovm--combo-prop-name-cross-after 'neovm--combo-prop-name-cross-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-name-cross-target 'neovm--combo-prop-name-cross-before)
                         (setq log nil)
                         (list
                           (funcall 'neovm--combo-prop-name-cross-target {n})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-name-cross-before 'neovm--combo-prop-name-cross-target) t nil)
                           (if (advice-member-p 'neovm--combo-prop-name-cross-after 'neovm--combo-prop-name-cross-target) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-name-cross-target 'neovm--combo-prop-name-cross-after)
                         (setq log nil)
                         (list
                           (funcall 'neovm--combo-prop-name-cross-target {n})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-name-cross-before 'neovm--combo-prop-name-cross-target) t nil)
                           (if (advice-member-p 'neovm--combo-prop-name-cross-after 'neovm--combo-prop-name-cross-target) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-name-cross-target 'neovm--combo-prop-name-cross-before)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-name-cross-target 'neovm--combo-prop-name-cross-after)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-name-cross-target)
                   (fmakunbound 'neovm--combo-prop-name-cross-before)
                   (fmakunbound 'neovm--combo-prop-name-cross-after))))",
            n = n,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_before_advice_lifecycle_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((log nil))
                 (fset 'neovm--combo-prop-plus-before
                       (lambda (&rest args)
                         (setq log (cons args log))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add '+ :before 'neovm--combo-prop-plus-before)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-before '+) t nil)))
                       (progn
                         (advice-remove '+ 'neovm--combo-prop-plus-before)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-before '+) t nil))))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-before)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-plus-before))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_before_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((log nil))
                 (fset 'neovm--combo-prop-plus-name-b1
                       (lambda (&rest args)
                         (setq log (cons (cons 'b1 args) log))))
                 (fset 'neovm--combo-prop-plus-name-b2
                       (lambda (&rest args)
                         (setq log (cons (cons 'b2 args) log))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add '+ :before 'neovm--combo-prop-plus-name-b1 '((name . neovm--combo-prop-plus-name-shared)))
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-b1 '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-b2 '+) t nil)))
                       (progn
                         (advice-add '+ :before 'neovm--combo-prop-plus-name-b2 '((name . neovm--combo-prop-plus-name-shared)))
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-b1 '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-b2 '+) t nil)))
                       (progn
                         (advice-remove '+ 'neovm--combo-prop-plus-name-b1)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-b1 '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-b2 '+) t nil)))
                       (progn
                         (advice-remove '+ 'neovm--combo-prop-plus-name-b2)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-b1 '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-b2 '+) t nil))))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-name-b1)
                     (error nil))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-name-b2)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-plus-name-b1)
                   (fmakunbound 'neovm--combo-prop-plus-name-b2))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_after_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((log nil))
                 (fset 'neovm--combo-prop-plus-name-a1
                       (lambda (&rest args)
                         (setq log (cons (cons 'a1 args) log))))
                 (fset 'neovm--combo-prop-plus-name-a2
                       (lambda (&rest args)
                         (setq log (cons (cons 'a2 args) log))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add '+ :after 'neovm--combo-prop-plus-name-a1 '((name . neovm--combo-prop-plus-name-after-shared)))
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-a1 '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-a2 '+) t nil)))
                       (progn
                         (advice-add '+ :after 'neovm--combo-prop-plus-name-a2 '((name . neovm--combo-prop-plus-name-after-shared)))
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-a1 '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-a2 '+) t nil)))
                       (progn
                         (advice-remove '+ 'neovm--combo-prop-plus-name-a1)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-a1 '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-a2 '+) t nil)))
                       (progn
                         (advice-remove '+ 'neovm--combo-prop-plus-name-a2)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-a1 '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-a2 '+) t nil))))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-name-a1)
                     (error nil))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-name-a2)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-plus-name-a1)
                   (fmakunbound 'neovm--combo-prop-plus-name-a2))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_around_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-plus-name-ar1 (lambda (orig x y) x))
               (fset 'neovm--combo-prop-plus-name-ar2 (lambda (orig x y) y))
               (unwind-protect
                   (list
                     (progn
                       (advice-add '+ :around 'neovm--combo-prop-plus-name-ar1 '((name . neovm--combo-prop-plus-name-around-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ar1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ar2 '+) t nil)))
                     (progn
                       (advice-add '+ :around 'neovm--combo-prop-plus-name-ar2 '((name . neovm--combo-prop-plus-name-around-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ar1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ar2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-ar1)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ar1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ar2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-ar2)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ar1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ar2 '+) t nil))))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-ar1)
                   (error nil))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-ar2)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-plus-name-ar1)
                 (fmakunbound 'neovm--combo-prop-plus-name-ar2)))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_around_depth_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-plus-name-ard1 (lambda (orig x y) x))
               (fset 'neovm--combo-prop-plus-name-ard2 (lambda (orig x y) y))
               (unwind-protect
                   (list
                     (progn
                       (advice-add '+ :around 'neovm--combo-prop-plus-name-ard1 '((name . neovm--combo-prop-plus-name-ard-shared) (depth . -50)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ard1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ard2 '+) t nil)))
                     (progn
                       (advice-add '+ :around 'neovm--combo-prop-plus-name-ard2 '((name . neovm--combo-prop-plus-name-ard-shared) (depth . 50)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ard1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ard2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-ard1)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ard1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ard2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-ard2)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ard1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ard2 '+) t nil))))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-ard1)
                   (error nil))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-ard2)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-plus-name-ard1)
                 (fmakunbound 'neovm--combo-prop-plus-name-ard2)))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_filter_return_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-plus-name-fr1 (lambda (ret) ret))
               (fset 'neovm--combo-prop-plus-name-fr2 (lambda (ret) (- ret)))
               (unwind-protect
                   (list
                     (progn
                       (advice-add '+ :filter-return 'neovm--combo-prop-plus-name-fr1 '((name . neovm--combo-prop-plus-name-fr-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fr1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fr2 '+) t nil)))
                     (progn
                       (advice-add '+ :filter-return 'neovm--combo-prop-plus-name-fr2 '((name . neovm--combo-prop-plus-name-fr-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fr1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fr2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-fr1)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fr1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fr2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-fr2)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fr1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fr2 '+) t nil))))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-fr1)
                   (error nil))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-fr2)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-plus-name-fr1)
                 (fmakunbound 'neovm--combo-prop-plus-name-fr2)))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_filter_args_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-plus-name-fa1 (lambda (args) args))
               (fset 'neovm--combo-prop-plus-name-fa2 (lambda (args) (list 0 0)))
               (unwind-protect
                   (list
                     (progn
                       (advice-add '+ :filter-args 'neovm--combo-prop-plus-name-fa1 '((name . neovm--combo-prop-plus-name-fa-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fa1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fa2 '+) t nil)))
                     (progn
                       (advice-add '+ :filter-args 'neovm--combo-prop-plus-name-fa2 '((name . neovm--combo-prop-plus-name-fa-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fa1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fa2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-fa1)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fa1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fa2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-fa2)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fa1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-fa2 '+) t nil))))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-fa1)
                   (error nil))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-fa2)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-plus-name-fa1)
                 (fmakunbound 'neovm--combo-prop-plus-name-fa2)))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_around_to_filter_return_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-plus-name-arfr-around (lambda (orig x y) x))
               (fset 'neovm--combo-prop-plus-name-arfr-fr (lambda (ret) (- ret)))
               (unwind-protect
                   (list
                     (progn
                       (advice-add '+ :around 'neovm--combo-prop-plus-name-arfr-around '((name . neovm--combo-prop-plus-name-arfr-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-arfr-around '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-arfr-fr '+) t nil)))
                     (progn
                       (advice-add '+ :filter-return 'neovm--combo-prop-plus-name-arfr-fr '((name . neovm--combo-prop-plus-name-arfr-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-arfr-around '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-arfr-fr '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-arfr-around)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-arfr-around '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-arfr-fr '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-arfr-fr)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-arfr-around '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-arfr-fr '+) t nil))))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-arfr-around)
                   (error nil))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-arfr-fr)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-plus-name-arfr-around)
                 (fmakunbound 'neovm--combo-prop-plus-name-arfr-fr)))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_override_to_after_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((log nil))
                 (fset 'neovm--combo-prop-plus-name-ovaf-ov (lambda (x y) x))
                 (fset 'neovm--combo-prop-plus-name-ovaf-af
                       (lambda (&rest args)
                         (setq log (cons args log))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add '+ :override 'neovm--combo-prop-plus-name-ovaf-ov '((name . neovm--combo-prop-plus-name-ovaf-shared)))
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-ovaf-ov '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-ovaf-af '+) t nil)))
                       (progn
                         (advice-add '+ :after 'neovm--combo-prop-plus-name-ovaf-af '((name . neovm--combo-prop-plus-name-ovaf-shared)))
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-ovaf-ov '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-ovaf-af '+) t nil)))
                       (progn
                         (advice-remove '+ 'neovm--combo-prop-plus-name-ovaf-ov)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-ovaf-ov '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-ovaf-af '+) t nil)))
                       (progn
                         (advice-remove '+ 'neovm--combo-prop-plus-name-ovaf-af)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-ovaf-ov '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-ovaf-af '+) t nil))))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-name-ovaf-ov)
                     (error nil))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-name-ovaf-af)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-plus-name-ovaf-ov)
                   (fmakunbound 'neovm--combo-prop-plus-name-ovaf-af))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_before_to_after_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((log nil))
                 (fset 'neovm--combo-prop-plus-name-btaf-b
                       (lambda (&rest args)
                         (setq log (cons (cons 'b args) log))))
                 (fset 'neovm--combo-prop-plus-name-btaf-a
                       (lambda (&rest args)
                         (setq log (cons (cons 'a args) log))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add '+ :before 'neovm--combo-prop-plus-name-btaf-b '((name . neovm--combo-prop-plus-name-btaf-shared)))
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-btaf-b '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-btaf-a '+) t nil)))
                       (progn
                         (advice-add '+ :after 'neovm--combo-prop-plus-name-btaf-a '((name . neovm--combo-prop-plus-name-btaf-shared)))
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-btaf-b '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-btaf-a '+) t nil)))
                       (progn
                         (advice-remove '+ 'neovm--combo-prop-plus-name-btaf-b)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-btaf-b '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-btaf-a '+) t nil)))
                       (progn
                         (advice-remove '+ 'neovm--combo-prop-plus-name-btaf-a)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-btaf-b '+) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-name-btaf-a '+) t nil))))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-name-btaf-b)
                     (error nil))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-name-btaf-a)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-plus-name-btaf-b)
                   (fmakunbound 'neovm--combo-prop-plus-name-btaf-a))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_alias_same_name_override_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defalias 'neovm--combo-prop-plus-alias '+)
               (fset 'neovm--combo-prop-plus-alias-name-ov1 (lambda (x y) x))
               (fset 'neovm--combo-prop-plus-alias-name-ov2 (lambda (x y) y))
               (unwind-protect
                   (list
                     (progn
                       (advice-add 'neovm--combo-prop-plus-alias :override 'neovm--combo-prop-plus-alias-name-ov1 '((name . neovm--combo-prop-plus-alias-name-ov-shared)))
                       (list
                         (neovm--combo-prop-plus-alias {a} {b})
                         (funcall 'neovm--combo-prop-plus-alias {a} {b})
                         (apply 'neovm--combo-prop-plus-alias (list {a} {b}))
                         (+ {a} {b})
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov1 'neovm--combo-prop-plus-alias) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov2 'neovm--combo-prop-plus-alias) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov2 '+) t nil)))
                     (progn
                       (advice-add 'neovm--combo-prop-plus-alias :override 'neovm--combo-prop-plus-alias-name-ov2 '((name . neovm--combo-prop-plus-alias-name-ov-shared)))
                       (list
                         (neovm--combo-prop-plus-alias {a} {b})
                         (funcall 'neovm--combo-prop-plus-alias {a} {b})
                         (apply 'neovm--combo-prop-plus-alias (list {a} {b}))
                         (+ {a} {b})
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov1 'neovm--combo-prop-plus-alias) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov2 'neovm--combo-prop-plus-alias) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov2 '+) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-prop-plus-alias 'neovm--combo-prop-plus-alias-name-ov1)
                       (list
                         (neovm--combo-prop-plus-alias {a} {b})
                         (funcall 'neovm--combo-prop-plus-alias {a} {b})
                         (apply 'neovm--combo-prop-plus-alias (list {a} {b}))
                         (+ {a} {b})
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov1 'neovm--combo-prop-plus-alias) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov2 'neovm--combo-prop-plus-alias) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov2 '+) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-prop-plus-alias 'neovm--combo-prop-plus-alias-name-ov2)
                       (list
                         (neovm--combo-prop-plus-alias {a} {b})
                         (funcall 'neovm--combo-prop-plus-alias {a} {b})
                         (apply 'neovm--combo-prop-plus-alias (list {a} {b}))
                         (+ {a} {b})
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov1 'neovm--combo-prop-plus-alias) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov2 'neovm--combo-prop-plus-alias) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-alias-name-ov2 '+) t nil))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-plus-alias 'neovm--combo-prop-plus-alias-name-ov1)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-plus-alias 'neovm--combo-prop-plus-alias-name-ov2)
                   (error nil))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-alias-name-ov1)
                   (error nil))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-alias-name-ov2)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-plus-alias)
                 (fmakunbound 'neovm--combo-prop-plus-alias-name-ov1)
                 (fmakunbound 'neovm--combo-prop-plus-alias-name-ov2)))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_alias_same_name_override_to_after_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((log nil))
                 (defalias 'neovm--combo-prop-plus-alias-ovaf '+)
                 (fset 'neovm--combo-prop-plus-alias-ovaf-ov (lambda (x y) x))
                 (fset 'neovm--combo-prop-plus-alias-ovaf-af
                       (lambda (&rest args)
                         (setq log (cons args log))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-plus-alias-ovaf :override 'neovm--combo-prop-plus-alias-ovaf-ov '((name . neovm--combo-prop-plus-alias-ovaf-shared)))
                         (setq log nil)
                         (list
                           (neovm--combo-prop-plus-alias-ovaf {a} {b})
                           (funcall 'neovm--combo-prop-plus-alias-ovaf {a} {b})
                           (apply 'neovm--combo-prop-plus-alias-ovaf (list {a} {b}))
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-ovaf-ov 'neovm--combo-prop-plus-alias-ovaf) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-ovaf-af 'neovm--combo-prop-plus-alias-ovaf) t nil)))
                       (progn
                         (advice-add 'neovm--combo-prop-plus-alias-ovaf :after 'neovm--combo-prop-plus-alias-ovaf-af '((name . neovm--combo-prop-plus-alias-ovaf-shared)))
                         (setq log nil)
                         (list
                           (neovm--combo-prop-plus-alias-ovaf {a} {b})
                           (funcall 'neovm--combo-prop-plus-alias-ovaf {a} {b})
                           (apply 'neovm--combo-prop-plus-alias-ovaf (list {a} {b}))
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-ovaf-ov 'neovm--combo-prop-plus-alias-ovaf) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-ovaf-af 'neovm--combo-prop-plus-alias-ovaf) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-plus-alias-ovaf 'neovm--combo-prop-plus-alias-ovaf-ov)
                         (setq log nil)
                         (list
                           (neovm--combo-prop-plus-alias-ovaf {a} {b})
                           (funcall 'neovm--combo-prop-plus-alias-ovaf {a} {b})
                           (apply 'neovm--combo-prop-plus-alias-ovaf (list {a} {b}))
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-ovaf-ov 'neovm--combo-prop-plus-alias-ovaf) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-ovaf-af 'neovm--combo-prop-plus-alias-ovaf) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-plus-alias-ovaf 'neovm--combo-prop-plus-alias-ovaf-af)
                         (setq log nil)
                         (list
                           (neovm--combo-prop-plus-alias-ovaf {a} {b})
                           (funcall 'neovm--combo-prop-plus-alias-ovaf {a} {b})
                           (apply 'neovm--combo-prop-plus-alias-ovaf (list {a} {b}))
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-ovaf-ov 'neovm--combo-prop-plus-alias-ovaf) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-ovaf-af 'neovm--combo-prop-plus-alias-ovaf) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-plus-alias-ovaf 'neovm--combo-prop-plus-alias-ovaf-ov)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-plus-alias-ovaf 'neovm--combo-prop-plus-alias-ovaf-af)
                     (error nil))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-alias-ovaf-ov)
                     (error nil))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-alias-ovaf-af)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-plus-alias-ovaf)
                   (fmakunbound 'neovm--combo-prop-plus-alias-ovaf-ov)
                   (fmakunbound 'neovm--combo-prop-plus-alias-ovaf-af))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_alias_same_name_before_to_after_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((log nil))
                 (defalias 'neovm--combo-prop-plus-alias-btaf '+)
                 (fset 'neovm--combo-prop-plus-alias-btaf-b
                       (lambda (&rest args)
                         (setq log (cons (cons 'b args) log))))
                 (fset 'neovm--combo-prop-plus-alias-btaf-a
                       (lambda (&rest args)
                         (setq log (cons (cons 'a args) log))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-plus-alias-btaf :before 'neovm--combo-prop-plus-alias-btaf-b '((name . neovm--combo-prop-plus-alias-btaf-shared)))
                         (setq log nil)
                         (list
                           (neovm--combo-prop-plus-alias-btaf {a} {b})
                           (funcall 'neovm--combo-prop-plus-alias-btaf {a} {b})
                           (apply 'neovm--combo-prop-plus-alias-btaf (list {a} {b}))
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-btaf-b 'neovm--combo-prop-plus-alias-btaf) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-btaf-a 'neovm--combo-prop-plus-alias-btaf) t nil)))
                       (progn
                         (advice-add 'neovm--combo-prop-plus-alias-btaf :after 'neovm--combo-prop-plus-alias-btaf-a '((name . neovm--combo-prop-plus-alias-btaf-shared)))
                         (setq log nil)
                         (list
                           (neovm--combo-prop-plus-alias-btaf {a} {b})
                           (funcall 'neovm--combo-prop-plus-alias-btaf {a} {b})
                           (apply 'neovm--combo-prop-plus-alias-btaf (list {a} {b}))
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-btaf-b 'neovm--combo-prop-plus-alias-btaf) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-btaf-a 'neovm--combo-prop-plus-alias-btaf) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-plus-alias-btaf 'neovm--combo-prop-plus-alias-btaf-b)
                         (setq log nil)
                         (list
                           (neovm--combo-prop-plus-alias-btaf {a} {b})
                           (funcall 'neovm--combo-prop-plus-alias-btaf {a} {b})
                           (apply 'neovm--combo-prop-plus-alias-btaf (list {a} {b}))
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-btaf-b 'neovm--combo-prop-plus-alias-btaf) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-btaf-a 'neovm--combo-prop-plus-alias-btaf) t nil)))
                       (progn
                         (advice-remove 'neovm--combo-prop-plus-alias-btaf 'neovm--combo-prop-plus-alias-btaf-a)
                         (setq log nil)
                         (list
                           (neovm--combo-prop-plus-alias-btaf {a} {b})
                           (funcall 'neovm--combo-prop-plus-alias-btaf {a} {b})
                           (apply 'neovm--combo-prop-plus-alias-btaf (list {a} {b}))
                           (+ {a} {b})
                           (nreverse log)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-btaf-b 'neovm--combo-prop-plus-alias-btaf) t nil)
                           (if (advice-member-p 'neovm--combo-prop-plus-alias-btaf-a 'neovm--combo-prop-plus-alias-btaf) t nil))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-plus-alias-btaf 'neovm--combo-prop-plus-alias-btaf-b)
                     (error nil))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-plus-alias-btaf 'neovm--combo-prop-plus-alias-btaf-a)
                     (error nil))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-alias-btaf-b)
                     (error nil))
                   (condition-case nil
                       (advice-remove '+ 'neovm--combo-prop-plus-alias-btaf-a)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-plus-alias-btaf)
                   (fmakunbound 'neovm--combo-prop-plus-alias-btaf-b)
                   (fmakunbound 'neovm--combo-prop-plus-alias-btaf-a))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_anonymous_same_name_override_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((ov1 (lambda (x y) x))
                     (ov2 (lambda (x y) y)))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add '+ :override ov1 '((name . neovm--combo-prop-plus-anon-ov-shared)))
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (if (advice-member-p ov1 '+) t nil)
                           (if (advice-member-p ov2 '+) t nil)))
                       (progn
                         (advice-add '+ :override ov2 '((name . neovm--combo-prop-plus-anon-ov-shared)))
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (if (advice-member-p ov1 '+) t nil)
                           (if (advice-member-p ov2 '+) t nil)))
                       (progn
                         (advice-remove '+ ov1)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (if (advice-member-p ov1 '+) t nil)
                           (if (advice-member-p ov2 '+) t nil)))
                       (progn
                         (advice-remove '+ ov2)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (if (advice-member-p ov1 '+) t nil)
                           (if (advice-member-p ov2 '+) t nil))))
                   (condition-case nil
                       (advice-remove '+ ov1)
                     (error nil))
                   (condition-case nil
                       (advice-remove '+ ov2)
                     (error nil)))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_anonymous_before_lifecycle_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((log nil)
                     (adv (lambda (&rest args)
                            (setq log (cons args log)))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add '+ :before adv)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p adv '+) t nil)))
                       (progn
                         (advice-remove '+ adv)
                         (setq log nil)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (nreverse log)
                           (if (advice-member-p adv '+) t nil))))
                   (condition-case nil
                       (advice-remove '+ adv)
                     (error nil)))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_anonymous_around_lifecycle_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (let ((adv (lambda (orig x y)
                            (funcall orig (+ x 1) y))))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add '+ :around adv)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (if (advice-member-p adv '+) t nil)))
                       (progn
                         (advice-remove '+ adv)
                         (list
                           (+ {a} {b})
                           (funcall '+ {a} {b})
                           (apply '+ (list {a} {b}))
                           (if (advice-member-p adv '+) t nil))))
                   (condition-case nil
                       (advice-remove '+ adv)
                     (error nil)))))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_cross_target_same_name_override_isolation_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-ct-name-ov-plus (lambda (x y) x))
               (fset 'neovm--combo-prop-ct-name-ov-minus (lambda (x y) y))
               (unwind-protect
                   (list
                     (progn
                       (advice-add '+ :override 'neovm--combo-prop-ct-name-ov-plus '((name . neovm--combo-prop-ct-name-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (- {a} {b})
                         (funcall '- {a} {b})
                         (apply '- (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-plus '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-minus '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-plus '-) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-minus '-) t nil)))
                     (progn
                       (advice-add '- :override 'neovm--combo-prop-ct-name-ov-minus '((name . neovm--combo-prop-ct-name-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (- {a} {b})
                         (funcall '- {a} {b})
                         (apply '- (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-plus '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-minus '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-plus '-) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-minus '-) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-ct-name-ov-plus)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (- {a} {b})
                         (funcall '- {a} {b})
                         (apply '- (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-plus '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-minus '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-plus '-) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-minus '-) t nil)))
                     (progn
                       (advice-remove '- 'neovm--combo-prop-ct-name-ov-minus)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (- {a} {b})
                         (funcall '- {a} {b})
                         (apply '- (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-plus '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-minus '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-plus '-) t nil)
                         (if (advice-member-p 'neovm--combo-prop-ct-name-ov-minus '-) t nil))))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-ct-name-ov-plus)
                   (error nil))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-ct-name-ov-minus)
                   (error nil))
                 (condition-case nil
                     (advice-remove '- 'neovm--combo-prop-ct-name-ov-plus)
                   (error nil))
                 (condition-case nil
                     (advice-remove '- 'neovm--combo-prop-ct-name-ov-minus)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-ct-name-ov-plus)
                 (fmakunbound 'neovm--combo-prop-ct-name-ov-minus)))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_subr_plus_same_name_override_replacement_consistency(
        a in -1_000i64..1_000i64,
        b in -1_000i64..1_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-plus-name-ov1 (lambda (x y) x))
               (fset 'neovm--combo-prop-plus-name-ov2 (lambda (x y) y))
               (unwind-protect
                   (list
                     (progn
                       (advice-add '+ :override 'neovm--combo-prop-plus-name-ov1 '((name . neovm--combo-prop-plus-name-ov-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ov1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ov2 '+) t nil)))
                     (progn
                       (advice-add '+ :override 'neovm--combo-prop-plus-name-ov2 '((name . neovm--combo-prop-plus-name-ov-shared)))
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ov1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ov2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-ov1)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ov1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ov2 '+) t nil)))
                     (progn
                       (advice-remove '+ 'neovm--combo-prop-plus-name-ov2)
                       (list
                         (+ {a} {b})
                         (funcall '+ {a} {b})
                         (apply '+ (list {a} {b}))
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ov1 '+) t nil)
                         (if (advice-member-p 'neovm--combo-prop-plus-name-ov2 '+) t nil))))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-ov1)
                   (error nil))
                 (condition-case nil
                     (advice-remove '+ 'neovm--combo-prop-plus-name-ov2)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-plus-name-ov1)
                 (fmakunbound 'neovm--combo-prop-plus-name-ov2)))",
            a = a,
            b = b,
        );
        assert_oracle_parity(&form);
    }

    #[test]
    fn oracle_prop_combination_macro_filter_return_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-fr-call (x)
                 `(neovm--combo-prop-fr-target ,x))
               (fset 'neovm--combo-prop-fr-target (lambda (x) (* 2 x)))
               (fset 'neovm--combo-prop-fr-filter (lambda (ret) (+ ret 9)))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-fr-target :filter-return 'neovm--combo-prop-fr-filter)
                     (list
                       (neovm--combo-prop-fr-call {n})
                       (eval '(neovm--combo-prop-fr-call {n}))
                       (funcall 'neovm--combo-prop-fr-target {n})
                       (apply 'neovm--combo-prop-fr-target (list {n}))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-fr-target 'neovm--combo-prop-fr-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-fr-target)
                 (fmakunbound 'neovm--combo-prop-fr-filter)
                 (fmakunbound 'neovm--combo-prop-fr-call)))",
            n = n,
        );

        let expected = 2 * n + 9;
        let expected_payload = format!("({expected} {expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_before_throw_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-before-call (x)
                 `(neovm--combo-prop-before-target ,x))
               (fset 'neovm--combo-prop-before-target (lambda (x) (* 10 x)))
               (fset 'neovm--combo-prop-before
                     (lambda (&rest args)
                       (throw 'neovm--combo-prop-before-tag
                              (list 'thrown (car args)))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-before-target :before 'neovm--combo-prop-before)
                     (list
                       (catch 'neovm--combo-prop-before-tag
                         (neovm--combo-prop-before-call {n}))
                       (catch 'neovm--combo-prop-before-tag
                         (eval '(neovm--combo-prop-before-call {n})))
                       (catch 'neovm--combo-prop-before-tag
                         (funcall 'neovm--combo-prop-before-target {n}))
                       (catch 'neovm--combo-prop-before-tag
                         (apply 'neovm--combo-prop-before-target (list {n})))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-before-target 'neovm--combo-prop-before)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-before-target)
                 (fmakunbound 'neovm--combo-prop-before)
                 (fmakunbound 'neovm--combo-prop-before-call)))",
            n = n,
        );

        let expected_payload = format!("((thrown {n}) (thrown {n}) (thrown {n}) (thrown {n}))");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_override_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-ov-call (x)
                 `(neovm--combo-prop-ov-target ,x))
               (fset 'neovm--combo-prop-ov-target (lambda (x) (* 4 x)))
               (fset 'neovm--combo-prop-ov (lambda (x) (+ x 100)))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-ov-target :override 'neovm--combo-prop-ov)
                     (list
                       (neovm--combo-prop-ov-call {n})
                       (eval '(neovm--combo-prop-ov-call {n}))
                       (funcall 'neovm--combo-prop-ov-target {n})
                       (apply 'neovm--combo-prop-ov-target (list {n}))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-ov-target 'neovm--combo-prop-ov)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-ov-target)
                 (fmakunbound 'neovm--combo-prop-ov)
                 (fmakunbound 'neovm--combo-prop-ov-call)))",
            n = n,
        );

        let expected = n + 100;
        let expected_payload = format!("({expected} {expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_after_side_effect_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-after-call (x)
                 `(neovm--combo-prop-after-target ,x))
               (fset 'neovm--combo-prop-after-target (lambda (x) x))
               (let ((log nil))
                 (fset 'neovm--combo-prop-after
                       (lambda (&rest args)
                         (setq log (cons (car args) log))))
                 (unwind-protect
                     (progn
                       (advice-add 'neovm--combo-prop-after-target :after 'neovm--combo-prop-after)
                       (list
                         (neovm--combo-prop-after-call {n})
                         (eval '(neovm--combo-prop-after-call {n}))
                         (funcall 'neovm--combo-prop-after-target {n})
                         (apply 'neovm--combo-prop-after-target (list {n}))
                         (length log)
                         (nreverse log)))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-after-target 'neovm--combo-prop-after)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-after-target)
                   (fmakunbound 'neovm--combo-prop-after)
                   (fmakunbound 'neovm--combo-prop-after-call))))",
            n = n,
        );

        let expected_payload = format!("({n} {n} {n} {n} 4 ({n} {n} {n} {n}))");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_condition_case_throw_before_advice_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-cc-throw-call (x)
                 `(condition-case err
                      (neovm--combo-prop-cc-throw-target ,x)
                    (error (list 'err (car err)))))
               (fset 'neovm--combo-prop-cc-throw-target
                     (lambda (x)
                       (throw 'neovm--combo-prop-cc-throw-tag x)))
               (let ((log nil))
                 (fset 'neovm--combo-prop-cc-throw-before
                       (lambda (&rest _args)
                         (setq log (cons 'before log))))
                 (unwind-protect
                     (progn
                       (advice-add 'neovm--combo-prop-cc-throw-target :before 'neovm--combo-prop-cc-throw-before)
                       (list
                         (catch 'neovm--combo-prop-cc-throw-tag
                           (neovm--combo-prop-cc-throw-call {n}))
                         (catch 'neovm--combo-prop-cc-throw-tag
                           (eval '(neovm--combo-prop-cc-throw-call {n})))
                         (catch 'neovm--combo-prop-cc-throw-tag
                           (condition-case err
                               (funcall 'neovm--combo-prop-cc-throw-target {n})
                             (error (list 'err (car err)))))
                         (catch 'neovm--combo-prop-cc-throw-tag
                           (condition-case err
                               (apply 'neovm--combo-prop-cc-throw-target (list {n}))
                             (error (list 'err (car err)))))
                         (length log)))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-cc-throw-target 'neovm--combo-prop-cc-throw-before)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-cc-throw-target)
                   (fmakunbound 'neovm--combo-prop-cc-throw-before)
                   (fmakunbound 'neovm--combo-prop-cc-throw-call))))",
            n = n,
        );

        let expected_payload = format!("({n} {n} {n} {n} 4)");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_filter_args_call_path_consistency(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-fa-call (x y)
                 `(neovm--combo-prop-fa-target ,x ,y))
               (fset 'neovm--combo-prop-fa-target (lambda (x y) (+ x y)))
               (fset 'neovm--combo-prop-fa-filter
                     (lambda (args)
                       (list (+ 10 (car args))
                             (+ 20 (car (cdr args))))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-fa-target :filter-args 'neovm--combo-prop-fa-filter)
                     (list
                       (neovm--combo-prop-fa-call {a} {b})
                       (eval '(neovm--combo-prop-fa-call {a} {b}))
                       (funcall 'neovm--combo-prop-fa-target {a} {b})
                       (apply 'neovm--combo-prop-fa-target (list {a} {b}))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-fa-target 'neovm--combo-prop-fa-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-fa-target)
                 (fmakunbound 'neovm--combo-prop-fa-filter)
                 (fmakunbound 'neovm--combo-prop-fa-call)))",
            a = a,
            b = b,
        );

        let expected = a + b + 30;
        let expected_payload = format!("({expected} {expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_stacked_around_filter_return_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-stack-call (x)
                 `(neovm--combo-prop-stack-target ,x))
               (fset 'neovm--combo-prop-stack-target (lambda (x) (+ x 2)))
               (fset 'neovm--combo-prop-stack-around
                     (lambda (orig x) (* 2 (funcall orig x))))
               (fset 'neovm--combo-prop-stack-fr
                     (lambda (ret) (+ ret 5)))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-stack-target :around 'neovm--combo-prop-stack-around)
                     (advice-add 'neovm--combo-prop-stack-target :filter-return 'neovm--combo-prop-stack-fr)
                     (list
                       (neovm--combo-prop-stack-call {n})
                       (eval '(neovm--combo-prop-stack-call {n}))
                       (funcall 'neovm--combo-prop-stack-target {n})
                       (apply 'neovm--combo-prop-stack-target (list {n}))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-stack-target 'neovm--combo-prop-stack-fr)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-stack-target 'neovm--combo-prop-stack-around)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-stack-target)
                 (fmakunbound 'neovm--combo-prop-stack-around)
                 (fmakunbound 'neovm--combo-prop-stack-fr)
                 (fmakunbound 'neovm--combo-prop-stack-call)))",
            n = n,
        );

        let expected = 2 * (n + 2) + 5;
        let expected_payload = format!("({expected} {expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_fset_after_advice_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-fset-call (x)
                 `(neovm--combo-prop-fset-target ,x))
               (fset 'neovm--combo-prop-fset-target (lambda (x) (+ x 1)))
               (fset 'neovm--combo-prop-fset-around
                     (lambda (orig x) (+ 100 (funcall orig x))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-fset-target :around 'neovm--combo-prop-fset-around)
                     (fset 'neovm--combo-prop-fset-target (lambda (x) (* 2 x)))
                     (list
                       (neovm--combo-prop-fset-call {n})
                       (eval '(neovm--combo-prop-fset-call {n}))
                       (funcall 'neovm--combo-prop-fset-target {n})
                       (apply 'neovm--combo-prop-fset-target (list {n}))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-fset-target 'neovm--combo-prop-fset-around)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-fset-target)
                 (fmakunbound 'neovm--combo-prop-fset-around)
                 (fmakunbound 'neovm--combo-prop-fset-call)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_non_symbol_throw_from_around_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((tag (list 'neovm--combo-prop-nsym-tag)))
               (progn
                 (defmacro neovm--combo-prop-nsym-call (x)
                   `(neovm--combo-prop-nsym-target ,x))
                 (fset 'neovm--combo-prop-nsym-target (lambda (x) x))
                 (fset 'neovm--combo-prop-nsym-around
                       (lambda (_orig x)
                         (throw tag (+ x 9))))
                 (unwind-protect
                     (progn
                       (advice-add 'neovm--combo-prop-nsym-target :around 'neovm--combo-prop-nsym-around)
                       (list
                         (catch tag (neovm--combo-prop-nsym-call {n}))
                         (catch tag (eval '(neovm--combo-prop-nsym-call {n})))
                         (catch tag (funcall 'neovm--combo-prop-nsym-target {n}))
                         (catch tag (apply 'neovm--combo-prop-nsym-target (list {n})))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-nsym-target 'neovm--combo-prop-nsym-around)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-nsym-target)
                   (fmakunbound 'neovm--combo-prop-nsym-around)
                   (fmakunbound 'neovm--combo-prop-nsym-call))))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_defalias_under_advice_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-alias-call (x)
                 `(neovm--combo-prop-alias ,x))
               (fset 'neovm--combo-prop-alias-target (lambda (x) (+ x 1)))
               (fset 'neovm--combo-prop-alias-around
                     (lambda (orig x) (* 2 (funcall orig x))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-alias-target :around 'neovm--combo-prop-alias-around)
                     (defalias 'neovm--combo-prop-alias 'neovm--combo-prop-alias-target)
                     (list
                       (neovm--combo-prop-alias-call {n})
                       (eval '(neovm--combo-prop-alias-call {n}))
                       (funcall 'neovm--combo-prop-alias {n})
                       (apply 'neovm--combo-prop-alias (list {n}))
                       (funcall 'neovm--combo-prop-alias-target {n})
                       (apply 'neovm--combo-prop-alias-target (list {n}))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-alias-target 'neovm--combo-prop-alias-around)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-alias)
                 (fmakunbound 'neovm--combo-prop-alias-target)
                 (fmakunbound 'neovm--combo-prop-alias-around)
                 (fmakunbound 'neovm--combo-prop-alias-call)))",
            n = n,
        );

        let expected = 2 * (n + 1);
        let expected_payload = format!("({expected} {expected} {expected} {expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_after_throw_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((log nil))
               (progn
                 (defmacro neovm--combo-prop-after-throw-call (x)
                   `(neovm--combo-prop-after-throw-target ,x))
                 (fset 'neovm--combo-prop-after-throw-target
                       (lambda (x)
                         (setq log (cons (list 'orig x) log))
                         x))
                 (fset 'neovm--combo-prop-after-throw
                       (lambda (&rest args)
                         (setq log (cons (list 'after (car args)) log))
                         (throw 'neovm--combo-prop-after-throw-tag (+ 50 (car args)))))
                 (unwind-protect
                     (progn
                       (advice-add 'neovm--combo-prop-after-throw-target :after 'neovm--combo-prop-after-throw)
                       (list
                         (catch 'neovm--combo-prop-after-throw-tag
                           (neovm--combo-prop-after-throw-call {n}))
                         (catch 'neovm--combo-prop-after-throw-tag
                           (eval '(neovm--combo-prop-after-throw-call {n})))
                         (catch 'neovm--combo-prop-after-throw-tag
                           (funcall 'neovm--combo-prop-after-throw-target {n}))
                         (catch 'neovm--combo-prop-after-throw-tag
                           (apply 'neovm--combo-prop-after-throw-target (list {n})))
                         (nreverse log)))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-after-throw-target 'neovm--combo-prop-after-throw)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-after-throw-target)
                   (fmakunbound 'neovm--combo-prop-after-throw)
                   (fmakunbound 'neovm--combo-prop-after-throw-call))))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_filter_return_advice_toggle_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-toggle-call (x)
                 `(neovm--combo-prop-toggle-target ,x))
               (fset 'neovm--combo-prop-toggle-target (lambda (x) x))
               (fset 'neovm--combo-prop-toggle-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (list
                     (neovm--combo-prop-toggle-call {n})
                     (funcall 'neovm--combo-prop-toggle-target {n})
                     (progn
                       (advice-add 'neovm--combo-prop-toggle-target :filter-return 'neovm--combo-prop-toggle-filter)
                       (list
                         (neovm--combo-prop-toggle-call {n})
                         (eval '(neovm--combo-prop-toggle-call {n}))
                         (funcall 'neovm--combo-prop-toggle-target {n})
                         (apply 'neovm--combo-prop-toggle-target (list {n}))))
                     (progn
                       (advice-remove 'neovm--combo-prop-toggle-target 'neovm--combo-prop-toggle-filter)
                       (list
                         (neovm--combo-prop-toggle-call {n})
                         (eval '(neovm--combo-prop-toggle-call {n}))
                         (funcall 'neovm--combo-prop-toggle-target {n})
                         (apply 'neovm--combo-prop-toggle-target (list {n})))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-toggle-target 'neovm--combo-prop-toggle-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-toggle-target)
                 (fmakunbound 'neovm--combo-prop-toggle-filter)
                 (fmakunbound 'neovm--combo-prop-toggle-call)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_around_error_to_throw_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-err-throw-call (x)
                 `(neovm--combo-prop-err-throw-target ,x))
               (fset 'neovm--combo-prop-err-throw-target
                     (lambda (_x) (/ 1 0)))
               (fset 'neovm--combo-prop-err-throw-around
                     (lambda (orig x)
                       (condition-case nil
                           (funcall orig x)
                         (arith-error
                          (throw 'neovm--combo-prop-err-throw-tag (+ 50 x))))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-err-throw-target :around 'neovm--combo-prop-err-throw-around)
                     (list
                       (catch 'neovm--combo-prop-err-throw-tag
                         (condition-case nil
                             (neovm--combo-prop-err-throw-call {n})
                           (arith-error 'arith)))
                       (catch 'neovm--combo-prop-err-throw-tag
                         (condition-case nil
                             (eval '(neovm--combo-prop-err-throw-call {n}))
                           (arith-error 'arith)))
                       (catch 'neovm--combo-prop-err-throw-tag
                         (condition-case nil
                             (funcall 'neovm--combo-prop-err-throw-target {n})
                           (arith-error 'arith)))
                       (catch 'neovm--combo-prop-err-throw-tag
                         (condition-case nil
                             (apply 'neovm--combo-prop-err-throw-target (list {n}))
                           (arith-error 'arith)))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-err-throw-target 'neovm--combo-prop-err-throw-around)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-err-throw-target)
                 (fmakunbound 'neovm--combo-prop-err-throw-around)
                 (fmakunbound 'neovm--combo-prop-err-throw-call)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_advice_member_state_and_paths_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-member-call (x)
                 `(neovm--combo-prop-member-target ,x))
               (fset 'neovm--combo-prop-member-target (lambda (x) x))
               (fset 'neovm--combo-prop-member-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (list
                     (if (advice-member-p 'neovm--combo-prop-member-filter 'neovm--combo-prop-member-target) t nil)
                     (progn
                       (advice-add 'neovm--combo-prop-member-target :filter-return 'neovm--combo-prop-member-filter)
                       (list
                         (if (advice-member-p 'neovm--combo-prop-member-filter 'neovm--combo-prop-member-target) t nil)
                         (neovm--combo-prop-member-call {n})
                         (eval '(neovm--combo-prop-member-call {n}))
                         (funcall 'neovm--combo-prop-member-target {n})
                         (apply 'neovm--combo-prop-member-target (list {n}))))
                     (progn
                       (advice-remove 'neovm--combo-prop-member-target 'neovm--combo-prop-member-filter)
                       (list
                         (if (advice-member-p 'neovm--combo-prop-member-filter 'neovm--combo-prop-member-target) t nil)
                         (neovm--combo-prop-member-call {n})
                         (eval '(neovm--combo-prop-member-call {n}))
                         (funcall 'neovm--combo-prop-member-target {n})
                         (apply 'neovm--combo-prop-member-target (list {n})))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-member-target 'neovm--combo-prop-member-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-member-target)
                 (fmakunbound 'neovm--combo-prop-member-filter)
                 (fmakunbound 'neovm--combo-prop-member-call)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_expansion_shape_under_around_advice_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-shape-direct (x)
                 `(neovm--combo-prop-shape-target ,x))
               (defmacro neovm--combo-prop-shape-funcall (x)
                 `(funcall 'neovm--combo-prop-shape-target ,x))
               (defmacro neovm--combo-prop-shape-apply (x)
                 `(apply 'neovm--combo-prop-shape-target (list ,x)))
               (fset 'neovm--combo-prop-shape-target (lambda (x) (+ x 1)))
               (fset 'neovm--combo-prop-shape-around
                     (lambda (orig x) (+ 100 (funcall orig x))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-shape-target :around 'neovm--combo-prop-shape-around)
                     (list
                       (neovm--combo-prop-shape-direct {n})
                       (eval '(neovm--combo-prop-shape-direct {n}))
                       (neovm--combo-prop-shape-funcall {n})
                       (eval '(neovm--combo-prop-shape-funcall {n}))
                       (neovm--combo-prop-shape-apply {n})
                       (eval '(neovm--combo-prop-shape-apply {n}))
                       (funcall 'neovm--combo-prop-shape-target {n})
                       (apply 'neovm--combo-prop-shape-target (list {n}))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-shape-target 'neovm--combo-prop-shape-around)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-shape-target)
                 (fmakunbound 'neovm--combo-prop-shape-around)
                 (fmakunbound 'neovm--combo-prop-shape-direct)
                 (fmakunbound 'neovm--combo-prop-shape-funcall)
                 (fmakunbound 'neovm--combo-prop-shape-apply)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_float_eq_hash_table_identity_consistency(
        n in -1000i64..1000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let float_src = format!("{}.0", n);
        let form = format!(
            "(let* ((k1 (car (read-from-string \"{src}\")))
                    (k2 (car (read-from-string \"{src}\")))
                    (ht (make-hash-table :test 'eq)))
               (list
                 (eq k1 k2)
                 (progn
                   (puthash k1 'v ht)
                   (gethash k1 ht 'missing))
                 (gethash k2 ht 'missing)
                 (progn
                   (puthash k2 'w ht)
                   (hash-table-count ht))
                 (list
                   (gethash k1 ht 'missing)
                   (gethash k2 ht 'missing))))",
            src = float_src,
        );

        let expected = "(nil v missing 2 (v w))";
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected, &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_generated_lambda_call_shape_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-make-caller (mode)
                 (cond
                   ((eq mode 'direct) '(lambda (x) (neovm--combo-prop-lambda-target x)))
                   ((eq mode 'funcall) '(lambda (x) (funcall 'neovm--combo-prop-lambda-target x)))
                   (t '(lambda (x) (apply 'neovm--combo-prop-lambda-target (list x))))))
               (fset 'neovm--combo-prop-lambda-target (lambda (x) x))
               (fset 'neovm--combo-prop-lambda-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-lambda-target :filter-return 'neovm--combo-prop-lambda-filter)
                     (let ((d (eval (macroexpand '(neovm--combo-prop-make-caller 'direct))))
                           (f (eval (macroexpand '(neovm--combo-prop-make-caller 'funcall))))
                           (a (eval (macroexpand '(neovm--combo-prop-make-caller 'apply)))))
                       (list
                         (funcall d {n})
                         (funcall f {n})
                         (funcall a {n})
                         (eval (list 'funcall d {n}))
                         (eval (list 'funcall f {n}))
                         (eval (list 'funcall a {n})))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-lambda-target 'neovm--combo-prop-lambda-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-lambda-target)
                 (fmakunbound 'neovm--combo-prop-lambda-filter)
                 (fmakunbound 'neovm--combo-prop-make-caller)))",
            n = n,
        );

        let expected = n + 7;
        let expected_payload =
            format!("({expected} {expected} {expected} {expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_eval_quoted_symbol_arg_lambda_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-min-caller (mode)
                 (cond
                   ((eq mode 'direct) '(lambda (x) (1+ x)))
                   ((eq mode 'funcall) '(lambda (x) (funcall '+ x 1)))
                   (t '(lambda (x) (apply '+ (list x 1))))))
               (unwind-protect
                   (list
                     (eval '(funcall (neovm--combo-prop-min-caller 'direct) {n}))
                     (eval '(funcall (neovm--combo-prop-min-caller 'funcall) {n}))
                     (eval '(funcall (neovm--combo-prop-min-caller 'apply) {n})))
                 (fmakunbound 'neovm--combo-prop-min-caller)))",
            n = n,
        );

        let expected = n + 1;
        let expected_payload = format!("({expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_generated_lambda_advice_toggle_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((n {n}))
               (progn
                 (defmacro neovm--combo-prop-toggle-make-caller (mode)
                   (cond
                     ((eq mode 'direct) '(lambda (x) (neovm--combo-prop-toggle-lambda-target x)))
                     (t '(lambda (x) (funcall 'neovm--combo-prop-toggle-lambda-target x)))))
                 (fset 'neovm--combo-prop-toggle-lambda-target (lambda (x) x))
                 (fset 'neovm--combo-prop-toggle-lambda-filter (lambda (ret) (+ ret 7)))
                 (unwind-protect
                     (let ((d (neovm--combo-prop-toggle-make-caller 'direct))
                           (f (neovm--combo-prop-toggle-make-caller 'funcall)))
                       (list
                         (funcall d n)
                         (funcall f n)
                         (progn
                           (advice-add 'neovm--combo-prop-toggle-lambda-target :filter-return 'neovm--combo-prop-toggle-lambda-filter)
                           (list (funcall d n) (funcall f n)))
                         (progn
                           (advice-remove 'neovm--combo-prop-toggle-lambda-target 'neovm--combo-prop-toggle-lambda-filter)
                           (list (funcall d n) (funcall f n)))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-toggle-lambda-target 'neovm--combo-prop-toggle-lambda-filter)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-toggle-lambda-target)
                   (fmakunbound 'neovm--combo-prop-toggle-lambda-filter)
                   (fmakunbound 'neovm--combo-prop-toggle-make-caller))))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_advice_member_alias_visibility_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-member-alias-target (lambda (x) x))
               (defalias 'neovm--combo-prop-member-alias 'neovm--combo-prop-member-alias-target)
               (fset 'neovm--combo-prop-member-alias-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-member-alias :filter-return 'neovm--combo-prop-member-alias-filter)
                     (list
                       (if (advice-member-p 'neovm--combo-prop-member-alias-filter 'neovm--combo-prop-member-alias) t nil)
                       (if (advice-member-p 'neovm--combo-prop-member-alias-filter 'neovm--combo-prop-member-alias-target) t nil)
                       (funcall 'neovm--combo-prop-member-alias {n})
                       (funcall 'neovm--combo-prop-member-alias-target {n})))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-member-alias 'neovm--combo-prop-member-alias-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-member-alias)
                 (fmakunbound 'neovm--combo-prop-member-alias-target)
                 (fmakunbound 'neovm--combo-prop-member-alias-filter)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_stacked_advice_order_call_path_logs_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((log nil))
               (fset 'neovm--combo-prop-order-target
                     (lambda (x)
                       (setq log (cons 'orig log))
                       x))
               (fset 'neovm--combo-prop-order-before
                     (lambda (&rest _args)
                       (setq log (cons 'before log))))
               (fset 'neovm--combo-prop-order-around
                     (lambda (orig x)
                       (setq log (cons 'around-enter log))
                       (unwind-protect
                           (funcall orig x)
                         (setq log (cons 'around-exit log)))))
               (fset 'neovm--combo-prop-order-after
                     (lambda (&rest _args)
                       (setq log (cons 'after log))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-order-target :before 'neovm--combo-prop-order-before)
                     (advice-add 'neovm--combo-prop-order-target :around 'neovm--combo-prop-order-around)
                     (advice-add 'neovm--combo-prop-order-target :after 'neovm--combo-prop-order-after)
                     (list
                       (progn
                         (setq log nil)
                         (list (neovm--combo-prop-order-target {n}) (nreverse log)))
                       (progn
                         (setq log nil)
                         (list (eval '(neovm--combo-prop-order-target {n})) (nreverse log)))
                       (progn
                         (setq log nil)
                         (list (funcall 'neovm--combo-prop-order-target {n}) (nreverse log)))
                       (progn
                         (setq log nil)
                         (list (apply 'neovm--combo-prop-order-target (list {n})) (nreverse log)))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-order-target 'neovm--combo-prop-order-after)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-order-target 'neovm--combo-prop-order-around)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-order-target 'neovm--combo-prop-order-before)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-order-target)
                 (fmakunbound 'neovm--combo-prop-order-before)
                 (fmakunbound 'neovm--combo-prop-order-around)
                 (fmakunbound 'neovm--combo-prop-order-after)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_stacked_advice_throw_order_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((log nil))
               (fset 'neovm--combo-prop-order-throw-target
                     (lambda (x)
                       (setq log (cons 'orig log))
                       (throw 'neovm--combo-prop-order-throw-tag x)))
               (fset 'neovm--combo-prop-order-throw-before
                     (lambda (&rest _args)
                       (setq log (cons 'before log))))
               (fset 'neovm--combo-prop-order-throw-around
                     (lambda (orig x)
                       (setq log (cons 'around-enter log))
                       (unwind-protect
                           (funcall orig x)
                         (setq log (cons 'around-exit log)))))
               (fset 'neovm--combo-prop-order-throw-after
                     (lambda (&rest _args)
                       (setq log (cons 'after log))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-order-throw-target :before 'neovm--combo-prop-order-throw-before)
                     (advice-add 'neovm--combo-prop-order-throw-target :around 'neovm--combo-prop-order-throw-around)
                     (advice-add 'neovm--combo-prop-order-throw-target :after 'neovm--combo-prop-order-throw-after)
                     (list
                       (progn
                         (setq log nil)
                         (list
                           (catch 'neovm--combo-prop-order-throw-tag
                             (neovm--combo-prop-order-throw-target {n}))
                           (nreverse log)))
                       (progn
                         (setq log nil)
                         (list
                           (catch 'neovm--combo-prop-order-throw-tag
                             (eval '(neovm--combo-prop-order-throw-target {n})))
                           (nreverse log)))
                       (progn
                         (setq log nil)
                         (list
                           (catch 'neovm--combo-prop-order-throw-tag
                             (funcall 'neovm--combo-prop-order-throw-target {n}))
                           (nreverse log)))
                       (progn
                         (setq log nil)
                         (list
                           (catch 'neovm--combo-prop-order-throw-tag
                             (apply 'neovm--combo-prop-order-throw-target (list {n})))
                           (nreverse log)))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-order-throw-target 'neovm--combo-prop-order-throw-after)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-order-throw-target 'neovm--combo-prop-order-throw-around)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-order-throw-target 'neovm--combo-prop-order-throw-before)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-order-throw-target)
                 (fmakunbound 'neovm--combo-prop-order-throw-before)
                 (fmakunbound 'neovm--combo-prop-order-throw-around)
                 (fmakunbound 'neovm--combo-prop-order-throw-after)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_alias_stacked_advice_order_visibility_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((log nil))
               (fset 'neovm--combo-prop-alias-order-target
                     (lambda (x)
                       (setq log (cons 'orig log))
                       x))
               (defalias 'neovm--combo-prop-alias-order 'neovm--combo-prop-alias-order-target)
               (fset 'neovm--combo-prop-alias-order-before
                     (lambda (&rest _args)
                       (setq log (cons 'before log))))
               (fset 'neovm--combo-prop-alias-order-around
                     (lambda (orig x)
                       (setq log (cons 'around-enter log))
                       (unwind-protect
                           (funcall orig x)
                         (setq log (cons 'around-exit log)))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-alias-order :before 'neovm--combo-prop-alias-order-before)
                     (advice-add 'neovm--combo-prop-alias-order :around 'neovm--combo-prop-alias-order-around)
                     (list
                       (progn
                         (setq log nil)
                         (list (funcall 'neovm--combo-prop-alias-order {n}) (nreverse log)))
                       (progn
                         (setq log nil)
                         (list (funcall 'neovm--combo-prop-alias-order-target {n}) (nreverse log)))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-alias-order 'neovm--combo-prop-alias-order-around)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-alias-order 'neovm--combo-prop-alias-order-before)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-alias-order)
                 (fmakunbound 'neovm--combo-prop-alias-order-target)
                 (fmakunbound 'neovm--combo-prop-alias-order-before)
                 (fmakunbound 'neovm--combo-prop-alias-order-around)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_symbol_function_identity_advice_toggle_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((orig nil))
               (fset 'neovm--combo-prop-sf-toggle-target (lambda (x) x))
               (setq orig (symbol-function 'neovm--combo-prop-sf-toggle-target))
               (fset 'neovm--combo-prop-sf-toggle-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (list
                     (eq (symbol-function 'neovm--combo-prop-sf-toggle-target) orig)
                     (progn
                       (advice-add 'neovm--combo-prop-sf-toggle-target :filter-return 'neovm--combo-prop-sf-toggle-filter)
                       (list
                         (eq (symbol-function 'neovm--combo-prop-sf-toggle-target) orig)
                         (funcall 'neovm--combo-prop-sf-toggle-target {n})
                         (apply 'neovm--combo-prop-sf-toggle-target (list {n}))))
                     (progn
                       (advice-remove 'neovm--combo-prop-sf-toggle-target 'neovm--combo-prop-sf-toggle-filter)
                       (list
                         (eq (symbol-function 'neovm--combo-prop-sf-toggle-target) orig)
                         (funcall 'neovm--combo-prop-sf-toggle-target {n})
                         (apply 'neovm--combo-prop-sf-toggle-target (list {n})))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-sf-toggle-target 'neovm--combo-prop-sf-toggle-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-sf-toggle-target)
                 (fmakunbound 'neovm--combo-prop-sf-toggle-filter)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_defalias_rebind_under_active_advice_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-alias-rebind-target-a (lambda (x) (+ x 1)))
               (fset 'neovm--combo-prop-alias-rebind-target-b (lambda (x) (* 2 x)))
               (defalias 'neovm--combo-prop-alias-rebind 'neovm--combo-prop-alias-rebind-target-a)
               (fset 'neovm--combo-prop-alias-rebind-filter (lambda (ret) (+ ret 100)))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-alias-rebind :filter-return 'neovm--combo-prop-alias-rebind-filter)
                     (list
                       (funcall 'neovm--combo-prop-alias-rebind {n})
                       (funcall 'neovm--combo-prop-alias-rebind-target-a {n})
                       (progn
                         (defalias 'neovm--combo-prop-alias-rebind 'neovm--combo-prop-alias-rebind-target-b)
                         (list
                           (funcall 'neovm--combo-prop-alias-rebind {n})
                           (funcall 'neovm--combo-prop-alias-rebind-target-b {n})
                           (apply 'neovm--combo-prop-alias-rebind (list {n}))
                           (eval '(neovm--combo-prop-alias-rebind {n}))))
                       (progn
                         (advice-remove 'neovm--combo-prop-alias-rebind 'neovm--combo-prop-alias-rebind-filter)
                         (list
                           (funcall 'neovm--combo-prop-alias-rebind {n})
                           (funcall 'neovm--combo-prop-alias-rebind-target-b {n})))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-alias-rebind 'neovm--combo-prop-alias-rebind-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-alias-rebind)
                 (fmakunbound 'neovm--combo-prop-alias-rebind-target-a)
                 (fmakunbound 'neovm--combo-prop-alias-rebind-target-b)
                 (fmakunbound 'neovm--combo-prop-alias-rebind-filter)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_defalias_rebind_filter_args_lifecycle_consistency(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-alias-fargs-target-a (lambda (x y) (+ x y)))
               (fset 'neovm--combo-prop-alias-fargs-target-b (lambda (x y) (* x y)))
               (defalias 'neovm--combo-prop-alias-fargs 'neovm--combo-prop-alias-fargs-target-a)
               (fset 'neovm--combo-prop-alias-fargs-filter
                     (lambda (args)
                       (list (+ 10 (car args))
                             (+ 20 (car (cdr args))))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-alias-fargs :filter-args 'neovm--combo-prop-alias-fargs-filter)
                     (list
                       (funcall 'neovm--combo-prop-alias-fargs {a} {b})
                       (apply 'neovm--combo-prop-alias-fargs (list {a} {b}))
                       (progn
                         (defalias 'neovm--combo-prop-alias-fargs 'neovm--combo-prop-alias-fargs-target-b)
                         (list
                           (neovm--combo-prop-alias-fargs {a} {b})
                           (eval '(neovm--combo-prop-alias-fargs {a} {b}))
                           (funcall 'neovm--combo-prop-alias-fargs {a} {b})
                           (apply 'neovm--combo-prop-alias-fargs (list {a} {b}))
                           (funcall 'neovm--combo-prop-alias-fargs-target-b {a} {b})))
                       (progn
                         (advice-remove 'neovm--combo-prop-alias-fargs 'neovm--combo-prop-alias-fargs-filter)
                         (list
                           (funcall 'neovm--combo-prop-alias-fargs {a} {b})
                           (apply 'neovm--combo-prop-alias-fargs (list {a} {b}))
                           (funcall 'neovm--combo-prop-alias-fargs-target-b {a} {b})))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-alias-fargs 'neovm--combo-prop-alias-fargs-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-alias-fargs)
                 (fmakunbound 'neovm--combo-prop-alias-fargs-target-a)
                 (fmakunbound 'neovm--combo-prop-alias-fargs-target-b)
                 (fmakunbound 'neovm--combo-prop-alias-fargs-filter)))",
            a = a,
            b = b,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_before_advice_error_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-before-err-call (x)
                 `(neovm--combo-prop-before-err-target ,x))
               (fset 'neovm--combo-prop-before-err-target (lambda (x) x))
               (fset 'neovm--combo-prop-before-err
                     (lambda (&rest _args) (/ 1 0)))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-before-err-target :before 'neovm--combo-prop-before-err)
                     (list
                       (condition-case nil
                           (neovm--combo-prop-before-err-call {n})
                         (arith-error 'arith))
                       (condition-case nil
                           (eval '(neovm--combo-prop-before-err-call {n}))
                         (arith-error 'arith))
                       (condition-case nil
                           (funcall 'neovm--combo-prop-before-err-target {n})
                         (arith-error 'arith))
                       (condition-case nil
                           (apply 'neovm--combo-prop-before-err-target (list {n}))
                         (arith-error 'arith))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-before-err-target 'neovm--combo-prop-before-err)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-before-err-target)
                 (fmakunbound 'neovm--combo-prop-before-err)
                 (fmakunbound 'neovm--combo-prop-before-err-call)))",
            n = n,
        );

        let expected = "(arith arith arith arith)";
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected, &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_multi_stage_advice_removal_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-stage-call (x)
                 `(neovm--combo-prop-stage-target ,x))
               (fset 'neovm--combo-prop-stage-target (lambda (x) x))
               (fset 'neovm--combo-prop-stage-before (lambda (&rest _args) nil))
               (fset 'neovm--combo-prop-stage-around
                     (lambda (orig x) (* 2 (funcall orig x))))
               (fset 'neovm--combo-prop-stage-filter (lambda (ret) (+ ret 10)))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-stage-target :before 'neovm--combo-prop-stage-before)
                     (advice-add 'neovm--combo-prop-stage-target :around 'neovm--combo-prop-stage-around)
                     (advice-add 'neovm--combo-prop-stage-target :filter-return 'neovm--combo-prop-stage-filter)
                     (list
                       (list
                         (neovm--combo-prop-stage-call {n})
                         (eval '(neovm--combo-prop-stage-call {n}))
                         (funcall 'neovm--combo-prop-stage-target {n})
                         (apply 'neovm--combo-prop-stage-target (list {n})))
                       (progn
                         (advice-remove 'neovm--combo-prop-stage-target 'neovm--combo-prop-stage-around)
                         (list
                           (neovm--combo-prop-stage-call {n})
                           (eval '(neovm--combo-prop-stage-call {n}))
                           (funcall 'neovm--combo-prop-stage-target {n})
                           (apply 'neovm--combo-prop-stage-target (list {n}))))
                       (progn
                         (advice-remove 'neovm--combo-prop-stage-target 'neovm--combo-prop-stage-filter)
                         (list
                           (neovm--combo-prop-stage-call {n})
                           (eval '(neovm--combo-prop-stage-call {n}))
                           (funcall 'neovm--combo-prop-stage-target {n})
                           (apply 'neovm--combo-prop-stage-target (list {n}))))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-stage-target 'neovm--combo-prop-stage-filter)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-stage-target 'neovm--combo-prop-stage-around)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-stage-target 'neovm--combo-prop-stage-before)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-stage-target)
                 (fmakunbound 'neovm--combo-prop-stage-before)
                 (fmakunbound 'neovm--combo-prop-stage-around)
                 (fmakunbound 'neovm--combo-prop-stage-filter)
                 (fmakunbound 'neovm--combo-prop-stage-call)))",
            n = n,
        );

        let stage1 = 2 * n + 10;
        let stage2 = n + 10;
        let stage3 = n;
        let expected = format!(
            "(({s1} {s1} {s1} {s1}) ({s2} {s2} {s2} {s2}) ({s3} {s3} {s3} {s3}))",
            s1 = stage1,
            s2 = stage2,
            s3 = stage3
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_symbol_function_capture_advice_lifecycle_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-sf-cap-target (lambda (x) x))
               (fset 'neovm--combo-prop-sf-cap-filter (lambda (ret) (+ ret 7)))
               (let ((f0 (symbol-function 'neovm--combo-prop-sf-cap-target)))
                 (unwind-protect
                     (list
                       (progn
                         (advice-add 'neovm--combo-prop-sf-cap-target :filter-return 'neovm--combo-prop-sf-cap-filter)
                         (let ((f1 (symbol-function 'neovm--combo-prop-sf-cap-target)))
                           (list
                             (eq f0 f1)
                             (funcall f0 {n})
                             (funcall f1 {n})
                             (funcall 'neovm--combo-prop-sf-cap-target {n})
                             (apply f1 (list {n}))
                             (apply 'neovm--combo-prop-sf-cap-target (list {n})))))
                       (progn
                         (advice-remove 'neovm--combo-prop-sf-cap-target 'neovm--combo-prop-sf-cap-filter)
                         (let ((f2 (symbol-function 'neovm--combo-prop-sf-cap-target)))
                           (list
                             (eq f0 f2)
                             (funcall f2 {n})
                             (funcall 'neovm--combo-prop-sf-cap-target {n})
                             (apply f2 (list {n}))
                             (apply 'neovm--combo-prop-sf-cap-target (list {n}))))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-sf-cap-target 'neovm--combo-prop-sf-cap-filter)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-sf-cap-target)
                   (fmakunbound 'neovm--combo-prop-sf-cap-filter))))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_recursive_around_advice_call_path_consistency(
        n in 0i64..20i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-rec-around-call (x)
                 `(neovm--combo-prop-rec-around-target ,x))
               (fset 'neovm--combo-prop-rec-around-target
                     (lambda (x) (* 10 x)))
               (fset 'neovm--combo-prop-rec-around
                     (lambda (orig x)
                       (if (= x 0)
                           (funcall orig x)
                         (+ 1 (funcall 'neovm--combo-prop-rec-around-target (1- x))))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-rec-around-target :around 'neovm--combo-prop-rec-around)
                     (list
                       (neovm--combo-prop-rec-around-call {n})
                       (eval '(neovm--combo-prop-rec-around-call {n}))
                       (funcall 'neovm--combo-prop-rec-around-target {n})
                       (apply 'neovm--combo-prop-rec-around-target (list {n}))
                       (funcall (symbol-function 'neovm--combo-prop-rec-around-target) {n})))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-rec-around-target 'neovm--combo-prop-rec-around)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-rec-around-target)
                 (fmakunbound 'neovm--combo-prop-rec-around)
                 (fmakunbound 'neovm--combo-prop-rec-around-call)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_advice_added_on_alias_removed_on_target_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-alias-cross-target (lambda (x) x))
               (defalias 'neovm--combo-prop-alias-cross 'neovm--combo-prop-alias-cross-target)
               (fset 'neovm--combo-prop-alias-cross-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (list
                     (progn
                       (advice-add 'neovm--combo-prop-alias-cross :filter-return 'neovm--combo-prop-alias-cross-filter)
                       (list
                         (funcall 'neovm--combo-prop-alias-cross {n})
                         (funcall 'neovm--combo-prop-alias-cross-target {n})
                         (neovm--combo-prop-alias-cross {n})
                         (eval '(neovm--combo-prop-alias-cross {n}))
                         (if (advice-member-p 'neovm--combo-prop-alias-cross-filter 'neovm--combo-prop-alias-cross) t nil)
                         (if (advice-member-p 'neovm--combo-prop-alias-cross-filter 'neovm--combo-prop-alias-cross-target) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-prop-alias-cross-target 'neovm--combo-prop-alias-cross-filter)
                       (list
                         (funcall 'neovm--combo-prop-alias-cross {n})
                         (funcall 'neovm--combo-prop-alias-cross-target {n})
                         (neovm--combo-prop-alias-cross {n})
                         (eval '(neovm--combo-prop-alias-cross {n}))
                         (if (advice-member-p 'neovm--combo-prop-alias-cross-filter 'neovm--combo-prop-alias-cross) t nil)
                         (if (advice-member-p 'neovm--combo-prop-alias-cross-filter 'neovm--combo-prop-alias-cross-target) t nil))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-alias-cross 'neovm--combo-prop-alias-cross-filter)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-alias-cross-target 'neovm--combo-prop-alias-cross-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-alias-cross)
                 (fmakunbound 'neovm--combo-prop-alias-cross-target)
                 (fmakunbound 'neovm--combo-prop-alias-cross-filter)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_advice_added_on_target_removed_on_alias_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-target-cross-target (lambda (x) x))
               (defalias 'neovm--combo-prop-target-cross 'neovm--combo-prop-target-cross-target)
               (fset 'neovm--combo-prop-target-cross-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (list
                     (progn
                       (advice-add 'neovm--combo-prop-target-cross-target :filter-return 'neovm--combo-prop-target-cross-filter)
                       (list
                         (funcall 'neovm--combo-prop-target-cross {n})
                         (funcall 'neovm--combo-prop-target-cross-target {n})
                         (neovm--combo-prop-target-cross {n})
                         (eval '(neovm--combo-prop-target-cross {n}))
                         (if (advice-member-p 'neovm--combo-prop-target-cross-filter 'neovm--combo-prop-target-cross) t nil)
                         (if (advice-member-p 'neovm--combo-prop-target-cross-filter 'neovm--combo-prop-target-cross-target) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-prop-target-cross 'neovm--combo-prop-target-cross-filter)
                       (list
                         (funcall 'neovm--combo-prop-target-cross {n})
                         (funcall 'neovm--combo-prop-target-cross-target {n})
                         (neovm--combo-prop-target-cross {n})
                         (eval '(neovm--combo-prop-target-cross {n}))
                         (if (advice-member-p 'neovm--combo-prop-target-cross-filter 'neovm--combo-prop-target-cross) t nil)
                         (if (advice-member-p 'neovm--combo-prop-target-cross-filter 'neovm--combo-prop-target-cross-target) t nil))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-target-cross-target 'neovm--combo-prop-target-cross-filter)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-target-cross 'neovm--combo-prop-target-cross-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-target-cross)
                 (fmakunbound 'neovm--combo-prop-target-cross-target)
                 (fmakunbound 'neovm--combo-prop-target-cross-filter)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_two_aliases_cross_advice_remove_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-two-alias-target (lambda (x) x))
               (defalias 'neovm--combo-prop-two-alias-a 'neovm--combo-prop-two-alias-target)
               (defalias 'neovm--combo-prop-two-alias-b 'neovm--combo-prop-two-alias-target)
               (fset 'neovm--combo-prop-two-alias-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (list
                     (progn
                       (advice-add 'neovm--combo-prop-two-alias-a :filter-return 'neovm--combo-prop-two-alias-filter)
                       (list
                         (funcall 'neovm--combo-prop-two-alias-a {n})
                         (funcall 'neovm--combo-prop-two-alias-b {n})
                         (funcall 'neovm--combo-prop-two-alias-target {n})
                         (if (advice-member-p 'neovm--combo-prop-two-alias-filter 'neovm--combo-prop-two-alias-a) t nil)
                         (if (advice-member-p 'neovm--combo-prop-two-alias-filter 'neovm--combo-prop-two-alias-b) t nil)
                         (if (advice-member-p 'neovm--combo-prop-two-alias-filter 'neovm--combo-prop-two-alias-target) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-prop-two-alias-b 'neovm--combo-prop-two-alias-filter)
                       (list
                         (funcall 'neovm--combo-prop-two-alias-a {n})
                         (funcall 'neovm--combo-prop-two-alias-b {n})
                         (funcall 'neovm--combo-prop-two-alias-target {n})
                         (if (advice-member-p 'neovm--combo-prop-two-alias-filter 'neovm--combo-prop-two-alias-a) t nil)
                         (if (advice-member-p 'neovm--combo-prop-two-alias-filter 'neovm--combo-prop-two-alias-b) t nil)
                         (if (advice-member-p 'neovm--combo-prop-two-alias-filter 'neovm--combo-prop-two-alias-target) t nil))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-two-alias-a 'neovm--combo-prop-two-alias-filter)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-two-alias-b 'neovm--combo-prop-two-alias-filter)
                   (error nil))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-two-alias-target 'neovm--combo-prop-two-alias-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-two-alias-target)
                 (fmakunbound 'neovm--combo-prop-two-alias-a)
                 (fmakunbound 'neovm--combo-prop-two-alias-b)
                 (fmakunbound 'neovm--combo-prop-two-alias-filter)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_duplicate_advice_add_remove_lifecycle_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-dup-target (lambda (x) x))
               (fset 'neovm--combo-prop-dup-filter (lambda (ret) (+ ret 1)))
               (unwind-protect
                   (list
                     (progn
                       (advice-add 'neovm--combo-prop-dup-target :filter-return 'neovm--combo-prop-dup-filter)
                       (list
                         (funcall 'neovm--combo-prop-dup-target {n})
                         (if (advice-member-p 'neovm--combo-prop-dup-filter 'neovm--combo-prop-dup-target) t nil)))
                     (progn
                       (advice-add 'neovm--combo-prop-dup-target :filter-return 'neovm--combo-prop-dup-filter)
                       (list
                         (funcall 'neovm--combo-prop-dup-target {n})
                         (if (advice-member-p 'neovm--combo-prop-dup-filter 'neovm--combo-prop-dup-target) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-prop-dup-target 'neovm--combo-prop-dup-filter)
                       (list
                         (funcall 'neovm--combo-prop-dup-target {n})
                         (if (advice-member-p 'neovm--combo-prop-dup-filter 'neovm--combo-prop-dup-target) t nil)))
                     (progn
                       (advice-remove 'neovm--combo-prop-dup-target 'neovm--combo-prop-dup-filter)
                       (list
                         (funcall 'neovm--combo-prop-dup-target {n})
                         (if (advice-member-p 'neovm--combo-prop-dup-filter 'neovm--combo-prop-dup-target) t nil))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-dup-target 'neovm--combo-prop-dup-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-dup-target)
                 (fmakunbound 'neovm--combo-prop-dup-filter)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_captured_advised_function_after_remove_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-cap-remove-target (lambda (x) x))
               (fset 'neovm--combo-prop-cap-remove-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-prop-cap-remove-target :filter-return 'neovm--combo-prop-cap-remove-filter)
                     (let ((f1 (symbol-function 'neovm--combo-prop-cap-remove-target)))
                       (list
                         (funcall f1 {n})
                         (funcall 'neovm--combo-prop-cap-remove-target {n})
                         (progn
                           (advice-remove 'neovm--combo-prop-cap-remove-target 'neovm--combo-prop-cap-remove-filter)
                           (list
                             (funcall f1 {n})
                             (funcall 'neovm--combo-prop-cap-remove-target {n})
                             (apply f1 (list {n}))
                             (apply 'neovm--combo-prop-cap-remove-target (list {n}))
                             (eq (symbol-function 'neovm--combo-prop-cap-remove-target) f1))))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-cap-remove-target 'neovm--combo-prop-cap-remove-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-cap-remove-target)
                 (fmakunbound 'neovm--combo-prop-cap-remove-filter)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_eval_advice_toggle_call_path_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-eval-toggle-call (x)
                 `(neovm--combo-prop-eval-toggle-target ,x))
               (fset 'neovm--combo-prop-eval-toggle-target (lambda (x) x))
               (fset 'neovm--combo-prop-eval-toggle-filter (lambda (ret) (+ ret 7)))
               (unwind-protect
                   (list
                     (eval '(neovm--combo-prop-eval-toggle-call {n}))
                     (progn
                       (advice-add 'neovm--combo-prop-eval-toggle-target :filter-return 'neovm--combo-prop-eval-toggle-filter)
                       (list
                         (eval '(neovm--combo-prop-eval-toggle-call {n}))
                         (neovm--combo-prop-eval-toggle-call {n})
                         (funcall 'neovm--combo-prop-eval-toggle-target {n})
                         (apply 'neovm--combo-prop-eval-toggle-target (list {n}))))
                     (progn
                       (advice-remove 'neovm--combo-prop-eval-toggle-target 'neovm--combo-prop-eval-toggle-filter)
                       (list
                         (eval '(neovm--combo-prop-eval-toggle-call {n}))
                         (neovm--combo-prop-eval-toggle-call {n})
                         (funcall 'neovm--combo-prop-eval-toggle-target {n})
                         (apply 'neovm--combo-prop-eval-toggle-target (list {n})))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-eval-toggle-target 'neovm--combo-prop-eval-toggle-filter)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-eval-toggle-target)
                 (fmakunbound 'neovm--combo-prop-eval-toggle-filter)
                 (fmakunbound 'neovm--combo-prop-eval-toggle-call)))",
            n = n,
        );

        let expected = format!(
            "({n} ({a} {a} {a} {a}) ({n} {n} {n} {n}))",
            n = n,
            a = n + 7
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_alias_symbol_function_snapshot_rebind_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-prop-alias-snap-target-a (lambda (x) (+ x 1)))
               (fset 'neovm--combo-prop-alias-snap-target-b (lambda (x) (* 2 x)))
               (defalias 'neovm--combo-prop-alias-snap 'neovm--combo-prop-alias-snap-target-a)
               (fset 'neovm--combo-prop-alias-snap-filter (lambda (ret) (+ ret 100)))
               (let ((f0 (symbol-function 'neovm--combo-prop-alias-snap)))
                 (unwind-protect
                     (progn
                       (advice-add 'neovm--combo-prop-alias-snap :filter-return 'neovm--combo-prop-alias-snap-filter)
                       (let ((f1 (symbol-function 'neovm--combo-prop-alias-snap)))
                         (list
                           (eq f0 f1)
                           (funcall f0 {n})
                           (funcall f1 {n})
                           (funcall 'neovm--combo-prop-alias-snap {n})
                           (progn
                             (defalias 'neovm--combo-prop-alias-snap 'neovm--combo-prop-alias-snap-target-b)
                             (list
                               (funcall f0 {n})
                               (funcall f1 {n})
                               (funcall (symbol-function 'neovm--combo-prop-alias-snap) {n})
                               (funcall 'neovm--combo-prop-alias-snap {n})
                               (apply 'neovm--combo-prop-alias-snap (list {n}))))
                           (progn
                             (advice-remove 'neovm--combo-prop-alias-snap 'neovm--combo-prop-alias-snap-filter)
                             (list
                               (funcall 'neovm--combo-prop-alias-snap {n})
                               (funcall (symbol-function 'neovm--combo-prop-alias-snap) {n}))))))
                   (condition-case nil
                       (advice-remove 'neovm--combo-prop-alias-snap 'neovm--combo-prop-alias-snap-filter)
                     (error nil))
                   (fmakunbound 'neovm--combo-prop-alias-snap)
                   (fmakunbound 'neovm--combo-prop-alias-snap-target-a)
                   (fmakunbound 'neovm--combo-prop-alias-snap-target-b)
                   (fmakunbound 'neovm--combo-prop-alias-snap-filter))))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_throw_caught_by_around_toggle_consistency(
        n in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-prop-throw-around-call (x)
                 `(neovm--combo-prop-throw-around-target ,x))
               (fset 'neovm--combo-prop-throw-around-target
                     (lambda (x)
                       (throw 'neovm--combo-prop-throw-around-tag x)))
               (fset 'neovm--combo-prop-throw-around
                     (lambda (orig x)
                       (+ 100
                          (catch 'neovm--combo-prop-throw-around-tag
                            (funcall orig x)))))
               (unwind-protect
                   (list
                     (progn
                       (advice-add 'neovm--combo-prop-throw-around-target :around 'neovm--combo-prop-throw-around)
                       (list
                         (catch 'neovm--combo-prop-throw-around-tag
                           (neovm--combo-prop-throw-around-call {n}))
                         (catch 'neovm--combo-prop-throw-around-tag
                           (eval '(neovm--combo-prop-throw-around-call {n})))
                         (catch 'neovm--combo-prop-throw-around-tag
                           (funcall 'neovm--combo-prop-throw-around-target {n}))
                         (catch 'neovm--combo-prop-throw-around-tag
                           (apply 'neovm--combo-prop-throw-around-target (list {n})))))
                     (progn
                       (advice-remove 'neovm--combo-prop-throw-around-target 'neovm--combo-prop-throw-around)
                       (list
                         (catch 'neovm--combo-prop-throw-around-tag
                           (neovm--combo-prop-throw-around-call {n}))
                         (catch 'neovm--combo-prop-throw-around-tag
                           (eval '(neovm--combo-prop-throw-around-call {n})))
                         (catch 'neovm--combo-prop-throw-around-tag
                           (funcall 'neovm--combo-prop-throw-around-target {n}))
                         (catch 'neovm--combo-prop-throw-around-tag
                           (apply 'neovm--combo-prop-throw-around-target (list {n}))))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-prop-throw-around-target 'neovm--combo-prop-throw-around)
                   (error nil))
                 (fmakunbound 'neovm--combo-prop-throw-around-target)
                 (fmakunbound 'neovm--combo-prop-throw-around)
                 (fmakunbound 'neovm--combo-prop-throw-around-call)))",
            n = n,
        );

        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        prop_assert_eq!(oracle, neovm);
    }

    #[test]
    fn oracle_prop_combination_filter_args_call_path_matrix_consistency(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (fset 'neovm--combo-fargs-prop-target (lambda (x y) (+ x y)))
               (fset 'neovm--combo-fargs-prop
                     (lambda (args)
                       (list (+ {a} (car args))
                             (+ {b} (car (cdr args))))))
               (unwind-protect
                   (progn
                     (advice-add 'neovm--combo-fargs-prop-target :filter-args 'neovm--combo-fargs-prop)
                     (list
                       (funcall 'neovm--combo-fargs-prop-target 3 4)
                       (apply 'neovm--combo-fargs-prop-target '(3 4))
                       (neovm--combo-fargs-prop-target 3 4)
                       (eval '(neovm--combo-fargs-prop-target 3 4))))
                 (condition-case nil
                     (advice-remove 'neovm--combo-fargs-prop-target 'neovm--combo-fargs-prop)
                   (error nil))
                 (fmakunbound 'neovm--combo-fargs-prop-target)
                 (fmakunbound 'neovm--combo-fargs-prop)))",
            a = a,
            b = b,
        );

        let expected = (a + 3) + (b + 4);
        let expected_payload = format!("({expected} {expected} {expected} {expected})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected_payload.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_error_or_throw_with_cleanup_state(
        a in -10_000i64..10_000i64,
        b in -10_000i64..10_000i64,
        c in -10_000i64..10_000i64,
        d in -10_000i64..10_000i64,
        signal_arith in any::<bool>(),
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let flow = if signal_arith {
            "(/ 1 0)"
        } else {
            "(throw 'neovm--combo-tag (+ x C))"
        };
        let form = format!(
            "(let ((x {a}))
               (list
                 (condition-case _err
                     (catch 'neovm--combo-tag
                       (unwind-protect
                           (progn
                             (setq x (+ x {b}))
                             {flow})
                         (setq x (+ x {d}))))
                   (arith-error 'arith))
                 x))",
            a = a,
            b = b,
            d = d,
            flow = flow.replace("C", &c.to_string()),
        );

        let x_after = a + b + d;
        let first = if signal_arith {
            "arith".to_string()
        } else {
            (a + b + c).to_string()
        };
        let expected = format!("({first} {x_after})");
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_parameterized_tag_value_roundtrip(
        v in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-catch-throw-param (tag value)
                 `(catch ,tag (throw ,tag ,value)))
               (unwind-protect
                   (neovm--combo-catch-throw-param 'neovm--combo-prop-tag {})
                 (fmakunbound 'neovm--combo-catch-throw-param)))",
            v
        );
        let expected = v.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_tag_condition_case_roundtrip(
        v in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-with-tag-cc (tag value)
                 `(catch ,tag
                    (condition-case err
                        (throw ,tag ,value)
                      (error (list 'err (car err))))))
               (unwind-protect
                   (neovm--combo-with-tag-cc 'neovm--combo-prop-cc {})
                 (fmakunbound 'neovm--combo-with-tag-cc)))",
            v
        );
        let expected = v.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_macro_wrapped_condition_case_throw_roundtrip(
        v in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(progn
               (defmacro neovm--combo-cc-throw (tag value)
                 `(condition-case err
                      (throw ,tag ,value)
                    (error (list 'err (car err)))))
               (unwind-protect
                   (let ((tag 'neovm--combo-prop-dyn-tag))
                     (catch tag
                       (neovm--combo-cc-throw tag {})))
                 (fmakunbound 'neovm--combo-cc-throw)))",
            v
        );
        let expected = v.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_throw_through_condition_case_roundtrip(
        v in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(let ((tag 'neovm--combo-prop-cc-pass-tag))
               (catch tag
                 (condition-case nil
                     (progn (throw tag {}) 'tail)
                   (arith-error 'arith)
                   (wrong-type-argument 'wta))))",
            v
        );
        let expected = v.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_throw_not_caught_by_condition_case_roundtrip(
        v in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(catch 'neovm--combo-cc-prop-tag
               (condition-case nil
                   (throw 'neovm--combo-cc-prop-tag {})
                 (error 'caught)))",
            v
        );
        let expected = v.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_integer_tag_throw_roundtrip(
        tag in -1000i64..1000i64,
        value in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!("(catch {} (throw {} {}))", tag, tag, value);
        let expected = value.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_integer_tag_throw_through_condition_case_roundtrip(
        tag in -1000i64..1000i64,
        value in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(catch {}
               (condition-case nil
                   (throw {} {})
                 (error 'caught)))",
            tag, tag, value
        );
        let expected = value.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }

    #[test]
    fn oracle_prop_combination_throw_from_funcall_through_condition_case_roundtrip(
        value in -10_000i64..10_000i64,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let form = format!(
            "(catch 'neovm--combo-funcall-prop-tag
               (condition-case nil
                   (funcall (lambda () (throw 'neovm--combo-funcall-prop-tag {})))
                 (error 'caught)))",
            value
        );
        let expected = value.to_string();
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        assert_ok_eq(expected.as_str(), &oracle, &neovm);
    }
}
