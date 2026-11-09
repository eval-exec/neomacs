//! Native static-closure frame regressions. Admission and source provenance
//! have separate front tests; these require emitted chains and inspect the
//! precise snapshot before transferring it to Tier-0.

use crate::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::{Context, push_scratch_gc_root};
use crate::emacs_core::jit::compile::{self, CompiledLeaf, NativeRun};
use neovm_eval::test_support::authority::{
    TestInline2Mode, TestOptMode, deopt_cause_name_for_test, deopt_inlined_frame_count_for_test,
    deopt_inlined_frame_pc_for_test, deopt_inlined_frame_stack_for_test,
    deopt_inlined_is_none_for_test, force_deopt_for_test, force_inline_for_test,
    force_inline2_token_for_test, force_profit_gate_for_test, jit_mark_hot_for_test,
    jit_patched_prefix_for_test, jit_root_stack_top_for_test, leaf_chains_empty_for_test,
    leaf_needs_vmctx_for_test, opt_mode_token_scope_for_test, resume_deopt_for_test,
    seal_hand_assembled_ops_for_test,
};
use neovm_value::error::{Flow, FlowRef, FlowResultExt as _};
use neovm_value::value::LambdaParams;
use neovm_value::value::Value;

fn lexical(required: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..required)
            .map(|i| neovm_value::intern::intern(&format!("closure-run-arg-{i}")))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(32);
    seal_hand_assembled_ops_for_test(&mut f);
    jit_mark_hot_for_test(&f);
    f
}

/// Threading: scalar inline configuration belongs to this fixture's compiler
/// thread; no Lisp state or mutator cache is stored. The returned backend guard
/// restores the enclosing mode while keeping the legacy v2 producer selected.
struct Knobs;
impl Knobs {
    #[doc(hidden)]
    pub fn enter(mode: TestInline2Mode) -> (Self, impl Drop) {
        let backend = opt_mode_token_scope_for_test(TestOptMode::Legacy);
        force_inline_for_test(Some(true));
        force_inline2_token_for_test(Some(mode));
        force_profit_gate_for_test(false);
        force_deopt_for_test(false);
        (Self, backend)
    }
}
impl Drop for Knobs {
    fn drop(&mut self) {
        force_inline_for_test(None);
        force_inline2_token_for_test(None);
    }
}

fn native(ev: &mut Context, f: &ByteCodeFunction) -> CompiledLeaf {
    let _ = ev.debug_on_next_call_is_armed();
    compile::compile_bytecode_function_with(f, Some(ev.obarray())).expect("native body")
}

