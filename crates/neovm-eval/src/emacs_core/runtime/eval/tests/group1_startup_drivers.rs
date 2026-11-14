//! Group-1 startup re-home. Each driver keeps a private call path inside
//! this crate and returns the words the moved test asserts.
//!
//! Threading: every entry takes the caller's mutator `Context` or reads
//! compiler TLS on the calling thread. Nothing here is shared across
//! mutators.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::eval::Context;
use crate::emacs_core::print::print_value;
use neovm_value::value::Value;

/// One fresh startup context per call. The test owns that context and the
/// assert loop. This walks the private map callee the same way the original
/// snapshot did.
pub(crate) fn map_resume_redefining_snapshot_for_test(eval: &mut Context, split: usize) -> String {
    use crate::emacs_core::builtins::higher_order::{
        MapCallee, MapSink, map_sequence_length, mapcar1_eval_from,
    };
    let roots = eval.save_vm_roots();
    eval.eval_str(
        "(defun map-resume-redefined (x)
           (when (= x 2)
             (fset 'map-resume-redefined (lambda (x) (+ x 100))))
           (+ x 10))",
    )
    .unwrap();
    let func = Value::symbol("map-resume-redefined");
    let sequence = Value::list((0..6).map(Value::fixnum).collect());
    eval.push_vm_frame_root(func);
    eval.push_vm_frame_root(sequence);
    let len = map_sequence_length(sequence).unwrap();
    let base = eval.reserve_vm_frame_root_slots(len);
    let cursor_root = eval.push_vm_frame_root_slot(sequence);
    let callee = MapCallee::resolve(eval, func);
    let mut cursor = sequence;
    for index in 0..split {
        eval.set_vm_frame_root_slot(cursor_root, cursor);
        let value = callee.call(eval, cursor.cons_car()).unwrap();
        eval.set_vm_frame_root_slot(base + index, value);
        cursor = cursor.cons_cdr();
    }
    // A resumed HOF resolves the original designator again; later calls must
    // observe its current cell, including a redefinition by the last prefix
    // callback. The running old callback itself still completed with old code.
    let callee = MapCallee::resolve(eval, func);
    let mapped = mapcar1_eval_from(
        eval,
        len,
        MapSink::RootSlots(base),
        sequence,
        cursor,
        split,
        |eval, item| callee.call(eval, item),
    )
    .unwrap();
    let result = print_value(&Value::list_from_slice(
        eval.vm_frame_root_slots(base, mapped),
    ));
    eval.restore_vm_roots(roots);
    result
}

/// `(warmup, mapped print, depth before, specpdl before, depth after,
/// specpdl after, entry-calls cdr is nil, entry-calls car is debug, entry-args
/// print)`. The test keeps the three assert messages.
#[cfg(feature = "jit")]
pub(crate) fn mapcar_debug_entry_obs_for_test(
    ctx: &mut Context,
) -> (
    Option<i64>,
    String,
    usize,
    usize,
    usize,
    usize,
    bool,
    bool,
    String,
) {
    use crate::emacs_core::builtins::higher_order::builtin_mapcar_2;
    use crate::emacs_core::bytecode::ByteCodeFunctionExt;
    use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
    use crate::emacs_core::jit::RuntimeStateTieringExt;
    use crate::emacs_core::subr::{FixedMin1, SubrSpec};
    use neovm_value::error::EvalResult;
    use neovm_value::intern::intern;
    use neovm_value::value::LambdaParams;

    fn identity(_: &mut Context, item: Value) -> EvalResult {
        Ok(item)
    }
    fn bytecode(target: Value) -> Value {
        let mut code = ByteCodeFunction::new(LambdaParams::simple(vec![intern("mapact-item")]));
        code.lexical = true;
        code.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(8);
        code.constants = vec![target].into();
        code.ops = vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
        code.jit_runtime().set_hot_for_test();
        Value::make_bytecode(code)
    }

    ctx.eval_str("(require 'debug)")
        .expect("load the actual GNU debug-on-entry implementation");
    ctx.register_subr(SubrSpec::fixed1(
        "mapact-debug-entry",
        identity,
        FixedMin1::One,
    ));
    let roots = ctx.save_vm_roots();
    let function = bytecode(Value::symbol("mapact-debug-entry"));
    ctx.push_vm_frame_root(function);
    let warmup = ctx
        .apply1_bytecode_unobserved(function, Value::fixnum(7))
        .unwrap()
        .as_fixnum();
    ctx.eval_str(
        "(setq mapact-entry-calls nil mapact-entry-args nil
               debugger
               (lambda (&rest args)
                 (setq mapact-entry-calls (cons args mapact-entry-calls))
                 (mapbacktrace
                   (lambda (_evaluated function arguments _flags)
                     (if (eq function 'mapact-debug-entry)
                         (setq mapact-entry-args arguments))))
                 (fset 'mapact-debug-entry (lambda (x) (+ x 100)))
                 nil))",
    )
    .expect("a debugger that observes the live frame and redefines its function");
    ctx.eval_str("(debug-on-entry 'mapact-debug-entry)")
        .expect("install actual debug-on-entry advice");
    let input = Value::list(vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]);
    ctx.push_vm_frame_root(input);
    let depth_before = ctx.depth;
    let specpdl_before = ctx.specpdl.len();
    let mapped = print_value(&builtin_mapcar_2(ctx, function, input).unwrap());
    let depth_after = ctx.depth;
    let specpdl_after = ctx.specpdl.len();
    let calls = ctx
        .obarray()
        .symbol_value_copied("mapact-entry-calls")
        .unwrap();
    let calls_cdr_nil = calls.cons_cdr() == Value::NIL;
    let calls_car_debug = calls.cons_car().cons_car() == Value::symbol("debug");
    let args = print_value(
        &ctx.obarray()
            .symbol_value_copied("mapact-entry-args")
            .unwrap(),
    );
    ctx.restore_vm_roots(roots);
    (
        warmup,
        mapped,
        depth_before,
        specpdl_before,
        depth_after,
        specpdl_after,
        calls_cdr_nil,
        calls_car_debug,
        args,
    )
}

#[cfg(feature = "jit")]
pub(crate) fn opt_passes_flags_for_test() -> (bool, bool, bool, bool, bool, bool, bool, bool) {
    let passes = crate::emacs_core::jit::compile::jit_opt_passes();
    (
        passes.fold,
        passes.bool_rep,
        passes.reps,
        passes.gvn,
        passes.reads,
        passes.range,
        passes.licm,
        passes.sink,
    )
}

#[cfg(feature = "jit")]
pub(crate) fn opt_profit_name_for_test() -> &'static str {
    use crate::emacs_core::jit::compile::OptProfitMode;
    match crate::emacs_core::jit::compile::jit_opt_profit() {
        OptProfitMode::Off => "Off",
        OptProfitMode::Loops => "Loops",
        OptProfitMode::Lists => "Lists",
        OptProfitMode::PrimitiveLists => "PrimitiveLists",
        OptProfitMode::PrimitiveLoops => "PrimitiveLoops",
        OptProfitMode::Kernels => "Kernels",
    }
}

