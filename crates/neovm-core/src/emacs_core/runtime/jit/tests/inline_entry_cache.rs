//! Native first-entry and post-service-poll semantics for both protocol policies.

use super::*;
use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::{
    Context, bytecode_branch_poll_count, reset_bytecode_branch_poll_count,
};
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile::{self, Inline2Mode, NativeRun};
use crate::emacs_core::jit::reopt::DeoptCause;
use crate::emacs_core::value::LambdaParams;

struct Knobs;
impl Knobs {
    fn enter() -> Self {
        force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(Inline2Mode::All));
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

fn lexical(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![intern("entry-cache-native-counter")],
        optional: vec![],
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 16;
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

fn caller(effect: bool) -> ByteCodeFunction {
    let step = Value::make_bytecode(lexical(vec![Op::StackRef(0), Op::Sub1, Op::Return], vec![]));
    caller_with_step(effect, false, step)
}

fn flag_caller() -> ByteCodeFunction {
    let step = Value::make_bytecode(lexical(vec![Op::StackRef(0), Op::Sub1, Op::Return], vec![]));
    // A Context setter before the loop prevents physical-entry admission.
    // The loop itself is pure, so it still tests the flag-cache fallback.
    caller_with_step(false, true, step)
}

fn caller_with_step(effect: bool, prelude: bool, step: Value) -> ByteCodeFunction {
    let mut prefix = if prelude {
        vec![Op::Constant(4), Op::VarSet(5)]
    } else {
        vec![]
    };
    let header = prefix.len();
    let mut ops = vec![
        Op::StackRef(0),
        Op::Constant(0),
        Op::Gtr,
        Op::GotoIfNil(0),
        Op::Constant(1),
        Op::StackRef(1),
        Op::Call(1),
        Op::StackSet(1),
    ];
    if effect {
        ops.extend([Op::Constant(2), Op::VarSet(3)]);
    }
    ops.push(Op::Goto(header as u32));
    let end = header + ops.len();
    ops[3] = Op::GotoIfNil(end as u32);
    ops.push(Op::Return);
    prefix.extend(ops);
    lexical(
        prefix,
        vec![
            Value::make_int(0),
            step,
            Value::make_int(100),
            Value::symbol("max-lisp-eval-depth"),
            Value::make_int(0),
            Value::symbol("entry-cache-prelude-marker"),
        ],
    )
}

enum ProtocolMode {
    Physical,
    Flags,
    Fresh,
}

fn compiled(
    ev: &mut Context,
    f: &ByteCodeFunction,
    expected: ProtocolMode,
) -> compile::CompiledLeaf {
    // Standalone leaves are outside the COMPILED cache's reloc-root walk.
    // Keep every source constant alive across native polls and Tier0 resume.
    ev.set_variable(
        "entry-cache-source-constants",
        Value::vector(f.constants.as_slice().to_vec()),
    );
    let _ = ev.debug_on_next_call_is_armed();
    let body = fuse_calls_v2(
        f.executable_ops(),
        &f.constants,
        None,
        1,
        &vec![crate::emacs_core::jit::NumericFeedback::FixnumOnly; f.executable_ops().len()],
    )
    .unwrap();
    let cfg =
        compile::analyze_cfg(&body.ops, &body.constants, body.offset_map.as_deref(), 1).unwrap();
    let _feedback = compile::publish_numeric_feedback_vec(body.feedback.clone());
    let physical = compile::inline_entry_cache::physical_admission_regions(&body, &cfg, 1, None, 0);
    let flags = compile::inline_entry_cache::candidate_regions(&body, &cfg, 1, None, 0);
    match expected {
        ProtocolMode::Physical => assert_eq!(physical, vec![true]),
        ProtocolMode::Flags => {
            assert_eq!(physical, vec![false]);
            assert_eq!(flags, vec![true]);
        }
        ProtocolMode::Fresh => {
            assert_eq!(physical, vec![false]);
            assert_eq!(flags, vec![false]);
        }
    }
    compile::compile_bytecode_function_with(f, Some(&ev.obarray)).unwrap()
}

#[test]
fn inline_entry_cache_empty_loop_never_checks_depth_and_first_call_keeps_original_pc() {
    let _knobs = Knobs::enter();
    let mut ev = Context::new();
    let f = flag_caller();
    let leaf = compiled(&mut ev, &f, ProtocolMode::Flags);
    ev.depth = 2;
    ev.max_depth = 2;
    assert_eq!(
        leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(0)]),
        NativeRun::Ok(Value::make_int(0).bits())
    );
    let NativeRun::DeoptAt(resume) =
        leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(1)])
    else {
        panic!("first real call must check depth");
    };
    assert_eq!(resume.pc, 8);
    assert_eq!(resume.cause, Some(DeoptCause::DepthLimit));
    assert!(resume.inlined.is_none());
    assert_eq!(resume.stack[0], Value::make_int(1));
    assert_eq!(resume.stack[2], Value::make_int(1));
    assert_eq!(ev.depth, 2);
}

