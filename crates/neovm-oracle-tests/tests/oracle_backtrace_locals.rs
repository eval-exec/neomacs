//! GNU Fbacktrace__locals, including owner entries after Green admission.

use crate::common;

use common::return_if_neovm_enable_oracle_proptest_not_set;

const SIM: &[(&str, &str)] = &[("NEOVM_THREADS", "sim"), ("NEOVM_JIT", "0")];
const GREEN: &[(&str, &str)] = &[("NEOVM_THREADS", "green")];

#[test]
fn backtrace_locals_dynamic_arguments_and_original_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r#"(progn
 (thread-join (make-thread (lambda () nil)))
 (progn
 (defalias 'btl-dynamic (eval '(lambda (btl-a btl-b &optional btl-c &rest btl-rest)
   (backtrace--locals 1)) nil))
 (list (btl-dynamic 1 2 3 4 5) (boundp 'btl-a))))"#,
        &[SIM, GREEN],
        expect_test::expect![[
            r#""OK (((btl-a . 1) (btl-b . 2) (btl-c . 3) (btl-rest 4 5)) nil)""#
        ]],
    );
}

#[test]
fn backtrace_locals_lexical_arguments_and_special_declarations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r#"(progn
 (thread-join (make-thread (lambda () nil)))
 (progn
 (defvar btl-special 'canonical)
 (defalias 'btl-lexical (eval '(function (lambda (btl-a btl-special btl-b)
   (backtrace--locals 1))) t))
 (list (btl-lexical 1 'bound 2) btl-special)))"#,
        &[SIM, GREEN],
        expect_test::expect![[
            r#""OK (((btl-a . 1) (btl-special . bound) (btl-b . 2)) canonical)""#
        ]],
    );
}

#[test]
fn backtrace_locals_duplicate_let_bindings_and_void_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r#"(progn
 (thread-join (make-thread (lambda () nil)))
 (eval '(let ((btl-a 'outer) (btl-a 'inner) (btl-empty))
 (makunbound 'btl-empty)
 (backtrace--locals 1)) nil))"#,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK ((btl-a . outer) (btl-a . inner) (btl-empty . unbound))""#]],
    );
}

#[test]
fn backtrace_locals_lexical_let_environment_and_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r#"(progn
 (thread-join (make-thread (lambda () nil)))
 (progn
 (defalias 'btl-capture (lambda () (backtrace--locals 1 'btl-capture)))
 (defalias 'btl-outer (eval '(function (lambda (btl-a)
   (let ((btl-b 2) (btl-a 3)) (btl-capture)))) t))
 (btl-outer 1)))"#,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK ((btl-a . 1) (btl-b . 2) (btl-a . 3))""#]],
    );
}

#[test]
fn backtrace_locals_buffer_local_bindings_restore_storage() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r#"(progn
 (thread-join (make-thread (lambda () nil)))
 (progn
 (defvar btl-local 'global)
 (set (make-local-variable 'btl-local) 'local)
 (list (eval '(let ((btl-local 'bound)) (backtrace--locals 1)) nil)
       btl-local (default-value 'btl-local))))"#,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK (((btl-local . bound)) local global)""#]],
    );
}

#[test]
fn backtrace_locals_owner_entries_after_admission() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r#"(progn
 (defvar btl-owner 'global)
 (thread-join (make-thread (lambda () nil)))
 (list (eval '(let ((btl-owner 'bound))
   (backtrace--locals 1)) nil) btl-owner))"#,
        &[SIM, GREEN],
        expect_test::expect![[r#""OK (((btl-owner . bound)) global)""#]],
    );
}

#[test]
fn backtrace_locals_owner_entries_keep_the_original_alias_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r#"(progn
 (defvar btl-source 'source-global)
 (defvar btl-target 'target-global)
 (setq btl-m (make-mutex) btl-c (make-condition-variable btl-m)
       btl-ready nil btl-go nil)
 (mutex-lock btl-m)
 (setq btl-worker (make-thread (lambda ()
   (let ((btl-source 'bound))
     (mutex-lock btl-m)
     (setq btl-ready t)
     (condition-notify btl-c)
     (while (not btl-go) (condition-wait btl-c))
     (mutex-unlock btl-m)
     (backtrace--locals 1)))))
 (while (not btl-ready) (condition-wait btl-c))
 (defvaralias 'btl-source 'btl-target)
 (setq btl-go t)
 (condition-notify btl-c)
 (mutex-unlock btl-m)
 (list (thread-join btl-worker) btl-source btl-target))"#,
        &[GREEN],
        expect_test::expect![[r#""OK (((btl-source . bound)) target-global target-global)""#]],
    );
}

#[test]
fn backtrace_locals_void_binding_exposes_the_real_unbound_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r#"(progn
 (thread-join (make-thread (lambda () nil)))
 (setq btl-snapshot
   (eval '(let ((btl-empty 1) (btl-other 2))
     (makunbound 'btl-empty) (makunbound 'btl-other)
     (backtrace--locals 1)) nil))
 (list (symbolp (cdar btl-snapshot))
       (type-of (cdar btl-snapshot))
       (cl-type-of (cdar btl-snapshot))
       (symbol-name (cdar btl-snapshot))
       (eq (cdar btl-snapshot) 'unbound)
       (eq (cdar btl-snapshot) (cdadr btl-snapshot))
       (let ((print-gensym nil)) (prin1-to-string (cdar btl-snapshot)))
       (let ((print-gensym t)) (prin1-to-string (cdar btl-snapshot)))
       (let ((print-gensym t) (print-circle t))
         (prin1-to-string (list (cdar btl-snapshot) (cdadr btl-snapshot))))))"#,
        &[SIM, GREEN],
        expect_test::expect![[
            r##""OK (t symbol symbol \"unbound\" nil t \"unbound\" \"#:unbound\" \"(#1=#:unbound #1#)\")""##
        ]],
    );
}
