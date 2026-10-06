//! Native list mapping and cold HOF transfer, including side effects that
//! must not replay, cursor mutations, exact roots and callback entry guards.
use super::*;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::bytecode::vm::{ChainBacktrace, ChainLink};
use crate::emacs_core::error::{FlowKind, FlowResultExt as _};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::{self, Inline2Mode, NativeRun};
use crate::emacs_core::jit::vframe::{
    BtState, DeoptChain, HofResume, InlinedFrameResume, Link, SpillRange, VFrameKind, VFrameMeta,
};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

struct Knobs;
impl Knobs {
    fn enter() -> Self {
        crate::test_utils::init_test_tracing();
        crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(Inline2Mode::Hof));
        compile::force_profit_gate_for_test(false);
        compile::force_deopt_for_test(false);
        Self
    }
}
impl Drop for Knobs {
    fn drop(&mut self) {
        crate::emacs_core::jit::inline::force_inline_for_test(None);
        compile::force_inline2_for_test(None);
    }
}

fn function(arity: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|i| intern(&format!("hof-runtime-arg-{i}")))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 24;
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

fn add_callback() -> Value {
    Value::make_bytecode(function(
        1,
        vec![Op::StackRef(0), Op::Add1, Op::Return],
        vec![],
    ))
}

fn caller(kind: HofKind, callback: Value) -> ByteCodeFunction {
    function(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![
            Value::symbol(match kind {
                HofKind::Mapc => "mapc",
                HofKind::Mapcar => "mapcar",
            }),
            callback,
        ],
    )
}

fn invoke(ctx: &mut Context, f: &ByteCodeFunction, sequence: Value) -> Result<Value, Flow> {
    let leaf =
        compile::compile_bytecode_function_with(f, Some(&ctx.obarray)).expect("HOF compiles");
    let _ = ctx.debug_on_next_call_is_armed();
    match leaf.call(ctx as *mut Context as *mut u8, &[sequence]) {
        NativeRun::Ok(bits) => Ok(Value::from_bits(bits)),
        NativeRun::DeoptAt(resume) => {
            compile::resumed_chain::resume_deopt(ctx, f, Value::NIL, &leaf, *resume)
        }
        NativeRun::Signal => Err(compile::take_pending_flow().expect("mapping pending flow")),
        other => panic!("unexpected mapping exit: {other:?}"),
    }
}

fn active(
    ctx: &mut Context,
    kind: HofKind,
    callback: Value,
    sequence: Value,
    index: usize,
) -> HofResume {
    let map = Value::symbol(match kind {
        HofKind::Mapc => "mapc",
        HofKind::Mapcar => "mapcar",
    });
    let words = [
        map.bits() as i64,
        callback.bits() as i64,
        sequence.bits() as i64,
    ];
    let len = neovm_jit_hof_length(sequence.bits() as i64);
    let bt = neovm_jit_hof_start(
        ctx as *mut Context as *mut u8,
        map.bits() as i64,
        words.as_ptr(),
        3,
        len,
        kind as i64,
    );
    assert!(bt >= 0);
    let mut tail = sequence;
    for _ in 0..index {
        tail = tail.cons_cdr();
    }
    ctx.set_vm_frame_root_slot(3, tail);
    HofResume {
        kind,
        function: callback,
        sequence,
        len: len as usize,
        tail,
        item: tail.cons_car(),
        index,
        bt: bt as usize,
        sink_base: 4,
        callback_entered: true,
    }
}

fn frame(callback: Value, pc: usize, stack: Vec<Value>) -> InlinedFrameResume {
    InlinedFrameResume {
        function: callback,
        pc,
        stack,
        binds: Vec::new(),
        link: ChainLink::Bcall { nargs: 1 },
        backtrace: ChainBacktrace::Virtual,
    }
}

fn abort_signal_observer(ctx: &mut Context, symbol: Value, _data: Value) -> Result<Value, Flow> {
    let count = ctx
        .obarray
        .symbol_value("hof-abort-hook-count")
        .copied()
        .unwrap()
        .as_fixnum()
        .unwrap();
    ctx.set_variable("hof-abort-hook-count", Value::make_int(count + 1));
    ctx.set_variable("hof-abort-hook-depth", Value::make_int(ctx.depth as i64));
    ctx.set_variable("hof-abort-hook-symbol", symbol);
    let mapping = ctx.specpdl.iter().find_map(|entry| {
        let (function, args, _, _) = ctx.backtrace_entry_values(entry)?;
        (function == Value::symbol("mapc")).then_some(args)
    });
    if let Some(args) = mapping {
        ctx.set_variable("hof-abort-hook-callback", args[0]);
        ctx.set_variable("hof-abort-hook-sequence", args[1]);
    }
    Ok(Value::NIL)
}

