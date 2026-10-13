//! Real-core coherence tests for the private admission header.
//!
//! Mutable public access is a method-only adapter, never a Drop-dependent
//! whole-manager guard. Raw storage replacement remains constructor/model-only.

use crate::emacs_core::error::{FlowKind, FlowResultExt};
use crate::emacs_core::eval::Context;
use crate::emacs_core::forward::{LispBoolFwd, alloc_boolfwd};
use crate::emacs_core::intern::intern;
use crate::emacs_core::pdump::{
    clone_active_evaluator, restore_snapshot, snapshot_active_evaluator,
};
use crate::emacs_core::symbol::FunctionEpochBump;
use crate::emacs_core::value::Value;
use std::panic::{AssertUnwindSafe, catch_unwind};

fn runtime() -> Context {
    crate::test_utils::init_test_tracing();
    Context::new()
}

fn coherent(context: &Context) {
    assert!(context.obarray.admission_matches_canonical());
    let projected = context.dispatch_projection();
    // Explicitly use the underlying manager: the view's epoch accessor is the
    // owner-header projection, and comparing it to itself would prove nothing.
    let canonical = context.obarray();
    assert_eq!(projected.function_epoch(), canonical.function_epoch());
    assert_eq!(
        projected.debug_on_next_call_armed_fast(),
        canonical.debug_on_next_call_armed_fast()
    );
}

fn debugger_cell(context: &Context) -> &'static LispBoolFwd {
    context
        .obarray()
        .bool_forwarder(intern("debug-on-next-call"))
        .expect("runtime owns a forwarded debugger Boolean")
}

#[test]
fn admission_header_constructors_and_cold_minimal_reset_are_coherent() {
    let context = runtime();
    coherent(&context);
    assert!(
        !context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    drop(context);

    // This constructor performs its whole-manager reset before its own final
    // activation, not beneath a compiled/native continuation. It is the real
    // cold-replacement control; the model additionally enforces its typestate.
    let mut minimal = Context::new_minimal_vm_harness();
    coherent(&minimal);
    minimal.resolve_debug_on_next_call_cell();
    coherent(&minimal);
    assert!(
        !minimal
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
}

#[test]
fn admission_header_real_fset_fmakunbound_and_unintern_publish() {
    let mut context = runtime();
    let before_fset = context.dispatch_projection().function_epoch();
    context
        .eval_str("(fset 'admission-header-function '(lambda () 11))")
        .expect("fset should succeed");
    coherent(&context);
    let after_fset = context.dispatch_projection().function_epoch();
    assert!(after_fset > before_fset);
    assert_eq!(
        context
            .eval_str("(funcall 'admission-header-function)")
            .expect("the installed function is callable"),
        Value::fixnum(11)
    );

    context
        .eval_str("(fmakunbound 'admission-header-function)")
        .expect("fmakunbound should succeed");
    coherent(&context);
    assert!(context.dispatch_projection().function_epoch() > after_fset);
    assert_eq!(
        context
            .eval_str("(fboundp 'admission-header-function)")
            .expect("inspect the unbound function"),
        Value::NIL
    );

    context
        .eval_str("(fset 'admission-header-uninterned '(lambda () 12))")
        .expect("install a second function");
    let before_unintern = context.dispatch_projection().function_epoch();
    assert_eq!(
        context
            .eval_str("(unintern 'admission-header-uninterned nil)")
            .expect("unintern should succeed"),
        Value::T
    );
    coherent(&context);
    assert!(context.dispatch_projection().function_epoch() > before_unintern);
}

#[test]
fn admission_header_whole_resolution_invalidation_retires() {
    let mut context = runtime();
    let before = context.dispatch_projection().function_epoch();
    context.with_obarray_mut(|mut access| {
        access.invalidate_all_function_bindings(FunctionEpochBump::CompilerOverrides);
    });
    coherent(&context);
    let after = context.dispatch_projection().function_epoch();
    assert!(after > before);
    context.with_obarray_mut(|mut access| {
        access.invalidate_all_function_bindings(FunctionEpochBump::SubrRewrite);
    });
    coherent(&context);
    assert!(context.dispatch_projection().function_epoch() > after);
}

#[test]
fn admission_header_funcall_callback_redefinition_invalidates_warm_resolution() {
    let mut context = runtime();
    context
        .eval_str(
            "(progn
               (fset 'admission-header-target '(lambda () 1))
               (fset 'admission-header-redefine
                     '(lambda (ignored)
                        (fset 'admission-header-target '(lambda () 2))
                        (admission-header-target))))",
        )
        .expect("install the callback and its target");
    for _ in 0..2 {
        assert_eq!(
            context
                .eval_str("(admission-header-target)")
                .expect("warm the named call target"),
            Value::fixnum(1)
        );
    }
    let before = context.dispatch_projection().function_epoch();
    assert_eq!(
        context
            .apply1(Value::symbol("admission-header-redefine"), Value::NIL)
            .expect("callback observes its own redefinition before returning"),
        Value::fixnum(2)
    );
    coherent(&context);
    assert!(context.dispatch_projection().function_epoch() > before);
    assert_eq!(
        context
            .eval_str("(funcall 'admission-header-target)")
            .expect("subsequent funcall sees the redefinition"),
        Value::fixnum(2)
    );
}

