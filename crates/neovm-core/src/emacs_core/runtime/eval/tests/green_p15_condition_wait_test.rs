//! GNU thread.c:466 clears event_object before reacquiring; :475 raises a
//! pending signal only after restoring the saved recursive ownership.
//!
//! A ready-condition handshake forces the contended case. It is not a retry:
//! every wait, signal and Green yield happens once, and every observation is an
//! exact assertion. No sleep or external readiness can hide a product hang.

use crate::emacs_core::error::EvalResult;
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::{FixedMin1, SubrSpec};
use crate::emacs_core::threads::MutexOwnership;
use crate::emacs_core::value::Value;
use crate::tagged::header::MutexObj;

fn eval(context: &mut Context, form: &str) -> Value {
    match context.eval_str(form) {
        Ok(value) => value,
        Err(crate::emacs_core::error::EvalError::Signal { symbol, data, .. }) => panic!(
            "condition-wait fixture {form}: {} {:?}",
            crate::emacs_core::intern::resolve_sym(symbol),
            data.iter().map(print_value).collect::<Vec<_>>()
        ),
        Err(error) => panic!("condition-wait fixture {form}: {error:?}"),
    }
}

fn green() -> Context {
    // GNU's with-mutex and the machinery defining it are Lisp loadup, not
    // native forms. Use the real runtime-startup image (as the evaluator's
    // differential tests do), then select Green before thread admission.
    // No fixture expansion replaces its lock/unwind behavior under test.
    let mut context = crate::test_utils::runtime_startup_context();
    context.set_threading_mode_at_construction(ThreadingMode::Green);
    context
}

fn assert_owned_depth_inside_handler(context: &mut Context, mutex: Value) -> EvalResult {
    // The active subr argument is the root. Check allocation membership before
    // decoding the saved ownership, so a missing edge cannot pass by UAF.
    assert!(context.tagged_heap.owns_heap_value_for_test(mutex));
    let handle = context
        .threads
        .mutex_id_from_handle(&mutex)
        .expect("same-World mutex argument");
    assert!(context.threads.mutex_owned_by_current_thread(handle));
    let current = context.threads.current_thread_id();
    let ptr = mutex
        .as_veclike_ptr()
        .expect("mutex layout")
        .cast::<MutexObj>();
    // SAFETY: membership and the canonical handle validation above prove the
    // live MutexObj; this callback holds the Baton and does not yield or GC.
    match unsafe { &(*ptr).payload.ownership } {
        MutexOwnership::Locked { owner, depth } => {
            assert_eq!(owner.native_key(), current);
            assert_eq!(depth.get(), 2, "saved depth before with-mutex unwinds");
        }
        MutexOwnership::Unlocked => panic!("signal raised before reacquisition"),
    }
    assert_eq!(context.threads.thread_blocker(current), Some(Value::NIL));
    Ok(Value::T)
}