#[test]
fn inline_hof_runtime_abort_dispatches_signal_with_live_map_depth_and_frame() {
    use crate::emacs_core::eval::{SubrEntry, register_global_subr_entry};
    use crate::tagged::header::{SubrDispatchKind, SubrFn};

    let mut ctx = Context::new();
    let observer = intern("hof-abort-signal-observer");
    register_global_subr_entry(
        observer,
        SubrEntry {
            function: Some(SubrFn::A2(abort_signal_observer)),
            min_args: 2,
            max_args: Some(2),
            dispatch_kind: SubrDispatchKind::Builtin,
            portability: crate::emacs_core::subr::SubrPortability::AllTargets,
            interactive_spec: None,
        },
    );
    ctx.set_function(
        "hof-abort-signal-observer",
        Value::subr_from_sym_id(observer),
    );
    ctx.set_variable("signal-hook-function", Value::from_sym_id(observer));
    ctx.set_variable("hof-abort-hook-count", Value::make_int(0));
    ctx.set_variable("hof-abort-hook-callback", Value::NIL);
    ctx.set_variable("hof-abort-hook-sequence", Value::NIL);
    // Preserve a parent root frame across the entire abort operation.
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(Value::make_int(7));
    let callback = add_callback();
    let sequence = Value::list(vec![Value::make_int(1)]);
    ctx.push_vm_frame_root(callback);
    ctx.push_vm_frame_root(sequence);
    for native_abort in [true, false] {
        ctx.set_variable("hof-abort-hook-count", Value::make_int(0));
        ctx.set_variable("hof-abort-hook-depth", Value::NIL);
        ctx.set_variable("hof-abort-hook-callback", Value::NIL);
        ctx.set_variable("hof-abort-hook-sequence", Value::NIL);
        let state = active(&mut ctx, HofKind::Mapc, callback, sequence, 0);
        assert_eq!(ctx.depth, 1);
        let flow = crate::emacs_core::error::signal("quit", Vec::new());
        let flow = if native_abort {
            stash_pending_flow(flow);
            assert_eq!(
                neovm_jit_hof_abort(&mut ctx as *mut Context as *mut u8, state.bt as i64),
                Value::NIL.bits() as i64
            );
            take_pending_flow().expect("abort preserves quit")
        } else {
            finish_mapping(
                &mut ctx,
                sequence,
                0,
                state.bt,
                state.sink_base,
                HofKind::Mapc,
                Err(flow),
            )
            .expect_err("cold completion preserves quit")
        };
        let FlowKind::Signal(signal) = flow.into_kind() else {
            panic!("mapping completion preserves signal kind")
        };
        assert_eq!(signal.symbol, intern("quit"));
        assert!(signal.search_complete);
        for (name, expected) in [
            ("hof-abort-hook-count", Value::make_int(1)),
            // One eager mapping call plus the hook's own Ffuncall frame.
            ("hof-abort-hook-depth", Value::make_int(2)),
            ("hof-abort-hook-symbol", Value::symbol("quit")),
            ("hof-abort-hook-callback", callback),
            ("hof-abort-hook-sequence", sequence),
        ] {
            assert_eq!(ctx.obarray.symbol_value(name), Some(&expected), "{name}");
        }
        assert_eq!(ctx.depth, 0);
        assert!(ctx.specpdl.is_empty());
        assert_eq!(ctx.save_vm_frame_roots(), 3);
    }
    ctx.pop_vm_root_frame();
}

#[test]
fn inline_hof_runtime_length_declines_invalid_lists_without_allocation() {
    let _ctx = Context::new();
    let pair = Value::cons(Value::make_int(1), Value::NIL);
    let list = Value::list(vec![
        Value::make_int(1),
        Value::make_int(2),
        Value::make_int(3),
    ]);
    let dotted = Value::cons(Value::make_int(1), Value::make_int(2));
    let circular = Value::cons(Value::make_int(1), Value::NIL);
    circular.set_cdr(circular);
    let long = Value::list((0..=HOF_LIST_PREFLIGHT_MAX).map(Value::make_int).collect());
    let at_bound = long.cons_cdr();
    let before = Value::memory_use_counts_snapshot();
    for (sequence, expected) in [
        (Value::NIL, 0),
        (pair, 1),
        (list, 3),
        (dotted, -1),
        (circular, -1),
        (Value::make_int(8), -1),
        (at_bound, HOF_LIST_PREFLIGHT_MAX),
        (long, -1),
    ] {
        assert_eq!(neovm_jit_hof_length(sequence.bits() as i64), expected);
    }
    assert_eq!(Value::memory_use_counts_snapshot(), before);
}

