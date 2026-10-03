//! Emitted named regions and their cold transfer to the existing GNU-shaped
//! Bcall protocol. Every fixture requires an emitted named dependency, so a
//! successful ordinary out-call cannot silently satisfy these regressions.

use super::*;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::error::{Flow, FlowRef};
use crate::emacs_core::eval::{Context, push_scratch_gc_root};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::lowering::RegallocPolicy;
use crate::emacs_core::jit::compile::{self, CompileRequest, CompiledLeaf, Inline2Mode, NativeRun};
use crate::emacs_core::jit::reopt::DeoptCause;
use crate::emacs_core::jit::stats::CompileOrigin;
use crate::emacs_core::value::LambdaParams;

/// Scalar test configuration belongs to this test's compiler thread; no
/// Lisp state or frame cache is shared with another mutator.
struct Knobs;
impl Knobs {
    fn enter() -> Self {
        crate::test_utils::init_test_tracing();
        force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(Inline2Mode::Named));
        compile::force_profit_gate_for_test(false);
        compile::force_deopt_for_test(false);
        Self
    }
}
impl Drop for Knobs {
    fn drop(&mut self) {
        force_inline_for_test(None);
        compile::force_inline2_for_test(None);
        compile::force_deopt_for_test(false);
    }
}

fn function(arity: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|i| intern(&format!("named-runtime-arg-{i}")))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 32;
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

fn caller(name: &str, arity: usize) -> ByteCodeFunction {
    let mut ops = vec![Op::Constant(0)];
    ops.extend((0..arity).map(|_| Op::StackRef(arity as u16)));
    ops.extend([Op::Call(arity as u16), Op::Return]);
    function(arity, ops, vec![Value::symbol(name)])
}

fn native(ctx: &mut Context, f: &ByteCodeFunction, name: &str) -> CompiledLeaf {
    let _ = ctx.debug_on_next_call_is_armed();
    let leaf = compile::compile_bytecode_function_requested(
        f,
        Some(&ctx.obarray),
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: true,
            origin: CompileOrigin::Retier,
            tier: crate::emacs_core::jit::tier2::CompileTier::Plain,
        },
    )
    .expect("named re-tier compile");
    assert!(
        leaf.inline_deps().contains(&intern(name)),
        "the named producer must execute"
    );
    leaf
}

#[test]
fn inline_named_runtime_inherited_full_before_retier_keeps_real_call() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let name = "named-runtime-inherited-full";
    ctx.obarray.set_symbol_function_id(
        intern(name),
        Value::make_bytecode(function(
            1,
            vec![Op::StackRef(0), Op::Add1, Op::Return],
            vec![],
        )),
    );
    let f = caller(name, 1);
    f.jit_runtime().set_heat_for_test(0);
    let leaf = compile::compile_bytecode_function_requested(
        &f,
        Some(&ctx.obarray),
        CompileRequest {
            regalloc: RegallocPolicy::Full,
            bypass_profit_gate: true,
            origin: CompileOrigin::DeferralExpired,
            tier: crate::emacs_core::jit::tier2::CompileTier::Plain,
        },
    )
    .unwrap();
    assert!(leaf.chains.is_empty(), "inherited Full is not re-tier heat");
    let retier = native(&mut ctx, &f, name);
    assert!(
        !retier.chains.is_empty(),
        "explicit re-tier admits named frames"
    );
}

fn finish(
    ctx: &mut Context,
    f: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    run: NativeRun,
) -> Result<Value, Flow> {
    match run {
        NativeRun::Ok(bits) => Ok(Value::from_bits(bits)),
        NativeRun::DeoptAt(resume) => {
            compile::resumed_chain::resume_deopt(ctx, f, Value::NIL, leaf, *resume)
        }
        NativeRun::Signal => Err(compile::take_pending_flow().expect("pending named flow")),
        other => panic!("unexpected named native outcome: {other:?}"),
    }
}

fn increment(ctx: &mut Context, name: &str) {
    ctx.set_function(
        name,
        Value::make_bytecode(function(
            1,
            vec![Op::StackRef(0), Op::Add1, Op::Return],
            vec![],
        )),
    );
}

fn capture_assigned_call(ctx: &mut Context, _kind: Value, _data: Value) -> Result<Value, Flow> {
    let frame = ctx.specpdl.iter().find_map(|entry| {
        let (function, args, _, _) = ctx.backtrace_entry_values(entry)?;
        (function == Value::symbol("named-runtime-assigned-argument"))
            .then(|| Value::list(vec![function, args[0]]))
    });
    ctx.set_variable("named-runtime-observed-call", frame.unwrap_or(Value::NIL));
    Ok(Value::NIL)
}

