//! Native static-closure frame regressions. Admission and source provenance
//! have separate front tests; these require emitted chains and inspect the
//! precise snapshot before transferring it to Tier-0.

use super::*;
use crate::emacs_core::error::{Flow, FlowRef, FlowResultExt as _};
use crate::emacs_core::eval::{Context, push_scratch_gc_root};
use crate::emacs_core::jit::compile::{self, CompiledLeaf, Inline2Mode, NativeRun};
use crate::emacs_core::jit::reopt::DeoptCause;
use crate::emacs_core::value::LambdaParams;

fn lexical(required: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..required)
            .map(|i| crate::emacs_core::intern::intern(&format!("closure-run-arg-{i}")))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(32);
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

struct Knobs;
impl Knobs {
    fn enter(mode: Inline2Mode) -> Self {
        force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(mode));
        compile::force_profit_gate_for_test(false);
        compile::force_deopt_for_test(false);
        Self
    }
}
impl Drop for Knobs {
    fn drop(&mut self) {
        force_inline_for_test(None);
        compile::force_inline2_for_test(None);
    }
}

fn native(ev: &mut Context, f: &ByteCodeFunction) -> CompiledLeaf {
    let _ = ev.debug_on_next_call_is_armed();
    compile::compile_bytecode_function_with(f, Some(&ev.obarray)).expect("native body")
}

fn finish(
    ev: &mut Context,
    f: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    run: NativeRun,
) -> Result<Value, Flow> {
    match run {
        NativeRun::Ok(bits) => Ok(Value::from_bits(bits)),
        NativeRun::DeoptAt(resume) => {
            compile::resumed_chain::resume_deopt(ev, f, Value::NIL, leaf, *resume)
        }
        other => panic!("unexpected native outcome: {other:?}"),
    }
}

fn capturing_caller(proto: Value) -> ByteCodeFunction {
    lexical(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Constant(2),
            Op::Call(1),
            Op::Return,
        ],
        vec![Value::symbol("make-closure"), proto, Value::make_int(0)],
    )
}

#[test]
fn inline_constant_identity_body_requires_context_for_entry_guards() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    let mut ev = Context::new();
    let proto = Value::make_bytecode(lexical(1, vec![Op::StackRef(0), Op::Return], vec![]));
    let f = lexical(
        1,
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![proto],
    );
    let leaf = native(&mut ev, &f);
    assert!(
        leaf.chains.is_empty(),
        "identity body has no mid-body exits"
    );
    assert!(leaf.needs_vmctx, "the call-entry guards still read Context");
    let run = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(17)]);
    assert_eq!(
        finish(&mut ev, &f, &leaf, run).unwrap(),
        Value::make_int(17)
    );
}