#[test]
fn inline_hof_runtime_native_mapc_mapcar_preserve_allocation_deltas() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let callback = add_callback();
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(callback);
    for len in [0, 1, 255, 256, 300] {
        let sequence = Value::list((0..len).map(Value::make_int).collect());
        ctx.push_vm_frame_root(sequence);
        for kind in [HofKind::Mapc, HofKind::Mapcar] {
            let f = caller(kind, callback);
            let leaf = compile::compile_bytecode_function_with(&f, Some(&ctx.obarray)).unwrap();
            assert!(
                leaf.chains.iter().any(|chain| chain
                    .frames
                    .iter()
                    .any(|frame| matches!(frame.kind, VFrameKind::HofMapping { .. }))),
                "native intrinsic emitted"
            );
            let _ = ctx.debug_on_next_call_is_armed();
            // Exact allocation parity is measured without collector timing;
            // a forced collection itself may allocate elapsed-time floats. Other
            // fixtures exercise the live prefix/cursor roots with stress enabled.
            let (value, native_delta, reference, reference_delta) = ctx.with_gc_inhibited(|ctx| {
                crate::emacs_core::eval::reset_bytecode_branch_poll_count();
                let before = Value::memory_use_counts_snapshot();
                let NativeRun::Ok(bits) = leaf.call(ctx as *mut Context as *mut u8, &[sequence])
                else {
                    panic!("valid fixnums stay native")
                };
                assert_eq!(
                    crate::emacs_core::eval::bytecode_branch_poll_count(),
                    if len == 0 {
                        0
                    } else {
                        ((len - 1) / 255) as usize
                    },
                    "{kind:?} length {len}: GNU-style bounded branch polls"
                );
                let value = Value::from_bits(bits);
                ctx.push_vm_frame_root(value);
                let native_delta = std::array::from_fn::<_, 7, _>(|i| {
                    Value::memory_use_counts_snapshot()[i] - before[i]
                });
                let before = Value::memory_use_counts_snapshot();
                let reference = ctx.apply2(f.constants[0], callback, sequence).unwrap();
                let reference_delta = std::array::from_fn::<_, 7, _>(|i| {
                    Value::memory_use_counts_snapshot()[i] - before[i]
                });
                (value, native_delta, reference, reference_delta)
            });
            assert_eq!(native_delta, reference_delta, "{kind:?}");
            assert_eq!(print_value(&value), print_value(&reference));
            if kind == HofKind::Mapc {
                assert_eq!(value, sequence);
            }
            assert_eq!(ctx.depth, 0);
            assert!(ctx.specpdl.is_empty());
        }
    }
    ctx.pop_vm_root_frame();
}