#[test]
fn p15_condition_wait_signal_restores_exact_depth_before_inner_handler() {
    let mut context = green();
    context.register_subr(crate::emacs_core::subr::world_subr!(SubrSpec::fixed1(
        "p15-condition-owned-depth",
        assert_owned_depth_inside_handler,
        FixedMin1::One
    )));
    eval(
        &mut context,
        "(progn
           (setq p15-wait-m (make-mutex)
                 p15-wait-c (make-condition-variable p15-wait-m)
                 p15-wait-ready (make-condition-variable p15-wait-m)
                 p15-wait-seen nil)
           (mutex-lock p15-wait-m)
           (setq p15-wait-worker
             (make-thread (lambda ()
               (with-mutex p15-wait-m
                 (mutex-lock p15-wait-m)
                 (condition-notify p15-wait-ready)
                 (condition-case problem
                     (condition-wait p15-wait-c)
                   (error
                    (setq p15-wait-seen
                      (list (p15-condition-owned-depth p15-wait-m)
                            (thread--blocker (current-thread))
                            (mutex-unlock p15-wait-m)
                            (mutex-unlock p15-wait-m)
                            (condition-case nil (mutex-unlock p15-wait-m)
                              (error 'released))))
                    (mutex-lock p15-wait-m)))))))
           (condition-wait p15-wait-ready))",
    );
    let worker = context.variable_value_by_name("p15-wait-worker").unwrap();
    let condition = context.variable_value_by_name("p15-wait-c").unwrap();
    assert_eq!(
        context.threads.thread_blocker_for_value(worker),
        Some(condition)
    );
    // Main still owns the mutex. FIFO Green yield must run the signalled
    // worker into contended reacquisition, rather than letting it finish.
    eval(
        &mut context,
        "(progn (thread-signal p15-wait-worker 'error '(p15-wait-signal))
                (thread-yield))",
    );
    assert_eq!(
        context.threads.thread_blocker_for_value(worker),
        Some(Value::NIL)
    );
    assert_eq!(
        eval(&mut context, "(thread-live-p p15-wait-worker)"),
        Value::T,
        "the signalled worker is still waiting to reacquire main's mutex"
    );
    let joined = eval(
        &mut context,
        "(progn (mutex-unlock p15-wait-m)
                (condition-case join-error
                    (list 'returned (thread-join p15-wait-worker))
                  (error (list 'raised join-error))))",
    );
    assert_eq!(print_value(&joined), "(raised (error p15-wait-signal))");
    let result = eval(&mut context, "p15-wait-seen");
    assert_eq!(print_value(&result), "(t nil nil nil released)");
    context.green_teardown();
}

#[test]
fn p15_condition_wait_argument_is_the_only_reacquiring_condition_root() {
    let mut context = green();
    eval(
        &mut context,
        "(setq p15-sole-m (make-mutex)
               p15-sole-c (make-condition-variable p15-sole-m)
               p15-sole-ready (make-condition-variable p15-sole-m))",
    );
    // Plain Value words here are observations, not registered root leases.
    let mutex = context.variable_value_by_name("p15-sole-m").unwrap();
    let condition = context.variable_value_by_name("p15-sole-c").unwrap();
    let ready = context.variable_value_by_name("p15-sole-ready").unwrap();
    eval(
        &mut context,
        "(progn
           (mutex-lock p15-sole-m)
           (setq p15-sole-worker
             (make-thread (lambda ()
               (mutex-lock p15-sole-m)
               (eval '(condition-notify p15-sole-ready))
               (condition-case nil
                   (condition-wait (prog1 p15-sole-c (setq p15-sole-c nil)))
                 (error 'signalled)))))
           (condition-wait p15-sole-ready)
           (setq p15-sole-ready nil p15-sole-m nil))",
    );
    let worker = context.variable_value_by_name("p15-sole-worker").unwrap();
    assert_eq!(
        context.threads.thread_blocker_for_value(worker),
        Some(condition)
    );
    context.gc_collect_exact();
    context.gc_collect_exact();
    assert!(context.tagged_heap.owns_heap_value_for_test(condition));
    assert!(context.tagged_heap.owns_heap_value_for_test(mutex));
    // apply.rs:1373-1389 intentionally retains the previous call's argument
    // array until overwritten, matching GNU's eval_sub C-frame residue. The
    // notification therefore runs inside Feval's separate boundary above
    // (GNU eval.c:2548-2562), whose argument temps end before condition-wait.
    // Otherwise ready itself is an alternate root to the mutex under test.
    assert!(
        !context.tagged_heap.owns_heap_value_for_test(ready),
        "completed ready handshake must not retain an alternate mutex edge"
    );

    eval(
        &mut context,
        "(progn (thread-signal p15-sole-worker 'error '(p15-sole-signal))
                (thread-yield))",
    );
    assert_eq!(
        context.threads.thread_blocker_for_value(worker),
        Some(Value::NIL)
    );
    // event_object is now nil; only the parked subr argument keeps the
    // condition, and only its canonical association keeps the mutex.
    context.gc_collect_exact();
    context.gc_collect_exact();
    assert!(context.tagged_heap.owns_heap_value_for_test(condition));
    assert!(context.tagged_heap.owns_heap_value_for_test(mutex));
    let handle = context.threads.mutex_id_from_handle(&mutex).unwrap();
    context.green_mutex_unlock(handle).unwrap();
    // GNU thread.c:1135-1143 snapshots an injected signal at join entry and
    // can re-raise it even though the worker handles it before exiting. Join
    // still waits for completion; assert the worker's actual result exactly.
    let joined = eval(
        &mut context,
        "(condition-case join-error
             (list 'returned (thread-join p15-sole-worker))
           (error (list 'raised join-error)))",
    );
    assert_eq!(print_value(&joined), "(raised (error p15-sole-signal))");
    assert_eq!(
        print_value(&context.threads.thread_result_for_value(worker)),
        "signalled"
    );
    context.gc_collect_exact();
    assert!(!context.tagged_heap.owns_heap_value_for_test(condition));
    assert!(!context.tagged_heap.owns_heap_value_for_test(mutex));
    context.green_teardown();
}
