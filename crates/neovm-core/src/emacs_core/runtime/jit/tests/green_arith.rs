//! Baseline arithmetic call boundaries survive first admission and handoff.
//! The source initially calls a real arithmetic builtin, so compilation emits
//! the Level-B native operation and records the binding as an inline
//! dependency (`RtCtx::release_epoch`). A peer rewrites the binding while the
//! original native caller is parked inside a call, then collects. The
//! park-returning call is a release point: its return-side revalidation sees
//! the moved epoch and deopts with resume-AFTER semantics (the completed call
//! is never re-executed), so the interpreter resolves `logand` through the
//! symbol and delivers the NEW definition — GNU's observable behavior, not
//! the baked op.

use crate::emacs_core::bytecode::{ByteCodeFunction, Op, StackDepth};
use crate::emacs_core::error::{EvalResult, signal};
use crate::emacs_core::eval::{Context, ThreadingMode};
use crate::emacs_core::jit::{self, bg, cache, compile};
use crate::emacs_core::subr::SubrSpec;
use crate::emacs_core::value::{LambdaParams, Value};

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut source = ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
    source.lexical = true;
    source.ops = ops;
    source.constants = constants.into();
    source.max_stack = StackDepth::for_test(16);
    source.seal_hand_assembled_ops();
    source
        .jit_runtime()
        .set_heat_for_test(jit::hot_threshold().saturating_mul(64));
    source
}

fn native_witness(_context: &mut Context) -> EvalResult {
    if cache::native_depth() == 0 {
        return Err(signal(
            "error",
            vec![Value::string(
                "arithmetic continuation lost its native frame",
            )],
        ));
    }
    Ok(Value::T)
}

fn redefine_and_collect(context: &mut Context) -> EvalResult {
    let mut replacement = ByteCodeFunction::new(LambdaParams::simple(vec![
        crate::emacs_core::intern::intern("green-arith-a"),
        crate::emacs_core::intern::intern("green-arith-b"),
    ]));
    replacement.lexical = true;
    replacement.ops = vec![Op::Constant(0), Op::Return];
    replacement.constants = vec![Value::fixnum(37)].into();
    replacement.max_stack = StackDepth::for_test(16);
    replacement.seal_hand_assembled_ops();
    let replacement = Value::make_bytecode(replacement);
    context.with_obarray_mut(|mut obarray| {
        obarray.set_symbol_function("logand", replacement);
    });
    context.apply(Value::symbol("garbage-collect"), Vec::new())?;
    Ok(Value::symbol("peer-finished"))
}

fn switch_from_native(context: &mut Context) -> EvalResult {
    native_witness(context)?;
    let peer = context.apply(
        Value::symbol("make-thread"),
        vec![Value::symbol("green-arith-peer")],
    )?;
    // In the admission fixture make-thread just projected the cells with this
    // compiled arithmetic activation still live on the original native stack.
    native_witness(context)?;
    let result = context.apply(Value::symbol("thread-join"), vec![peer])?;
    if result != Value::symbol("peer-finished") {
        return Err(signal(
            "error",
            vec![Value::string("arithmetic peer failed"), result],
        ));
    }
    native_witness(context)
}

