//! Native selected sqrt behavior follows independent sealed Tier-0 baselines.
//! Frozen GNU19 sqrt observations supply the argument/identity cases.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::{captured_clif, function};
use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::jit::tier2::{CompileTier, T2Upgrade};
use crate::emacs_core::print::print_value;

// Cranelift prints the controlling type when the payload is defined before
// a guard in another block. Match the actual instruction token in both forms.
fn has_native_sqrt(clif: &str) -> bool {
    clif.lines()
        .filter_map(|line| line.split_once(" = "))
        .filter_map(|(_, instruction)| instruction.split_whitespace().next())
        .any(|op| matches!(op, "sqrt" | "sqrt.f64"))
}

struct Settings;
impl Settings {
    fn enter() -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses::default()));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        force_deopt_for_test(false);
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
        force_deopt_for_test(false);
    }
}
struct Roots(usize);
impl Roots {
    fn new(values: &[Value]) -> Self {
        let n = save_scratch_gc_roots();
        push_scratch_gc_roots(values);
        Self(n)
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        restore_scratch_gc_roots(self.0);
    }
}
fn tier0(ctx: &mut Context, f: &ByteCodeFunction, args: &[Value]) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.execute(f, args.to_vec())
}
fn resume(ctx: &mut Context, f: &ByteCodeFunction, exit: &DeoptResume) -> Result<Value, Flow> {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    vm.run_resumed_frame(
        f,
        Value::NIL,
        exit.pc,
        &exit.stack,
        exit.handlers,
        &exit.binds,
        exit.spec_base,
        exit.cond_base,
    )
}
fn compile(ctx: &Context, f: &ByteCodeFunction, sink: bool) -> (CompiledLeaf, String) {
    force_opt_passes_for_test(Some(OptPasses {
        sink,
        ..OptPasses::default()
    }));
    let mut leaf = None;
    let clif = captured_clif(|| {
        leaf = Some(
            compile_bytecode_function_requested(
                f,
                Some(&ctx.obarray),
                CompileRequest {
                    tier: CompileTier::Upgrade(T2Upgrade::Feedback),
                    regalloc: lowering::RegallocPolicy::Full,
                    bypass_profit_gate: true,
                    origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
                },
            )
            .expect("source is normally native-capable before sqrt selection"),
        )
    });
    assert_eq!(clif.len(), 1);
    let leaf = leaf.unwrap();
    assert_eq!(leaf.selected_tier(), SelectedTier::Opt);
    (leaf, clif.into_iter().next().unwrap())
}
fn native_ok(
    ctx: &mut Context,
    f: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    args: &[Value],
) -> Value {
    let NativeRun::Ok(bits) =
        leaf.call_consts(ctx as *mut Context as *mut u8, f.constants.as_ptr(), args)
    else {
        panic!("actual native positive case")
    };
    Value::from_bits(bits)
}
fn cold(
    ctx: &mut Context,
    f: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    cell: Value,
    argument: Value,
) -> Box<DeoptResume> {
    cell.set_car(Value::fixnum(0));
    let NativeRun::DeoptAt(exit) = leaf.call_consts(
        ctx as *mut Context as *mut u8,
        f.constants.as_ptr(),
        &[cell, argument],
    ) else {
        panic!("selected sqrt must replay the original Call under this guard")
    };
    assert_eq!(cell.cons_car(), Value::fixnum(1), "visible effect ran once");
    assert_eq!(exit.pc, 8);
    assert_eq!(exit.stack, vec![cell, argument, f.constants[0], argument]);
    assert_eq!(exit.handlers, 0);
    exit
}
fn after_store() -> ByteCodeFunction {
    // (setcar cell (1+ (car cell))) followed by (sqrt x). The visible
    // increment makes rerun-from-entry observable; the exact Call is pc8.
    function(
        vec![
            Op::StackRef(1),
            Op::Dup,
            Op::Car,
            Op::Add1,
            Op::Setcar,
            Op::Pop,
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![Value::symbol("sqrt")],
        2,
    )
}
fn outcome(value: Result<Value, Flow>) -> (String, String, Option<String>) {
    match value {
        Ok(value) => ("ok".to_owned(), print_value(&value), None),
        Err(flow) => {
            let signal = flow.as_signal().expect("numeric Call signals");
            (
                signal.symbol_name().to_owned(),
                print_value(&Value::list(signal.data.clone())),
                signal.raw_data.map(|value| print_value(&value)),
            )
        }
    }
}

#[test]
fn opt_sink_sqrt_native_payload_and_argument_replay_keep_fresh_identity_and_store() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let _ = ctx.debug_on_next_call_is_armed();
    // Root each runtime-evaluated setup result before the next Lisp call;
    // later direct value construction cannot trigger a Lisp safepoint.
    let large_integer = ctx.eval_str("(ash 1 80)").unwrap();
    let _integer_root = Roots::new(&[large_integer]);
    let marker = ctx.eval_str("(point-marker)").unwrap();
    let _marker_root = Roots::new(&[marker]);
    let f = after_store();
    let cell = Value::cons(Value::fixnum(0), Value::NIL);
    let positive = [
        Value::fixnum(4),
        Value::make_float(4.0),
        Value::make_float(1.0e-300),
        Value::make_float(1.0e300),
        Value::fixnum(Value::MOST_POSITIVE_FIXNUM),
    ];
    let exceptional = [
        Value::fixnum(0),
        Value::fixnum(-4),
        Value::make_float(0.0),
        Value::make_float(-0.0),
        Value::make_float(-1.0),
        Value::make_float(f64::NAN),
        Value::make_float(f64::INFINITY),
        Value::make_float(f64::NEG_INFINITY),
        Value::symbol("not-a-number"),
        Value::string("not-a-number"),
        large_integer,
        marker,
    ];
    let mut all = vec![cell];
    all.extend(positive);
    all.extend(exceptional);
    let _roots = Roots::new(&all);
    let (baseline, baseline_clif) = compile(&ctx, &f, false);
    assert!(!has_native_sqrt(&baseline_clif));
    for &argument in &positive {
        cell.set_car(Value::fixnum(0));
        let expected = tier0(&mut ctx, &f, &[cell, argument]).unwrap();
        let _expected_root = Roots::new(&[expected]);
        cell.set_car(Value::fixnum(0));
        let actual = native_ok(&mut ctx, &f, &baseline, &[cell, argument]);
        assert_eq!(actual.xfloat().to_bits(), expected.xfloat().to_bits());
        assert_eq!(cell.cons_car(), Value::fixnum(1));
    }
    let (selected, selected_clif) = compile(&ctx, &f, true);
    // Genuine missing native feature assertion, after baseline success.
    assert!(
        has_native_sqrt(&selected_clif),
        "actual selected sqrt payload instruction: {selected_clif}"
    );
    for &argument in &positive {
        cell.set_car(Value::fixnum(0));
        let expected = tier0(&mut ctx, &f, &[cell, argument]).unwrap();
        let _expected_root = Roots::new(&[expected]);
        cell.set_car(Value::fixnum(0));
        let a = native_ok(&mut ctx, &f, &selected, &[cell, argument]);
        let _a_root = Roots::new(&[a]);
        cell.set_car(Value::fixnum(0));
        let b = native_ok(&mut ctx, &f, &selected, &[cell, argument]);
        assert_eq!(a.xfloat().to_bits(), expected.xfloat().to_bits());
        assert_eq!(b.xfloat().to_bits(), expected.xfloat().to_bits());
        assert_ne!(
            a.bits(),
            b.bits(),
            "two successful calls are two GNU Float identities"
        );
        assert_ne!(
            a.bits(),
            argument.bits(),
            "sqrt never returns its original float object"
        );
    }
    for &argument in &exceptional {
        cell.set_car(Value::fixnum(0));
        let expected = outcome(tier0(&mut ctx, &f, &[cell, argument]));
        let exit = cold(&mut ctx, &f, &selected, cell, argument);
        assert_eq!(outcome(resume(&mut ctx, &f, &exit)), expected);
        assert_eq!(
            cell.cons_car(),
            Value::fixnum(1),
            "resume did not repeat pre-call store"
        );
        assert_eq!(ctx.jit_root_stack_top, 0);
    }
}