#[test]
fn inline_hof_runtime_mid_callback_keeps_prefix_results_and_does_not_replay() {
    let mut ctx = Context::new();
    // Replaying the prefix would replace the already-written car with 999.
    let callback = Value::make_bytecode(function(
        1,
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Setcar,
            Op::Pop,
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::make_int(999), Value::make_int(42)],
    ));
    let first = Value::cons(Value::make_int(10), Value::NIL);
    let second = Value::cons(Value::make_int(20), Value::NIL);
    let sequence = Value::list(vec![first, second]);
    let mut state = active(&mut ctx, HofKind::Mapcar, callback, sequence, 1);
    // The first callback already produced this result. The second callback's
    // store committed before its guard; resumption begins after that store.
    ctx.set_vm_frame_root_slot(state.sink_base, Value::make_int(17));
    second.set_car(Value::make_int(123));
    state.callback_entered = true;
    let result = resume_mapping(&mut ctx, &state, &frame(callback, 4, vec![second]), 0).unwrap();
    assert_eq!(print_value(&result), "(17 42)");
    assert_eq!(second.cons_car().as_fixnum(), Some(123));
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_hof_runtime_callback_reads_cdr_after_mutation() {
    let mut ctx = Context::new();
    let sequence = Value::list(vec![
        Value::make_int(1),
        Value::make_int(2),
        Value::make_int(3),
    ]);
    let callback = Value::make_bytecode(function(
        1,
        vec![
            Op::Constant(0),
            Op::Nil,
            Op::Setcdr,
            Op::Pop,
            Op::StackRef(0),
            Op::Return,
        ],
        vec![sequence],
    ));
    let state = active(&mut ctx, HofKind::Mapcar, callback, sequence, 0);
    let result = resume_mapping(
        &mut ctx,
        &state,
        &frame(callback, 0, vec![Value::make_int(1)]),
        0,
    )
    .unwrap();
    assert_eq!(print_value(&result), "(1)");
    assert!(sequence.cons_cdr().is_nil());
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_hof_runtime_entry_depth_guard_uses_funcall_and_restores_map() {
    let mut ctx = Context::new();
    let callback = add_callback();
    let sequence = Value::list(vec![Value::make_int(1)]);
    let mut state = active(&mut ctx, HofKind::Mapcar, callback, sequence, 0);
    state.callback_entered = false;
    ctx.depth = 120;
    ctx.set_variable("max-lisp-eval-depth", Value::make_int(120));
    ctx.max_depth = 120;
    let error = resume_mapping(
        &mut ctx,
        &state,
        &frame(callback, 0, vec![Value::make_int(1)]),
        0,
    )
    .unwrap_err();
    let FlowKind::Signal(signal) = error.into_kind() else {
        panic!("depth guard must signal")
    };
    assert_eq!(signal.symbol_name(), "excessive-lisp-nesting");
    assert_eq!(signal.data[0].as_fixnum(), Some(121));
    assert_eq!(
        ctx.depth, 119,
        "only the eager map frame remains ours to decrement"
    );
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_hof_runtime_mid_callback_skips_entry_depth_guard_and_unwinds_error() {
    let mut ctx = Context::new();
    let callback = Value::make_bytecode(function(
        1,
        vec![Op::StackRef(0), Op::Car, Op::Return],
        vec![],
    ));
    let sequence = Value::list(vec![Value::make_int(1)]);
    let state = active(&mut ctx, HofKind::Mapc, callback, sequence, 0);
    ctx.max_depth = ctx.depth;
    let error = resume_mapping(
        &mut ctx,
        &state,
        &frame(callback, 1, vec![Value::make_int(1), Value::make_int(1)]),
        0,
    )
    .unwrap_err();
    let FlowKind::Signal(signal) = error.into_kind() else {
        panic!("car must signal")
    };
    assert_eq!(signal.symbol_name(), "wrong-type-argument");
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

fn metadata(parent_len: u32, callback_pc: u32, callback_len: u32) -> DeoptChain {
    DeoptChain {
        frames: vec![
            VFrameMeta {
                kind: VFrameKind::PhysicalBytecode,
                pc: 3,
                stack: SpillRange {
                    start: 0,
                    len: parent_len,
                },
                binds: 0,
                handlers: 0,
                bt: BtState::Physical,
            },
            VFrameMeta {
                kind: VFrameKind::HofMapping {
                    kind: HofKind::Mapcar,
                    callback_entered: true,
                },
                pc: 3,
                stack: SpillRange {
                    start: parent_len,
                    len: 8,
                },
                binds: 0,
                handlers: 0,
                bt: BtState::Virtual,
            },
            VFrameMeta {
                kind: VFrameKind::ClosureBytecode {
                    function_slot: parent_len,
                    link: Link::HofCallback,
                },
                pc: callback_pc,
                stack: SpillRange {
                    start: parent_len + 8,
                    len: callback_len,
                },
                binds: 0,
                handlers: 0,
                bt: BtState::Virtual,
            },
        ]
        .into_boxed_slice(),
    }
}

#[test]
fn inline_hof_runtime_metadata_preserves_actual_callback_and_state() {
    let mut ctx = Context::new();
    let callback = add_callback();
    let sequence = Value::list(vec![Value::make_int(7), Value::make_int(8)]);
    let state = active(&mut ctx, HofKind::Mapcar, callback, sequence, 1);
    let spill = [
        Value::symbol("mapcar"),
        callback,
        sequence,
        callback,
        sequence,
        Value::make_int(2),
        state.tail,
        Value::make_int(1),
        Value::make_int(state.bt as i64),
        Value::make_int(4),
        Value::make_int(8),
        Value::make_int(8),
        Value::make_int(8),
    ];
    let readback = metadata(3, 1, 2)
        .readback(&spill, &[], &[], &ctx, 0, 21)
        .unwrap();
    assert_eq!(readback.stack, spill[..3]);
    assert_eq!(readback.inlined.hof.as_ref(), Some(&state));
    assert_eq!(readback.inlined.guard_site_pc, 21);
    assert_eq!(readback.inlined.frames[0].function, callback);
    assert_eq!(readback.inlined.frames[0].pc, 1);
    assert_eq!(readback.inlined.frames[0].stack, spill[11..]);
    finish_mapping(&mut ctx, sequence, 0, state.bt, 4, HofKind::Mapc, Ok(())).unwrap();
}

#[test]
fn inline_hof_runtime_metadata_rejects_malformed_state_and_active_handlers() {
    let mut ctx = Context::new();
    let callback = add_callback();
    let sequence = Value::list(vec![Value::make_int(7)]);
    let state = active(&mut ctx, HofKind::Mapcar, callback, sequence, 0);
    let spill = vec![
        Value::symbol("mapcar"),
        callback,
        sequence,
        callback,
        sequence,
        Value::make_int(1),
        sequence,
        Value::make_int(0),
        Value::make_int(state.bt as i64),
        Value::make_int(4),
        Value::make_int(7),
        Value::make_int(7),
    ];
    assert!(
        metadata(3, 0, 1)
            .readback(&spill, &[], &[], &ctx, 0, 21)
            .is_ok()
    );
    for slot in [5, 7, 8, 9] {
        let mut bad = spill.clone();
        bad[slot] = Value::make_int(-1);
        assert!(
            metadata(3, 0, 1)
                .readback(&bad, &[], &[], &ctx, 0, 21)
                .is_err(),
            "state slot {slot}"
        );
    }
    for frame in 0..3 {
        let mut bad = metadata(3, 0, 1);
        bad.frames[frame].handlers = 1;
        assert!(bad.readback(&spill, &[], &[], &ctx, 0, 21).is_err());
    }
    let mut bad = metadata(3, 1, 1);
    bad.frames[1].kind = VFrameKind::HofMapping {
        kind: HofKind::Mapcar,
        callback_entered: false,
    };
    assert!(bad.readback(&spill, &[], &[], &ctx, 0, 21).is_err());
    ctx.pop_vm_root_frame();
    assert!(
        metadata(3, 0, 1)
            .readback(&spill, &[], &[], &ctx, 0, 21)
            .is_err(),
        "a forged map backtrace cannot replace its missing activation roots"
    );
    ctx.depth -= 1;
    ctx.unbind_to(state.bt);
}

#[test]
fn inline_hof_runtime_mid_callback_backtrace_retains_original_argument() {
    let mut ctx = crate::test_utils::runtime_startup_context();
    let callback = Value::make_bytecode(function(
        1,
        vec![Op::Constant(0), Op::Constant(1), Op::Call(1), Op::Return],
        vec![Value::symbol("backtrace-frame"), Value::make_int(1)],
    ));
    let sequence = Value::list(vec![Value::make_int(1)]);
    let state = active(&mut ctx, HofKind::Mapcar, callback, sequence, 0);
    sequence.set_car(Value::make_int(9));
    // The local parameter also differs: neither it nor the mutated list car
    // is the argument GNU's record_in_backtrace retained on callback entry.
    let result = resume_mapping(
        &mut ctx,
        &state,
        &frame(
            callback,
            2,
            vec![
                Value::make_int(8),
                Value::symbol("backtrace-frame"),
                Value::make_int(1),
            ],
        ),
        0,
    )
    .unwrap();
    let backtrace = result.cons_car();
    assert_eq!(backtrace.cons_cdr().cons_car(), callback);
    assert_eq!(
        backtrace.cons_cdr().cons_cdr().cons_car().as_fixnum(),
        Some(1)
    );
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_hof_runtime_native_guard_resumes_lambda_and_parent_after_call() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let callback = add_callback();
    let f = caller(HofKind::Mapcar, callback);
    let sequence = Value::list(vec![
        Value::make_int(1),
        Value::make_float(1.5),
        Value::make_int(3),
    ]);
    let leaf = compile::compile_bytecode_function_with(&f, Some(&ctx.obarray)).unwrap();
    let _ = ctx.debug_on_next_call_is_armed();
    let before = Value::memory_use_counts_snapshot();
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence])
    else {
        panic!("mixed Add1 guard transfers the mapping")
    };
    let readback = resume.inlined.as_ref().expect("mapping chain");
    let state = readback.hof.as_ref().expect("HOF state");
    assert_eq!(state.index, 1);
    assert!(state.callback_entered);
    assert_eq!(state.item.as_float(), Some(1.5));
    assert_eq!(readback.frames[0].pc, 1);
    assert_eq!(
        ctx.vm_frame_root_slots(state.sink_base, 1),
        &[Value::make_int(2)]
    );
    let result =
        compile::resumed_chain::resume_deopt(&mut ctx, &f, Value::NIL, &leaf, *resume).unwrap();
    assert_eq!(print_value(&result), "(2 2.5 4)");
    assert_eq!(
        Value::memory_use_counts_snapshot()[0] - before[0],
        3,
        "one result list, no call replay"
    );
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
    assert!(ctx.bc_buf.is_empty());
    assert!(ctx.bc_frames.is_empty());
}

#[test]
fn inline_hof_runtime_native_deopt_at_each_element_preserves_committed_capture_once() {
    let _knobs = Knobs::enter();
    for gc_stress in [false, true] {
        for kind in [HofKind::Mapc, HofKind::Mapcar] {
            for guard_index in 0..4 {
                let mut ctx = Context::new();
                ctx.gc_stress = gc_stress;
                let cell = Value::cons(Value::make_int(0), Value::NIL);
                let prototype = Value::make_bytecode(function(
                    1,
                    vec![
                        Op::Constant(0),
                        Op::Dup,
                        Op::Car,
                        Op::Add1,
                        Op::Setcar,
                        Op::Pop,
                        Op::StackRef(0),
                        Op::Add1,
                        Op::Return,
                    ],
                    vec![Value::symbol("V0")],
                ));
                ctx.push_vm_root_frame();
                ctx.push_vm_frame_root(cell);
                ctx.push_vm_frame_root(prototype);
                let callback = ctx
                    .apply2(Value::symbol("make-closure"), prototype, cell)
                    .unwrap();
                ctx.push_vm_frame_root(callback);
                let sequence = Value::list(
                    (0..4)
                        .map(|index| {
                            if index == guard_index {
                                Value::make_float(1.5)
                            } else {
                                Value::make_int(index as i64)
                            }
                        })
                        .collect(),
                );
                ctx.push_vm_frame_root(sequence);
                let f = caller(kind, callback);
                let leaf = compile::compile_bytecode_function_with(&f, Some(&ctx.obarray)).unwrap();
                let _ = ctx.debug_on_next_call_is_armed();
                let run = |ctx: &mut Context| {
                    let before = Value::memory_use_counts_snapshot();
                    let NativeRun::DeoptAt(resume) =
                        leaf.call(ctx as *mut Context as *mut u8, &[sequence])
                    else {
                        panic!("{kind:?} element {guard_index}: mixed Add1 must deopt")
                    };
                    let inlined = resume.inlined.as_ref().expect("native mapping chain");
                    let state = inlined.hof.as_ref().expect("native HOF state");
                    assert_eq!(state.kind, kind);
                    assert_eq!(state.index, guard_index);
                    assert!(state.callback_entered);
                    assert_eq!(inlined.frames[0].pc, 7);
                    assert_eq!(state.item.as_float(), Some(1.5));
                    assert_eq!(cell.cons_car().as_fixnum(), Some(guard_index as i64 + 1));
                    if kind == HofKind::Mapcar {
                        assert_eq!(
                            ctx.vm_frame_root_slots(state.sink_base, guard_index),
                            (1..=guard_index as i64)
                                .map(Value::make_int)
                                .collect::<Vec<_>>(),
                        );
                    }
                    let value =
                        compile::resumed_chain::resume_deopt(ctx, &f, Value::NIL, &leaf, *resume)
                            .unwrap();
                    let native_delta = std::array::from_fn::<_, 7, _>(|i| {
                        Value::memory_use_counts_snapshot()[i] - before[i]
                    });
                    assert_eq!(cell.cons_car().as_fixnum(), Some(4), "no callback replay");
                    assert_eq!(ctx.depth, 0);
                    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty());
                    assert!(ctx.bc_frames.is_empty() && ctx.jit_bind_stack.is_empty());
                    assert_eq!(ctx.save_vm_frame_roots(), 4, "mapping roots completed");
                    ctx.push_vm_frame_root(value);
                    cell.set_car(Value::make_int(0));
                    callback
                        .get_bytecode_data()
                        .unwrap()
                        .jit_runtime()
                        .set_cold_for_test();
                    let before = Value::memory_use_counts_snapshot();
                    let reference = ctx.apply2(f.constants[0], callback, sequence).unwrap();
                    let reference_delta = std::array::from_fn::<_, 7, _>(|i| {
                        Value::memory_use_counts_snapshot()[i] - before[i]
                    });
                    ctx.push_vm_frame_root(reference);
                    assert_eq!(cell.cons_car().as_fixnum(), Some(4));
                    assert_eq!(print_value(&value), print_value(&reference));
                    if kind == HofKind::Mapc {
                        assert_eq!(value.bits(), sequence.bits());
                    }
                    assert_eq!(ctx.depth, 0);
                    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty());
                    assert!(ctx.bc_frames.is_empty() && ctx.jit_bind_stack.is_empty());
                    assert_eq!(ctx.save_vm_frame_roots(), 6);
                    (native_delta, reference_delta)
                };
                // Compare all allocation counters without collector timing,
                // then repeat every element with actual stress collections.
                let (native_delta, reference_delta) = if gc_stress {
                    run(&mut ctx)
                } else {
                    ctx.with_gc_inhibited(run)
                };
                if gc_stress {
                    assert_eq!(native_delta[0], reference_delta[0]);
                } else {
                    assert_eq!(
                        native_delta, reference_delta,
                        "{kind:?} element {guard_index}"
                    );
                }
                assert_eq!(native_delta[0], if kind == HofKind::Mapcar { 4 } else { 0 });
                ctx.pop_vm_root_frame();
                assert!(ctx.vm_frame_root_slots_checked(0, 0).is_none());
            }
        }
    }
}

