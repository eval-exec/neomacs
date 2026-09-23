use super::*;
use crate::emacs_core::value::list_to_vec;

unsafe extern "C" fn dummy_module_function(
    _env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    std::ptr::null_mut()
}

struct TestEnv {
    env: Box<emacs_env>,
    priv_: Box<emacs_env_private>,
}

impl TestEnv {
    fn new() -> Self {
        let mut priv_ = Box::new(emacs_env_private {
            pending_non_local_exit: emacs_funcall_exit::Return,
            non_local_exit_symbol: Value::NIL,
            non_local_exit_data: Value::NIL,
            storage: emacs_value_storage::new(),
        });
        let mut env = Box::new(unsafe { std::mem::zeroed::<emacs_env>() });
        let env_ptr = &mut *env as *mut emacs_env;
        let priv_ptr = &mut *priv_ as *mut emacs_env_private;
        unsafe {
            initialize_environment(env_ptr, priv_ptr);
        }
        Self { env, priv_ }
    }

    fn env_ptr(&mut self) -> *mut emacs_env {
        &mut *self.env
    }

    fn priv_ptr(&mut self) -> *mut emacs_env_private {
        &mut *self.priv_
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        unsafe {
            finalize_storage(&mut self.priv_.storage);
        }
    }
}

/// `ModuleContextGuard` must restore the PREVIOUS context on drop —
/// nested module→elisp→module calls depend on the outer pointer coming
/// back — and must do the same when its scope unwinds.
#[test]
fn module_context_guard_restores_previous_on_drop_and_panic() {
    // Fake, never-dereferenced pointers: the guard only moves them in
    // and out of the MODULE_CTX cell.
    let outer = 0x1000 as *mut Context;
    let inner = 0x2000 as *mut Context;
    let current = || MODULE_CTX.with(|c| c.get());

    assert!(current().is_null());
    {
        let _outer_guard = ModuleContextGuard::install(outer);
        assert_eq!(current(), outer);
        {
            let _inner_guard = ModuleContextGuard::install(inner);
            assert_eq!(current(), inner);
        }
        assert_eq!(
            current(),
            outer,
            "inner drop must restore the outer context"
        );
        let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _guard = ModuleContextGuard::install(inner);
            panic!("boom under module context");
        }));
        assert!(panicked.is_err());
        assert_eq!(
            current(),
            outer,
            "unwinding must restore the previous context"
        );
    }
    assert!(current().is_null());
}

#[test]
fn module_make_interactive_wraps_specs_like_gnu() {
    let mut fixture = TestEnv::new();
    let env = fixture.env_ptr();
    let func = unsafe {
        module_make_function(
            env,
            0,
            0,
            dummy_module_function,
            std::ptr::null(),
            std::ptr::null_mut(),
        )
    };

    unsafe {
        module_make_interactive(env, func, lisp_to_value(env, Value::NIL));
    }
    let form = value_to_lisp(func)
        .as_module_function()
        .unwrap()
        .interactive_form;
    assert_eq!(
        list_to_vec(&form).unwrap(),
        vec![Value::symbol("interactive")]
    );

    let spec = lisp_to_value(env, Value::string("p"));
    unsafe {
        module_make_interactive(env, func, spec);
    }
    let form = value_to_lisp(func)
        .as_module_function()
        .unwrap()
        .interactive_form;
    assert_eq!(
        list_to_vec(&form).unwrap(),
        vec![Value::symbol("interactive"), Value::string("p")]
    );
}

#[test]
fn active_module_environment_values_are_gc_roots() {
    let mut fixture = TestEnv::new();
    let env = fixture.env_ptr();
    let priv_ptr = fixture.priv_ptr();
    let rooted = Value::string("module-local-root");
    let _value = lisp_to_value(env, rooted);
    unsafe {
        (*priv_ptr).pending_non_local_exit = emacs_funcall_exit::Signal;
        (*priv_ptr).non_local_exit_symbol = Value::symbol("error");
        (*priv_ptr).non_local_exit_data = Value::list(vec![rooted]);
    }

    let active = ActiveModuleEnv::push(priv_ptr);
    let mut roots = Vec::new();
    collect_dynamic_module_gc_roots(&mut roots);
    drop(active);

    assert!(roots.contains(&rooted));
    assert!(roots.contains(&Value::symbol("error")));
}

