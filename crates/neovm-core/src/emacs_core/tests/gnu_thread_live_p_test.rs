//! GNU 31.1 oracle for the simulated thread-creation window (peer #56).
//!
//! GNU `Fmake_thread` (`src/thread.c:885-947`) allocates the thread state,
//! links it into `all_threads` (`:924-925`), starts the OS thread and returns
//! while the creating thread still holds the global lock; the worker's
//! `run_thread` (`:757+`) executes only after `acquire_global_lock`, i.e. at a
//! release point such as `Fthread_yield` (`:1102-1110`) or `Fthread_join`
//! (`:1118-1144`). `thread_live_p` is `m_specpdl != NULL` (`:76`,
//! `Fthread_live_p` `:1036-1045`), and `all-threads` (`:1146-1160`) lists
//! every linked live thread. So a worker that has been created but has not
//! run yet is live and listed.
//!
//! Probes on the pinned build (GNU Emacs 31.1, `build/bin/emacs`, rev
//! a360712c9d): `(thread-live-p w)` + `(memq w (all-threads))` -> `(t t)`
//! 15/15 runs; the body-seen/join form -> `(t t nil t t)` 5/5 runs.
//!
//! These tests are the executable regression for the simulated backend: the
//! eval-hosted `vm_thread_mutex_and_condition_builtins_use_shared_runtime`
//! names the same expectation but its lib-test target is pre-existing red.

use neovm_value::value::{Value, list_to_vec};

#[test]
fn gnu_created_worker_is_live_and_listed_before_any_release_point() {
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(let* ((main (current-thread))
                      (worker (make-thread (lambda () 42) "vm-worker"))
                      (threads-before (all-threads)))
                 (list (thread-live-p worker)
                       (and (memq worker threads-before) t)
                       (consp (memq main threads-before))))"#,
        )
        .expect("GNU thread liveness oracle");
    assert_eq!(
        list_to_vec(&result),
        Some(vec![Value::T, Value::T, Value::T])
    );
}

#[test]
fn gnu_created_worker_runs_at_the_join_release_point() {
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(progn
                 (setq vm-thread-seen nil)
                 (let* ((main (current-thread))
                        (worker (make-thread
                                 (lambda ()
                                   (setq vm-thread-seen (current-thread))
                                   (current-thread))
                                 "vm-worker")))
                   (list
                    (threadp worker)
                    (null (eq main worker))
                    (eq vm-thread-seen worker)
                    (eq (thread-join worker) worker)
                    (eq (current-thread) main))))"#,
        )
        .expect("GNU join release-point oracle");
    assert_eq!(
        list_to_vec(&result),
        Some(vec![Value::T, Value::T, Value::NIL, Value::T, Value::T])
    );
}

