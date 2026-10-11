//! Activation-local native callback capabilities retain GNU's funcall protocol.
//! These tests call the checked seam directly so they exercise both knob states.

use super::CheckedNativeCallback;
use crate::emacs_core::error::{EvalResult, Flow, LispCondition, signal};
use crate::emacs_core::eval::{
    Context, SpecBinding, SubrEntry, global_subr_lookup_count, lookup_global_subr_entry,
    register_global_subr_entry, reset_global_subr_lookup_count,
};
use crate::emacs_core::intern::intern;
use crate::emacs_core::print::print_value;
use crate::emacs_core::subr::{FixedMin1, FixedMin2, FixedMin3, SubrSpec};
use crate::emacs_core::value::Value;
use crate::tagged::header::SubrFn;

fn original(_: &mut Context, _: Value) -> EvalResult {
    Ok(Value::symbol("native-callback-original"))
}

fn replacement(_: &mut Context, _: Value) -> EvalResult {
    Ok(Value::symbol("native-callback-replacement"))
}

fn optional2(_: &mut Context, first: Value, second: Value) -> EvalResult {
    Ok(Value::list(vec![first, second]))
}

fn optional3(_: &mut Context, first: Value, second: Value, third: Value) -> EvalResult {
    Ok(Value::list(vec![first, second, third]))
}

fn prepared(
    ctx: &mut Context,
    name: &str,
    nargs: usize,
) -> (Value, Value, u64, CheckedNativeCallback) {
    let designator = Value::symbol(name);
    let epoch = ctx.obarray().function_epoch();
    let subr = ctx
        .obarray()
        .symbol_function(name)
        .expect("registered builtin");
    let proof = CheckedNativeCallback::resolve(subr, nargs).expect("accepted native arity");
    (designator, subr, epoch, proof)
}

#[test]
fn native_callback_cache_reuses_fixed_body_without_interactive_metadata_reads() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    ctx.register_subr(SubrSpec::fixed1(
        "native-callback-cache-reuse",
        original,
        FixedMin1::One,
    ));
    let (designator, subr, epoch, proof) = prepared(&mut ctx, "native-callback-cache-reuse", 1);
    reset_global_subr_lookup_count();
    for arg in [Value::fixnum(1), Value::fixnum(2)] {
        assert_eq!(
            ctx.apply1_checked_subr(designator, subr, epoch, proof, arg)
                .expect("checked callback"),
            Value::symbol("native-callback-original")
        );
    }
    assert_eq!(
        global_subr_lookup_count(),
        0,
        "a copied call body needs no command metadata"
    );
}

#[test]
fn native_callback_cache_nil_pads_optional_native_slots() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    ctx.register_subr(SubrSpec::fixed2(
        "native-callback-cache-optional-two",
        optional2,
        FixedMin2::One,
    ));
    ctx.register_subr(SubrSpec::fixed3(
        "native-callback-cache-optional-three",
        optional3,
        FixedMin3::One,
    ));
    let two_slots = ctx
        .obarray()
        .symbol_function("native-callback-cache-optional-two")
        .expect("two-slot builtin");
    assert!(CheckedNativeCallback::resolve(two_slots, 0).is_none());
    assert!(CheckedNativeCallback::resolve(two_slots, 3).is_none());
    assert!(CheckedNativeCallback::resolve(Value::fixnum(17), 1).is_none());
    for (name, expected1, expected2) in [
        ("native-callback-cache-optional-two", "(11 nil)", "(11 22)"),
        (
            "native-callback-cache-optional-three",
            "(11 nil nil)",
            "(11 22 nil)",
        ),
    ] {
        let (designator, subr, epoch, proof) = prepared(&mut ctx, name, 1);
        let value = ctx
            .apply1_checked_subr(designator, subr, epoch, proof, Value::fixnum(11))
            .expect("one native argument");
        assert_eq!(print_value(&value), expected1);
        let (_, _, epoch, proof) = prepared(&mut ctx, name, 2);
        let value = ctx
            .apply2_checked_subr(
                designator,
                subr,
                epoch,
                proof,
                Value::fixnum(11),
                Value::fixnum(22),
            )
            .expect("two native arguments");
        assert_eq!(print_value(&value), expected2);
    }
}