#[test]
fn opt_sink_sqrt_native_call_controls_and_rebinding_use_complete_original_frame() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let _ = ctx.debug_on_next_call_is_armed();
    let f = after_store();
    let cell = Value::cons(Value::fixnum(0), Value::NIL);
    let x = Value::make_float(4.0);
    let sym = crate::emacs_core::intern::intern("sqrt");
    let original = ctx.obarray.symbol_function_id(sym).unwrap();
    let _roots = Roots::new(&[cell, x, original]);
    let (baseline, _) = compile(&ctx, &f, false);
    cell.set_car(Value::fixnum(0));
    let expected = tier0(&mut ctx, &f, &[cell, x]).unwrap();
    let _expected_root = Roots::new(&[expected]);
    cell.set_car(Value::fixnum(0));
    assert_eq!(
        native_ok(&mut ctx, &f, &baseline, &[cell, x])
            .xfloat()
            .to_bits(),
        expected.xfloat().to_bits()
    );
    let (selected, selected_clif) = compile(&ctx, &f, true);
    assert!(has_native_sqrt(&selected_clif));

    ctx.eval_str("(fset 'sqrt (lambda (x) (list 'changed x)))")
        .unwrap();
    cell.set_car(Value::fixnum(0));
    let changed = tier0(&mut ctx, &f, &[cell, x]).unwrap();
    let _changed_root = Roots::new(&[changed]);
    let exit = cold(&mut ctx, &f, &selected, cell, x);
    assert_eq!(
        print_value(&resume(&mut ctx, &f, &exit).unwrap()),
        print_value(&changed)
    );
    assert_eq!(cell.cons_car(), Value::fixnum(1));
    ctx.obarray.set_symbol_function_id(sym, original);
    // Restoring the same bits changes epoch: old selected code must still
    // replay rather than pretending its original epoch became current again.
    let exit = cold(&mut ctx, &f, &selected, cell, x);
    assert_eq!(
        resume(&mut ctx, &f, &exit).unwrap().xfloat().to_bits(),
        expected.xfloat().to_bits()
    );
    let (selected, _) = compile(&ctx, &f, true);

    // Real debugger control. Tier0 first supplies observed debugger arguments;
    // native only deopts, and resumption triggers that same original Call.
    ctx.eval_str(
        "(setq t34-sqrt-debug-seen nil debugger (lambda (&rest a) (setq t34-sqrt-debug-seen a)))",
    )
    .unwrap();
    let debug = ctx
        .obarray
        .debug_on_next_call_bool_fwd(crate::emacs_core::intern::intern("debug-on-next-call"))
        .unwrap();
    cell.set_car(Value::fixnum(0));
    debug.set(true);
    let _ = tier0(&mut ctx, &f, &[cell, x]);
    debug.set(false);
    let expected_debug = ctx.eval_str("t34-sqrt-debug-seen").unwrap();
    let _debug_root = Roots::new(&[expected_debug]);
    ctx.eval_str("(setq t34-sqrt-debug-seen nil)").unwrap();
    debug.set(true);
    let exit = cold(&mut ctx, &f, &selected, cell, x);
    let _ = resume(&mut ctx, &f, &exit);
    debug.set(false);
    let actual_debug = ctx.eval_str("t34-sqrt-debug-seen").unwrap();
    assert_eq!(print_value(&actual_debug), print_value(&expected_debug));

    cell.set_car(Value::fixnum(0));
    ctx.set_quit_flag_value(Value::T);
    let expected_quit = outcome(tier0(&mut ctx, &f, &[cell, x]));
    ctx.set_quit_flag_value(Value::NIL);
    ctx.set_quit_flag_value(Value::T);
    let exit = cold(&mut ctx, &f, &selected, cell, x);
    let actual_quit = outcome(resume(&mut ctx, &f, &exit));
    ctx.set_quit_flag_value(Value::NIL);
    assert_eq!(actual_quit, expected_quit);
    assert_eq!(cell.cons_car(), Value::fixnum(1));

    // Async attention is deliberately global; use the existing API and clear
    // only this source. No new runtime/TLS test state. Clearing before resume
    // avoids prescribing host-service delivery outside this fixture's scope.
    use crate::emacs_core::eval::{ASYNC_ATTENTION, AsyncSource};
    ASYNC_ATTENTION.raise(AsyncSource::ProfilerTick);
    let exit = cold(&mut ctx, &f, &selected, cell, x);
    ASYNC_ATTENTION.take(AsyncSource::ProfilerTick);
    assert_eq!(
        resume(&mut ctx, &f, &exit).unwrap().xfloat().to_bits(),
        expected.xfloat().to_bits()
    );

    // A native body is already entered, so test the same current depth==limit
    // gate used by MIR inlining. Do not execute Tier0 from function entry at an
    // exhausted depth: only the original Call owns the resumed error/frame.
    let saved_depth = ctx.depth;
    ctx.depth = ctx.max_depth;
    let exit = cold(&mut ctx, &f, &selected, cell, x);
    ctx.depth = saved_depth;
    assert_eq!(
        resume(&mut ctx, &f, &exit).unwrap().xfloat().to_bits(),
        expected.xfloat().to_bits()
    );
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn opt_sink_sqrt_native_epoch_guard_is_fresh_after_an_earlier_callback() {
    let _settings = Settings::enter();
    let mut ctx = Context::new();
    let _ = ctx.debug_on_next_call_is_armed();
    let sym = crate::emacs_core::intern::intern("sqrt");
    let original = ctx.obarray.symbol_function_id(sym).unwrap();
    let callback = ctx
        .eval_str("(lambda () (fset 'sqrt (lambda (x) (list 'changed x))))")
        .unwrap();
    let cell = Value::cons(Value::fixnum(0), Value::NIL);
    let x = Value::make_float(4.0);
    let _roots = Roots::new(&[original, callback, cell, x]);
    let f = function(
        vec![
            Op::StackRef(2),
            Op::Dup,
            Op::Car,
            Op::Add1,
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Call(0),
            Op::Pop,
            Op::Constant(0),
            Op::StackRef(2),
            Op::Call(1),
            Op::Return,
        ],
        vec![Value::from_sym_id(sym)],
        3,
    );
    let args = [cell, x, callback];
    cell.set_car(Value::fixnum(0));
    let expected = tier0(&mut ctx, &f, &args).unwrap();
    let _expected_root = Roots::new(&[expected]);
    ctx.obarray.set_symbol_function_id(sym, original);
    let (baseline, _) = compile(&ctx, &f, false);
    cell.set_car(Value::fixnum(0));
    assert_eq!(
        print_value(&native_ok(&mut ctx, &f, &baseline, &args)),
        print_value(&expected)
    );
    assert_eq!(cell.cons_car(), Value::fixnum(1));
    ctx.obarray.set_symbol_function_id(sym, original);
    let (selected, clif) = compile(&ctx, &f, true);
    assert!(
        has_native_sqrt(&clif),
        "selected second Call owns an actual payload instruction"
    );
    // The selected leaf begins with its original epoch valid. The earlier
    // callback changes the cell while this leaf is already running. Checking
    // only function entry, a stale SpecSlot or an earlier guard is insufficient.
    cell.set_car(Value::fixnum(0));
    let NativeRun::DeoptAt(exit) = selected.call_consts(
        &mut ctx as *mut Context as *mut u8,
        f.constants.as_ptr(),
        &args,
    ) else {
        panic!("per-call current epoch must observe callback rewrite")
    };
    assert_eq!(exit.pc, 11);
    assert_eq!(exit.stack, vec![cell, x, callback, f.constants[0], x]);
    assert_eq!(cell.cons_car(), Value::fixnum(1));
    assert_eq!(
        print_value(&resume(&mut ctx, &f, &exit).unwrap()),
        print_value(&expected)
    );
    assert_eq!(
        cell.cons_car(),
        Value::fixnum(1),
        "neither callback nor visible store was replayed"
    );
    ctx.obarray.set_symbol_function_id(sym, original);
    assert_eq!(ctx.jit_root_stack_top, 0);
}

