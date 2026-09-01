use super::super::eval::Context;
use super::*;
use crate::heap_types::LispString;

// -- ThreadManager unit tests -------------------------------------------

#[test]
fn thread_manager_new_has_main_thread() {
    crate::test_utils::init_test_tracing();
    let mgr = ThreadManager::new();
    assert!(mgr.is_thread(0));
    assert_eq!(mgr.current_thread_id(), 0);
    assert!(mgr.thread_alive_p(0));
    assert_eq!(mgr.thread_name(0), None);
    assert_eq!(mgr.thread_buffer_disposition(0), Some(Value::NIL));
}

#[test]
fn create_thread_assigns_unique_ids() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id1 = mgr.create_thread(Value::NIL, Some(LispString::from_unibyte(b"t1".to_vec())));
    let id2 = mgr.create_thread(Value::NIL, Some(LispString::from_unibyte(b"t2".to_vec())));
    assert_ne!(id1, id2);
    assert!(mgr.is_thread(id1));
    assert!(mgr.is_thread(id2));
    assert!(!mgr.is_thread(999));
}

#[test]
fn thread_lifecycle_created_running_finished() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_thread(Value::NIL, None);
    assert_eq!(mgr.get_thread(id).unwrap().status, ThreadStatus::Created);
    assert!(mgr.thread_alive_p(id));

    mgr.start_thread(id);
    assert_eq!(mgr.get_thread(id).unwrap().status, ThreadStatus::Running);
    assert!(mgr.thread_alive_p(id));

    mgr.finish_thread(id, Value::fixnum(42));
    assert_eq!(mgr.get_thread(id).unwrap().status, ThreadStatus::Finished);
    assert!(mgr.thread_alive_p(id));
    assert_eq!(mgr.thread_result(id).as_int(), Some(42));

    mgr.join_thread(id);
    assert!(!mgr.thread_alive_p(id));
}

#[test]
fn thread_signal_records_error() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_thread(Value::NIL, None);
    mgr.start_thread(id);
    mgr.signal_thread(id, Value::symbol("test-error"));
    assert_eq!(mgr.get_thread(id).unwrap().status, ThreadStatus::Signaled);
    assert!(!mgr.thread_alive_p(id));
}

#[test]
fn all_thread_ids_includes_main_and_created() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_thread(Value::NIL, None);
    let ids = mgr.all_thread_ids();
    assert!(ids.len() >= 2);
    assert!(ids.contains(&0));
    assert!(ids.contains(&id));
}

#[test]
fn all_thread_ids_excludes_finished_thread() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_thread(Value::NIL, None);
    let before_join = mgr.all_thread_ids();
    assert!(before_join.contains(&id));

    mgr.finish_thread(id, Value::fixnum(1));
    let after_finish = mgr.all_thread_ids();
    assert!(after_finish.contains(&id));

    mgr.join_thread(id);
    let after_join = mgr.all_thread_ids();
    assert!(!after_join.contains(&id));
    assert!(after_join.contains(&0));
}

#[test]
fn thread_buffer_disposition_round_trips() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_thread(Value::NIL, None);
    assert_eq!(mgr.thread_buffer_disposition(id), Some(Value::NIL));
    assert!(mgr.set_thread_buffer_disposition(id, Value::symbol("silently")));
    assert_eq!(
        mgr.thread_buffer_disposition(id),
        Some(Value::symbol("silently"))
    );
}

#[test]
fn thread_manager_tracks_current_buffer_and_blocker_state() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_thread(Value::NIL, None);
    let buffer_id = crate::buffer::BufferId(99);
    assert!(mgr.set_thread_current_buffer(id, Some(buffer_id)));
    assert_eq!(mgr.thread_current_buffer(id), Some(buffer_id));
    assert!(mgr.set_thread_blocker(id, Value::symbol("test-blocker")));
    assert_eq!(mgr.thread_blocker(id), Some(Value::symbol("test-blocker")));
    assert!(mgr.clear_thread_blocker(id));
    assert_eq!(mgr.thread_blocker(id), Some(Value::NIL));
}

#[test]
fn last_error_get_and_cleanup() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    mgr.record_last_error(Value::symbol("oops"));

    let err = mgr.last_error(false);
    assert!(err.is_truthy());
    // Still there after no cleanup
    let err2 = mgr.last_error(true);
    assert!(err2.is_truthy());
    // Now gone
    let err3 = mgr.last_error(false);
    assert!(err3.is_nil());
}

// -- Mutex unit tests ---------------------------------------------------

#[test]
fn mutex_create_and_lookup() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_mutex(Some(LispString::from_unibyte(b"my-lock".to_vec())));
    assert!(mgr.is_mutex(id));
    assert_eq!(
        mgr.mutex_name(id).and_then(|s| s.as_utf8_str()),
        Some("my-lock")
    );
    assert!(!mgr.is_mutex(999));
}