#[test]
fn native_callback_cache_raw_body_rewrite_invalidates_without_function_epoch_change() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let name = "native-callback-cache-raw-body";
    ctx.register_subr(SubrSpec::fixed1(name, original, FixedMin1::One));
    let (designator, subr, epoch, proof) = prepared(&mut ctx, name, 1);
    let sym = intern(name);
    let entry = lookup_global_subr_entry(sym).expect("original entry");
    register_global_subr_entry(
        sym,
        SubrEntry {
            function: Some(SubrFn::A1(replacement)),
            ..entry
        },
    );
    assert_eq!(
        ctx.obarray().function_epoch(),
        epoch,
        "raw registration does not write the function cell"
    );
    assert_eq!(
        ctx.obarray().symbol_function(name),
        Some(subr),
        "object identity stays fixed"
    );
    assert_eq!(
        ctx.apply1_checked_subr(designator, subr, epoch, proof, Value::NIL)
            .expect("raw replacement callback"),
        Value::symbol("native-callback-replacement")
    );
}

#[test]
fn native_callback_cache_raw_same_body_arity_rewrite_retains_deferred_error() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let name = "native-callback-cache-raw-arity";
    ctx.register_subr(SubrSpec::fixed2(name, optional2, FixedMin2::One));
    let (designator, subr, epoch, proof) = prepared(&mut ctx, name, 1);
    let sym = intern(name);
    let entry = lookup_global_subr_entry(sym).expect("original entry");
    register_global_subr_entry(
        sym,
        SubrEntry {
            min_args: 2,
            ..entry
        },
    );
    assert_eq!(ctx.obarray().function_epoch(), epoch);
    let depth = ctx.depth;
    let specpdl = ctx.specpdl.len();
    let expected = crate::emacs_core::format_eval_result(
        &ctx.apply1(designator, Value::NIL)
            .map_err(crate::emacs_core::error::map_flow),
    );
    let actual = crate::emacs_core::format_eval_result(
        &ctx.apply1_checked_subr(designator, subr, epoch, proof, Value::NIL)
            .map_err(crate::emacs_core::error::map_flow),
    );
    assert_eq!(actual, expected);
    assert!(actual.contains("wrong-number-of-arguments"));
    assert_eq!((ctx.depth, ctx.specpdl.len()), (depth, specpdl));
}

#[test]
fn native_callback_cache_typed_registration_rewrite_calls_replacement() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let name = "native-callback-cache-typed-body";
    ctx.register_subr(SubrSpec::fixed1(name, original, FixedMin1::One));
    let (designator, subr, epoch, proof) = prepared(&mut ctx, name, 1);
    ctx.register_subr(SubrSpec::fixed1(name, replacement, FixedMin1::One));
    assert_ne!(ctx.obarray().function_epoch(), epoch);
    assert_eq!(ctx.obarray().symbol_function(name), Some(subr));
    assert_eq!(
        ctx.apply1_checked_subr(designator, subr, epoch, proof, Value::NIL)
            .expect("typed replacement callback"),
        Value::symbol("native-callback-replacement")
    );
}

