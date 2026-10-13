//! List HOF front admission, original/fused call mapping, and profitability
//! credit use the same callback metadata the native loop consumes.

use super::*;
use crate::emacs_core::jit::compile::{self, Inline2Mode};
use crate::emacs_core::value::LambdaParams;

fn lexical(required: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..required)
            .map(|i| crate::emacs_core::intern::intern(&format!("v2-hof-arg-{i}")))
            .collect(),
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = crate::emacs_core::bytecode::StackDepth::for_test(16);
    f.seal_hand_assembled_ops_for_test();
    f.jit_runtime().set_hot_for_test();
    f
}

fn callback() -> Value {
    Value::make_bytecode(lexical(
        1,
        vec![Op::StackRef(0), Op::Constant(0), Op::Add, Op::Return],
        vec![Value::make_int(2)],
    ))
}

fn map_caller(name: &str, callback: Value) -> ByteCodeFunction {
    lexical(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::symbol(name), callback],
    )
}

fn fuse(f: &ByteCodeFunction, mode: Inline2Mode) -> Option<FusedBody> {
    force_inline_for_test(Some(true));
    compile::force_inline2_for_test(Some(mode));
    let body = fuse_calls_v2(
        f.executable_ops(),
        &f.constants,
        None,
        f.params
            .stack_shape()
            .expect("fixture stack parameters")
            .required(),
        &vec![NumericFeedback::FixnumOnly; f.executable_ops().len()],
    );
    compile::force_inline2_for_test(None);
    force_inline_for_test(None);
    body
}

#[test]
fn inline_v2_hof_only_body_keeps_calls_and_roots_callback_constants() {
    for (name, kind) in [("mapc", HofKind::Mapc), ("mapcar", HofKind::Mapcar)] {
        let callback = callback();
        let f = map_caller(name, callback);
        let body = fuse(&f, Inline2Mode::Hof).expect("list intrinsic candidate");
        assert_eq!(body.ops, f.executable_ops());
        assert_eq!(body.caller_of_fused, (0..5).collect::<Vec<_>>());
        assert_eq!(body.region_of, vec![None; 5]);
        assert!(body.regions.is_empty());
        assert_eq!(
            body.constants,
            vec![f.constants[0], callback, Value::make_int(2), callback]
        );
        let site = body.admitted_hof_at(3).expect("Call2 annotation");
        assert_eq!(site.callback, callback);
        assert_eq!(site.kind, kind);
        assert_eq!(site.call_site_pc, 3);
        assert_eq!(site.const_base, 2);
        assert!(!site.closure);
        assert_eq!(site.prefix, 0);
    }
}

#[test]
fn inline_v2_hof_closure_provenance_survives_a_cfg_join() {
    let proto = Value::make_bytecode(lexical(
        1,
        vec![Op::Constant(0), Op::StackRef(1), Op::Add, Op::Return],
        vec![Value::symbol("V0")],
    ));
    let f = lexical(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Constant(3),
            Op::Call(2),
            Op::StackRef(2),
            Op::GotoIfNil(9),
            Op::Nil,
            Op::Pop,
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![
            Value::symbol("mapcar"),
            Value::symbol("make-closure"),
            proto,
            Value::make_int(9),
        ],
    );
    let body = fuse(&f, Inline2Mode::Hof).expect("HOF mode includes in-unit callback closures");
    let site = body.admitted_hof_at(10).unwrap();
    assert_eq!(site.callback, proto);
    assert_eq!(site.prefix, 1);
    assert!(site.closure);
    assert_eq!(body.ops[4], Op::Call(2), "closure allocation is preserved");
    assert_eq!(site.const_base, 4);
}

#[test]
fn inline_v2_hof_producer_is_scoped_to_hof_and_all_modes() {
    let f = map_caller("mapc", callback());
    for mode in [Inline2Mode::Off, Inline2Mode::Named, Inline2Mode::Closure] {
        assert!(fuse(&f, mode).is_none(), "{mode:?}");
    }
    for mode in [Inline2Mode::Hof, Inline2Mode::All] {
        assert!(fuse(&f, mode).is_some(), "{mode:?}");
    }
}