#[test]
fn mutex_lock_unlock_cycle() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_mutex(None);
    assert!(mgr.mutex_lock(id));
    assert!(mgr.mutex_unlock(id));
    assert!(!mgr.mutex_unlock(id));
}

#[test]
fn mutex_recursive_lock() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_mutex(None);
    assert!(mgr.mutex_lock(id));
    assert!(mgr.mutex_lock(id));
    // lock_count is 2
    assert!(mgr.mutex_unlock(id));
    // Still locked (count=1)
    let m = mgr.mutexes.get(&id).unwrap();
    assert!(m.owner.is_some());
    assert!(mgr.mutex_unlock(id));
    // Now fully unlocked
    let m = mgr.mutexes.get(&id).unwrap();
    assert!(m.owner.is_none());
}

#[test]
fn mutex_contended_lock_preserves_existing_owner() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let id = mgr.create_mutex(None);
    assert!(mgr.mutex_lock(id));

    let worker = mgr.create_thread(Value::NIL, None);
    let saved = mgr.enter_thread(worker);
    assert!(!mgr.mutex_lock(id));
    mgr.restore_thread(saved);

    let m = mgr.mutexes.get(&id).unwrap();
    assert_eq!(m.owner, Some(0));
    assert_eq!(m.lock_count, 1);
    assert!(mgr.mutex_unlock(id));
}

// -- Condition variable unit tests --------------------------------------

#[test]
fn condition_variable_create() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let mx = mgr.create_mutex(None);
    let cv = mgr.create_condition_variable(mx, Some(LispString::from_unibyte(b"cv1".to_vec())));
    assert!(cv.is_some());
    let cv_id = cv.unwrap();
    assert!(mgr.is_condition_variable(cv_id));
    assert_eq!(
        mgr.condition_variable_name(cv_id)
            .and_then(|s| s.as_utf8_str()),
        Some("cv1")
    );
    assert_eq!(mgr.condition_variable_mutex(cv_id), Some(mx));
}

#[test]
fn condition_variable_requires_valid_mutex() {
    crate::test_utils::init_test_tracing();
    let mut mgr = ThreadManager::new();
    let cv = mgr.create_condition_variable(999, None);
    assert!(cv.is_none());
}

// -- Builtin-level tests -----------------------------------------------

#[test]
fn test_builtin_make_thread_runs_function() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // Define a simple function that returns 42
    eval.set_variable("thread-test-result", Value::NIL);
    eval.set_function(
        "thread-test-fn",
        Value::make_lambda(super::super::value::LambdaData {
            params: super::super::value::LambdaParams::simple(vec![]),
            body: vec![].into(), // empty body → nil
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        }),
    );

    let result = builtin_make_thread(
        &mut eval,
        vec![Value::symbol("thread-test-fn"), Value::string("worker")],
    );
    assert!(result.is_ok());
    let tid = result.unwrap();
    assert_eq!(tagged_object_id(&tid, "thread"), Some(1));
}

#[test]
fn test_builtin_make_thread_accepts_buffer_disposition_arg() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_make_thread(
        &mut eval,
        vec![
            Value::make_lambda(super::super::value::LambdaData {
                params: super::super::value::LambdaParams::simple(vec![]),
                body: vec![].into(),
                env: None,
                docstring: None,
                doc_form: None,
                interactive: None,
            }),
            Value::string("worker"),
            Value::symbol("silently"),
        ],
    );
    assert!(result.is_ok());
    let thread = result.unwrap();
    let thread_id = tagged_object_id(&thread, "thread").unwrap();
    assert_eq!(thread_id, 1);
    assert_eq!(
        eval.threads.thread_buffer_disposition(thread_id),
        Some(Value::symbol("silently"))
    );
}

#[test]
fn test_builtin_make_thread_rejects_more_than_three_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_make_thread(
        &mut eval,
        vec![
            Value::make_lambda(super::super::value::LambdaData {
                params: super::super::value::LambdaParams::simple(vec![]),
                body: vec![].into(),
                env: None,
                docstring: None,
                doc_form: None,
                interactive: None,
            }),
            Value::NIL,
            Value::NIL,
            Value::NIL,
        ],
    );
    assert!(matches!(
        result,
        Err(Flow::Signal(sig)) if sig.symbol_name() == "wrong-number-of-arguments"
    ));
}