#[test]
fn inline_hof_runtime_native_mutation_shortens_mapping() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let sequence = Value::list(vec![
        Value::make_int(1),
        Value::make_int(2),
        Value::make_int(3),
    ]);
    let callback = Value::make_bytecode(function(
        1,
        vec![
            Op::Constant(0),
            Op::Nil,
            Op::Setcdr,
            Op::Pop,
            Op::StackRef(0),
            Op::Return,
        ],
        vec![sequence],
    ));
    let result = invoke(&mut ctx, &caller(HofKind::Mapcar, callback), sequence).unwrap();
    assert_eq!(print_value(&result), "(1)");
    assert!(sequence.cons_cdr().is_nil());
    assert_eq!(ctx.depth, 0);
    assert!(ctx.specpdl.is_empty());
}

#[test]
fn inline_hof_runtime_nonlist_or_invalid_list_keeps_builtin_protocol() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    let callback = add_callback();
    let f = caller(HofKind::Mapcar, callback);
    let dotted = Value::cons(Value::make_int(1), Value::make_int(2));
    let circular = Value::cons(Value::make_int(1), Value::NIL);
    circular.set_cdr(circular);
    let vector = Value::vector(vec![Value::make_int(1), Value::make_int(2)]);
    let sequences = [Value::NIL, dotted, circular, vector, Value::string("ab")];
    // Later inputs survive collections performed while an earlier variant
    // runs. A Rust fixture variable is not a Lisp root.
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(callback);
    for &sequence in &sequences {
        ctx.push_vm_frame_root(sequence);
    }
    for sequence in sequences {
        let native = invoke(&mut ctx, &f, sequence);
        if let Ok(value) = native.as_ref() {
            ctx.push_vm_frame_root(*value);
        }
        let reference = ctx.apply2(f.constants[0], callback, sequence);
        match (native.kinded(), reference.kinded()) {
            (Ok(native), Ok(reference)) => {
                assert_eq!(print_value(&native), print_value(&reference))
            }
            (Err(FlowKind::Signal(native)), Err(FlowKind::Signal(reference))) => {
                assert_eq!(native.symbol_name(), reference.symbol_name());
                assert_eq!(native.data, reference.data);
            }
            other => panic!("builtin protocol differs: {other:?}"),
        }
        assert_eq!(ctx.depth, 0);
        assert!(ctx.specpdl.is_empty());
    }
    ctx.pop_vm_root_frame();
}