#[test]
fn inline_named_runtime_pure_native_region_preserves_frame_depth_and_roots() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let name = "named-runtime-native-increment";
    increment(&mut ctx, name);
    let f = caller(name, 1);
    let leaf = native(&mut ctx, &f, name);
    assert!(!leaf.chains.is_empty(), "the Add1 guard owns a named chain");
    let depth = ctx.depth;
    let spec = ctx.specpdl.len();
    let roots = ctx.jit_root_stack_top;
    for n in [0, 1, -17, 1_000] {
        assert_eq!(
            leaf.call(&mut ctx as *mut Context as *mut u8, &[Value::make_int(n)]),
            NativeRun::Ok(Value::make_int(n + 1).bits())
        );
        assert_eq!(
            (ctx.depth, ctx.specpdl.len(), ctx.jit_root_stack_top),
            (depth, spec, roots)
        );
    }
}

#[test]
fn inline_named_runtime_conditional_returns_preserve_each_path() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let name = "named-runtime-conditional";
    ctx.set_function(
        name,
        Value::make_bytecode(function(
            1,
            vec![
                Op::StackRef(0),
                Op::GotoIfNil(5),
                Op::StackRef(0),
                Op::Add1,
                Op::Return,
                Op::Constant(0),
                Op::Return,
            ],
            vec![Value::make_int(99)],
        )),
    );
    let f = caller(name, 1);
    let leaf = native(&mut ctx, &f, name);
    for (arg, value) in [(Value::NIL, 99), (Value::make_int(7), 8)] {
        assert_eq!(
            leaf.call(&mut ctx as *mut Context as *mut u8, &[arg]),
            NativeRun::Ok(Value::make_int(value).bits())
        );
    }
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_named_runtime_cell_redefinition_resumes_original_call_with_new_function() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let name = "named-runtime-redefined";
    increment(&mut ctx, name);
    let f = caller(name, 1);
    let leaf = native(&mut ctx, &f, name);
    ctx.set_function(
        name,
        Value::make_bytecode(function(
            1,
            vec![Op::Constant(0), Op::Return],
            vec![Value::make_int(77)],
        )),
    );
    let run = leaf.call(&mut ctx as *mut Context as *mut u8, &[Value::make_int(3)]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("named cell compare: {run:?}")
    };
    assert_eq!(resume.pc, 2);
    assert_eq!(resume.cause, Some(DeoptCause::InlineIdentity));
    assert!(
        resume.inlined.is_none(),
        "failed entry has not entered its virtual callee"
    );
    assert_eq!(
        resume.stack,
        vec![Value::make_int(3), Value::symbol(name), Value::make_int(3)]
    );
    assert_eq!(
        compile::resumed_chain::resume_deopt(&mut ctx, &f, Value::NIL, &leaf, *resume).unwrap(),
        Value::make_int(77)
    );
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_named_runtime_unrelated_function_cell_write_keeps_native_entry() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let name = "named-runtime-unrelated-write";
    increment(&mut ctx, name);
    let f = caller(name, 1);
    let leaf = native(&mut ctx, &f, name);
    ctx.set_function(
        "named-runtime-unrelated-other",
        Value::make_bytecode(function(0, vec![Op::Nil, Op::Return], vec![])),
    );
    assert_eq!(
        leaf.call(&mut ctx as *mut Context as *mut u8, &[Value::make_int(3)]),
        NativeRun::Ok(Value::make_int(4).bits())
    );
}

#[test]
fn inline_named_runtime_depth_guard_precedes_callee_and_uses_bcall_error() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let name = "named-runtime-depth";
    increment(&mut ctx, name);
    let f = caller(name, 1);
    let leaf = native(&mut ctx, &f, name);
    ctx.depth = 120;
    ctx.max_depth = 120;
    ctx.set_variable("max-lisp-eval-depth", Value::make_int(120));
    let run = leaf.call(&mut ctx as *mut Context as *mut u8, &[Value::make_int(3)]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("named depth guard: {run:?}")
    };
    assert_eq!(resume.pc, 2);
    assert_eq!(resume.cause, Some(DeoptCause::DepthLimit));
    assert!(resume.inlined.is_none());
    let error =
        compile::resumed_chain::resume_deopt(&mut ctx, &f, Value::NIL, &leaf, *resume).unwrap_err();
    assert!(
        if matches!(error.kind(), FlowRef::Signal(signal) if signal.symbol_name() == "error") {
            drop(error);
            true
        } else {
            false
        }
    );
    assert_eq!(ctx.depth, 120);
    assert!(ctx.specpdl.is_empty());
    ctx.depth = 0;
}