#[test]
fn test_builtin_threadp() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = builtin_current_thread(&mut eval, vec![]).unwrap();

    let r = builtin_threadp(&mut eval, vec![current]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_truthy());

    let r = builtin_threadp(&mut eval, vec![Value::fixnum(0)]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());

    let r = builtin_threadp(&mut eval, vec![Value::string("nope")]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());

    let fake = Value::cons(Value::symbol("thread"), Value::fixnum(999));
    let r = builtin_threadp(&mut eval, vec![fake]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());

    let forged_main = Value::cons(Value::symbol("thread"), Value::fixnum(0));
    let r = builtin_threadp(&mut eval, vec![forged_main]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());
}

#[test]
fn test_builtin_current_thread() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_current_thread(&mut eval, vec![]);
    assert!(result.is_ok());
    assert_eq!(tagged_object_id(&result.unwrap(), "thread"), Some(0));
}

#[test]
fn test_builtin_current_thread_returns_stable_handle_identity() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let first = builtin_current_thread(&mut eval, vec![]).unwrap();
    let second = builtin_current_thread(&mut eval, vec![]).unwrap();
    assert!(eq_value(&first, &second));
}

#[test]
fn test_main_thread_variable_matches_current_thread() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = builtin_current_thread(&mut eval, vec![]).unwrap();
    let main_thread = eval.obarray.symbol_value("main-thread").copied().unwrap();
    assert!(eq_value(&current, &main_thread));
}

#[test]
fn test_main_thread_tracks_current_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let original = eval.buffers.current_buffer_id().expect("current buffer");
    assert_eq!(eval.threads.thread_current_buffer(0), Some(original));
    let other = eval.buffers.create_buffer("thread-main-other");
    eval.switch_current_buffer(other)
        .expect("switch current buffer");
    assert_eq!(eval.threads.thread_current_buffer(0), Some(other));
}

#[test]
fn test_builtin_thread_yield() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_thread_yield(&mut eval, vec![]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn test_thread_yielding_worker_can_be_signaled_and_joined() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(condition-case e
                 (let ((worker (make-thread (lambda () (while t (thread-yield))))))
                   (thread-signal worker 'quit nil)
                   (thread-join worker))
                 (quit (car e)))"#,
        )
        .expect("yielding worker signal form");
    assert_eq!(result, Value::symbol("quit"));
}

#[test]
fn worker_default_toplevel_write_cannot_mutate_callers_suspended_binding() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(let ((undo-limit 5))
                 (let ((worker
                        (make-thread
                         (lambda ()
                           (set-default-toplevel-value 'undo-limit "bad")))))
                   (list undo-limit (thread-live-p worker))))"#,
        )
        .expect("worker failure remains local to the worker");
    assert_eq!(
        super::super::value::list_to_vec(&result),
        Some(vec![Value::fixnum(5), Value::NIL])
    );
}

#[test]
fn worker_signal_cannot_invoke_callers_handler_bind() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(progn
                 (setq vm-thread-caller-handler-fired nil)
                 (handler-bind-1
                  (lambda ()
                    (make-thread (lambda () (signal 'error '(worker))))
                    vm-thread-caller-handler-fired)
                  '(error)
                  (lambda (_data)
                    (setq vm-thread-caller-handler-fired t))))"#,
        )
        .expect("worker signal remains local to its thread");
    assert_eq!(result, Value::NIL);
}

#[test]
fn worker_throw_cannot_target_callers_catch() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(catch 'vm-thread-caller-catch
                 (make-thread
                  (lambda () (throw 'vm-thread-caller-catch 'worker)))
                 'caller)"#,
        )
        .expect("worker throw is recorded as the worker's uncaught throw");
    assert_eq!(result, Value::symbol("caller"));
}

#[test]
fn parked_caller_specpdl_remains_gc_rooted_while_worker_runs() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            r#"(let ((vm-thread-parked-root (list 'kept)))
                 (make-thread (lambda () (garbage-collect)))
                 vm-thread-parked-root)"#,
        )
        .expect("parked caller binding survives worker GC");
    assert_eq!(
        super::super::value::list_to_vec(&result),
        Some(vec![Value::symbol("kept")])
    );
}

#[test]
fn failed_thread_entry_discards_unreturned_thread() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let symbol = super::super::intern::intern("undo-limit");
    let specpdl_depth = eval.specpdl.len();
    let outer_default = super::super::data::default_value_by_id(&eval, symbol)
        .expect("undo-limit has a forwarded default");
    eval.try_specbind(symbol, Value::fixnum(5))
        .expect("bind valid undo-limit");
    super::super::eval::set_default_toplevel_value(
        &mut eval,
        vec![Value::from_sym_id(symbol), Value::string("bad")],
    )
    .expect("install rejected saved value");
    let before = eval.threads.all_thread_ids();

    assert!(matches!(
        builtin_make_thread(&mut eval, vec![Value::NIL]),
        Err(Flow::Signal(_))
    ));
    assert_eq!(
        eval.threads.all_thread_ids(),
        before,
        "a thread whose runtime entry failed was never returned to Lisp"
    );

    super::super::eval::set_default_toplevel_value(
        &mut eval,
        vec![Value::from_sym_id(symbol), outer_default],
    )
    .expect("restore valid saved value for cleanup");
    eval.unbind_to_with_result(specpdl_depth, Ok(Value::NIL))
        .expect("clean up dynamic binding");
}