#[test]
fn native_callback_cache_validates_after_debugger_redefines_designator() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let name = "native-callback-cache-debug-redefinition";
    ctx.register_subr(SubrSpec::fixed1(name, original, FixedMin1::One));
    ctx.eval_str(
        "(setq native-callback-debug-log nil
               debugger (lambda (&rest args)
                          (if (eq (car args) 'exit)
                              (car (cdr args))
                            (setq native-callback-debug-log args)
                            (fset 'native-callback-cache-debug-redefinition
                                  (lambda (_) 'native-callback-debug-replacement))
                            nil)))",
    )
    .expect("debugger setup");
    let (designator, subr, epoch, proof) = prepared(&mut ctx, name, 1);
    let depth = ctx.depth;
    let specpdl = ctx.specpdl.len();
    ctx.assign("debug-on-next-call", Value::T);
    assert_eq!(
        ctx.apply1_checked_subr(designator, subr, epoch, proof, Value::fixnum(5))
            .expect("debugger-mutated callback"),
        Value::symbol("native-callback-debug-replacement")
    );
    assert_eq!(
        print_value(
            &ctx.obarray()
                .symbol_value_copied("native-callback-debug-log")
                .expect("debug log")
        ),
        "(lambda)"
    );
    assert_eq!((ctx.depth, ctx.specpdl.len()), (depth, specpdl));
}

#[test]
fn native_callback_cache_validates_after_gc_hook_redefines_designator() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    let name = "native-callback-cache-gc-redefinition";
    ctx.register_subr(SubrSpec::fixed1(name, original, FixedMin1::One));
    ctx.eval_str(
        "(setq native-callback-gc-hook-ran nil
               post-gc-hook
               (list (lambda ()
                       (setq native-callback-gc-hook-ran t post-gc-hook nil)
                       (fset 'native-callback-cache-gc-redefinition
                             (lambda (_) 'native-callback-gc-replacement)))))",
    )
    .expect("GC hook setup");
    let (designator, subr, epoch, proof) = prepared(&mut ctx, name, 1);
    ctx.gc_stress = true;
    ctx.tagged_heap.set_gc_threshold(1);
    let gc_count = ctx.gc_count;
    let depth = ctx.depth;
    let specpdl = ctx.specpdl.len();
    let value = ctx
        .apply1_checked_subr(designator, subr, epoch, proof, Value::fixnum(5))
        .expect("GC-mutated callback");
    assert!(ctx.gc_count > gc_count);
    assert_eq!(
        ctx.obarray()
            .symbol_value_copied("native-callback-gc-hook-ran"),
        Some(Value::T)
    );
    assert_eq!(value, Value::symbol("native-callback-gc-replacement"));
    assert_eq!((ctx.depth, ctx.specpdl.len()), (depth, specpdl));
}

#[test]
fn native_callback_cache_preserves_backtrace_argument_roots_during_collection() {
    crate::test_utils::init_test_tracing();
    fn collect_one(ctx: &mut Context, arg: Value) -> EvalResult {
        let Some(SpecBinding::Backtrace1 {
            function,
            arg: rooted,
            ..
        }) = ctx.specpdl.last()
        else {
            panic!("one-argument callback frame must be live before the body");
        };
        assert_eq!(*function, Value::symbol("native-callback-cache-root-one"));
        assert_eq!(*rooted, arg);
        ctx.gc_collect_exact();
        assert_eq!(arg.as_utf8_str(), Some("one argument"));
        Ok(arg)
    }
    fn collect_two(ctx: &mut Context, arg0: Value, arg1: Value) -> EvalResult {
        let Some(SpecBinding::Backtrace2 {
            function,
            arg0: rooted0,
            arg1: rooted1,
        }) = ctx.specpdl.last()
        else {
            panic!("two-argument callback frame must be live before the body");
        };
        assert_eq!(*function, Value::symbol("native-callback-cache-root-two"));
        assert_eq!((*rooted0, *rooted1), (arg0, arg1));
        ctx.gc_collect_exact();
        assert_eq!(arg0.as_utf8_str(), Some("first argument"));
        assert_eq!(arg1.as_utf8_str(), Some("second argument"));
        Ok(arg1)
    }
    let mut ctx = Context::new();
    ctx.register_subr(SubrSpec::fixed1(
        "native-callback-cache-root-one",
        collect_one,
        FixedMin1::One,
    ));
    ctx.register_subr(SubrSpec::fixed2(
        "native-callback-cache-root-two",
        collect_two,
        FixedMin2::Two,
    ));
    let (designator, subr, epoch, proof) = prepared(&mut ctx, "native-callback-cache-root-one", 1);
    let depth = ctx.depth;
    let specpdl = ctx.specpdl.len();
    let arg = Value::string("one argument");
    let value = ctx
        .apply1_checked_subr(designator, subr, epoch, proof, arg)
        .expect("rooted one-argument callback");
    assert_eq!(value, arg);
    assert_eq!((ctx.depth, ctx.specpdl.len()), (depth, specpdl));
    let (designator, subr, epoch, proof) = prepared(&mut ctx, "native-callback-cache-root-two", 2);
    let arg0 = Value::string("first argument");
    let arg1 = Value::string("second argument");
    let value = ctx
        .apply2_checked_subr(designator, subr, epoch, proof, arg0, arg1)
        .expect("rooted two-argument callback");
    assert_eq!(value, arg1);
    assert_eq!((ctx.depth, ctx.specpdl.len()), (depth, specpdl));
}

#[test]
fn native_callback_cache_preserves_signal_throw_and_debug_exit_unwind() {
    crate::test_utils::init_test_tracing();
    fn unwind_probe(ctx: &mut Context, mode: Value) -> EvalResult {
        let mode = mode.as_int().expect("numeric probe mode");
        if mode == 2 {
            assert!(ctx.set_backtrace_debug_on_exit(ctx.specpdl.len() - 1, true));
        }
        ctx.specbind_resolved(intern("native-callback-bound"), Value::fixnum(42))?;
        let value = Value::string("native-callback-result");
        match mode {
            0 => Err(signal(LispCondition::Error, vec![value])),
            1 => Err(Flow::throw(
                Value::symbol("native-callback-missing-tag"),
                value,
            )),
            _ => Ok(value),
        }
    }
    let mut ctx = Context::new();
    ctx.register_subr(SubrSpec::fixed1(
        "native-callback-cache-unwind",
        unwind_probe,
        FixedMin1::One,
    ));
    for mode in 0..3 {
        let mut results = Vec::new();
        for checked in [false, true] {
            ctx.eval_str(
                "(setq native-callback-bound 7
                       native-callback-signal-log nil
                       native-callback-exit-log nil
                       debugger (lambda (&rest args)
                                  (setq native-callback-exit-log args)
                                  (garbage-collect)
                                  'native-callback-debug-result)
                       signal-hook-function
                       (lambda (symbol data)
                         (setq native-callback-signal-log
                               (list symbol data native-callback-bound))))",
            )
            .expect("unwind setup");
            let (designator, subr, epoch, proof) =
                prepared(&mut ctx, "native-callback-cache-unwind", 1);
            let depth = ctx.depth;
            let specpdl = ctx.specpdl.len();
            let result = if checked {
                ctx.apply1_checked_subr(designator, subr, epoch, proof, Value::fixnum(mode))
            } else {
                ctx.apply1(designator, Value::fixnum(mode))
            };
            let outcome = crate::emacs_core::format_eval_result(
                &result.map_err(crate::emacs_core::error::map_flow),
            );
            assert_eq!((ctx.depth, ctx.specpdl.len()), (depth, specpdl));
            assert_eq!(
                ctx.obarray().symbol_value_copied("native-callback-bound"),
                Some(Value::fixnum(7))
            );
            let signal_log = print_value(
                &ctx.obarray()
                    .symbol_value_copied("native-callback-signal-log")
                    .expect("signal log"),
            );
            let exit_log = print_value(
                &ctx.obarray()
                    .symbol_value_copied("native-callback-exit-log")
                    .expect("exit log"),
            );
            results.push((outcome, signal_log, exit_log));
        }
        assert_eq!(
            results[0], results[1],
            "mode {mode}: checked and generic protocols agree"
        );
    }
}
