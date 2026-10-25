//! GNU 31.1 locks for the Stage A two-owner binding scenarios.
//!
//! Joins and a mutex/condition handshake keep the owners' bindings live
//! without relying on OS scheduling. Refresh only with the GNU oracle and PID sandbox:
//! NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1.

use crate::common;

use common::return_if_neovm_enable_oracle_proptest_not_set;

const GREEN_THREADS: &[(&str, &str)] = &[("NEOVM_THREADS", "green")];

#[test]
fn green_two_owner_creator_let_and_unbound_peer_write() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1)
 (list (let ((tg2-v 2))
         (list tg2-v (thread-join (make-thread (lambda () (list tg2-v (setq tg2-v 3))))) tg2-v))
       tg2-v))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK ((2 (1 3) 2) 3)""#]],
    );
}

#[test]
fn green_two_owner_nested_bindings_do_not_restore_stale_canonical_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1)
 (setq tg2-m (make-mutex) tg2-c (make-condition-variable tg2-m) tg2-phase 0)
 (list (let ((tg2-v 2))
         (mutex-lock tg2-m)
         (setq tg2-worker (make-thread (lambda ()
           (let ((tg2-v 3))
             (mutex-lock tg2-m)
             (setq tg2-phase 1)
             (condition-notify tg2-c t)
             (while (= tg2-phase 1) (condition-wait tg2-c))
             (setq tg2-worker-result (list tg2-v (let ((tg2-v 4)) tg2-v) tg2-v))
             (set-default-toplevel-value 'tg2-v 6)
             (mutex-unlock tg2-m)
             tg2-worker-result))))
         (while (= tg2-phase 0) (condition-wait tg2-c))
         (setq tg2-main-result (list tg2-v (let ((tg2-v 5)) tg2-v) tg2-v))
         (setq tg2-phase 2)
         (condition-notify tg2-c t)
         (mutex-unlock tg2-m)
         (list tg2-main-result (thread-join tg2-worker) tg2-v))
       tg2-v))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK (((2 5 2) (3 4 3) 2) 6)""#]],
    );
}