#[test]
fn inline_hof_runtime_rechecks_callback_depth_after_a_serviced_poll() {
    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.eval_str(
        "(setq post-gc-hook (list (lambda () (setq post-gc-hook nil max-lisp-eval-depth 100))))",
    )
    .unwrap();
    let callback = add_callback();
    let f = caller(HofKind::Mapcar, callback);
    let sequence = Value::list((0..300).map(Value::make_int).collect());
    let leaf = compile::compile_bytecode_function_with(&f, Some(&ctx.obarray)).unwrap();
    let _ = ctx.debug_on_next_call_is_armed();
    ctx.depth = 100;
    ctx.max_depth = 1000;
    ctx.gc_stress = true;
    crate::emacs_core::eval::reset_bytecode_branch_poll_count();
    let NativeRun::DeoptAt(resume) = leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence])
    else {
        panic!("the next callback must observe the poll hook's new depth limit")
    };
    assert_eq!(crate::emacs_core::eval::bytecode_branch_poll_count(), 1);
    assert_eq!(
        resume.cause,
        Some(crate::emacs_core::jit::reopt::DeoptCause::DepthLimit)
    );
    let readback = resume.inlined.as_ref().expect("mapping chain");
    let state = readback.hof.as_ref().expect("HOF state");
    assert_eq!(state.index, 255);
    assert!(!state.callback_entered);
    assert_eq!(state.item, Value::make_int(255));
    assert_eq!(ctx.depth, 101, "only the eager mapping frame is live");
    assert_eq!(ctx.vm_frame_root_slots(state.sink_base, 255).len(), 255);
    // Finish against a restored limit to verify the suspended callback and
    // remaining tail each run once, retaining the first 255 result slots.
    ctx.gc_stress = false;
    ctx.max_depth = 1000;
    ctx.set_variable("max-lisp-eval-depth", Value::make_int(1000));
    let result =
        compile::resumed_chain::resume_deopt(&mut ctx, &f, Value::NIL, &leaf, *resume).unwrap();
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&result).unwrap(),
        (1..=300).map(Value::make_int).collect::<Vec<_>>()
    );
    assert_eq!(ctx.depth, 100);
    assert!(ctx.specpdl.is_empty() && ctx.bc_buf.is_empty() && ctx.bc_frames.is_empty());
}