#[test]
fn module_big_integer_zero_and_null_outputs_match_gnu() {
    let mut fixture = TestEnv::new();
    let env = fixture.env_ptr();

    let zero = unsafe { module_make_big_integer(env, 0, 0, std::ptr::null()) };
    assert_eq!(value_to_lisp(zero), Value::fixnum(0));

    let zero_value = lisp_to_value(env, Value::fixnum(0));
    let ok = unsafe {
        module_extract_big_integer(
            env,
            zero_value,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
    };
    assert!(ok);
    assert_eq!(
        unsafe { (*fixture.priv_ptr()).pending_non_local_exit },
        emacs_funcall_exit::Return
    );
}

#[test]
fn module_extract_big_integer_reports_too_small_buffer() {
    let mut fixture = TestEnv::new();
    let env = fixture.env_ptr();
    let large = Value::make_integer(Integer::from(1u64) << 80u32);
    let value = lisp_to_value(env, large);
    let mut sign = 0;
    let mut count = 1;
    let mut magnitude = [0_u64; 1];

    let ok = unsafe {
        module_extract_big_integer(env, value, &mut sign, &mut count, magnitude.as_mut_ptr())
    };

    assert!(!ok);
    assert_eq!(sign, 1);
    assert_eq!(count, 2);
    let priv_ = unsafe { &*fixture.priv_ptr() };
    assert_eq!(priv_.pending_non_local_exit, emacs_funcall_exit::Signal);
    assert_eq!(
        priv_.non_local_exit_symbol,
        Value::symbol("memory-buffer-too-small")
    );
}

// ------------------------------------------------------------------
// PS-T4: panic containment at the module ABI boundary
// ------------------------------------------------------------------

use crate::emacs_core::eval::{ConditionFrame, ResumeTarget, SpecBinding};

fn string_of(value: Value) -> String {
    String::from_utf8_lossy(value.as_lisp_string().expect("a string").as_bytes()).into_owned()
}

/// A host-defined module function whose panic must be caught by
/// `apply_module_function` — the same-std case containment exists for
/// (foreign Rust modules abort in their own runtime before reaching us).
unsafe extern "C-unwind" fn panicking_module_function(
    _env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    panic!("intentional panic from host module function");
}

/// A host-defined module function that runs elisp via `env->funcall`; the
/// elisp panics (host subr `neovm--internal-panic`), so the panic must be
/// contained by `module_funcall`'s `contain_lisp_panics`, surface as this
/// call's pending exit, and propagate out as an ordinary Lisp error.
unsafe extern "C" fn module_function_calling_panicking_elisp(
    env: *mut emacs_env,
    _nargs: isize,
    _args: *mut emacs_value,
    _data: *mut c_void,
) -> emacs_value {
    unsafe {
        let e = &*env;
        let sym = (e.intern.unwrap())(env, c"neovm--internal-panic".as_ptr());
        (e.funcall.unwrap())(env, sym, 0, std::ptr::null_mut())
    }
}

fn install_module_function(
    ev: &mut Context,
    name: &str,
    func: unsafe extern "C" fn(*mut emacs_env, isize, *mut emacs_value, *mut c_void) -> emacs_value,
) {
    let value = crate::tagged::gc::with_tagged_heap(|h| {
        h.alloc_module_function(
            0,
            0,
            func as *const c_void,
            std::ptr::null_mut(),
            Value::NIL,
            Value::NIL,
        )
    });
    ev.obarray.set_symbol_function(name, value);
}

/// `module_guard` must convert a panic into a pending `error` exit whose
/// message carries the marker + panic text, and return the sentinel.
#[test]
fn module_guard_converts_panic_to_pending_error() {
    let mut fixture = TestEnv::new();
    let env = fixture.env_ptr();

    let out = module_guard(env, 17_i64, || panic!("guard-probe-text"));
    assert_eq!(out, 17);

    let priv_ = unsafe { &*fixture.priv_ptr() };
    assert_eq!(priv_.pending_non_local_exit, emacs_funcall_exit::Signal);
    assert_eq!(priv_.non_local_exit_symbol, Value::symbol("error"));
    let data = list_to_vec(&priv_.non_local_exit_data).unwrap();
    let message = string_of(data[0]);
    assert!(
        message.contains("neomacs internal error") && message.contains("guard-probe-text"),
        "unexpected message: {message}"
    );
}

/// First exit wins (GNU convention): an exit recorded before the panic is
/// preserved by the guard's `set_pending_signal`.
#[test]
fn module_guard_preserves_earlier_pending_exit() {
    let mut fixture = TestEnv::new();
    let env = fixture.env_ptr();
    unsafe {
        set_pending_signal(env, "wrong-type-argument", Value::NIL);
    }

    module_guard(env, (), || panic!("later panic"));

    let priv_ = unsafe { &*fixture.priv_ptr() };
    assert_eq!(
        priv_.non_local_exit_symbol,
        Value::symbol("wrong-type-argument"),
        "the pre-panic exit must win"
    );
}

/// With no MODULE_CTX installed, the guard's probe must still see GC
/// lock poison through the thread heap (the JIT ctx-less arm's probe)
/// and refuse to contain: re-raise, no pending exit recorded. Poison is
/// permanent for this process — fine under nextest's process-per-test.
#[test]
fn module_guard_re_raises_on_poisoned_gc_locks_without_ctx() {
    crate::tagged::gc::with_tagged_heap(|h| h.poison_gc_locks_for_test());
    let mut fixture = TestEnv::new();
    let env = fixture.env_ptr();

    let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
        module_guard(env, 17_i64, || panic!("guard-poison-flee"))
    }));
    let payload = caught.expect_err("poisoned GC locks must re-raise, not contain");
    assert_eq!(panic_message(&*payload), "guard-poison-flee");

    let priv_ = unsafe { &*fixture.priv_ptr() };
    assert_eq!(
        priv_.pending_non_local_exit,
        emacs_funcall_exit::Return,
        "no pending exit may be recorded on the re-raise path"
    );
}