#[test]
fn green_two_owner_void_binding_stays_void_while_an_unbound_peer_writes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1)
 (list (let ((tg2-v 2))
         (makunbound 'tg2-v)
         (list (boundp 'tg2-v)
               (thread-join (make-thread (lambda () (list tg2-v (setq tg2-v 3)))))
               (boundp 'tg2-v)))
       tg2-v
       (let (tg2-empty) (list tg2-empty (thread-join (make-thread (lambda () (boundp 'tg2-empty))))))))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK ((nil (1 3) nil) 3 (nil nil))""#]],
    );
}

#[test]
fn green_two_owner_aliases_select_the_binding_owners_resolved_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1)
 (defvaralias 'tg2-alias 'tg2-v)
 (list (let ((tg2-alias 2))
         (list tg2-v (thread-join (make-thread (lambda () (list tg2-alias (setq tg2-alias 3))))) tg2-alias))
       tg2-v tg2-alias))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK ((2 (1 3) 2) 3 3)""#]],
    );
}

#[test]
fn green_two_owner_blv_local_and_default_bindings_are_independent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1)
 (setq tg2-a (get-buffer-create "tg2-a") tg2-b (get-buffer-create "tg2-b"))
 (set-buffer tg2-a) (set (make-local-variable 'tg2-v) 10)
 (list (let ((tg2-v 20))
         (list tg2-v
               (thread-join (make-thread (lambda ()
                 (list tg2-v (default-value 'tg2-v) (local-variable-p 'tg2-v)
                       (progn (setq tg2-v 11) tg2-v)
                       (progn (set-buffer tg2-b)
                              (let ((tg2-v 30)) (list tg2-v (default-value 'tg2-v))))))))
               tg2-v (default-value 'tg2-v)))
       tg2-v (default-value 'tg2-v)))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK ((20 (10 1 t 11 (30 30)) 20 1) 11 1)""#]],
    );
}

#[test]
fn green_two_owner_local_if_set_obeys_each_owners_let_shadow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1) (make-variable-buffer-local 'tg2-v)
 (list (let ((tg2-v 2))
         (setq tg2-v 3)
         (list tg2-v (local-variable-p 'tg2-v)
               (thread-join (make-thread (lambda ()
                 (list tg2-v (progn (setq tg2-v 4) tg2-v) (local-variable-p 'tg2-v)))))
               tg2-v (local-variable-p 'tg2-v)))
       tg2-v (default-value 'tg2-v) (local-variable-p 'tg2-v)))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK ((3 nil (1 4 t) 4 t) 4 1 t)""#]],
    );
}

#[test]
fn green_two_owner_own_kill_and_remake_uses_gnu_local_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1) (set (make-local-variable 'tg2-v) 10)
 (list (let ((tg2-v 20))
         (kill-local-variable 'tg2-v)
         (set (make-local-variable 'tg2-v) 30)
         (list tg2-v (thread-join (make-thread (lambda () (list tg2-v (local-variable-p 'tg2-v))))) tg2-v))
       tg2-v (local-variable-p 'tg2-v)))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK ((30 (10 t) 30) 10 t)""#]],
    );
}

#[test]
fn green_two_owner_a_peer_kill_retires_the_parked_local_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1) (set (make-local-variable 'tg2-v) 10)
 (list (let ((tg2-v 20))
         (list (thread-join (make-thread (lambda () (kill-local-variable 'tg2-v) (list tg2-v (local-variable-p 'tg2-v)))))
               tg2-v (local-variable-p 'tg2-v)))
       tg2-v (local-variable-p 'tg2-v)))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK (((1 nil) 1 nil) 1 nil)""#]],
    );
}

#[test]
fn green_two_owner_a_peer_kill_and_remake_follows_gnu_membership_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1) (set (make-local-variable 'tg2-v) 10)
 (list (let ((tg2-v 20))
         (list (thread-join (make-thread (lambda ()
           (kill-local-variable 'tg2-v) (set (make-local-variable 'tg2-v) 30)
           (list tg2-v (local-variable-p 'tg2-v)))))
               tg2-v (local-variable-p 'tg2-v)))
       tg2-v (local-variable-p 'tg2-v)))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK (((30 t) 20 t) 30 t)""#]],
    );
}

#[test]
fn green_two_owner_typed_forwarders_and_direct_buffer_slots_keep_owner_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (setq fill-column 70)
 (list (let ((fill-column 20) (case-fold-search nil) (print-length 3))
         (list fill-column case-fold-search print-length
               (thread-join (make-thread (lambda ()
                 (list fill-column case-fold-search print-length
                       (condition-case nil (setq fill-column 'wrong) (wrong-type-argument 'typed-error))
                       (progn (setq fill-column 80) fill-column)))))
               fill-column case-fold-search print-length))
       fill-column))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK ((20 nil 3 (70 t nil typed-error 80) 20 nil 3) 80)""#]],
    );
}

#[test]
fn green_two_owner_toplevel_access_reads_and_writes_the_shared_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1)
 (list (let ((tg2-v 2))
         (list (default-toplevel-value 'tg2-v)
               (thread-join (make-thread (lambda ()
                 (let ((tg2-v 3))
                   (list tg2-v (default-toplevel-value 'tg2-v)
                         (set-default-toplevel-value 'tg2-v 4) tg2-v)))))
               tg2-v (default-toplevel-value 'tg2-v)))
       tg2-v))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK ((1 (3 1 nil 3) 2 4) 4)""#]],
    );
}

#[test]
fn green_two_owner_watcher_created_threads_and_unlet_errors_follow_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (defvar tg2-v 1) (setq tg2-log nil)
 (fset 'tg2-watch (lambda (_sym new op _where)
   (if (memq op '(let unlet))
       (setq tg2-log (cons (list op new tg2-v (thread-join (make-thread (lambda () tg2-v)))) tg2-log)))))
 (add-variable-watcher 'tg2-v 'tg2-watch)
 (let ((tg2-v 2)) tg2-v)
 (remove-variable-watcher 'tg2-v 'tg2-watch)
 (fset 'tg2-error (lambda (_sym _new op _where) (if (eq op 'unlet) (error "tg2 unlet"))))
 (add-variable-watcher 'tg2-v 'tg2-error)
 (setq tg2-error-result (condition-case nil (let ((tg2-v 3)) tg2-v) (error 'unlet-error)))
 (remove-variable-watcher 'tg2-v 'tg2-error)
 (list (nreverse tg2-log) tg2-error-result tg2-v (thread-join (make-thread (lambda () tg2-v)))))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK (((let 2 1 1) (unlet 1 2 2)) unlet-error 3 3)""#]],
    );
}

#[test]
fn green_two_owner_backtrace_eval_rewinds_owner_bindings_after_a_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"(progn
 (thread-join (make-thread (lambda () nil)))
 (defvar tg2-v 1)
 (fset 'tg2-inner (lambda ()
   (let ((tg2-v 3))
     (list tg2-v (backtrace-eval '(list tg2-v (setq tg2-v 4)) 0 'tg2-outer) tg2-v))))
 (fset 'tg2-outer (lambda ()
   (let ((tg2-v 2))
     (list (tg2-inner) tg2-v (thread-join (make-thread (lambda () (setq tg2-v 5)))) tg2-v))))
 (list (tg2-outer) tg2-v))"##,
        &[GREEN_THREADS],
        expect_test::expect![[r#""OK (((3 (1 4) 3) 2 5 2) 5)""#]],
    );
}