#[test]
fn inline_named_runtime_attention_guard_leaves_before_virtual_entry() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let name = "named-runtime-attention";
    increment(&mut ctx, name);
    let f = caller(name, 1);
    let leaf = native(&mut ctx, &f, name);
    ctx.set_variable("throw-on-input", Value::T);
    let run = leaf.call(&mut ctx as *mut Context as *mut u8, &[Value::make_int(3)]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("named attention guard: {run:?}")
    };
    assert_eq!(resume.pc, 2);
    assert_eq!(resume.cause, Some(DeoptCause::InlineAttention));
    assert!(resume.inlined.is_none());
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_named_runtime_assigned_argument_is_resumed_without_losing_original_call_args() {
    use crate::emacs_core::eval::{SubrEntry, register_global_subr_entry};
    use crate::tagged::header::{SubrDispatchKind, SubrFn};

    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let name = "named-runtime-assigned-argument";
    let target = Value::make_bytecode(function(
        1,
        vec![
            Op::Constant(0),
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Car,
            Op::Return,
        ],
        vec![Value::make_int(1)],
    ));
    ctx.set_function(name, target);
    let f = caller(name, 1);
    let leaf = native(&mut ctx, &f, name);
    let observer = intern("named-runtime-signal-capture");
    register_global_subr_entry(
        observer,
        SubrEntry {
            function: Some(SubrFn::A2(capture_assigned_call)),
            min_args: 2,
            max_args: Some(2),
            dispatch_kind: SubrDispatchKind::Builtin,
            interactive_spec: None,
        },
    );
    ctx.set_function(
        "named-runtime-signal-capture",
        Value::subr_from_sym_id(observer),
    );
    ctx.set_variable("signal-hook-function", Value::from_sym_id(observer));
    ctx.set_variable("named-runtime-observed-call", Value::NIL);
    let original = Value::cons(Value::make_int(7), Value::NIL);
    push_scratch_gc_root(original);
    let run = leaf.call(&mut ctx as *mut Context as *mut u8, &[original]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("inner Car guard: {run:?}")
    };
    assert_eq!(resume.stack, vec![original, Value::symbol(name), original]);
    let frames = &resume.inlined.as_ref().expect("named chain").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].function, target);
    assert_eq!(frames[0].pc, 3);
    assert_eq!(
        frames[0].stack,
        vec![Value::make_int(1), Value::make_int(1)]
    );
    let error =
        compile::resumed_chain::resume_deopt(&mut ctx, &f, Value::NIL, &leaf, *resume).unwrap_err();
    assert!(
        if matches!(error.kind(), FlowRef::Signal(signal) if signal.symbol_name() == "wrong-type-argument")
        {
            drop(error);
            true
        } else {
            false
        }
    );
    let observed = ctx
        .obarray
        .symbol_value("named-runtime-observed-call")
        .copied()
        .unwrap();
    assert_eq!(
        observed.cons_car(),
        Value::symbol(name),
        "materialized Bcall records its symbol"
    );
    assert_eq!(
        observed.cons_cdr().cons_car(),
        original,
        "backtrace retains the entry argument"
    );
    assert_eq!(original.cons_car(), Value::make_int(7));
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_named_runtime_forced_entry_deopt_preserves_cons_identity_under_gc() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let name = "named-runtime-forced-identity";
    ctx.set_function(
        name,
        Value::make_bytecode(function(1, vec![Op::StackRef(0), Op::Return], vec![])),
    );
    let f = caller(name, 1);
    compile::force_deopt_for_test(true);
    let leaf = native(&mut ctx, &f, name);
    let pair = Value::cons(Value::make_int(17), Value::NIL);
    push_scratch_gc_root(pair);
    ctx.gc_stress = true;
    let run = leaf.call(&mut ctx as *mut Context as *mut u8, &[pair]);
    assert!(
        matches!(run, NativeRun::DeoptAt(_)),
        "forced named entry really deopts"
    );
    assert_eq!(finish(&mut ctx, &f, &leaf, run).unwrap(), pair);
    assert_eq!(pair.cons_car(), Value::make_int(17));
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
    assert_eq!(ctx.jit_root_stack_top, 0);
    ctx.gc_stress = false;
}