/// `contain_lisp_panics` must restore every boundary-snapshot dimension
/// the panicked extent dirtied and surface the panic as an `error` Flow.
#[test]
fn contain_lisp_panics_restores_boundary_state() {
    let mut ev = Context::new();
    let spec0 = ev.specpdl.len();
    let cond0 = ev.condition_stack.len();
    let bc0 = ev.bc_buf.len();
    let depth0 = ev.depth;
    let roots0 = crate::emacs_core::eval::save_scratch_gc_roots();

    let result: Result<Value, Flow> = contain_lisp_panics(&mut ev, |ctx| {
        ctx.specpdl.push(SpecBinding::Nop);
        ctx.push_condition_frame(ConditionFrame::Catch {
            tag: Value::symbol("neovm-test-tag"),
            resume: ResumeTarget::InterpreterCatch,
        });
        ctx.bc_buf.push(Value::NIL);
        // Skipped scratch-root pops of the panicked extent: the
        // boundary restore must truncate them (they would pin their
        // objects forever otherwise).
        crate::emacs_core::eval::push_scratch_gc_root(Value::NIL);
        crate::emacs_core::eval::push_scratch_gc_root(Value::T);
        ctx.depth += 3;
        panic!("boundary-dirt-probe");
    });

    let Err(Flow::Signal(sig)) = result else {
        panic!("expected a Signal flow");
    };
    assert_eq!(sig.symbol, intern("error"));
    let message = string_of(sig.data[0]);
    assert!(
        message.contains("neomacs internal error") && message.contains("boundary-dirt-probe"),
        "unexpected message: {message}"
    );
    assert_eq!(ev.specpdl.len(), spec0);
    assert_eq!(ev.condition_stack.len(), cond0);
    assert_eq!(ev.bc_buf.len(), bc0);
    assert_eq!(ev.depth, depth0);
    assert_eq!(
        crate::emacs_core::eval::save_scratch_gc_roots(),
        roots0,
        "scratch-root residue truncated by the boundary restore"
    );
}