#[test]
fn opt_sink_sqrt_native_equal_epoch_context_and_same_pointer_rewrite_replay_current_call() {
    use crate::emacs_core::subr::{FixedMin1, SubrSpec};
    use crate::emacs_core::symbol::FunctionEpochBump;
    let _settings = Settings::enter();
    let mut owner = Context::new();
    let _ = owner.debug_on_next_call_is_armed();
    // All source constants are stable immediate symbol words. Heap arguments
    // are made/rooted in the CURRENT owner below; no heap Value migrates.
    let f = after_store();
    let sym = intern("sqrt");
    let original = owner.obarray.symbol_function_id(sym).unwrap();
    let mut other = Context::new();
    let _ = other.debug_on_next_call_is_armed();
    other
        .eval_str("(fset 'sqrt (lambda (x) (list 'changed x)))")
        .unwrap();
    let epoch = owner
        .obarray
        .function_epoch()
        .max(other.obarray.function_epoch());
    for ctx in [&mut owner, &mut other] {
        while ctx.obarray.function_epoch() != epoch {
            ctx.obarray
                .invalidate_all_function_bindings(FunctionEpochBump::SubrRewrite);
        }
    }
    assert_ne!(owner.obarray.generation(), other.obarray.generation());
    assert_eq!(
        Value::from_sym_id(intern("sqrt")).bits(),
        f.constants[0].bits()
    );
    assert_eq!(
        owner.obarray.function_epoch(),
        other.obarray.function_epoch()
    );

    let selected = {
        owner.setup_thread_locals();
        let cell = Value::cons(Value::fixnum(0), Value::NIL);
        let x = Value::make_float(4.0);
        let _roots = Roots::new(&[cell, x, original]);
        let (baseline, _) = compile(&owner, &f, false);
        let expected = tier0(&mut owner, &f, &[cell, x]).unwrap();
        let _expected_root = Roots::new(&[expected]);
        cell.set_car(Value::fixnum(0));
        assert_eq!(
            native_ok(&mut owner, &f, &baseline, &[cell, x])
                .xfloat()
                .to_bits(),
            expected.xfloat().to_bits()
        );
        let (selected, clif) = compile(&owner, &f, true);
        assert!(has_native_sqrt(&clif));
        cell.set_car(Value::fixnum(0));
        assert_eq!(
            native_ok(&mut owner, &f, &selected, &[cell, x])
                .xfloat()
                .to_bits(),
            expected.xfloat().to_bits()
        );
        selected
    };
    // Actual CompiledLeaf::call_consts permits a live different vmctx. Its
    // heap assertion checks CURRENT vmctx/TLS heap agreement, not leaf owner.
    // Same scalar epoch and runtime callee symbol must not admit this lambda.
    {
        other.setup_thread_locals();
        let cell = Value::cons(Value::fixnum(0), Value::NIL);
        let x = Value::make_float(4.0);
        let _roots = Roots::new(&[cell, x]);
        let expected = tier0(&mut other, &f, &[cell, x]).unwrap();
        let _expected_root = Roots::new(&[expected]);
        let (baseline, _) = compile(&other, &f, false);
        cell.set_car(Value::fixnum(0));
        assert_eq!(
            print_value(&native_ok(&mut other, &f, &baseline, &[cell, x])),
            print_value(&expected)
        );
        assert_eq!(other.obarray.function_epoch(), epoch);
        let exit = cold(&mut other, &f, &selected, cell, x);
        assert_eq!(
            print_value(&resume(&mut other, &f, &exit).unwrap()),
            print_value(&expected)
        );
        assert_eq!(cell.cons_car(), Value::fixnum(1));
    }

    // Production registration in ANOTHER same-thread Context rewrites the
    // leaked static object in place. Owner's cell bits and epoch remain equal
    // to the captured witness, so only a synchronized fresh actual-entry
    // check can reject this sin entry. Expected result comes from Tier0.
    other.register_subr(SubrSpec::fixed1(
        "sqrt",
        crate::emacs_core::builtins::builtin_sin_1,
        FixedMin1::One,
    ));
    assert_eq!(owner.obarray.function_epoch(), epoch);
    assert_eq!(
        owner.obarray.symbol_function_id(sym).unwrap().bits(),
        original.bits()
    );
    {
        owner.setup_thread_locals();
        let cell = Value::cons(Value::fixnum(0), Value::NIL);
        let x = Value::make_float(4.0);
        let _roots = Roots::new(&[cell, x, original]);
        let expected = tier0(&mut owner, &f, &[cell, x]).unwrap();
        let _expected_root = Roots::new(&[expected]);
        let exit = cold(&mut owner, &f, &selected, cell, x);
        assert_eq!(
            resume(&mut owner, &f, &exit).unwrap().xfloat().to_bits(),
            expected.xfloat().to_bits()
        );
        assert_eq!(cell.cons_car(), Value::fixnum(1));
    }
    other.register_subr(SubrSpec::fixed1(
        "sqrt",
        crate::emacs_core::builtins::builtin_sqrt_1,
        FixedMin1::One,
    ));
    owner.setup_thread_locals();
    assert_eq!(owner.jit_root_stack_top, 0);
    assert_eq!(other.jit_root_stack_top, 0);
}