#[test]
fn admission_header_private_writer_panic_retires_before_recovery() {
    let mut context = runtime();
    // Ensure the symbol slot exists before the unchanged-storage raw phase.
    context
        .obarray_mut()
        .set_symbol_function("admission-header-panic", Value::fixnum(16));
    let before = context.dispatch_projection().function_epoch();
    let descriptor = alloc_boolfwd(true);
    let result = catch_unwind(AssertUnwindSafe(|| {
        // SAFETY: this is one noncalling legacy storage phase on the current
        // owner's view. It neither replaces/grows the manager storage domain
        // nor changes its generation, and no canonical reader overlaps. The
        // function change uses its canonical producer; descriptor installation
        // uses its canonical setter. Panic happens AFTER these completed writes,
        // inside the phase, so private retirement must run while unwinding.
        unsafe {
            context.obarray.with_legacy_storage_write(|obarray| {
                obarray.set_symbol_function("admission-header-panic", Value::fixnum(17));
                obarray.install_boolfwd(intern("debug-on-next-call"), descriptor);
                panic!("permitted current-writer panic after canonical publication");
            });
        }
    }));
    assert!(result.is_err());
    coherent(&context);
    assert!(context.dispatch_projection().function_epoch() > before);
    assert!(
        context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    // Do not enter Lisp with the deliberately armed debugger fixture.
    descriptor.set(false);
    coherent(&context);
    assert_eq!(
        context.eval_str("(+ 20 22)").expect("owner remains usable"),
        Value::fixnum(42)
    );
}

#[test]
fn admission_header_absent_lazy_resolution_and_later_install_are_coherent() {
    crate::test_utils::init_test_tracing();
    let mut context = Context::new_minimal_vm_harness();
    context.resolve_debug_on_next_call_cell();
    coherent(&context);
    assert!(
        context
            .obarray()
            .bool_forwarder(intern("debug-on-next-call"))
            .is_none()
    );
    assert!(
        !context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );

    let descriptor = alloc_boolfwd(false);
    context.with_obarray_mut(|mut access| {
        access.install_boolfwd(intern("debug-on-next-call"), descriptor);
    });
    coherent(&context);
    descriptor.set(true);
    assert!(
        context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    descriptor.set(false);
    coherent(&context);
}

#[test]
fn admission_header_live_boolean_and_callback_argument_survive_collection() {
    let mut context = runtime();
    let descriptor = debugger_cell(&context);
    let before_flag = context
        .dispatch_projection()
        .debug_on_next_call_armed_fast();
    assert!(!before_flag);
    descriptor.set(true);
    context.gc_collect_exact();
    coherent(&context);
    assert!(
        context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    // A copied old Boolean is an observation, not authority for a new admission.
    assert!(!before_flag);
    descriptor.set(false);

    context
        .eval_str(
            "(fset 'admission-header-gc-callback
                   '(lambda (value)
                      (garbage-collect)
                      (setq debug-on-next-call t)
                      value))",
        )
        .expect("install a forced-GC callback");
    let argument = Value::cons(Value::string("rooted callback argument"), Value::NIL);
    let result = context
        .apply1(Value::symbol("admission-header-gc-callback"), argument)
        .expect("callback completes despite forced collection");
    coherent(&context);
    assert!(
        context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    assert_eq!(
        result.cons_car().as_str_owned().as_deref(),
        Some("rooted callback argument")
    );
    descriptor.set(false);
    coherent(&context);
}

#[test]
fn admission_header_cloned_evaluator_owns_independent_boolean_descriptor() {
    let mut original = runtime();
    let original_cell = debugger_cell(&original);
    original_cell.set(false);
    let cloned = clone_active_evaluator(&mut original).expect("clone the active runtime");
    coherent(&original);
    coherent(&cloned);
    let clone_cell = debugger_cell(&cloned);
    assert!(!std::ptr::eq(original_cell, clone_cell));
    original_cell.set(true);
    assert!(
        original
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    assert!(!cloned.dispatch_projection().debug_on_next_call_armed_fast());
    original_cell.set(false);
    clone_cell.set(true);
    assert!(
        !original
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    assert!(cloned.dispatch_projection().debug_on_next_call_armed_fast());
    clone_cell.set(false);
    coherent(&original);
    coherent(&cloned);
}

#[test]
fn admission_header_dump_restore_initializes_surface_then_tracks_new_writers() {
    let mut original = runtime();
    original
        .eval_str("(fset 'admission-header-restored '(lambda () 31))")
        .expect("install the dumped function");
    let snapshot = snapshot_active_evaluator(&mut original);
    drop(original);
    let mut restored = restore_snapshot(&snapshot).expect("restore the serialized runtime");
    coherent(&restored);
    assert_eq!(
        restored
            .eval_str("(funcall 'admission-header-restored)")
            .expect("restored function surface is callable"),
        Value::fixnum(31)
    );
    let before = restored.dispatch_projection().function_epoch();
    restored
        .eval_str("(fset 'admission-header-restored '(lambda () 32))")
        .expect("redefine the restored function");
    coherent(&restored);
    assert!(restored.dispatch_projection().function_epoch() > before);
    assert_eq!(
        restored
            .eval_str("(admission-header-restored)")
            .expect("new owner observes the redefinition"),
        Value::fixnum(32)
    );
}

#[test]
fn admission_header_forgetting_public_method_adapter_cannot_skip_retirement() {
    let mut context = runtime();
    let before = context.dispatch_projection().function_epoch();
    let mut access = context.obarray_mut();
    access.set_symbol_function("admission-header-forgotten-access", Value::fixnum(43));
    // The adapter owns no pending retirement. Every mutating method completed
    // its private scope before this safe forget, unlike a public Drop publisher.
    std::mem::forget(access);
    coherent(&context);
    assert!(context.dispatch_projection().function_epoch() > before);
}

#[test]
fn admission_header_data_only_writes_keep_dispatch_metadata_unchanged() {
    let mut context = runtime();
    let before = context.dispatch_projection().function_epoch();
    let descriptor = debugger_cell(&context);
    let id = intern("admission-header-value-only");
    context.with_obarray_mut(|mut access| {
        access.set_symbol_value_id(id, Value::fixnum(5));
        access
            .put_property_id(id, intern("admission-header-property"), Value::fixnum(6))
            .expect("a pure property update succeeds");
    });
    coherent(&context);
    assert_eq!(context.dispatch_projection().function_epoch(), before);
    assert!(std::ptr::eq(descriptor, debugger_cell(&context)));
    assert!(
        !context
            .dispatch_projection()
            .debug_on_next_call_armed_fast()
    );
    assert_eq!(
        context.obarray().symbol_value_id_copied(id),
        Some(Value::fixnum(5))
    );
    // This proves unchanged dispatch semantics, not a store-frequency count.
    // No production counter is added merely for this test.
}

#[derive(Clone, Copy, Debug)]
enum CallbackExit {
    Signal,
    Throw,
}

#[test]
fn admission_header_callback_signal_and_throw_preserve_current_owner_coherence() {
    for exit in [CallbackExit::Signal, CallbackExit::Throw] {
        let mut context = runtime();
        let definition = match exit {
            CallbackExit::Signal => {
                "(fset 'admission-header-exit
                       '(lambda (ignored)
                          (fset 'admission-header-exit-target '(lambda () 51))
                          (signal 'error '(\"admission header signal\"))))"
            }
            CallbackExit::Throw => {
                "(fset 'admission-header-exit
                       '(lambda (ignored)
                          (fset 'admission-header-exit-target '(lambda () 52))
                          (throw 'admission-header-tag 53)))"
            }
        };
        context.eval_str(definition).expect("install the callback");
        let before = context.dispatch_projection().function_epoch();
        // A real catch target must be active: GNU turns an unmatched throw into
        // a no-catch signal before Flow transport, which would not test Throw.
        match exit {
            CallbackExit::Signal => {}
            CallbackExit::Throw => context.push_condition_frame(super::ConditionFrame::Catch {
                tag: Value::symbol("admission-header-tag"),
                resume: super::ResumeTarget::InterpreterCatch,
            }),
        }
        let result = context
            .apply1(Value::symbol("admission-header-exit"), Value::NIL)
            .kinded();
        match exit {
            CallbackExit::Signal => {}
            CallbackExit::Throw => {
                context.pop_condition_frame();
            }
        }
        match exit {
            CallbackExit::Signal => assert!(matches!(result, Err(FlowKind::Signal(_)))),
            CallbackExit::Throw => assert!(matches!(result, Err(FlowKind::Throw(_)))),
        }
        coherent(&context);
        assert!(context.dispatch_projection().function_epoch() > before);
        assert_eq!(
            context
                .eval_str("(admission-header-exit-target)")
                .expect("owner remains callable after nonlocal flow"),
            Value::fixnum(match exit {
                CallbackExit::Signal => 51,
                CallbackExit::Throw => 52,
            })
        );
    }
}