#[cfg(feature = "jit")]
pub(crate) fn compiled_leaf_selected_tier_name_for_test(
    leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
) -> &'static str {
    leaf.selected_tier().name()
}

#[cfg(feature = "jit")]
pub(crate) fn compiled_leaf_opt_sink_counts_for_test(
    leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
) -> Option<(usize, usize)> {
    crate::emacs_core::jit::compile::opt_census::snapshot_leaf(leaf)
        .opt_sink
        .map(|stats| (stats.cons_sources, stats.cons_reads_elided))
}

#[cfg(feature = "jit")]
pub(crate) fn compiled_leaf_required_for_test(
    leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
) -> usize {
    leaf.required
}

/// The register ABI's argument ceiling. The const stays in `reg_abi`.
#[cfg(feature = "jit")]
pub(crate) fn max_reg_args_for_test() -> usize {
    crate::emacs_core::jit::compile::reg_abi::MAX_REG_ARGS
}

/// `(MaterializeCons count, every such pc is after Baset)`. The IR walk
/// stays here. The test keeps the two materialization messages.
#[cfg(feature = "jit")]
pub(crate) fn sink_only_materialize_at_aset_for_test(
    source: &crate::emacs_core::bytecode::ByteCodeFunction,
    leaf: &crate::emacs_core::jit::compile::CompiledLeaf,
) -> (u32, bool) {
    use crate::emacs_core::bytecode::ByteCodeFunctionExt;
    use crate::emacs_core::bytecode::Op;
    use crate::emacs_core::bytecode::function_slots::FunctionParamsExt as _;
    use crate::emacs_core::jit::compile;
    use crate::emacs_core::jit::opt::{build, ir, passes::sink, sink_recipes, verify};
    let selected = compile::opt_census::snapshot_leaf(leaf)
        .opt_sink
        .expect("sink pass selected");
    let cfg = compile::analyze_cfg(
        source.executable_ops(),
        &source.constants,
        source.executable_gnu_byte_offset_map(),
        source.params.fixed_arity().unwrap(),
    )
    .expect("exact sink repro CFG");
    let constants = source
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    let mut plan = build::build(build::BuildInput {
        ops: source.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params: ir::ParamShape {
            required: source.params.fixed_arity().unwrap(),
            ..Default::default()
        },
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .expect("exact sink-only builder input");
    let stats = sink::run(&mut plan, &[]).expect("numeric-free sink-only plan");
    assert_eq!(
        &stats,
        selected.as_ref(),
        "independent exact-source plan matches selected native census"
    );
    let (aset_index, aset) = plan
        .insts
        .iter()
        .enumerate()
        .find(|(_, inst)| {
            matches!(
                inst.op,
                ir::Opcode::Opaque(Op::Aset) | ir::Opcode::Builtin(Op::Aset)
            )
        })
        .expect("Baset stays in the selected plan");
    let aset_id = ir::Inst(u32::try_from(aset_index).unwrap());
    let owner = plan
        .sink_recipes
        .owners
        .values()
        .find(|owner| matches!(owner.origin, sink_recipes::RecipeOrigin::ConsSource { .. }))
        .expect("fresh cons recipe")
        .owner;
    let native_proof = verify::verify_for_native(&plan).expect("independent complete native proof");
    let recipes = native_proof
        .sink()
        .expect("verified sink recipe capability");
    let before_aset = recipes
        .version_at(sink_recipes::RecipePoint::Before(aset_id), owner)
        .expect("exact cons recipe immediately before Baset");
    assert!(
        !recipes.guaranteed_boxed(before_aset),
        "cons must still be virtual across Baset"
    );
    let mut materializations = 0u32;
    let mut after_aset = true;
    for inst in &plan.insts {
        if matches!(
            inst.op,
            ir::Opcode::Sink(sink_recipes::SinkOp::MaterializeCons)
        ) {
            materializations += 1;
            after_aset &= inst.pc > aset.pc;
        }
    }
    (materializations, after_aset)
}

/// `(callback bits, backtrace function bits, argument fixnum, depth, specpdl
/// empty)`. `HofKind::Mapcar` is fixed. The active frame stays here.
#[cfg(feature = "jit")]
pub(crate) fn inline_hof_mapcar_backtrace_obs_for_test(
    ctx: &mut Context,
) -> (usize, usize, Option<i64>, usize, bool) {
    use crate::emacs_core::bytecode::ByteCodeFunctionExt;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::bytecode::vm::{ChainBacktrace, ChainLink};
    use crate::emacs_core::jit::RuntimeStateTieringExt;
    use crate::emacs_core::jit::compile::hof_runtime::{
        neovm_jit_hof_length, neovm_jit_hof_start, resume_mapping,
    };
    use crate::emacs_core::jit::vframe::{HofKind, HofResume, InlinedFrameResume};
    use neovm_value::intern::intern;
    use neovm_value::value::LambdaParams;

    fn function(
        ops: Vec<Op>,
        constants: Vec<Value>,
    ) -> crate::emacs_core::bytecode::ByteCodeFunction {
        let mut f = crate::emacs_core::bytecode::ByteCodeFunction::new(LambdaParams {
            required: vec![intern("hof-runtime-arg-0")],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = ops;
        f.constants = constants.into();
        f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(24);
        ByteCodeFunctionExt::seal_hand_assembled_ops(&mut f);
        f.jit_runtime().set_hot_for_test();
        f
    }

    let callback = Value::make_bytecode(function(
        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        vec![Value::symbol("backtrace-frame"), Value::make_int(1)],
    ));
    let sequence = Value::list(vec![Value::make_int(1)]);
    let map = Value::symbol("mapcar");
    let words = [
        map.bits() as i64,
        callback.bits() as i64,
        sequence.bits() as i64,
    ];
    let len = unsafe { neovm_jit_hof_length(sequence.bits() as i64) };
    // SAFETY: this frame supplies the live `ctx` and argument words; the shim contract holds.
    let bt = unsafe {
        neovm_jit_hof_start(
            ctx as *mut Context as *mut u8,
            map.bits() as i64,
            words.as_ptr(),
            3,
            len,
            HofKind::Mapcar as i64,
        )
    };
    assert!(bt >= 0);
    ctx.set_vm_frame_root_slot(3, sequence);
    let state = HofResume {
        kind: HofKind::Mapcar,
        function: callback,
        sequence,
        len: len as usize,
        tail: sequence,
        item: sequence.cons_car(),
        index: 0,
        bt: bt as usize,
        sink_base: 4,
        callback_entered: true,
    };
    sequence.set_car(Value::make_int(9));
    let frame = InlinedFrameResume {
        function: callback,
        pc: 2,
        stack: vec![
            Value::make_int(8),
            Value::symbol("backtrace-frame"),
            Value::make_int(1),
        ],
        binds: Vec::new(),
        link: ChainLink::Bcall { nargs: 1 },
        backtrace: ChainBacktrace::Virtual,
    };
    let result = resume_mapping(ctx, &state, &frame, 0).unwrap();
    let backtrace = result.cons_car();
    (
        callback.bits(),
        backtrace.cons_cdr().cons_car().bits(),
        backtrace.cons_cdr().cons_cdr().cons_car().as_fixnum(),
        ctx.depth,
        ctx.specpdl.is_empty(),
    )
}

/// `(sites nonempty, no site hoisted, printed native answer)`. The caller
/// has already eval'd the mutation setup and holds the switch settings.
#[cfg(feature = "jit")]
pub(crate) fn opt_switch_hoist_call_mutation_for_test(ctx: &mut Context) -> (bool, bool, String) {
    use crate::emacs_core::bytecode::ByteCodeFunctionExt;
    use crate::emacs_core::bytecode::function_slots::FunctionParamsExt as _;
    use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
    use crate::emacs_core::eval::{
        push_scratch_gc_roots, restore_scratch_gc_roots, save_scratch_gc_roots,
    };
    use crate::emacs_core::jit::compile::{self, NativeRun};
    use crate::emacs_core::jit::opt::passes::switch_epochs::SwitchEpochGuards;
    use crate::emacs_core::jit::opt::{build, ir};
    use std::collections::HashSet;

    let f = ctx.eval_str("switch-hoist-function").unwrap();
    let saved = save_scratch_gc_roots();
    push_scratch_gc_roots(&[f]);
    let data = f.get_bytecode_data().unwrap();
    let arity = data
        .params
        .stack_shape()
        .expect("fixture stack parameters")
        .required();
    let cfg = compile::analyze_cfg(
        data.executable_ops(),
        &data.constants,
        data.executable_gnu_byte_offset_map(),
        arity,
    )
    .unwrap();
    let constants = data
        .constants
        .iter()
        .copied()
        .map(ir::ValueBits::from_value)
        .collect::<Vec<_>>();
    let plan = build::build(build::BuildInput {
        ops: data.executable_ops(),
        constants: &constants,
        cfg: &cfg,
        params: ir::ParamShape {
            required: arity,
            ..ir::ParamShape::default()
        },
        dynamic_prefix: 0,
        fused: None,
        osr: None,
    })
    .unwrap();
    let sites: HashSet<ir::Block> = plan
        .blocks
        .iter()
        .enumerate()
        .filter_map(|(i, block)| {
            matches!(block.term, ir::Term::Switch { .. }).then_some(ir::Block(i as u32))
        })
        .collect();
    let nonempty = !sites.is_empty();
    let schedule = SwitchEpochGuards::build(&plan, &sites).unwrap();
    let blocked = sites.iter().all(|&site| !schedule.is_hoisted(site));
    plan.verify()
        .expect("manual typed IR verifies before native lowering");
    let leaf = compile::lower_opt_ir_for_test(
        data.executable_ops(),
        &data.constants,
        arity,
        data.executable_gnu_byte_offset_map(),
        &plan,
    )
    .expect("verified typed integer IR has native support");
    assert_eq!(leaf.selected_tier(), compile::opt_census::SelectedTier::Opt);
    let args = [Value::symbol("a"), Value::fixnum(300)];
    let actual = match leaf.call(&mut *ctx as *mut Context as *mut u8, &args) {
        NativeRun::Ok(bits) => {
            // SAFETY: these bits are the completed native result of `leaf.call`.
            unsafe { Value::from_bits(bits) }
        }
        NativeRun::DeoptAt(resume) => {
            let mut vm = crate::emacs_core::bytecode::Vm::from_context(ctx);
            vm.force_interpreter_only_for_test();
            vm.run_resumed_frame(
                data,
                Value::NIL,
                resume.pc,
                &resume.stack,
                resume.handlers,
                &resume.binds,
                resume.spec_base,
                resume.cond_base,
            )
            .unwrap()
        }
        other => panic!("native call mutation: {other:?}"),
    };
    let printed = print_value(&actual);
    restore_scratch_gc_roots(saved);
    (nonempty, blocked, printed)
}

/// Observations from the lists48-osr aset run. The caller holds
/// [`enter_lists48_osr_profile_for_test`] and has installed the advice.
///
/// `(op len, osr returned the alias, osr tier, osr vector is the alias, osr
/// pair car is 3, normal returned the alias, normal tier, normal vector is
/// the alias, normal pair car is 3, advice calls after native, jit root top,
/// genuine aset print, advice calls after the genuine call)`.
#[cfg(feature = "jit")]
pub(crate) fn lists48_osr_aset_advice_obs_for_test(
    ctx: &mut Context,
) -> (
    usize,
    bool,
    &'static str,
    bool,
    bool,
    bool,
    &'static str,
    bool,
    bool,
    Option<i64>,
    usize,
    String,
    Option<i64>,
) {
    use crate::emacs_core::bytecode::ByteCodeFunctionExt;
    use crate::emacs_core::bytecode::opcode::Op;
    use crate::emacs_core::eval::push_scratch_gc_root;
    use crate::emacs_core::jit::RuntimeStateTieringExt;
    use crate::emacs_core::jit::cache::{self, try_run_compiled};
    use crate::emacs_core::jit::compile::NativeRun;
    use neovm_value::intern::intern;
    use neovm_value::value::LambdaParams;

    let mut source = crate::emacs_core::bytecode::ByteCodeFunction::new(LambdaParams {
        required: (0..4)
            .map(|i| intern(&format!("hof-runtime-arg-{i}")))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    source.lexical = true;
    source.ops = vec![
        Op::Constant(0),
        Op::StackRef(0),
        Op::StackRef(2),
        Op::Lss,
        Op::GotoIfNil(18),
        Op::StackRef(4),
        Op::Constant(0),
        Op::StackRef(5),
        Op::Aset,
        Op::Pop,
        Op::StackRef(2),
        Op::StackRef(1),
        Op::Setcar,
        Op::Pop,
        Op::StackRef(0),
        Op::Add1,
        Op::StackSet(1),
        Op::Goto(1),
        Op::Pop,
        Op::StackRef(2),
        Op::Return,
    ];
    source.constants = vec![Value::make_int(0)].into();
    source.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(24);
    ByteCodeFunctionExt::seal_hand_assembled_ops(&mut source);
    source.jit_runtime().set_hot_for_test();
    let op_len = source.executable_ops().len();
    let vector = Value::vector(vec![Value::NIL]);
    push_scratch_gc_root(vector);
    let alias = Value::make_float(3.25);
    push_scratch_gc_root(alias);
    let pair = Value::cons(Value::make_int(-1), Value::NIL);
    push_scratch_gc_root(pair);
    let args = [vector, alias, pair, Value::make_int(4)];
    let mut snapshot = args.to_vec();
    snapshot.push(Value::make_int(2));
    ctx.bc_buf.extend_from_slice(&snapshot);
    let osr_ok = unsafe { cache::try_run_osr(ctx, &source, 1, &snapshot, &[]) }
        == Some(NativeRun::Ok(alias.bits()));
    let osr_tier = cache::osr_leaf_ptr_for_test(&source, 1).expect("installed actual OSR leaf");
    let osr_tier = unsafe { &*osr_tier }.selected_tier().name();
    let osr_vector = vector.as_vector_data().unwrap()[0].bits() == alias.bits();
    let osr_pair = pair.cons_car() == Value::make_int(3);
    let function_value = Value::make_bytecode(source.clone());
    push_scratch_gc_root(function_value);
    let normal_ok = unsafe { try_run_compiled(ctx, &source, function_value, &args) }.unwrap()
        == Some(alias.bits());
    let normal = cache::resolve_compiled_leaf_ptr(ctx, &source).unwrap();
    let normal_tier = unsafe { &*normal }.selected_tier().name();
    let normal_vector = vector.as_vector_data().unwrap()[0].bits() == alias.bits();
    let normal_pair = pair.cons_car() == Value::make_int(3);
    let advice_native = ctx
        .obarray()
        .symbol_value_copied("opt-profit--aset-advice-calls")
        .and_then(|value| value.as_fixnum());
    let root_top = ctx.jit_root_stack_top;
    let genuine = print_value(&ctx.eval_str("(funcall 'aset (vector 0) 0 7)").unwrap());
    let advice_genuine = ctx
        .obarray()
        .symbol_value_copied("opt-profit--aset-advice-calls")
        .and_then(|value| value.as_fixnum());
    (
        op_len,
        osr_ok,
        osr_tier,
        osr_vector,
        osr_pair,
        normal_ok,
        normal_tier,
        normal_vector,
        normal_pair,
        advice_native,
        root_top,
        genuine,
        advice_genuine,
    )
}