#[test]
fn inline_closure_reads_the_executing_capture_after_cell_mutation() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    let mut ev = Context::new();
    let proto = Value::make_bytecode(lexical(
        1,
        vec![
            Op::Constant(0),
            Op::Car,
            Op::StackRef(1),
            Op::Add,
            Op::Return,
        ],
        vec![Value::make_int(0)],
    ));
    let f = lexical(
        2,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(3),
            Op::Call(2),
            Op::StackRef(2),
            Op::Constant(2),
            Op::Setcar,
            Op::Pop,
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![Value::symbol("make-closure"), proto, Value::make_int(9)],
    );
    let cell = Value::cons(Value::make_int(3), Value::NIL);
    push_scratch_gc_root(cell);
    let leaf = native(&mut ev, &f);
    assert!(!leaf.chains.is_empty(), "closure body was inlined");
    let run = leaf.call(
        &mut ev as *mut Context as *mut u8,
        &[cell, Value::make_int(4)],
    );
    assert_eq!(
        finish(&mut ev, &f, &leaf, run).unwrap(),
        Value::make_int(13)
    );
    assert_eq!(cell.cons_car(), Value::make_int(9));
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn inline_closure_chain_retains_assigned_argument_and_committed_store_once() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    let mut ev = Context::new();
    let assigned = Value::symbol("closure-assigned-not-a-number");
    let proto = Value::make_bytecode(lexical(
        1,
        vec![
            Op::Constant(0),
            Op::Dup,
            Op::Car,
            Op::Add1,
            Op::Setcar,
            Op::Pop,
            Op::Constant(1),
            Op::StackSet(1),
            Op::StackRef(0),
            Op::Add1,
            Op::Return,
        ],
        vec![Value::NIL, assigned],
    ));
    let f = lexical(
        2,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(3),
            Op::Call(2),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![Value::symbol("make-closure"), proto],
    );
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    push_scratch_gc_root(cell);
    let leaf = native(&mut ev, &f);
    let run = leaf.call(
        &mut ev as *mut Context as *mut u8,
        &[cell, Value::make_int(8)],
    );
    let NativeRun::DeoptAt(resume) = run else {
        panic!("inner guard: {run:?}")
    };
    assert_eq!(resume.pc, 5);
    let frames = &resume.inlined.as_ref().expect("closure chain").frames;
    assert_eq!(frames.len(), 1);
    assert_eq!(frames[0].pc, 9, "resume after the committed cons write");
    assert_eq!(frames[0].stack, vec![assigned, assigned]);
    assert_eq!(cell.cons_car(), Value::make_int(1));
    let flow = compile::resumed_chain::resume_deopt(&mut ev, &f, Value::NIL, &leaf, *resume);
    assert!(
        if matches!(flow.kinded_ref(), Err(FlowRef::Signal(sig)) if sig.symbol_name() == "wrong-type-argument")
        {
            drop(flow);
            true
        } else {
            false
        }
    );
    assert_eq!(cell.cons_car(), Value::make_int(1), "no call replay");
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn inline_closure_source_guard_returns_to_the_original_call() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    let mut ev = Context::new();
    let proto = Value::make_bytecode(lexical(
        1,
        vec![
            Op::StackRef(0),
            Op::Add1,
            Op::Pop,
            Op::Constant(0),
            Op::Return,
        ],
        vec![Value::NIL],
    ));
    let f = capturing_caller(proto);
    let leaf = native(&mut ev, &f);
    assert!(!leaf.chains.is_empty(), "closure body was inlined");
    let replacement = Value::make_bytecode(lexical(
        1,
        vec![Op::Constant(0), Op::Return],
        vec![Value::make_int(77)],
    ));
    let maker = Value::make_bytecode(lexical(
        2,
        vec![Op::Constant(0), Op::Return],
        vec![replacement],
    ));
    ev.set_function("make-closure", maker);
    let run = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(3)]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("source guard: {run:?}")
    };
    assert_eq!(resume.pc, 5);
    assert_eq!(resume.cause, Some(DeoptCause::InlineIdentity));
    assert!(resume.inlined.is_none());
    assert_eq!(
        resume.stack,
        vec![Value::make_int(3), replacement, Value::make_int(0)]
    );
    assert_eq!(
        compile::resumed_chain::resume_deopt(&mut ev, &f, Value::NIL, &leaf, *resume).unwrap(),
        Value::make_int(77)
    );
}

#[test]
fn inline_closure_entry_attention_and_depth_leave_before_the_body() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    for cause in [DeoptCause::InlineAttention, DeoptCause::DepthLimit] {
        let mut ev = Context::new();
        let cell = Value::cons(Value::make_int(0), Value::NIL);
        let callee = Value::make_bytecode(lexical(
            1,
            vec![Op::Constant(0), Op::Constant(1), Op::Setcar, Op::Return],
            vec![cell, Value::make_int(1)],
        ));
        let f = lexical(
            0,
            vec![Op::Constant(0), Op::Nil, Op::Call(1), Op::Return],
            vec![callee],
        );
        let leaf = native(&mut ev, &f);
        assert!(!leaf.chains.is_empty());
        match cause {
            DeoptCause::InlineAttention => ev.set_variable("throw-on-input", Value::T),
            DeoptCause::DepthLimit => ev.depth = ev.max_depth,
            _ => unreachable!(),
        }
        let run = leaf.call(&mut ev as *mut Context as *mut u8, &[]);
        let NativeRun::DeoptAt(resume) = run else {
            panic!("entry guard: {run:?}")
        };
        assert_eq!(resume.pc, 2);
        assert_eq!(resume.cause, Some(cause));
        assert!(resume.inlined.is_none(), "activation has not entered");
        assert_eq!(cell.cons_car(), Value::make_int(0));
    }
}

#[test]
fn inline_closure_forced_entry_deopt_executes_the_store_once_on_resume() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    compile::force_deopt_for_test(true);
    let mut ev = Context::new();
    let cell = Value::cons(Value::make_int(0), Value::NIL);
    push_scratch_gc_root(cell);
    let proto = Value::make_bytecode(lexical(
        1,
        vec![
            Op::Constant(0),
            Op::Dup,
            Op::Car,
            Op::Add1,
            Op::Setcar,
            Op::Return,
        ],
        vec![Value::NIL],
    ));
    let f = capturing_caller(proto);
    let leaf = native(&mut ev, &f);
    let run = leaf.call(&mut ev as *mut Context as *mut u8, &[cell]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("forced entry guard: {run:?}")
    };
    assert_eq!(resume.pc, 5);
    assert_eq!(resume.cause, Some(DeoptCause::InlineIdentity));
    assert!(resume.inlined.is_none());
    assert_eq!(cell.cons_car(), Value::make_int(0));
    assert_eq!(
        compile::resumed_chain::resume_deopt(&mut ev, &f, Value::NIL, &leaf, *resume).unwrap(),
        Value::make_int(1)
    );
    assert_eq!(cell.cons_car(), Value::make_int(1));
    compile::force_deopt_for_test(false);
}

