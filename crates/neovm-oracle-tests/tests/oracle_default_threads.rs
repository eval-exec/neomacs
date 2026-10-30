//! GNU-oracle locks for the DEFAULT `make-thread' dispatch (E8-S, package
//! P2): every case runs GNU once, then Neomacs twice — once under today's
//! default simulated threads (`NEOVM_THREADS' unset) and once with
//! `NEOVM_THREADS=green'.  The GNU contract is DESIGN §2 (pentry), seeded by
//! `tmp/wt-peer-pentry/tmp/pentry/oracle/{apis,edges,main-event}.el'; the
//! shapes here are those seeds re-targeted through the default dispatch.
//!
//! This is the LANDING RANGE of the suite: first the red-0 cases both modes
//! passed on the landing binaries — main-thread identity, a worker's join
//! value, name and liveness, and join-self plus a worker's internal error
//! through `thread-last-error` — then the two divergences E8-S found and the
//! shared runtime now fixes: `thread-name` identity (`make-thread` stores the
//! NAME object, so `(eq name (thread-name th))` is t in both modes) and
//! `thread-signal` to the main thread arriving as GNU's `THREAD_EVENT`,
//! dispatched through `special-event-map` to `thread-handle-event`.  The
//! rest of the 19-case matrix (mutex handoff, condition-wait, the
//! blocking-I/O Baton, buffer dispositions) lives on the sibling red branch
//! `v10x/gapsurvey-threads-oracle', where the remaining divergences are
//! recorded as failing snapshots; nothing here is ignored, cfg-hidden or
//! weakened.
//!
//! GNU picks which thread runs after a `thread-yield' by OS scheduling, so
//! every form reaches its answer through joins, never through one
//! particular interleaving.
//!
//! Refresh empty expectations from GNU 31.1 with
//! `NEOVM_ORACLE_MODE=refresh UPDATE_EXPECT=1'; never hand-write answers.
//! Run this generated Lisp only through the mandatory PID sandbox.

use crate::common;

use common::return_if_neovm_enable_oracle_proptest_not_set;

/// The two dispatch modes under test: today's default (simulated threads,
/// `NEOVM_THREADS` unset) and the Green runtime.
const THREAD_MODES: &[&[(&str, &str)]] = &[&[], &[("NEOVM_THREADS", "green")]];

fn assert_default_threads_oracle(form: &str, expected: expect_test::Expect) {
    common::assert_oracle_parity_under_envs_expect(form, THREAD_MODES, expected);
}

#[test]
fn e8s_make_thread_arity_and_main_thread_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(list (func-arity 'make-thread)
      (threadp main-thread) (eq main-thread (current-thread))
      (thread-live-p main-thread) (thread-name main-thread)
      (length (all-threads)) (eq (car (all-threads)) main-thread))
"##,
        expect_test::expect![[r#""OK ((1 . 3) t t t nil 1 t)""#]],
    );
}

#[test]
fn e8s_make_thread_join_value_name_and_liveness() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(let* ((name (copy-sequence "e8s-receipt"))
       (th (make-thread (lambda ()
                          (sleep-for 0.3)
                          (list 'result (eq (current-thread) main-thread)))
                        name)))
  (list (thread-live-p th) (string= name (thread-name th))
        (and (memq th (all-threads)) t)
        (and (memq main-thread (all-threads)) t)
        (thread-join th) (thread-join th)
        (thread-live-p th) (memq th (all-threads))))
"##,
        expect_test::expect![[r#""OK (t t t t (result nil) (result nil) nil nil)""#]],
    );
}

#[test]
fn e8s_thread_name_returns_the_stored_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(let* ((name (copy-sequence "e8s-stored"))
       (th (make-thread (lambda () nil) name)))
  (list (eq name (thread-name th)) (eq name (thread-name th))))
"##,
        expect_test::expect![[r#""OK (t t)""#]],
    );
}

#[test]
fn e8s_thread_join_self_and_internal_error_last_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(progn
  (thread-last-error t)
  (let ((th (make-thread (lambda () (error "inside")) "e8s-internal")))
    (list (condition-case e (thread-join (current-thread))
              (error (list 'join-self (car (cdr e)))))
          (thread-join th)
          (thread-last-error)
          (thread-last-error t)
          (thread-last-error))))
"##,
        expect_test::expect![[
            r#""OK ((join-self \"Cannot join current thread\") nil (error \"inside\") (error \"inside\") nil)""#
        ]],
    );
}

#[test]
fn e8s_thread_signal_to_main_is_a_thread_event() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(progn
  (require 'thread)
  (require 'cl-lib)
  (let ((seen nil) (messages nil)
        (old (lookup-key special-event-map [thread-event])))
    (unwind-protect
        (progn
          (define-key special-event-map [thread-event]
            (lambda (event)
              (interactive "e")
              (setq seen (list (eq (car event) 'thread-event)
                               (thread-name (nth 1 event))
                               (nth 2 event) (nth 3 event)))
              (cl-letf (((symbol-function 'message)
                         (lambda (fmt &rest args)
                           (setq messages (list fmt (cdr args))))))
                (thread-handle-event event))))
          (thread-join (make-thread
                        (lambda ()
                          (thread-signal main-thread 'error '("to-main"))
                          'sent)
                        "e8s-sender"))
          ;; read-event dispatches special events; the time bound terminates it.
          (list (read-event nil nil 0.1) seen messages))
      (define-key special-event-map [thread-event] old))))
"##,
        expect_test::expect![[
            r#""OK (nil (t \"e8s-sender\" error (\"to-main\")) (\"Error %s: %S\" ((error (\"to-main\")))))""#
        ]],
    );
}
