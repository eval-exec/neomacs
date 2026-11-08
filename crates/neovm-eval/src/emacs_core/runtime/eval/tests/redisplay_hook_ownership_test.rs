use super::*;
#[doc(hidden)]
pub use crate::emacs_core::subr::{NativeFn, SubrArity, SubrSpec};

#[test]
fn pending_targets_join_until_transaction_reset() {
    let mut scope = PendingScope::None;
    scope.raise(PendingScope::Some);
    scope.raise(PendingScope::None);
    assert_eq!(scope, PendingScope::Some);
    scope.raise(PendingScope::All);
    scope.raise(PendingScope::Some);
    assert_eq!(scope, PendingScope::All);
}

#[test]
fn callback_and_active_guard_restore_during_rust_unwind() {
    use std::cell::Cell;
    use std::rc::Rc;
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let restored = Rc::new(Cell::new(false));
    let observed = restored.clone();
    eval.redisplay_fn = Some(Box::new(move |_| observed.set(true)));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        eval.gnu_redisplay_hooks.active = RedisplayActiveOwner::RedisplayTransaction;
        let binding_count = eval.specpdl.len();
        let restrictions = eval.buffers.reset_outermost_restrictions();
        let callback = eval.redisplay_fn.take();
        let _guard = RedisplayTransaction {
            eval: &mut eval,
            restrictions: Some(restrictions),
            callback,
            preparation: None,
            binding_count,
        };
        panic!("simulated frontend unwind");
    }));
    assert!(outcome.is_err());
    assert!(!eval.gnu_redisplay_hooks.active.is_active());
    let mut callback = eval.redisplay_fn.take().expect("restored frontend");
    callback(&mut eval);
    assert!(restored.get());
}

#[test]
fn exceptional_layout_payload_remains_owned_until_taken() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let payload = Value::string("fresh layout throw");
    eval.defer_redisplay_hook_flow(Flow::throw(Value::symbol("layout-test"), payload));
    assert!(eval.redisplay_hook_flow_pending());
    eval.defer_redisplay_hook_flow(Flow::throw(Value::symbol("later"), Value::NIL));
    let flow = eval
        .gnu_redisplay_hooks
        .layout_flow
        .take()
        .expect("first flow retained");
    assert_eq!(flow.as_throw().expect("throw").value, payload);
    assert!(!eval.redisplay_hook_flow_pending());
}

#[test]
fn physical_redraw_waits_for_exact_rendered_frame_not_hook_or_display_ack() {
    let mut state = RedisplayHookOwnership::default();
    let first = FrameId(1);
    let second = FrameId(2);
    state.request_physical_redraw(first);
    state.request_physical_redraw(second);
    state.redisplay_frames.insert(first);
    state.acknowledge_frame_target(first);
    assert!(!state.redisplay_frames.contains(&first));
    assert!(state.take_physical_redraw(first));
    assert!(!state.take_physical_redraw(first));
    assert!(state.take_physical_redraw(second));
}

#[test]
fn committed_scroll_guard_keeps_outer_active_owner_and_restores_idle_owner() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    {
        let guard = eval.gnu_guard_committed_scroll();
        assert!(guard.eval.gnu_redisplay_hooks.active.is_active());
    }
    assert!(!eval.gnu_redisplay_hooks.active.is_active());
    eval.gnu_redisplay_hooks.active = RedisplayActiveOwner::RedisplayTransaction;
    {
        let _guard = eval.gnu_guard_committed_scroll();
    }
    assert!(eval.gnu_redisplay_hooks.active.is_active());
}

#[test]
fn redisplay_policy_guard_restores_nested_numeric_selector_on_unwind() {
    let initial = gnu_redisplay_hooks_enabled();
    {
        let _outer = RedisplayHookPolicyGuard::legacy();
        assert!(!gnu_redisplay_hooks_enabled());
        {
            let _inner = RedisplayHookPolicyGuard::gnu();
            assert!(gnu_redisplay_hooks_enabled());
        }
        assert!(!gnu_redisplay_hooks_enabled());
        let result = std::panic::catch_unwind(|| {
            let _inner = RedisplayHookPolicyGuard::gnu();
            assert!(gnu_redisplay_hooks_enabled());
            panic!("exercise numeric redisplay policy cleanup");
        });
        assert!(result.is_err());
        assert!(!gnu_redisplay_hooks_enabled());
    }
    assert_eq!(gnu_redisplay_hooks_enabled(), initial);
}

/// A Rust panic through the frontend callback must not evaluate Lisp while
/// unwinding (P4.11): the transaction's Drop retires the pushed bindings
/// storage-only, so neither the unwind form nor the variable watcher runs.
#[test]
fn redisplay_transaction_panic_fallback_never_calls_panicking_lisp() {
    fn panic_on_recovery_lisp(eval: &mut Context, _: Vec<Value>) -> EvalResult {
        eval.obarray
            .set_symbol_value("redisplay-recovery-lisp-ran", Value::T);
        panic!("Lisp callback must not run during Rust panic recovery");
    }
    let _policy = RedisplayHookPolicyGuard::gnu();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let sentinel = neovm_value::intern::intern("redisplay-recovery-lisp-ran");
    let symbol = neovm_value::intern::intern("redisplay-recovery-watched");
    eval.obarray.set_symbol_value_id(sentinel, Value::NIL);
    eval.obarray.set_symbol_value_id(symbol, Value::fixnum(10));
    let count = eval.specpdl.len();
    eval.register_subr(SubrSpec::new(
        "redisplay-recovery-panic",
        NativeFn::ContextVec(panic_on_recovery_lisp),
        SubrArity::new(0, None),
    ));
    eval.redisplay_fn = Some(Box::new(move |eval| {
        eval.try_specbind(symbol, Value::fixnum(20))
            .expect("frontend binds saved value");
        crate::emacs_core::advice::builtin_add_variable_watcher(
            eval,
            vec![
                Value::from_sym_id(symbol),
                Value::symbol("redisplay-recovery-panic"),
            ],
        )
        .expect("register recovery watcher");
        let cleanup = Value::list(vec![Value::symbol("redisplay-recovery-panic")]);
        eval.specpdl.push(SpecBinding::UnwindProtect {
            forms: cleanup,
            lexenv: eval.lexenv,
        });
        panic!("simulate frontend panic");
    }));
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = eval.redisplay_with_force_flow(true);
    }));
    assert!(outcome.is_err());
    assert_eq!(eval.obarray.symbol_value_id_or_nil(sentinel), Value::NIL);
    assert_eq!(
        eval.obarray.symbol_value_id_or_nil(symbol),
        Value::fixnum(10)
    );
    assert_eq!(eval.specpdl.len(), count);
    assert!(!eval.gnu_redisplay_hooks.active.is_active());
}