#[test]
fn inline_closure_chain_roots_outer_residual_across_resumed_gc() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    let mut ev = crate::test_utils::runtime_startup_context();
    let callee = Value::make_bytecode(lexical(
        1,
        vec![Op::StackRef(0), Op::Add1, Op::Return],
        vec![],
    ));
    let f = lexical(
        1,
        vec![
            Op::Constant(0),
            Op::Nil,
            Op::Cons,
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(1),
            Op::Pop,
            Op::Constant(2),
            Op::Call(0),
            Op::Pop,
            Op::Return,
        ],
        vec![
            Value::make_int(77),
            callee,
            Value::symbol("garbage-collect"),
        ],
    );
    let caller_value = Value::make_bytecode(f.clone());
    push_scratch_gc_root(caller_value);
    let float = Value::make_float(1.5);
    push_scratch_gc_root(float);
    let leaf = native(&mut ev, &f);
    let run = leaf.call(&mut ev as *mut Context as *mut u8, &[float]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("float guard: {run:?}")
    };
    assert_eq!(resume.inlined.as_ref().expect("chain").frames[0].pc, 1);
    let residual = resume.stack[1];
    // This newly allocated cons has no scratch root: the suspended physical
    // operand stack must keep it live while the child and explicit GC run.
    ev.gc_stress = true;
    let result = compile::resumed_chain::resume_deopt(&mut ev, &f, caller_value, &leaf, *resume);
    ev.gc_stress = false;
    let result = result.unwrap();
    assert_eq!(result.bits(), residual.bits());
    assert_eq!(result.cons_car(), Value::make_int(77));
    assert!(result.cons_cdr().is_nil());
    assert_eq!(ev.jit_root_stack_top, 0);
}

#[test]
fn inline_closure_preserves_capture_cons_and_float_identity_and_off_path() {
    for mode in [Inline2Mode::Closure, Inline2Mode::Off] {
        let _knobs = Knobs::enter(mode);
        let mut ev = Context::new();
        let proto = Value::make_bytecode(lexical(
            1,
            vec![
                Op::StackRef(0),
                Op::Add1,
                Op::Pop,
                Op::Constant(0),
                Op::Return,
            ],
            vec![Value::make_int(0)],
        ));
        let f = capturing_caller(proto);
        let leaf = native(&mut ev, &f);
        assert_eq!(leaf.chains.is_empty(), mode == Inline2Mode::Off);
        for capture in [
            Value::cons(Value::make_int(5), Value::NIL),
            Value::make_float(2.5),
        ] {
            push_scratch_gc_root(capture);
            let run = leaf.call(&mut ev as *mut Context as *mut u8, &[capture]);
            assert_eq!(
                finish(&mut ev, &f, &leaf, run).unwrap().bits(),
                capture.bits(),
                "{mode:?}"
            );
        }
    }
}

#[test]
fn inline_closure_wider_same_source_prefix_returns_to_the_original_call() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    let mut ev = Context::new();
    let proto = Value::make_bytecode(lexical(
        1,
        vec![
            Op::StackRef(0),
            Op::Add1,
            Op::Pop,
            Op::Constant(1),
            Op::Return,
        ],
        vec![Value::NIL, Value::make_int(2)],
    ));
    push_scratch_gc_root(proto);
    let f = capturing_caller(proto);
    let leaf = native(&mut ev, &f);
    let wide = crate::emacs_core::builtins::builtin_make_closure(&[
        proto,
        Value::make_int(3),
        Value::make_int(99),
    ])
    .expect("a same-source closure with two patched slots");
    push_scratch_gc_root(wide);
    assert_eq!(
        compile::jit_layout::runtime_identity_word(
            proto.get_bytecode_data().unwrap().jit_runtime()
        ),
        compile::jit_layout::runtime_identity_word(wide.get_bytecode_data().unwrap().jit_runtime()),
    );
    assert_eq!(
        proto
            .get_bytecode_data()
            .unwrap()
            .jit_runtime()
            .patched_prefix(),
        2
    );
    ev.set_function(
        "make-closure",
        Value::make_bytecode(lexical(2, vec![Op::Constant(0), Op::Return], vec![wide])),
    );
    let run = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(3)]);
    let NativeRun::DeoptAt(resume) = run else {
        panic!("prefix bound: {run:?}")
    };
    assert_eq!(resume.pc, 5);
    assert_eq!(resume.cause, Some(DeoptCause::InlineIdentity));
    assert!(resume.inlined.is_none());
    assert_eq!(
        resume.stack,
        vec![Value::make_int(3), wide, Value::make_int(0)]
    );
    assert_eq!(
        compile::resumed_chain::resume_deopt(&mut ev, &f, Value::NIL, &leaf, *resume).unwrap(),
        Value::make_int(99),
        "the old baked tail value 2 must not be used",
    );
}
