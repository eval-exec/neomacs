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
//! dispatched through `special-event-map` to `thread-handle-event`.
//!
//! Below that landing range, this commit adds the rows that match GNU in both
//! dispatch modes: signal to a worker, `thread-yield`, mutex handoff,
//! condition notify, and the dynamic and buffer-local binding rows.
//!
//! GNU picks which thread runs after a `thread-yield' by OS scheduling, so
//! every form reaches its answer through joins, mutexes, conditions, or a
//! flag, never through one particular interleaving.  A simulated worker only
//! makes progress inside `make-thread` or at a release point that resumes it.
//! The waits in these rows are bounded.
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

#[test]
fn e8s_thread_signal_to_worker_pending_at_join_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(let* ((m (make-mutex)) (ready nil)
       (th (make-thread (lambda ()
                          (setq ready t)
                          (mutex-lock m)
                          (mutex-unlock m))
                        "e8s-target")))
  (mutex-lock m)
  (while (not ready) (thread-yield))
  ;; No yield between setting the pending signal and join's entry snapshot.
  (list (thread-signal th 'error '("external"))
        (condition-case e (thread-join th) (error (list 'join-raised (car (cdr e)))))
        (mutex-unlock m)
        (condition-case e (thread-join th) (error (list 'join2-raised (car (cdr e)))))
        (thread-last-error t)))
"##,
        expect_test::expect![[
            r#""OK (nil (join-raised \"external\") nil nil (error \"external\"))""#
        ]],
    );
}

#[test]
fn e8s_thread_yield_in_main_and_worker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(list (thread-yield)
      (thread-join (make-thread (lambda () (thread-yield) 'after-yield)
                                "e8s-yielder")))
"##,
        expect_test::expect![[r#""OK (nil after-yield)""#]],
    );
}

#[test]
fn e8s_mutex_recursion_and_contention_handoff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(let ((m (make-mutex "e8s-m")))
  (mutex-lock m)
  (let ((th (make-thread (lambda ()
                           (mutex-lock m)
                           (mutex-unlock m)
                           'worker-got)
                         "e8s-contender")))
    ;; Recursive acquisition while the contender waits; it can only take the
    ;; mutex after BOTH levels are released.
    (mutex-lock m)
    (list (mutex-unlock m)
          (mutex-unlock m)
          (thread-join th)
          (condition-case e (mutex-unlock m)
            (error (list 'unowned (car (cdr e))))))))
"##,
        expect_test::expect![[
            r#""OK (nil nil worker-got (unowned \"Cannot unlock mutex owned by another thread\"))""#
        ]],
    );
}

#[test]
fn e8s_condition_notify_all_wakes_every_waiter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(let* ((m (make-mutex)) (c (make-condition-variable m)) (ready 0) (go nil)
       (worker (lambda ()
                 (with-mutex m
                   (setq ready (1+ ready))
                   (condition-notify c t)
                   (let ((rounds 0))
                     (while (and (not go) (< rounds 200000))
                       (setq rounds (1+ rounds))
                       (condition-wait c))
                     (list 'woke (> rounds 199000))))))
       (a (make-thread worker "e8s-broadcast-a"))
       (b (make-thread worker "e8s-broadcast-b")))
  (with-mutex m
    (while (< ready 2) (condition-wait c))
    (setq go t)
    (condition-notify c t))
  (list (thread-join a) (thread-join b)))
"##,
        expect_test::expect![[r#""OK ((woke nil) (woke nil))""#]],
    );
}

#[test]
fn e8s_condition_notify_one_wakes_exactly_one_waiter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(let* ((m (make-mutex))
       (c (make-condition-variable m))
       (gate (make-condition-variable m))
       (ready 0)
       (go nil)
       (returns 0)
       (worker (lambda ()
                 (with-mutex m
                   (setq ready (1+ ready))
                   (condition-notify gate t)
                   (let ((rounds 0))
                     (while (and (not go) (< rounds 200000))
                       (condition-wait c)
                       ;; Count every return, including a wake that loops
                       ;; because `go` is still nil. Readiness uses `gate`,
                       ;; so this count is wakes of `c` only.
                       (setq returns (1+ returns))
                       (setq rounds (1+ rounds)))))))
       (a (make-thread worker "e8s-one"))
       (b (make-thread worker "e8s-two"))
       (polls 0))
  (with-mutex m
    (while (< ready 2) (condition-wait gate)))
  (with-mutex m (condition-notify c))
  ;; `returns` counts condition-wait returns on `c`. One notify wakes one
  ;; waiter. The poll is bounded so a mode that cannot park still ends.
  (while (and (< returns 1) (< polls 100000))
    (setq polls (1+ polls))
    (thread-yield))
  (let ((after-one returns))
    (with-mutex m (setq go t) (condition-notify c t))
    (list after-one (thread-join a) (thread-join b) returns)))
"##,
        expect_test::expect![[r#""OK (1 nil nil 3)""#]],
    );
}

#[test]
fn e8s_binding_child_does_not_see_parent_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(progn
  (defvar e8s-special 'global)
  (defun e8s-read-special () e8s-special)
  (let ((e8s-special 'parent-let))
    (list e8s-special
          (thread-join
           (make-thread
            (lambda ()
              (list e8s-special (e8s-read-special)
                    (default-value 'e8s-special)))
            "e8s-bind-child"))
          e8s-special)))
"##,
        expect_test::expect![[r#""OK (parent-let (global global global) parent-let)""#]],
    );
}

/// The worker's `let` is live across `set-default` from main. The wait is
/// bounded so a mode that cannot park still returns; GNU hits the notify.
#[test]
fn e8s_binding_worker_let_and_set_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(progn
  (defvar e8s-special 'global)
  (defun e8s-read-special () e8s-special)
  (let* ((m (make-mutex))
         (c (make-condition-variable m))
         (ready nil)
         (go nil)
         (seen nil)
         (th (make-thread
              (lambda ()
                (let ((e8s-special 'worker-let))
                  (with-mutex m
                    (setq ready t)
                    (condition-notify c t)
                    (let ((rounds 0))
                      (while (and (not go) (< rounds 200000))
                        (setq rounds (1+ rounds))
                        (condition-wait c))))
                  (setq seen (list e8s-special (e8s-read-special)
                                   (default-value 'e8s-special)
                                   go))))
              "e8s-bind-worker")))
    (with-mutex m
      (let ((rounds 0))
        (while (and (not ready) (< rounds 200000))
          (setq rounds (1+ rounds))
          (condition-wait c))))
    (let ((other (list e8s-special (default-value 'e8s-special) ready)))
      (set-default 'e8s-special 'new-global)
      (with-mutex m (setq go t) (condition-notify c t))
      (thread-join th)
      (list other seen (list e8s-special (default-value 'e8s-special))))))
"##,
        expect_test::expect![[
            r#""OK ((global global t) (worker-let worker-let worker-let t) (new-global new-global))""#
        ]],
    );
}

/// Buffer-local value, the default, and the worker's own `let` stay distinct.
/// `make-thread` copies `m_current_buffer`, so the child starts in this buffer.
#[test]
fn e8s_binding_buffer_local_across_threads() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(progn
  (defvar e8s-local 'default)
  (let ((b (generate-new-buffer " *e8s-local*")))
    (unwind-protect
        (with-current-buffer b
          (set (make-local-variable 'e8s-local) 'buffer-base)
          (let* ((m (make-mutex))
                 (c (make-condition-variable m))
                 (ready nil)
                 (go nil)
                 (seen nil)
                 (th (make-thread
                      (lambda ()
                        (let ((e8s-local 'buffer-let))
                          (with-mutex m
                            (setq ready t)
                            (condition-notify c t)
                            (let ((rounds 0))
                              (while (and (not go) (< rounds 200000))
                                (setq rounds (1+ rounds))
                                (condition-wait c))))
                          (setq seen (list (eq (current-buffer) b)
                                           e8s-local
                                           (default-value 'e8s-local)
                                           go))))
                      "e8s-bind-buffer")))
            (with-mutex m
              (let ((rounds 0))
                (while (and (not ready) (< rounds 200000))
                  (setq rounds (1+ rounds))
                  (condition-wait c))))
            (let ((other (list e8s-local (default-value 'e8s-local) ready)))
              (setq e8s-local 'buffer-updated)
              (set-default 'e8s-local 'default-updated)
              (with-mutex m (setq go t) (condition-notify c t))
              (thread-join th)
              (list other seen
                    (list e8s-local (default-value 'e8s-local))))))
      (kill-buffer b))))
"##,
        expect_test::expect![[
            r#""OK ((buffer-base default t) (t buffer-let default-updated t) (buffer-updated default-updated))""#
        ]],
    );
}

/// Search regs live on the thread state. The child starts clear; the parent's
/// match data is unchanged after the child matches.
#[test]
fn p15_condition_wait_signal_reacquires_before_inner_handler() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_under_envs_expect(
        r##"
(let* ((m (make-mutex))
       (ready (make-condition-variable m))
       (condition (make-condition-variable m))
       (seen nil)
       (waiting nil)
       (joined nil)
       (worker nil))
  (mutex-lock m)
  (setq worker
    (make-thread
      (lambda ()
        (with-mutex m
          (mutex-lock m)
          (condition-notify ready)
          (condition-case problem
              (condition-wait condition)
            (error
             (setq seen
               (list (car problem)
                     (thread--blocker (current-thread))
                     (mutex-unlock m)
                     (mutex-unlock m)
                     (condition-case nil (mutex-unlock m)
                       (error 'released))))
             (mutex-lock m)))))))
  (condition-wait ready)
  (setq waiting (eq (thread--blocker worker) condition))
  (thread-signal worker 'error '(p15-condition-signal))
  (mutex-unlock m)
  ;; GNU thread.c:1135-1143 snapshots the pending injected error at join entry;
  ;; it may re-raise that error after waiting even though the worker handled it.
  ;; Keep the complete condition/data: a different join error must fail parity.
  (setq joined
    (condition-case join-error
        (list 'returned (thread-join worker))
      (error (list 'raised join-error))))
  (list waiting seen joined))
"##,
        &[&[("NEOVM_THREADS", "green")]],
        expect_test::expect![[
            r#""OK (t (error nil nil nil released) (raised (error p15-condition-signal)))""#
        ]],
    );
}

/// GNU thread.c:304-326 creates an ordinary GC mutex object; :410-413 and
/// :592-595 finalize collectible synchronization objects. A held mutex is not
/// rooted by ownership. The condition's validated association is a real edge.
/// Each allocating helper returns before the fixed full/full checkpoints.
#[test]
fn p15_unreferenced_held_mutex_has_no_implicit_root() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_default_threads_oracle(
        r##"
(let ((held (make-hash-table :test 'eq :weakness 'key))
      (paired (make-hash-table :test 'eq :weakness 'key))
      (kept nil))
  (funcall
    (lambda ()
      (let ((mutex (make-mutex)))
        (mutex-lock mutex)
        (puthash mutex t held))))
  (garbage-collect)
  (garbage-collect)
  (let ((held-count (hash-table-count held)))
    (funcall
      (lambda ()
        (let* ((mutex (make-mutex))
               (condition (make-condition-variable mutex)))
          (puthash mutex t paired)
          (puthash condition t paired)
          (setq kept condition)
          nil)))
    (garbage-collect)
    (garbage-collect)
    (let ((retained-count (hash-table-count paired)))
      (setq kept nil)
      (garbage-collect)
      (garbage-collect)
      (list held-count retained-count (hash-table-count paired)))))
"##,
        expect_test::expect![[r#""OK (0 2 0)""#]],
    );
}