#[test]
fn failed_blocked_thread_entry_preserves_continuation() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let thread_id = eval.threads.create_thread(Value::NIL, None);
    eval.threads.start_thread(thread_id);
    let continuation = Value::list(vec![Value::symbol("ignore")]);
    eval.threads
        .block_thread(thread_id, Value::NIL, continuation);

    let symbol = super::super::intern::intern("undo-limit");
    let specpdl_depth = eval.specpdl.len();
    let outer_default = super::super::data::default_value_by_id(&eval, symbol)
        .expect("undo-limit has a forwarded default");
    eval.try_specbind(symbol, Value::fixnum(5))
        .expect("bind valid undo-limit");
    super::super::eval::set_default_toplevel_value(
        &mut eval,
        vec![Value::from_sym_id(symbol), Value::string("bad")],
    )
    .expect("install rejected saved value");

    assert!(matches!(
        resume_blocked_thread(&mut eval, thread_id),
        Err(Flow::Signal(_))
    ));
    let thread = eval
        .threads
        .get_thread(thread_id)
        .expect("blocked thread remains registered");
    assert_eq!(thread.status, ThreadStatus::Blocked);
    assert_eq!(thread.blocked_remaining_forms, continuation);

    super::super::eval::set_default_toplevel_value(
        &mut eval,
        vec![Value::from_sym_id(symbol), outer_default],
    )
    .expect("restore valid saved value for cleanup");
    eval.unbind_to_with_result(specpdl_depth, Ok(Value::NIL))
        .expect("clean up dynamic binding");
}

#[test]
fn test_builtin_thread_name_main() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = builtin_current_thread(&mut eval, vec![]).unwrap();
    let result = builtin_thread_name(&mut eval, vec![current]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
}

#[test]
fn test_builtin_thread_live_p_main() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = builtin_current_thread(&mut eval, vec![]).unwrap();
    let result = builtin_thread_live_p(&mut eval, vec![current]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_truthy());
}

#[test]
fn test_builtin_all_threads_includes_main() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_all_threads(&mut eval, vec![]);
    assert!(result.is_ok());
    let list = super::super::value::list_to_vec(&result.unwrap()).unwrap();
    assert!(!list.is_empty());
    assert!(
        list.iter()
            .any(|v| tagged_object_id(v, "thread") == Some(0))
    );
}

#[test]
fn threads_feature_and_main_thread_binding_match_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = eval
        .eval_str(
            "(list (featurep 'threads)
                   (memq 'threads features)
                   (boundp 'main-thread)
                   (threadp main-thread)
                   (fboundp 'main-thread))",
        )
        .expect("thread feature probe");

    let items = super::super::value::list_to_vec(&result).expect("list result");
    assert_eq!(items[0], Value::T);
    assert!(items[1].is_cons());
    assert_eq!(items[2], Value::T);
    assert_eq!(items[3], Value::T);
    assert_eq!(items[4], Value::NIL);
}

#[test]
fn test_builtin_all_threads_includes_finished_unjoined_worker() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let worker = builtin_make_thread(
        &mut eval,
        vec![Value::make_lambda(super::super::value::LambdaData {
            params: super::super::value::LambdaParams::simple(vec![]),
            body: vec![].into(),
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        })],
    )
    .unwrap();
    let result = builtin_all_threads(&mut eval, vec![]).unwrap();
    let list = super::super::value::list_to_vec(&result).unwrap();
    assert!(list.iter().any(|value| eq_value(value, &worker)));

    builtin_thread_join(&mut eval, vec![worker]).unwrap();
    let after_join = builtin_all_threads(&mut eval, vec![]).unwrap();
    let after_join = super::super::value::list_to_vec(&after_join).unwrap();
    assert!(!after_join.iter().any(|value| eq_value(value, &worker)));
}

#[test]
fn test_builtin_thread_join_finished() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // Create and run a thread
    let tid_val = builtin_make_thread(
        &mut eval,
        vec![Value::make_lambda(super::super::value::LambdaData {
            params: super::super::value::LambdaParams::simple(vec![]),
            body: vec![].into(),
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        })],
    )
    .unwrap();

    // Join it
    let result = builtin_thread_join(&mut eval, vec![tid_val]);
    assert!(result.is_ok());
}