#[test]
fn inline_v2_hof_rejects_other_hofs_named_callbacks_and_wrong_arity() {
    for name in ["mapcan", "mapconcat", "sort", "maphash", "seq-map"] {
        let f = map_caller(name, callback());
        assert!(fuse(&f, Inline2Mode::Hof).is_none(), "{name}");
    }
    let named = map_caller("mapcar", Value::symbol("identity"));
    assert!(fuse(&named, Inline2Mode::Hof).is_none());
    let two_args = Value::make_bytecode(lexical(2, vec![Op::StackRef(0), Op::Return], vec![]));
    assert!(fuse(&map_caller("mapcar", two_args), Inline2Mode::Hof).is_none());
}

#[test]
fn inline_v2_hof_call_pc_rebases_after_an_ordinary_splice() {
    let direct = callback();
    let mapped = callback();
    let f = lexical(
        1,
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Pop,
            Op::Constant(1),
            Op::Constant(2),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![direct, Value::symbol("mapcar"), mapped],
    );
    let body = fuse(&f, Inline2Mode::All).unwrap();
    assert_eq!(body.regions.len(), 1);
    let (&fused_pc, site) = body.v2.as_ref().unwrap().hof_at.first_key_value().unwrap();
    assert!(fused_pc > site.call_site_pc);
    assert_eq!(site.call_site_pc, 7);
    assert_eq!(body.ops[fused_pc], Op::Call(2));
    assert_eq!(body.caller_of_fused[fused_pc], 7);
    assert_eq!(site.const_base, 4, "ordinary callee constants come first");
    assert_eq!(body.constants[site.const_base], Value::make_int(2));
}

#[test]
fn inline_v2_hof_profit_credit_uses_only_admitted_call_metadata() {
    let f = map_caller("mapcar", callback());
    let body = std::rc::Rc::new(fuse(&f, Inline2Mode::Hof).unwrap());
    {
        let _scope = FusedScope::enter(body);
        assert!(hof_profit_credit_at(3));
        assert!(!hof_profit_credit_at(0));
        assert!(!hof_profit_credit_at(100));
    }
    assert!(!hof_profit_credit_at(3));
}

#[test]
fn inline_v2_hof_profit_credit_amortizes_a_retained_closure_maker() {
    compile::force_gate_relax_for_test(false);
    let proto = Value::make_bytecode(lexical(
        1,
        vec![Op::StackRef(0), Op::Return],
        vec![Value::make_int(0)],
    ));
    let f = lexical(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Constant(2),
            Op::Constant(3),
            Op::Call(2),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![
            Value::symbol("mapc"),
            Value::symbol("make-closure"),
            proto,
            Value::make_int(1),
        ],
    );
    assert!(compile::body_is_call_heavy(
        f.executable_ops(),
        &f.constants
    ));
    let body = std::rc::Rc::new(fuse(&f, Inline2Mode::Hof).unwrap());
    {
        let _scope = FusedScope::enter(body.clone());
        assert!(!compile::body_is_call_heavy(&body.ops, &body.constants));
    }
    assert!(compile::body_is_call_heavy(&body.ops, &body.constants));
    compile::force_gate_relax_for_test(false);
}

#[test]
fn inline_v2_constant_chain_admission_allows_guarded_stores_above_legacy_size() {
    let mut ops = vec![Op::Constant(0), Op::StackRef(1), Op::Setcar];
    for _ in 0..20 {
        ops.extend([Op::Dup, Op::Pop]);
    }
    ops.push(Op::Return);
    let callee = Value::make_bytecode(lexical(1, ops, vec![Value::NIL]));
    let f = lexical(
        1,
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![callee],
    );
    let feedback = vec![NumericFeedback::FixnumOnly; f.executable_ops().len()];
    assert!(fuse_calls(f.executable_ops(), &f.constants, None, 1, &feedback).is_none());
    let body = fuse(&f, Inline2Mode::Closure).expect("v2 chain resumes past committed stores");
    assert_eq!(
        body.v2.as_ref().unwrap().region_kind[0],
        RegionKind::Constant
    );
}