#[test]
fn inline_entry_cache_successful_service_poll_rechecks_depth_at_the_next_call() {
    let _knobs = Knobs::enter();
    let mut ev = Context::new();
    let f = flag_caller();
    let leaf = compiled(&mut ev, &f, ProtocolMode::Flags);
    ev.eval_str(
        "(setq post-gc-hook (list (lambda () (setq post-gc-hook nil max-lisp-eval-depth 100))))",
    )
    .unwrap();
    ev.depth = 100;
    ev.max_depth = 1000;
    ev.gc_stress = true;
    reset_bytecode_branch_poll_count();
    let run = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(300)]);
    ev.gc_stress = false;
    assert_eq!(bytecode_branch_poll_count(), 1);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("fresh depth after serviced poll: {run:?}")
    };
    assert_eq!(resume.pc, 8);
    assert_eq!(resume.cause, Some(DeoptCause::DepthLimit));
    assert!(resume.inlined.is_none());
    assert_eq!(resume.stack[0], Value::make_int(45));
    assert_eq!(resume.stack[2], Value::make_int(45));
    assert_eq!(ev.depth, 100);
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn inline_entry_cache_context_write_keeps_every_entry_protocol() {
    let _knobs = Knobs::enter();
    let mut ev = Context::new();
    let f = caller(true);
    let leaf = compiled(&mut ev, &f, ProtocolMode::Fresh);
    ev.depth = 100;
    ev.max_depth = 1000;
    let NativeRun::DeoptAt(resume) =
        leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(3)])
    else {
        panic!("context write must invalidate a dominating protocol");
    };
    assert_eq!(resume.pc, 6);
    assert_eq!(resume.cause, Some(DeoptCause::DepthLimit));
    assert_eq!(resume.stack[0], Value::make_int(2));
    assert_eq!(resume.stack[2], Value::make_int(2));
}

#[test]
fn inline_entry_physical_admission_fallback_keeps_empty_loops_observation_free() {
    let _knobs = Knobs::enter();
    for cause in [DeoptCause::DepthLimit, DeoptCause::InlineAttention] {
        let mut ev = Context::new();
        let f = caller(false);
        let leaf = compiled(&mut ev, &f, ProtocolMode::Physical);
        ev.depth = 100;
        ev.max_depth = 1000;
        match cause {
            DeoptCause::DepthLimit => ev.max_depth = 100,
            DeoptCause::InlineAttention => ev.set_variable("throw-on-input", Value::T),
            _ => unreachable!(),
        }
        let NativeRun::DeoptAt(resume) =
            leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(0)])
        else {
            panic!("failed admission returns the unchanged physical entry");
        };
        assert_eq!(resume.pc, 0);
        assert_eq!(resume.cause, Some(cause));
        assert!(resume.inlined.is_none());
        assert_eq!(resume.stack.as_slice(), &[Value::make_int(0)]);
        reset_bytecode_branch_poll_count();
        assert_eq!(
            compile::resumed_chain::resume_deopt(&mut ev, &f, Value::NIL, &leaf, *resume).unwrap(),
            Value::make_int(0),
            "admission failure cannot manufacture a call/depth/attention error"
        );
        assert_eq!(bytecode_branch_poll_count(), 0);
        assert_eq!(ev.depth, 100);
    }
}