#[test]
fn test_builtin_thread_join_current_thread_errors() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = builtin_current_thread(&mut eval, vec![]).unwrap();
    let result = builtin_thread_join(&mut eval, vec![current]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data.len(), 1);
            assert_eq!(
                sig.data[0].as_utf8_str(),
                Some("Cannot join current thread")
            );
        }
        other => panic!("expected error signal for self-join, got {other:?}"),
    }
}

#[test]
fn test_builtin_thread_signal_non_current_is_noop() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let tid_val = builtin_make_thread(
        &mut eval,
        vec![Value::make_lambda(super::super::value::LambdaData {
            params: super::super::value::LambdaParams::simple(vec![]),
            body: vec![].into(),
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        })],
    )
    .unwrap();

    let result = builtin_thread_signal(
        &mut eval,
        vec![tid_val, Value::symbol("test-error"), Value::string("oops")],
    );
    assert!(result.is_ok());

    // Signaling an already-finished non-current thread does not set global last-error.
    let err = builtin_thread_last_error(&mut eval, vec![]);
    assert!(err.is_ok());
    assert!(err.unwrap().is_nil());
}

#[test]
fn test_builtin_thread_signal_current_thread_raises() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = builtin_current_thread(&mut eval, vec![]).unwrap();
    let result = builtin_thread_signal(
        &mut eval,
        vec![current, Value::symbol("foo"), Value::fixnum(1)],
    );
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "foo");
            assert_eq!(sig.raw_data, Some(Value::fixnum(1)));
        }
        other => panic!("expected signal from thread-signal current thread, got {other:?}"),
    }
}

#[test]
fn test_builtin_thread_signal_preserves_raw_symbol_identity_through_join() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let thread = builtin_make_thread(
        &mut eval,
        vec![Value::make_lambda(super::super::value::LambdaData {
            params: super::super::value::LambdaParams::simple(vec![]),
            body: vec![Value::fixnum(42)],
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        })],
    )
    .unwrap();
    let raw_symbol =
        crate::emacs_core::intern::intern_lisp_string(&LispString::from_unibyte(vec![0xff]));
    let data = Value::list(vec![Value::string("oops")]);

    assert_eq!(
        builtin_thread_signal(
            &mut eval,
            vec![thread, Value::from_sym_id(raw_symbol), data],
        )
        .unwrap(),
        Value::NIL
    );

    match builtin_thread_join(&mut eval, vec![thread]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol, raw_symbol);
            assert_eq!(sig.raw_data, Some(data));
        }
        other => panic!("expected thread-join to preserve the raw signal: {other:?}"),
    }
}

#[test]
fn test_builtin_thread_last_error_cleanup() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    eval.threads
        .record_last_error(Value::list(vec![Value::symbol("err"), Value::fixnum(1)]));

    let e1 = builtin_thread_last_error(&mut eval, vec![Value::NIL]).unwrap();
    assert!(e1.is_truthy());

    // Cleanup
    let e2 = builtin_thread_last_error(&mut eval, vec![Value::T]).unwrap();
    assert!(e2.is_truthy());

    // Should be gone now
    let e3 = builtin_thread_last_error(&mut eval, vec![]).unwrap();
    assert!(e3.is_nil());
}

#[test]
fn test_builtin_thread_blocker_reads_runtime_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let current = builtin_current_thread(&mut eval, vec![]).unwrap();
    eval.threads
        .set_thread_blocker(0, Value::symbol("vm-blocked"));
    assert_eq!(
        builtin_thread_blocker(&mut eval, vec![current]).unwrap(),
        Value::symbol("vm-blocked")
    );
    eval.threads.clear_thread_blocker(0);
    assert_eq!(
        builtin_thread_blocker(&mut eval, vec![current]).unwrap(),
        Value::NIL
    );
}

#[test]
fn test_builtin_thread_buffer_disposition_round_trips() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let worker = builtin_make_thread(
        &mut eval,
        vec![Value::make_lambda(super::super::value::LambdaData {
            params: super::super::value::LambdaParams::simple(vec![]),
            body: vec![].into(),
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        })],
    )
    .unwrap();

    assert_eq!(
        builtin_thread_buffer_disposition(&mut eval, vec![worker]).unwrap(),
        Value::NIL
    );
    assert_eq!(
        builtin_thread_set_buffer_disposition(&mut eval, vec![worker, Value::T]).unwrap(),
        Value::T
    );
    assert_eq!(
        builtin_thread_buffer_disposition(&mut eval, vec![worker]).unwrap(),
        Value::T
    );
}