fn exercise(preadmitted: bool, osr: bool) {
    struct Scope {
        profit: compile::ProfitGateTestPolicy,
        jit_off: bool,
        arithmetic: bool,
    }
    impl Drop for Scope {
        fn drop(&mut self) {
            compile::replace_profit_gate_for_test(self.profit);
            jit::force_jit_off_for_test(self.jit_off);
            compile::force_inline_arith_for_test(self.arithmetic);
        }
    }
    let scope = Scope {
        profit: compile::replace_profit_gate_for_test(compile::ProfitGateTestPolicy::Bypass),
        jit_off: jit::jit_forced_off_for_test(),
        arithmetic: compile::jit_inline_arith_on(),
    };
    jit::force_jit_off_for_test(false);
    compile::force_inline_arith_for_test(true);
    let _scope = scope;
    let _backend = compile::opt_mode_scope_for_test(compile::OptMode::Off);
    let _background = bg::TestModeScope::enter(bg::BgMode::Sync);
    let mut context = Context::new_with_threading(ThreadingMode::Green);
    context.set_lexical_binding(true);
    context.register_subr(SubrSpec::fixed0(
        "green-arith-native-witness",
        native_witness,
    ));
    context.register_subr(SubrSpec::fixed0("green-arith-switch", switch_from_native));
    context.register_subr(SubrSpec::fixed0("green-arith-peer", redefine_and_collect));
    context
        .apply(
            Value::symbol("symbol-function"),
            vec![Value::symbol("logand")],
        )
        .unwrap();
    if preadmitted {
        context
            .eval_str("(thread-join (make-thread (lambda () nil)))")
            .unwrap();
    }
    // The witness call must sit BEFORE the switch call: the switch's return is
    // the release point where the moved epoch deopts (resume-AFTER — the
    // completed call is never re-executed), so everything after it runs
    // interpreted. `switch_from_native` carries its own post-join witness
    // internally, still native inside the call.
    let ops = if osr {
        vec![
            Op::Constant(5),
            Op::StackRef(0),
            Op::Constant(6),
            Op::Lss,
            Op::GotoIfNil(13),
            Op::Constant(3),
            Op::Call(0),
            Op::Pop,
            Op::Constant(4),
            Op::Call(0),
            Op::Pop,
            Op::Add1,
            Op::Goto(1),
            Op::Pop,
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Call(2),
            Op::Return,
        ]
    } else {
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Call(2),
            Op::Pop,
            Op::Constant(3),
            Op::Call(0),
            Op::Pop,
            Op::Constant(4),
            Op::Call(0),
            Op::Pop,
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Call(2),
            Op::Return,
        ]
    };
    let source = Value::make_bytecode(function(
        ops,
        vec![
            Value::symbol("logand"),
            Value::fixnum(7),
            Value::fixnum(3),
            Value::symbol("green-arith-native-witness"),
            Value::symbol("green-arith-switch"),
            Value::fixnum(0),
            Value::fixnum(1),
        ],
    ));
    context
        .with_obarray_mut(|mut obarray| obarray.set_symbol_function("green-arith-caller", source));
    let source_root = context.retain_gc_roots([source]);
    let source_data = source.get_bytecode_data().unwrap();
    // Compilation happens before the make-thread inside the native callback.
    let result = if osr {
        cache::prepare_osr_for_test(&mut context, source_data, 1);
        let _leaf = cache::osr_leaf_ptr_for_test(source_data, 1).expect("baseline OSR leaf");
        // SAFETY: `ctx` is the live `Context` this test owns; the call runs in this thread under the dormant-Context contract.
        let run =
            unsafe { cache::try_run_osr(&mut context, source_data, 1, &[Value::fixnum(0)], &[]) }
                .expect("the exact snapshot enters native OSR");
        assert!(
            cache::osr_leaf_ptr_for_test(source_data, 1).is_none(),
            "epoch move retires OSR admission"
        );
        // The park happened inside the switch call, so the OSR continuation's
        // FIRST release point after the redefinition is that call's return:
        // the return-side revalidation deopts with resume-AFTER semantics (a
        // completed call is never re-executed) and the interpreter delivers
        // the NEW definition for the remaining `logand` call.
        let compile::NativeRun::DeoptAt(resume) = run else {
            panic!(
                "the redefinition must deopt the parked OSR continuation at the release point: {run:?}"
            );
        };
        let resume = *resume;
        crate::emacs_core::bytecode::vm::Vm::from_context(&mut context).run_resumed_frame(
            source_data,
            source,
            resume.pc,
            &resume.stack,
            resume.handlers,
            &resume.binds,
            resume.spec_base,
            resume.cond_base,
        )
    } else {
        cache::prepare_entry_for_test(&mut context, source_data);
        let id = source_data.jit_runtime().compiled_id().unwrap();
        let original_pointer =
            cache::compiled_leaf_ptr_for_test(id).expect("original baseline leaf");
        // SAFETY: the source's current entry owns this pointer. Retain its Rc
        // for diagnostic inspection even if the production retirement is broken.
        let original = unsafe { cache::current_leaf_of(&(*original_pointer).obs) }
            .expect("the original leaf is still the source's current entry");
        assert!(
            cache::inline_dependent_count_for_test(crate::emacs_core::intern::intern("logand")) > 0
        );
        let precise_before = cache::native_run_counters().2;
        let result = context.apply(Value::symbol("green-arith-caller"), Vec::new());
        // Exactly one precise deopt: the park happened inside the switch
        // call, and its return-side revalidation is the first release point
        // that can observe the moved epoch. Every earlier check passed with
        // the epoch still valid, and after the deopt the frame runs
        // interpreted to completion.
        assert_eq!(
            cache::native_run_counters().2,
            precise_before + 1,
            "the park-returning call must revalidate and deopt exactly once"
        );
        let current_pointer = cache::compiled_leaf_ptr_for_test(id);
        assert!(
            original.retired.get(),
            "the exact original arithmetic leaf must retire: original={original_pointer:p}, current={current_pointer:?}"
        );
        // The apply seam arms a successful native result. That re-arm may
        // compile a replacement; it must never expose the old definition's
        // retired leaf again.
        assert_ne!(
            current_pointer,
            Some(original_pointer),
            "future native admission must use a fresh leaf after redefinition"
        );
        result
    };
    // The parked continuation delivers the NEW definition through the
    // release-point deopt: the interpreter resolves `logand` through the
    // symbol, GNU-exact pickup.
    assert_eq!(result.unwrap(), Value::fixnum(37));
    let fresh = context
        .apply(Value::symbol("green-arith-caller"), Vec::new())
        .unwrap();
    assert_eq!(
        fresh,
        Value::fixnum(37),
        "the next admission sees the replacement"
    );
    context.green_teardown();
    drop(source_root);
    assert_eq!(context.jit_root_stack_top, 0);
    assert_eq!(cache::native_depth(), 0);
}

#[test]
fn green_baseline_arithmetic_admits_threads_with_native_frame_live() {
    exercise(false, false);
}

#[test]
fn green_baseline_arithmetic_resumes_after_peer_redefinition_and_collection() {
    exercise(true, false);
}

#[test]
fn green_baseline_arithmetic_osr_completes_after_peer_redefinition() {
    exercise(true, true);
}
