//! Native HOF loop allocation uses admission without widening source policy.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::compile::{self, CompileRequest, Inline2Mode};
use crate::emacs_core::jit::inline::{FusedScope, force_inline_for_test, fuse_calls_v2};
use crate::emacs_core::value::LambdaParams;
use std::rc::Rc;

struct Knobs;

impl Knobs {
    fn enter(mode: Inline2Mode) -> Self {
        force_inline_for_test(Some(true));
        compile::force_inline2_for_test(Some(mode));
        compile::force_gate_relax_for_test(false);
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

fn lexical(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams::simple(vec![intern("hof-regalloc-arg")]));
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

fn caller(extra_call: bool) -> ByteCodeFunction {
    let callback = Value::make_bytecode(lexical(
        vec![Op::Constant(0), Op::StackRef(1), Op::Add, Op::Return],
        vec![Value::symbol("V0")],
    ));
    let mut ops = vec![
        Op::Constant(0),
        Op::Constant(1),
        Op::Constant(2),
        Op::Constant(3),
        Op::Call(2),
        Op::StackRef(2),
        Op::Call(2),
    ];
    if extra_call {
        ops.extend([Op::Pop, Op::Constant(4), Op::StackRef(1), Op::Call(1)]);
    }
    ops.push(Op::Return);
    lexical(
        ops,
        vec![
            Value::symbol("mapcar"),
            Value::symbol("make-closure"),
            callback,
            Value::make_int(9),
            Value::symbol("identity"),
        ],
    )
}

fn fused(f: &ByteCodeFunction) -> Rc<inline::FusedBody> {
    Rc::new(
        fuse_calls_v2(
            f.executable_ops(),
            &f.constants,
            None,
            1,
            &vec![NumericFeedback::FixnumOnly; f.executable_ops().len()],
        )
        .expect("in-unit map closure is admitted"),
    )
}

fn compile(f: &ByteCodeFunction, ctx: &Context) -> compile::CompiledLeaf {
    compile::compile_bytecode_function_requested(
        f,
        Some(&ctx.obarray),
        CompileRequest {
            regalloc: RegallocPolicy::Auto,
            bypass_profit_gate: true,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier: crate::emacs_core::jit::tier2::CompileTier::Plain,
        },
    )
    .unwrap()
}

#[test]
fn inline_hof_regalloc_selector_preserves_both_forced_choices() {
    assert_eq!(
        select_hof_allocator(false, None),
        Some(RegallocChoice::Full)
    );
    assert_eq!(
        select_hof_allocator(false, Some(RegallocChoice::Fast)),
        Some(RegallocChoice::Fast)
    );
    assert_eq!(
        select_hof_allocator(false, Some(RegallocChoice::Full)),
        Some(RegallocChoice::Full)
    );
}

#[test]
fn inline_hof_regalloc_off_keeps_the_source_allocator() {
    let _knobs = Knobs::enter(Inline2Mode::Off);
    let ctx = Context::new();
    let f = caller(false);
    assert!(for_admitted_hof(None, f.executable_ops(), &f.constants).is_none());
    let leaf = compile(&f, &ctx);
    assert_eq!(
        leaf.regalloc,
        forced_regalloc().unwrap_or(RegallocChoice::Fast)
    );
    assert!(leaf.call_heavy);
    assert!(leaf.chains.is_empty());
}

#[test]
fn inline_hof_regalloc_unadmitted_callback_keeps_the_source_allocator() {
    let _knobs = Knobs::enter(Inline2Mode::Hof);
    let ctx = Context::new();
    let f = lexical(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::symbol("mapcar"), Value::symbol("identity")],
    );
    assert!(
        fuse_calls_v2(
            f.executable_ops(),
            &f.constants,
            None,
            1,
            &vec![NumericFeedback::FixnumOnly; f.executable_ops().len()],
        )
        .is_none()
    );
    let leaf = compile(&f, &ctx);
    assert_eq!(
        leaf.regalloc,
        forced_regalloc().unwrap_or(RegallocChoice::Fast)
    );
    assert!(leaf.call_heavy);
    assert!(leaf.chains.is_empty());
}

#[test]
fn inline_hof_regalloc_empty_hof_table_keeps_ordinary_inline_policy() {
    let _knobs = Knobs::enter(Inline2Mode::Closure);
    let callback =
        Value::make_bytecode(lexical(vec![Op::StackRef(0), Op::Add1, Op::Return], vec![]));
    let f = lexical(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![callback],
    );
    let body = fused(&f);
    assert!(!body.regions.is_empty());
    assert!(body.v2.as_ref().unwrap().hof_at.is_empty());
    let _scope = FusedScope::enter(Rc::clone(&body));
    assert!(!body_is_call_heavy(&body.ops, &body.constants));
    assert!(for_admitted_hof(Some(&body), &body.ops, &body.constants).is_none());
}

#[test]
fn inline_hof_regalloc_remaining_calls_keep_the_source_allocator() {
    let _knobs = Knobs::enter(Inline2Mode::Hof);
    let ctx = Context::new();
    let f = caller(true);
    let body = fused(&f);
    assert!(body.admitted_hof_at(6).is_some());
    let _scope = FusedScope::enter(Rc::clone(&body));
    assert!(body_is_call_heavy(&body.ops, &body.constants));
    assert!(for_admitted_hof(Some(&body), &body.ops, &body.constants).is_none());
    let leaf = compile(&f, &ctx);
    assert_eq!(
        leaf.regalloc,
        forced_regalloc().unwrap_or(RegallocChoice::Fast)
    );
    assert!(leaf.call_heavy);
}

#[test]
fn inline_hof_regalloc_admitted_native_loop_uses_full_and_keeps_source_metadata() {
    let _knobs = Knobs::enter(Inline2Mode::Hof);
    let ctx = Context::new();
    let f = caller(false);
    assert!(!compile::has_back_edge(f.executable_ops()));
    assert!(body_is_call_heavy(f.executable_ops(), &f.constants));
    let body = fused(&f);
    assert!(body.admitted_hof_at(6).is_some());
    {
        let _scope = FusedScope::enter(Rc::clone(&body));
        assert!(!body_is_call_heavy(&body.ops, &body.constants));
        assert_eq!(
            for_admitted_hof(Some(&body), &body.ops, &body.constants),
            Some(forced_regalloc().unwrap_or(RegallocChoice::Full))
        );
    }
    let leaf = compile(&f, &ctx);
    assert_eq!(
        leaf.regalloc,
        forced_regalloc().unwrap_or(RegallocChoice::Full)
    );
    assert!(
        leaf.call_heavy,
        "source classification survives allocator promotion"
    );
    assert!(!leaf.chains.is_empty(), "native HOF body was lowered");
}