#[test]
fn test_builtin_thread_set_buffer_disposition_rejects_non_nil_main_thread_value() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let main_thread = builtin_current_thread(&mut eval, vec![]).unwrap();
    match builtin_thread_set_buffer_disposition(&mut eval, vec![main_thread, Value::T]) {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(sig.data, vec![Value::symbol("null"), Value::T]);
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn test_builtin_make_thread_preserves_caller_current_buffer() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let main_buffer = eval.buffers.current_buffer_id().expect("current buffer");
    let worker_buffer = eval.buffers.create_buffer("thread-worker-buffer");

    eval.set_function(
        "thread-switch-buffer",
        Value::make_lambda(super::super::value::LambdaData {
            params: super::super::value::LambdaParams::simple(vec![]),
            body: vec![
                Value::list(vec![
                    Value::symbol("set-buffer"),
                    Value::make_buffer(worker_buffer),
                ]),
                Value::list(vec![Value::symbol("current-buffer")]),
            ],
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        }),
    );

    let thread = builtin_make_thread(&mut eval, vec![Value::symbol("thread-switch-buffer")])
        .expect("make-thread");
    let thread_id = tagged_object_id(&thread, "thread").expect("thread id");
    let joined = builtin_thread_join(&mut eval, vec![thread]).expect("thread-join");

    assert_eq!(joined, Value::make_buffer(worker_buffer));
    assert_eq!(eval.buffers.current_buffer_id(), Some(main_buffer));
    assert_eq!(
        eval.threads.thread_current_buffer(thread_id),
        Some(worker_buffer)
    );
    assert_eq!(eval.threads.thread_current_buffer(0), Some(main_buffer));
}

// -- Mutex builtin tests ------------------------------------------------

#[test]
fn test_builtin_make_mutex() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_make_mutex(&mut eval, vec![Value::string("my-mutex")]);
    assert!(result.is_ok());
    let mx = result.unwrap();
    assert_eq!(tagged_object_id(&mx, "mutex"), Some(1));
}

#[test]
fn test_builtin_mutexp() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();

    let r = builtin_mutexp(&mut eval, vec![mx]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_truthy());

    let r = builtin_mutexp(&mut eval, vec![Value::fixnum(1)]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());

    let r = builtin_mutexp(&mut eval, vec![Value::NIL]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());

    let forged = Value::cons(Value::symbol("mutex"), Value::fixnum(1));
    let r = builtin_mutexp(&mut eval, vec![forged]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());
}

#[test]
fn test_builtin_mutex_name() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![Value::string("named-mx")]).unwrap();
    let result = builtin_mutex_name(&mut eval, vec![mx]);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_utf8_str(), Some("named-mx"));
}

#[test]
fn test_builtin_mutex_lock_unlock() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
    let lock_result = builtin_mutex_lock(&mut eval, vec![mx]);
    assert!(lock_result.is_ok());
    let unlock_result = builtin_mutex_unlock(&mut eval, vec![mx]);
    assert!(unlock_result.is_ok());
}

// -- Condition variable builtin tests -----------------------------------

#[test]
fn test_builtin_make_condition_variable() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
    let result = builtin_make_condition_variable(&mut eval, vec![mx, Value::string("my-cv")]);
    assert!(result.is_ok());
    assert!(tagged_object_id(&result.unwrap(), "condition-variable").is_some());
}

#[test]
fn test_builtin_condition_variable_p() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
    let cv = builtin_make_condition_variable(&mut eval, vec![mx]).unwrap();

    let r = builtin_condition_variable_p(&mut eval, vec![cv]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_truthy());

    let r = builtin_condition_variable_p(&mut eval, vec![Value::fixnum(1)]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());

    let r = builtin_condition_variable_p(&mut eval, vec![Value::NIL]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());

    let forged = Value::cons(Value::symbol("condition-variable"), Value::fixnum(1));
    let r = builtin_condition_variable_p(&mut eval, vec![forged]);
    assert!(r.is_ok());
    assert!(r.unwrap().is_nil());
}

#[test]
fn test_builtin_condition_name() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
    let unnamed = builtin_make_condition_variable(&mut eval, vec![mx]).unwrap();
    let named =
        builtin_make_condition_variable(&mut eval, vec![mx, Value::string("cv-compat-name")])
            .unwrap();

    let unnamed_name = builtin_condition_name(&mut eval, vec![unnamed]).unwrap();
    assert!(unnamed_name.is_nil());

    let named_name = builtin_condition_name(&mut eval, vec![named]).unwrap();
    assert_eq!(named_name, Value::string("cv-compat-name"));
}