/// A panic that escaped the GC collection driver must NOT be contained:
/// the original payload is re-raised (and would abort at the extern "C"
/// shim in production).
#[test]
fn contain_lisp_panics_re_raises_when_gc_driver_was_active() {
    let mut ev = Context::new();
    ev.gc_driver_active = true;

    let caught = std::panic::catch_unwind(AssertUnwindSafe(|| {
        let _: Result<Value, Flow> = contain_lisp_panics(&mut ev, |_ctx| panic!("must-flee"));
    }));

    let payload = caught.expect_err("the panic must be re-raised, not contained");
    assert_eq!(panic_message(&*payload), "must-flee");
    ev.gc_driver_active = false;
    assert_eq!(ev.specpdl.len(), 0);
}

/// End-to-end over the real dispatch: a panicking module function becomes
/// a `condition-case`-able `error` carrying the panic text, and afterwards
/// the evaluator still evaluates and a full GC still runs (Task-1 guard
/// regression: `gc_inhibit_depth` must be balanced).
#[test]
fn module_function_panic_is_condition_case_error_and_evaluator_survives() {
    let mut ev = Context::new();
    install_module_function(&mut ev, "neovm-test--panicking-module-fn", {
        // Coerce the C-unwind fn to the vtable's "C" shape for storage;
        // apply_module_function calls through the C-unwind view.
        unsafe {
            std::mem::transmute::<
                unsafe extern "C-unwind" fn(
                    *mut emacs_env,
                    isize,
                    *mut emacs_value,
                    *mut c_void,
                ) -> emacs_value,
                unsafe extern "C" fn(
                    *mut emacs_env,
                    isize,
                    *mut emacs_value,
                    *mut c_void,
                ) -> emacs_value,
            >(panicking_module_function)
        }
    });

    let spec0 = ev.specpdl.len();
    let caught = ev
        .eval_str("(condition-case err (neovm-test--panicking-module-fn) (error (car (cdr err))))")
        .expect("condition-case must catch the contained panic");
    let message = string_of(caught);
    assert!(
        message.contains("neomacs internal error")
            && message.contains("intentional panic from host module function"),
        "unexpected message: {message}"
    );

    assert_eq!(ev.specpdl.len(), spec0, "specpdl must be balanced");
    assert_eq!(ev.gc_inhibit_depth, 0, "GC inhibition must be balanced");
    let sum = ev.eval_str("(+ 1 2)").expect("evaluator must still work");
    assert_eq!(sum, Value::fixnum(3));
    ev.eval_str("(garbage-collect)")
        .expect("a full GC must still run after a contained panic");
}

/// Panic inside module-INVOKED elisp (the `module_funcall` trampoline):
/// module code runs `env->funcall` on a host subr that panics; the panic
/// is contained at `module_funcall`, becomes this call's pending exit, and
/// propagates to `condition-case` like any Lisp error. Also repeats the
/// call to prove no poisoned-lock cascade is left behind.
#[test]
fn panic_in_module_invoked_elisp_is_contained_at_module_funcall() {
    let mut ev = Context::new();
    install_module_function(
        &mut ev,
        "neovm-test--module-calls-panicking-elisp",
        module_function_calling_panicking_elisp,
    );

    for _ in 0..2 {
        let caught = ev
            .eval_str(
                "(condition-case err (neovm-test--module-calls-panicking-elisp) \
                   (error (car (cdr err))))",
            )
            .expect("condition-case must catch the contained panic");
        let message = string_of(caught);
        assert!(
            message.contains("neomacs internal error") && message.contains("neovm--internal-panic"),
            "unexpected message: {message}"
        );
        let sum = ev.eval_str("(+ 20 22)").expect("evaluator must still work");
        assert_eq!(sum, Value::fixnum(42));
    }
    assert_eq!(ev.gc_inhibit_depth, 0);
    ev.eval_str("(garbage-collect)")
        .expect("a full GC must still run after a contained panic");
}