#[test]
fn inline_hof_runtime_collection_capture_declines_native_loop_and_records_mapping_reads() {
    use crate::emacs_core::jit::reopt::{self, DeoptCause, ReoptKnobs};
    use crate::tagged::collection_reads::capture;

    let _knobs = Knobs::enter();
    reopt::force_reopt_for_test(Some(ReoptKnobs {
        site_limit: 1,
        ..ReoptKnobs::stress()
    }));
    for kind in [HofKind::Mapc, HofKind::Mapcar] {
        for changed_index in 0..4 {
            let mut ctx = Context::new();
            let callback = add_callback();
            let f = caller(kind, callback);
            let sequence = Value::list((0..4).map(Value::make_int).collect());
            ctx.push_vm_root_frame();
            ctx.push_vm_frame_root(callback);
            ctx.push_vm_frame_root(sequence);
            let mut cells = Vec::new();
            let mut tail = sequence;
            while tail.is_cons() {
                cells.push(tail);
                tail = tail.cons_cdr();
            }
            let leaf = compile::compile_bytecode_function_with(&f, Some(&ctx.obarray))
                .expect("admitted HOF compiles");
            assert!(
                !leaf.chains.is_empty(),
                "the native mapping intrinsic is present"
            );
            let _ = ctx.debug_on_next_call_is_armed();
            let mut last_reads = None;
            for attempt in 0..3 {
                let (result, reads) = capture(|| {
                    assert_eq!(neovm_jit_hof_length(sequence.bits() as i64), -1);
                    let NativeRun::DeoptAt(resume) =
                        leaf.call(&mut ctx as *mut Context as *mut u8, &[sequence])
                    else {
                        panic!("an active capture must decline the direct-load mapping loop")
                    };
                    assert_eq!(resume.cause, Some(DeoptCause::ColdFlagged));
                    assert_eq!(resume.pc, 3, "decline before the original mapping call");
                    assert!(
                        resume.inlined.is_none(),
                        "no eager mapping frame was entered"
                    );
                    compile::resumed_chain::resume_deopt(&mut ctx, &f, Value::NIL, &leaf, *resume)
                        .expect("ordinary mapping keeps its capture protocol")
                });
                match kind {
                    HofKind::Mapc => assert_eq!(result.bits(), sequence.bits()),
                    HofKind::Mapcar => assert_eq!(print_value(&result), "(1 2 3 4)"),
                }
                assert_eq!(f.jit_runtime().reopt_count(), 0, "capture {attempt}");
                assert!(
                    !f.jit_runtime().call_site_no_inline(3),
                    "semantic capture {attempt} must leave HOF admission enabled"
                );
                assert_eq!(leaf.obs.reopt_deopt_count_at(3), 0);
                last_reads = reads;
            }
            let reads = last_reads.expect("read-only mapping has coherent dependencies");
            assert!(reads.unchanged());
            cells[changed_index].set_car(Value::make_int(99));
            assert!(
                !reads.unchanged(),
                "{kind:?} cell {changed_index} was observed"
            );
            assert_eq!(ctx.depth, 0);
            assert!(ctx.specpdl.is_empty() && ctx.bc_frames.is_empty() && ctx.bc_buf.is_empty());
            ctx.pop_vm_root_frame();
        }
    }
    reopt::force_reopt_for_test(None);
}