#[test]
fn thread_mutex_and_condition_names_preserve_raw_unibyte_payloads() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));

    let thread = builtin_make_thread(
        &mut eval,
        vec![
            Value::make_lambda(super::super::value::LambdaData {
                params: super::super::value::LambdaParams::simple(vec![]),
                body: vec![].into(),
                env: None,
                docstring: None,
                doc_form: None,
                interactive: None,
            }),
            raw,
        ],
    )
    .unwrap();
    let thread_name = builtin_thread_name(&mut eval, vec![thread]).unwrap();
    let thread_name = thread_name.as_lisp_string().expect("thread name");
    assert!(!thread_name.is_multibyte());
    assert_eq!(thread_name.as_bytes(), &[0xFF]);

    let mutex = builtin_make_mutex(&mut eval, vec![raw]).unwrap();
    let mutex_name = builtin_mutex_name(&mut eval, vec![mutex]).unwrap();
    let mutex_name = mutex_name.as_lisp_string().expect("mutex name");
    assert!(!mutex_name.is_multibyte());
    assert_eq!(mutex_name.as_bytes(), &[0xFF]);

    let cv = builtin_make_condition_variable(&mut eval, vec![mutex, raw]).unwrap();
    let cv_name = builtin_condition_name(&mut eval, vec![cv]).unwrap();
    let cv_name = cv_name.as_lisp_string().expect("condition name");
    assert!(!cv_name.is_multibyte());
    assert_eq!(cv_name.as_bytes(), &[0xFF]);
}

#[test]
fn test_builtin_condition_mutex() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
    let cv = builtin_make_condition_variable(&mut eval, vec![mx]).unwrap();
    let result = builtin_condition_mutex(&mut eval, vec![cv]).unwrap();
    assert!(eq_value(&result, &mx));
}

#[test]
fn test_builtin_condition_name_wrong_type_argument() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_condition_name(&mut eval, vec![Value::NIL]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("condition-variable-p"), Value::NIL]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn test_builtin_condition_mutex_wrong_type_argument() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_condition_mutex(&mut eval, vec![Value::fixnum(1)]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-type-argument");
            assert_eq!(
                sig.data,
                vec![Value::symbol("condition-variable-p"), Value::fixnum(1)]
            );
        }
        other => panic!("expected wrong-type-argument signal, got {other:?}"),
    }
}

#[test]
fn test_builtin_condition_wait_noop() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
    let cv = builtin_make_condition_variable(&mut eval, vec![mx]).unwrap();
    let owner_error = builtin_condition_wait(&mut eval, vec![cv]);
    assert!(owner_error.is_err());
    let lock = builtin_mutex_lock(&mut eval, vec![mx]);
    assert!(lock.is_ok());
    let result = builtin_condition_wait(&mut eval, vec![cv]);
    assert!(result.is_ok());
    assert!(result.unwrap().is_nil());
    let unlock = builtin_mutex_unlock(&mut eval, vec![mx]);
    assert!(unlock.is_ok());
}

#[test]
fn test_builtin_condition_notify_noop() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
    let cv = builtin_make_condition_variable(&mut eval, vec![mx]).unwrap();
    let owner_error = builtin_condition_notify(&mut eval, vec![cv]);
    assert!(owner_error.is_err());
    let lock = builtin_mutex_lock(&mut eval, vec![mx]);
    assert!(lock.is_ok());
    let result = builtin_condition_notify(&mut eval, vec![cv]);
    assert!(result.is_ok());
    let unlock = builtin_mutex_unlock(&mut eval, vec![mx]);
    assert!(unlock.is_ok());
}

// -- with-mutex special form tests --------------------------------------

#[test]
fn test_sf_with_mutex_executes_body() {
    crate::test_utils::init_test_tracing();

    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
    let mx_id = tagged_object_id(&mx, "mutex").unwrap();

    // Store the mutex id in a variable so the special form can look it up
    eval.set_variable("test-mx", mx);

    // (with-mutex test-mx 42)
    let tail = vec![Value::symbol("test-mx"), Value::fixnum(42)];
    let result = sf_with_mutex(&mut eval, &tail);
    assert!(result.is_ok());
    assert_eq!(result.unwrap().as_int(), Some(42));

    // Mutex should be unlocked after with-mutex completes
    let m = eval.threads.mutexes.get(&mx_id).unwrap();
    assert!(m.owner.is_none());
}

#[test]
fn test_sf_with_mutex_unlocks_on_error() {
    crate::test_utils::init_test_tracing();

    let mut eval = Context::new();
    let mx = builtin_make_mutex(&mut eval, vec![]).unwrap();
    let mx_id = tagged_object_id(&mx, "mutex").unwrap();
    eval.set_variable("test-mx2", mx);

    // (with-mutex test-mx2 (/ 1 0))  -- will signal arith-error
    let tail = vec![
        Value::symbol("test-mx2"),
        Value::list(vec![Value::symbol("/"), Value::fixnum(1), Value::fixnum(0)]),
    ];
    let result = sf_with_mutex(&mut eval, &tail);
    // Should propagate the error
    assert!(result.is_err());

    // But the mutex should still be unlocked
    let m = eval.threads.mutexes.get(&mx_id).unwrap();
    assert!(m.owner.is_none());
}