#[test]
fn inline_entry_physical_admission_fallback_preserves_argument_and_depth_error_order() {
    let _knobs = Knobs::enter();
    let mut ev = Context::new();
    let step = Value::make_bytecode(lexical(vec![Op::StackRef(0), Op::Sub1, Op::Return], vec![]));
    let f = lexical(
        vec![
            Op::StackRef(0),
            Op::GotoIfNil(11),
            Op::Constant(0),
            Op::StackRef(1),
            Op::Car,
            Op::Call(1),
            Op::Pop,
            Op::StackRef(0),
            Op::Cdr,
            Op::StackSet(1),
            Op::Goto(0),
            Op::Return,
        ],
        vec![step],
    );
    let leaf = compiled(&mut ev, &f, ProtocolMode::Physical);
    ev.depth = 100;
    ev.max_depth = 100;
    for (arg, expected) in [
        (Value::make_int(42), "wrong-type-argument"),
        (Value::cons(Value::make_int(42), Value::NIL), "error"),
    ] {
        let NativeRun::DeoptAt(resume) = leaf.call(&mut ev as *mut Context as *mut u8, &[arg])
        else {
            panic!("depth admission falls back before argument evaluation");
        };
        assert_eq!(resume.pc, 0);
        assert_eq!(resume.cause, Some(DeoptCause::DepthLimit));
        assert_eq!(resume.stack.as_slice(), &[arg]);
        let error = compile::resumed_chain::resume_deopt(&mut ev, &f, Value::NIL, &leaf, *resume)
            .unwrap_err();
        let Flow::Signal(signal) = error else {
            panic!("the original bytecode must signal")
        };
        assert_eq!(signal.symbol_name(), expected);
        assert_eq!(ev.depth, 100);
    }
}

#[test]
fn inline_entry_physical_poll_fallback_resumes_the_header_without_repeating_the_prefix() {
    let _knobs = Knobs::enter();
    let mut ev = Context::new();
    let counter = Value::cons(Value::make_int(0), Value::NIL);
    ev.set_variable("entry-physical-counter", counter);
    let step = Value::make_bytecode(lexical(
        vec![
            Op::Constant(0),
            Op::Dup,
            Op::Car,
            Op::Add1,
            Op::Setcar,
            Op::Pop,
            Op::StackRef(0),
            Op::Sub1,
            Op::Return,
        ],
        vec![counter],
    ));
    let f = caller_with_step(false, false, step);
    let leaf = compiled(&mut ev, &f, ProtocolMode::Physical);
    // Direct leaves bypass dispatch's heap-cache synchronization. Both cases
    // use the same installed Context and leaf across the native GC boundary.
    for iterations in [255, 300] {
        ev.depth = 0;
        ev.max_depth = 1000;
        ev.eval_str(
            "(progn (setcar entry-physical-counter 0) \
             (setq post-gc-hook \
             (list (lambda () (setq post-gc-hook nil max-lisp-eval-depth 100)))))",
        )
        .unwrap();
        ev.depth = 100;
        ev.max_depth = 1000;
        ev.gc_stress = true;
        reset_bytecode_branch_poll_count();
        let run = leaf.call(
            &mut ev as *mut Context as *mut u8,
            &[Value::make_int(iterations)],
        );
        ev.gc_stress = false;
        let NativeRun::DeoptAt(resume) = run else {
            panic!("changed depth exits at the physical header: {run:?}")
        };
        assert_eq!(bytecode_branch_poll_count(), 1);
        assert_eq!(resume.pc, 0);
        assert_eq!(resume.cause, Some(DeoptCause::DepthLimit));
        assert!(resume.inlined.is_none());
        assert_eq!(
            resume.stack.as_slice(),
            &[Value::make_int(iterations - 255)]
        );
        assert_eq!(counter.cons_car(), Value::make_int(255));
        assert_eq!(ev.jit_root_stack_top, 0);
        if iterations > 255 {
            ev.set_variable("max-lisp-eval-depth", Value::make_int(1000));
        }
        assert_eq!(
            compile::resumed_chain::resume_deopt(&mut ev, &f, Value::NIL, &leaf, *resume).unwrap(),
            Value::make_int(0)
        );
        assert_eq!(counter.cons_car(), Value::make_int(iterations));
        assert_eq!(ev.depth, 100);
        // At the final poll the unchanged lower limit must still permit the
        // empty tail to return without manufacturing another callback entry.
        if iterations == 255 {
            assert_eq!(ev.max_depth, 100);
        }
    }
}
