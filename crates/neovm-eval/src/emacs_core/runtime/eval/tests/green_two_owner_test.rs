//! Thirteen Stage A scenarios on the real Green scheduler.
//!
//! Joins and a mutex/condition handshake determine every observation.
//! Each scenario has separate late/eager admission tests: nextest runs each
//! in a fresh process, as Green seals the process-wide native subr tables.
//! GNU31.1 expectations were captured in the PID sandbox; the mirrored
//! oracle family refreshes its expectations from GNU, never by hand.

use super::{eval, green};

fn run_case(form: &str, expected: &str, eager_admission: bool) {
    let mut ev = green();
    if eager_admission {
        ev.admit_threads().expect("eager admission");
    }
    assert_eq!(
        eval(&mut ev, form),
        expected,
        "eager admission: {eager_admission}"
    );
}

macro_rules! owner_scenario {
    ($name:ident, $form:literal, $expected:literal) => {
        mod $name {
            #[test]
            fn late_admission() {
                super::run_case($form, $expected, false);
            }
            #[test]
            fn eager_admission() {
                super::run_case($form, $expected, true);
            }
        }
    };
}

owner_scenario!(
    creator_let_and_unbound_peer_write,
    r##"(progn
 (defvar tg2-v 1)
 (list (let ((tg2-v 2))
         (list tg2-v (thread-join (make-thread (lambda () (list tg2-v (setq tg2-v 3))))) tg2-v))
       tg2-v))"##,
    r##"((2 (1 3) 2) 3)"##
);

owner_scenario!(
    nested_bindings_do_not_restore_stale_canonical_values,
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
    r##"(((2 5 2) (3 4 3) 2) 6)"##
);

owner_scenario!(
    void_binding_stays_void_while_an_unbound_peer_writes,
    r##"(progn
 (defvar tg2-v 1)
 (list (let ((tg2-v 2))
         (makunbound 'tg2-v)
         (list (boundp 'tg2-v)
               (thread-join (make-thread (lambda () (list tg2-v (setq tg2-v 3)))))
               (boundp 'tg2-v)))
       tg2-v
       (let (tg2-empty) (list tg2-empty (thread-join (make-thread (lambda () (boundp 'tg2-empty))))))))"##,
    r##"((nil (1 3) nil) 3 (nil nil))"##
);

owner_scenario!(
    aliases_select_the_binding_owners_resolved_base,
    r##"(progn
 (defvar tg2-v 1)
 (defvaralias 'tg2-alias 'tg2-v)
 (list (let ((tg2-alias 2))
         (list tg2-v (thread-join (make-thread (lambda () (list tg2-alias (setq tg2-alias 3))))) tg2-alias))
       tg2-v tg2-alias))"##,
    r##"((2 (1 3) 2) 3 3)"##
);

owner_scenario!(
    blv_local_and_default_bindings_are_independent,
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
    r##"((20 (10 1 t 11 (30 30)) 20 1) 11 1)"##
);

owner_scenario!(
    local_if_set_obeys_each_owners_let_shadow,
    r##"(progn
 (defvar tg2-v 1) (make-variable-buffer-local 'tg2-v)
 (list (let ((tg2-v 2))
         (setq tg2-v 3)
         (list tg2-v (local-variable-p 'tg2-v)
               (thread-join (make-thread (lambda ()
                 (list tg2-v (progn (setq tg2-v 4) tg2-v) (local-variable-p 'tg2-v)))))
               tg2-v (local-variable-p 'tg2-v)))
       tg2-v (default-value 'tg2-v) (local-variable-p 'tg2-v)))"##,
    r##"((3 nil (1 4 t) 4 t) 4 1 t)"##
);

owner_scenario!(
    own_kill_and_remake_uses_gnu_local_unwind,
    r##"(progn
 (defvar tg2-v 1) (set (make-local-variable 'tg2-v) 10)
 (list (let ((tg2-v 20))
         (kill-local-variable 'tg2-v)
         (set (make-local-variable 'tg2-v) 30)
         (list tg2-v (thread-join (make-thread (lambda () (list tg2-v (local-variable-p 'tg2-v))))) tg2-v))
       tg2-v (local-variable-p 'tg2-v)))"##,
    r##"((30 (10 t) 30) 10 t)"##
);

owner_scenario!(
    a_peer_kill_retires_the_parked_local_binding,
    r##"(progn
 (defvar tg2-v 1) (set (make-local-variable 'tg2-v) 10)
 (list (let ((tg2-v 20))
         (list (thread-join (make-thread (lambda () (kill-local-variable 'tg2-v) (list tg2-v (local-variable-p 'tg2-v)))))
               tg2-v (local-variable-p 'tg2-v)))
       tg2-v (local-variable-p 'tg2-v)))"##,
    r##"(((1 nil) 1 nil) 1 nil)"##
);

owner_scenario!(
    a_peer_kill_and_remake_follows_gnu_membership_semantics,
    r##"(progn
 (defvar tg2-v 1) (set (make-local-variable 'tg2-v) 10)
 (list (let ((tg2-v 20))
         (list (thread-join (make-thread (lambda ()
           (kill-local-variable 'tg2-v) (set (make-local-variable 'tg2-v) 30)
           (list tg2-v (local-variable-p 'tg2-v)))))
               tg2-v (local-variable-p 'tg2-v)))
       tg2-v (local-variable-p 'tg2-v)))"##,
    r##"(((30 t) 20 t) 30 t)"##
);

owner_scenario!(
    typed_forwarders_and_direct_buffer_slots_keep_owner_values,
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
    r##"((20 nil 3 (70 t nil typed-error 80) 20 nil 3) 80)"##
);

owner_scenario!(
    toplevel_access_reads_and_writes_the_shared_default,
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
    r##"((1 (3 1 nil 3) 2 4) 4)"##
);

owner_scenario!(
    watcher_created_threads_and_unlet_errors_follow_gnu,
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
    r##"(((let 2 1 1) (unlet 1 2 2)) unlet-error 3 3)"##
);

owner_scenario!(
    backtrace_eval_rewinds_owner_bindings_after_a_join,
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
    r##"(((3 (1 4) 3) 2 5 2) 5)"##
);