#[test]
fn inline_hof_runtime_native_cache_call_preserves_mid_map_deopt_floor_and_prefix() {
    use crate::emacs_core::jit::cache::{self, NativeCallOutcome};

    let _knobs = Knobs::enter();
    let mut ctx = Context::new();
    ctx.push_vm_root_frame();
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    ctx.push_vm_frame_root(cell);
    let prototype = Value::make_bytecode(function(
        1,
        vec![
            Op::Constant(0),
            Op::Dup,
            Op::Car,
            Op::Add1,
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Add1,
            Op::Return,
        ],
        vec![Value::symbol("V0")],
    ));
    ctx.push_vm_frame_root(prototype);
    let callback = ctx
        .apply2(Value::symbol("make-closure"), prototype, cell)
        .unwrap();
    ctx.push_vm_frame_root(callback);
    let sequence = Value::list(vec![
        Value::make_int(1),
        Value::make_float(1.5),
        Value::make_int(3),
    ]);
    ctx.push_vm_frame_root(sequence);
    let func_value = Value::make_bytecode(caller(HofKind::Mapcar, callback));
    ctx.push_vm_frame_root(func_value);
    let f = func_value.get_bytecode_data().unwrap();
    // Exercise the memory entry regardless of the process's direct-call knob.
    compile::force_register_abi_for_test(Some(false));
    let leaf = compile::compile_bytecode_function_with(f, Some(&ctx.obarray)).unwrap();
    compile::force_register_abi_for_test(None);
    assert!(!leaf.chains.is_empty(), "the callback is admitted natively");
    let _ = ctx.debug_on_next_call_is_armed();

    // Model the enclosing native caller's extent and preserve its live frame.
    let depth0 = ctx.depth;
    let spec0 = ctx.specpdl.len();
    let outer = Value::symbol("hof-native-cache-outer");
    ctx.push_backtrace_frame(outer, &[sequence]);
    ctx.depth += 1;
    let floors = (
        ctx.depth,
        ctx.specpdl.len(),
        ctx.jit_bind_stack.len(),
        ctx.condition_stack_len(),
        ctx.bc_frames.len(),
        ctx.bc_buf.len(),
        ctx.save_vm_frame_roots(),
    );
    let scratch0 = crate::emacs_core::eval::save_scratch_gc_roots();
    let bases = compile::JitLeafBases {
        snap: ctx.module_boundary_snapshot(),
    };
    let outer_bases = compile::CURRENT_LEAF_BASES
        .with(|slot| slot.replace(Some(std::ptr::NonNull::from(&bases))));
    let before = Value::memory_use_counts_snapshot();
    let arguments = [sequence.bits() as i64];
    let outcome = ctx.with_gc_inhibited(|ctx| {
        cache::run_resolved_leaf_native(
            ctx as *mut Context,
            f,
            func_value,
            &leaf,
            arguments.as_ptr(),
        )
    });
    compile::CURRENT_LEAF_BASES.with(|slot| slot.set(outer_bases));
    let result = match outcome {
        NativeCallOutcome::Value(value) => value,
        NativeCallOutcome::FlowStashed => panic!(
            "mid-map callback guard must resume, not signal invalid metadata: {:?}",
            compile::take_pending_flow()
        ),
        NativeCallOutcome::Fallback => panic!("the admitted callback must resume its native chain"),
    };
    assert_eq!(print_value(&result), "(2 2.5 4)");
    assert_eq!(cell.cons_car().as_fixnum(), Some(3), "no callback replay");
    let observations = leaf.obs.snapshot();
    assert_eq!(
        observations.chain_deopts, 1,
        "the native callback guard ran"
    );
    assert_eq!(
        observations.chain_pcs,
        vec![(callback.get_bytecode_data().unwrap().source_id, 7, 1)],
        "the first result was committed before the second callback's guard"
    );
    assert_eq!(
        Value::memory_use_counts_snapshot()[0] - before[0],
        3,
        "one result list retains the completed native prefix"
    );
    assert!(leaf.has_binds, "the producer owns an activation floor");
    assert_eq!(leaf.abi, compile::LeafAbi::Memory);
    assert_eq!(leaf.entry_shape, compile::EntryShape::Framed);
    assert!(!leaf.direct_call_eligible());
    assert!(compile::take_pending_flow().is_none());
    assert_eq!(
        (
            ctx.depth,
            ctx.specpdl.len(),
            ctx.jit_bind_stack.len(),
            ctx.condition_stack_len(),
            ctx.bc_frames.len(),
            ctx.bc_buf.len(),
            ctx.save_vm_frame_roots(),
        ),
        floors,
        "the eager map activation and its root frame are balanced"
    );
    assert_eq!(crate::emacs_core::eval::save_scratch_gc_roots(), scratch0);
    let (function, args, _, _) = ctx.backtrace_entry_values(&ctx.specpdl[spec0]).unwrap();
    assert_eq!(function, outer);
    assert_eq!(args.as_slice(), &[sequence]);
    ctx.unbind_to(spec0);
    ctx.depth = depth0;
    ctx.pop_vm_root_frame();
    assert!(ctx.vm_frame_root_slots_checked(0, 0).is_none());
}