#[test]
fn gnu_thread_last_error_is_published_by_the_join_release_point() {
    // Pinned GNU 31.1 probe: thread-last-error is still nil before the join
    // release point, and the worker's internal error is published by the time
    // join returns; join itself returns normally. `(t t t t)` 3/3.
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(let* ((worker (make-thread (lambda () (signal 'error '(99))) "w"))
                      (published (thread-last-error))
                      (joined (condition-case e
                                  (progn (thread-join worker) :no-error)
                                (error e)))
                      (after (thread-last-error)))
                 (list (threadp worker)
                       (null published)
                       (eq joined :no-error)
                       (equal after '(error 99))))"#,
        )
        .expect("GNU thread-last-error oracle");
    assert_eq!(
        list_to_vec(&result),
        Some(vec![Value::T, Value::T, Value::T, Value::T])
    );
}

#[test]
fn gnu_signal_then_join_reports_the_injected_error() {
    // Pinned GNU 31.1 probe: a `thread-signal` aimed at a created worker is
    // re-raised by `thread-join` (src/thread.c:1118-1144) and caught as
    // `quit`; `quit` 3/3.
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(condition-case e
                 (let ((worker (make-thread (lambda () (while t (thread-yield))))))
                   (thread-signal worker 'quit nil)
                   (thread-join worker))
                 (quit (car e)))"#,
        )
        .expect("GNU injected-error join oracle");
    assert_eq!(result, Value::symbol("quit"));
}

#[test]
fn gnu_join_runs_the_worker_while_the_caller_binding_is_rooted() {
    // Pinned GNU 31.1 probe: the worker's collection runs at the join release
    // point while the caller's let binding is alive; the bound list survives.
    // `(kept)` 2/2.
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(let ((vm-thread-parked-root (list 'kept)))
                 (let ((worker (make-thread (lambda () (garbage-collect)))))
                   (thread-join worker))
                 vm-thread-parked-root)"#,
        )
        .expect("GNU rooted-caller join oracle");
    assert_eq!(list_to_vec(&result), Some(vec![Value::symbol("kept")]));
}

/// Exact form and expectation of the P2 baseline row
/// `vm_thread_mutex_and_condition_builtins_use_shared_runtime` (peer #56),
/// executed here because the eval-hosted copy's lib-test target is
/// pre-existing red. The expected value is the pinned GNU 31.1 result.
#[test]
fn gnu_thread_mutex_and_condition_builtins_use_shared_runtime() {
    let mut eval = crate::test_utils::new_context();
    let result = eval
        .eval_str(
            r#"(let* ((main (current-thread))
                      (worker (make-thread (lambda () 42) "vm-worker"))
                      (threads-before (all-threads))
                      (mx (make-mutex "vm-mutex"))
                      (cv (make-condition-variable mx "vm-cond")))
                 (list
                  (threadp main)
                  (threadp worker)
                  (equal (thread-name worker) "vm-worker")
                  (thread-live-p worker)
                  (consp (memq main threads-before))
                  (consp (memq worker threads-before))
                  (null (thread-yield))
                  (null (thread-signal worker 'error '("oops")))
                  (condition-case err
                      (thread-join worker)
                    (error (car err)))
                  (null (thread-last-error))
                  (eq main-thread main)
                  (null (thread-buffer-disposition main))
                  (eq (thread-set-buffer-disposition worker 'silently) 'silently)
                  (eq (thread-buffer-disposition worker) 'silently)
                  (condition-case err
                      (progn
                        (thread-set-buffer-disposition main t)
                        nil)
                    (wrong-type-argument (car err)))
                  (mutexp mx)
                  (equal (mutex-name mx) "vm-mutex")
                  ;; GNU condition variables do not count notifications:
                  ;; `condition-wait` blocks until another thread signals
                  ;; COND (src/thread.c:446-501). The row therefore creates
                  ;; the notifier immediately before the wait; it reaches the
                  ;; wait's release point and returns nil. It uses globals
                  ;; because a worker does not see the main thread's dynamic
                  ;; `let` bindings (GNU Fmake_thread gives it an empty
                  ;; specpdl, src/thread.c:913-919). Pinned GNU 31.1 probe of
                  ;; this exact form: the 24-element list below 5/5.
                  (let ((notifier (progn
                                    (setq vm-p2-mx mx vm-p2-cv cv)
                                    (make-thread (lambda ()
                                                   (mutex-lock vm-p2-mx)
                                                   (condition-notify vm-p2-cv)
                                                   (mutex-unlock vm-p2-mx))
                                                 "vm-notifier"))))
                    (null (mutex-lock mx)))
                  (condition-variable-p cv)
                  (equal (condition-name cv) "vm-cond")
                  (eq (condition-mutex cv) mx)
                  (null (condition-notify cv))
                  (null (condition-wait cv))
                  (null (mutex-unlock mx))))"#,
        )
        .expect("GNU thread mutex/condition oracle");
    let items = list_to_vec(&result).expect("list result");
    let mut expected = vec![Value::T; 8];
    expected.push(Value::symbol("error"));
    expected.extend(std::iter::repeat_n(Value::T, 5));
    expected.push(Value::symbol("wrong-type-argument"));
    expected.extend(std::iter::repeat_n(Value::T, 9));
    assert_eq!(items.len(), 24);
    assert_eq!(items, expected);
}