fn finish(
    ev: &mut Context,
    f: &ByteCodeFunction,
    leaf: &CompiledLeaf,
    run: NativeRun,
) -> Result<Value, Flow> {
    match run {
        NativeRun::Ok(bits) => Ok(unsafe {
            /* SAFETY: NativeRun::Ok is a real Value's tagged bits */
            Value::from_bits(bits)
        }),
        NativeRun::DeoptAt(resume) => resume_deopt_for_test(ev, f, Value::NIL, leaf, *resume),
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
    let _knobs = Knobs::enter(TestInline2Mode::Closure);
    let mut ev = crate::test_utils::new_context();
    let proto = Value::make_bytecode(lexical(1, vec![Op::StackRef(0), Op::Return], vec![]));
    let f = lexical(
        1,
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![proto],
    );
    let leaf = native(&mut ev, &f);
    assert!(
        leaf_chains_empty_for_test(&leaf),
        "identity body has no mid-body exits"
    );
    assert!(
        leaf_needs_vmctx_for_test(&leaf),
        "the call-entry guards still read Context"
    );
    let run = leaf.call(&mut ev as *mut Context as *mut u8, &[Value::make_int(17)]);
    assert_eq!(
        finish(&mut ev, &f, &leaf, run).unwrap(),
        Value::make_int(17)
    );
}

#[test]
fn inline_closure_reads_the_executing_capture_after_cell_mutation() {
    let _knobs = Knobs::enter(TestInline2Mode::Closure);
    let mut ev = crate::test_utils::new_context();
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
    assert!(
        !leaf_chains_empty_for_test(&leaf),
        "closure body was inlined"
    );
    let run = leaf.call(
        &mut ev as *mut Context as *mut u8,
        &[cell, Value::make_int(4)],
    );
    assert_eq!(
        finish(&mut ev, &f, &leaf, run).unwrap(),
        Value::make_int(13)
    );
    assert_eq!(cell.cons_car(), Value::make_int(9));
    assert_eq!(jit_root_stack_top_for_test(&ev), 0);
}

#[test]
fn inline_closure_chain_retains_assigned_argument_and_committed_store_once() {
    let _knobs = Knobs::enter(TestInline2Mode::Closure);
    let mut ev = crate::test_utils::new_context();
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
    assert_eq!(deopt_inlined_frame_count_for_test(&resume), Some(1));
    assert_eq!(
        deopt_inlined_frame_pc_for_test(&resume, 0),
        9,
        "resume after the committed cons write"
    );
    assert_eq!(
        deopt_inlined_frame_stack_for_test(&resume, 0),
        vec![assigned, assigned]
    );
    assert_eq!(cell.cons_car(), Value::make_int(1));
    let flow = resume_deopt_for_test(&mut ev, &f, Value::NIL, &leaf, *resume);
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
    assert_eq!(jit_root_stack_top_for_test(&ev), 0);
}

#[test]
fn inline_closure_source_guard_returns_to_the_original_call() {
    let _knobs = Knobs::enter(TestInline2Mode::Closure);
    let mut ev = crate::test_utils::new_context();
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
    assert!(
        !leaf_chains_empty_for_test(&leaf),
        "closure body was inlined"
    );
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
    assert_eq!(deopt_cause_name_for_test(&resume), Some("InlineIdentity"));
    assert!(deopt_inlined_is_none_for_test(&resume));
    assert_eq!(
        resume.stack,
        vec![Value::make_int(3), replacement, Value::make_int(0)]
    );
    assert_eq!(
        resume_deopt_for_test(&mut ev, &f, Value::NIL, &leaf, *resume).unwrap(),
        Value::make_int(77)
    );
}

#[test]
fn inline_closure_entry_attention_and_depth_leave_before_the_body() {
    let _knobs = Knobs::enter(TestInline2Mode::Closure);
    for cause in ["InlineAttention", "DepthLimit"] {
        let mut ev = crate::test_utils::new_context();
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
        assert!(!leaf_chains_empty_for_test(&leaf));
        match cause {
            "InlineAttention" => ev.set_variable("throw-on-input", Value::T),
            "DepthLimit" => {
                let depth = ev.max_depth_for_test();
                neovm_eval::test_support::authority::set_depth_for_test(&mut ev, depth);
            }
            _ => unreachable!(),
        }
        let run = leaf.call(&mut ev as *mut Context as *mut u8, &[]);
        let NativeRun::DeoptAt(resume) = run else {
            panic!("entry guard: {run:?}")
        };
        assert_eq!(resume.pc, 2);
        assert_eq!(deopt_cause_name_for_test(&resume), Some(cause));
        assert!(
            deopt_inlined_is_none_for_test(&resume),
            "activation has not entered"
        );
        assert_eq!(cell.cons_car(), Value::make_int(0));
    }
}

#[test]
fn inline_closure_forced_entry_deopt_executes_the_store_once_on_resume() {
    let _knobs = Knobs::enter(TestInline2Mode::Closure);
    force_deopt_for_test(true);
    let mut ev = crate::test_utils::new_context();
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
    assert_eq!(deopt_cause_name_for_test(&resume), Some("InlineIdentity"));
    assert!(deopt_inlined_is_none_for_test(&resume));
    assert_eq!(cell.cons_car(), Value::make_int(0));
    assert_eq!(
        resume_deopt_for_test(&mut ev, &f, Value::NIL, &leaf, *resume).unwrap(),
        Value::make_int(1)
    );
    assert_eq!(cell.cons_car(), Value::make_int(1));
    force_deopt_for_test(false);
}

#[test]
fn inline_closure_chain_roots_outer_residual_across_resumed_gc() {
    let _knobs = Knobs::enter(TestInline2Mode::Closure);
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
    assert_eq!(deopt_inlined_frame_pc_for_test(&resume, 0), 1);
    let residual = resume.stack[1];
    // This newly allocated cons has no scratch root: the suspended physical
    // operand stack must keep it live while the child and explicit GC run.
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut ev, true);
    let result = resume_deopt_for_test(&mut ev, &f, caller_value, &leaf, *resume);
    neovm_eval::test_support::authority::set_gc_stress_for_test(&mut ev, false);
    let result = result.unwrap();
    assert_eq!(result.bits(), residual.bits());
    assert_eq!(result.cons_car(), Value::make_int(77));
    assert!(result.cons_cdr().is_nil());
    assert_eq!(jit_root_stack_top_for_test(&ev), 0);
}

#[test]
fn inline_closure_preserves_capture_cons_and_float_identity_and_off_path() {
    for mode in [TestInline2Mode::Closure, TestInline2Mode::Off] {
        let _knobs = Knobs::enter(mode);
        let mut ev = crate::test_utils::new_context();
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
        assert_eq!(
            leaf_chains_empty_for_test(&leaf),
            mode == TestInline2Mode::Off
        );
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
    let _knobs = Knobs::enter(TestInline2Mode::Closure);
    let mut ev = crate::test_utils::new_context();
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
        neovm_value::value::jit_runtime::runtime_shared_identity_word(
            proto
                .get_bytecode_data()
                .unwrap()
                .runtime()
                .as_ref()
                .unwrap()
                .shared()
        ),
        neovm_value::value::jit_runtime::runtime_shared_identity_word(
            wide.get_bytecode_data()
                .unwrap()
                .runtime()
                .as_ref()
                .unwrap()
                .shared()
        ),
    );
    assert_eq!(
        jit_patched_prefix_for_test(proto.get_bytecode_data().unwrap()),
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
    assert_eq!(deopt_cause_name_for_test(&resume), Some("InlineIdentity"));
    assert!(deopt_inlined_is_none_for_test(&resume));
    assert_eq!(
        resume.stack,
        vec![Value::make_int(3), wide, Value::make_int(0)]
    );
    assert_eq!(
        resume_deopt_for_test(&mut ev, &f, Value::NIL, &leaf, *resume).unwrap(),
        Value::make_int(99),
        "the old baked tail value 2 must not be used",
    );
}