#[test]
fn test_sf_with_mutex_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    // No arguments at all
    let result = sf_with_mutex(&mut eval, &[]);
    match result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "wrong-number-of-arguments");
            assert_eq!(
                sig.data,
                vec![
                    Value::cons(Value::fixnum(1), Value::fixnum(1)),
                    Value::fixnum(0)
                ]
            );
        }
        other => panic!("expected wrong-number-of-arguments signal, got {other:?}"),
    }
}

// -- Arity / type error tests -------------------------------------------

#[test]
fn test_thread_yield_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_thread_yield(&mut eval, vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn test_current_thread_wrong_args() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_current_thread(&mut eval, vec![Value::fixnum(1)]);
    assert!(result.is_err());
}

#[test]
fn test_make_thread_non_callable_returns_thread_object() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let result = builtin_make_thread(&mut eval, vec![Value::fixnum(42)]).unwrap();
    let is_thread = builtin_threadp(&mut eval, vec![result]).unwrap();
    assert!(is_thread.is_truthy());
}

#[test]
fn test_make_thread_non_callable_last_error_shape() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let thread = builtin_make_thread(&mut eval, vec![Value::fixnum(1)]).unwrap();
    let result = builtin_thread_join(&mut eval, vec![thread]);
    // GNU `thread-join` does not re-raise an error signalled inside the thread
    // function; it returns the thread's result. The error remains observable
    // through `thread-last-error`.
    assert!(result.is_ok());
    let err = builtin_thread_last_error(&mut eval, vec![]).unwrap();
    assert_eq!(
        super::super::print::print_value(&err),
        "(invalid-function 1)"
    );
}

#[test]
fn test_thread_last_error_is_published_when_signaled_thread_exits() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _ = builtin_thread_last_error(&mut eval, vec![Value::T]).unwrap();

    let thread = builtin_make_thread(&mut eval, vec![Value::symbol("car")]).unwrap();
    let published = builtin_thread_last_error(&mut eval, vec![]).unwrap();
    assert_eq!(
        super::super::print::print_value(&published),
        "(wrong-number-of-arguments #<subr car> 0)"
    );

    // Internal thread errors are not re-raised by join (GNU semantics); the
    // error stays in thread-last-error (asserted above) and join returns the
    // thread's result.
    let join_result = builtin_thread_join(&mut eval, vec![thread]);
    assert!(join_result.is_ok());

    let _ = builtin_thread_last_error(&mut eval, vec![Value::T]).unwrap();
    let cleared = builtin_thread_last_error(&mut eval, vec![]).unwrap();
    assert!(cleared.is_nil());
}

#[test]
fn test_thread_signal_noncurrent_thread_changes_join_outcome_without_publishing_last_error() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let _ = builtin_thread_last_error(&mut eval, vec![Value::T]).unwrap();
    let thread = builtin_make_thread(
        &mut eval,
        vec![Value::make_lambda(super::super::value::LambdaData {
            params: super::super::value::LambdaParams::simple(vec![]),
            body: vec![Value::fixnum(42)],
            env: None,
            docstring: None,
            doc_form: None,
            interactive: None,
        })],
    )
    .unwrap();

    assert_eq!(
        builtin_thread_signal(
            &mut eval,
            vec![
                thread,
                Value::symbol("error"),
                Value::list(vec![Value::string("oops")])
            ],
        )
        .unwrap(),
        Value::NIL
    );

    let join_result = builtin_thread_join(&mut eval, vec![thread]);
    match join_result {
        Err(Flow::Signal(sig)) => {
            assert_eq!(sig.symbol_name(), "error");
            assert_eq!(sig.data, vec![Value::string("oops")]);
        }
        other => panic!("expected thread-join to re-signal stored thread error, got {other:?}"),
    }

    let last_error = builtin_thread_last_error(&mut eval, vec![]).unwrap();
    assert!(last_error.is_nil());
}

#[test]
fn test_thread_name_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let fake = Value::cons(Value::symbol("thread"), Value::fixnum(999));
    let result = builtin_thread_name(&mut eval, vec![fake]);
    assert!(result.is_err());
}

#[test]
fn test_mutex_lock_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let fake = Value::cons(Value::symbol("mutex"), Value::fixnum(999));
    let result = builtin_mutex_lock(&mut eval, vec![fake]);
    assert!(result.is_err());
}

#[test]
fn test_condition_wait_nonexistent() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let fake = Value::cons(Value::symbol("condition-variable"), Value::fixnum(999));
    let result = builtin_condition_wait(&mut eval, vec![fake]);
    assert!(result.is_err());
}
