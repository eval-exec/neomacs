//! Direct-cell named selection and virtual-only F-I1 admission.
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::jit::compile::{self, Inline2Mode, force_inline2_for_test};
use crate::emacs_core::jit::inline;
use crate::emacs_core::value::LambdaParams;

fn function(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![crate::emacs_core::intern::intern("named-front-x")],
        optional: vec![],
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

fn caller(symbol: Value) -> ByteCodeFunction {
    function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![symbol],
    )
}

fn fuse(ev: &Context, f: &ByteCodeFunction) -> Option<inline::FusedBody> {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            force_inline_for_test(None);
        }
    }
    let _reset = Reset;
    force_inline_for_test(Some(true));
    inline::fuse_named_calls_v2(
        f.executable_ops(),
        &f.constants,
        None,
        1,
        &vec![NumericFeedback::FixnumOnly; f.executable_ops().len()],
        &ev.obarray,
    )
}

#[test]
fn named_front_preserves_symbol_and_roots_observed_object() {
    let mut ev = Context::new();
    let symbol = Value::symbol("named-front-id");
    let target = Value::make_bytecode(function(vec![Op::StackRef(0), Op::Return], vec![]));
    ev.obarray
        .set_symbol_function_id(symbol.as_symbol_id().unwrap(), target);
    let body = fuse(&ev, &caller(symbol)).unwrap();
    assert_eq!(body.constants[0], symbol);
    assert!(body.constants.contains(&target));
    assert_eq!(body.regions[0].callee_bits, target.bits() as u64);
    assert_eq!(
        body.v2.as_ref().unwrap().region_kind[0],
        inline::RegionKind::Named {
            symbol: symbol.as_symbol_id().unwrap()
        }
    );
    assert!(body.v2.as_ref().unwrap().materialize_at.is_empty());
    assert_eq!(body.v2.as_ref().unwrap().exit_regions.len(), 1);
}

#[test]
fn named_front_refuses_symbol_aliases_and_unbound_cells() {
    let mut ev = Context::new();
    let symbol = Value::symbol("named-front-alias");
    assert!(fuse(&ev, &caller(symbol)).is_none());
    ev.obarray
        .set_symbol_function_id(symbol.as_symbol_id().unwrap(), Value::symbol("car"));
    assert!(fuse(&ev, &caller(symbol)).is_none());
}

#[test]
fn named_front_refuses_observing_callees() {
    let mut ev = Context::new();
    let symbol = Value::symbol("named-front-observing");
    for ops in [
        vec![Op::Constant(0), Op::Call(0), Op::Return],
        vec![Op::VarRef(0), Op::Return],
    ] {
        ev.obarray.set_symbol_function_id(
            symbol.as_symbol_id().unwrap(),
            Value::make_bytecode(function(ops, vec![Value::symbol("named-front-observed")])),
        );
        assert!(fuse(&ev, &caller(symbol)).is_none());
    }
}

#[test]
fn named_front_enforces_fused_growth_budget() {
    let mut ev = Context::new();
    let symbol = Value::symbol("named-front-growth");
    let mut ops = vec![Op::StackRef(0)];
    ops.extend(std::iter::repeat_n(Op::Dup, 10));
    ops.extend(std::iter::repeat_n(Op::Pop, 10));
    ops.push(Op::Return);
    ev.obarray.set_symbol_function_id(
        symbol.as_symbol_id().unwrap(),
        Value::make_bytecode(function(ops, vec![])),
    );
    assert!(fuse(&ev, &caller(symbol)).is_none());
}

#[test]
fn named_front_requires_matching_required_arity() {
    let mut ev = Context::new();
    let symbol = Value::symbol("named-front-arity");
    let mut callee = function(vec![Op::StackRef(0), Op::Return], vec![]);
    let mut params = callee
        .params
        .named()
        .expect("named fixture parameters")
        .clone();
    params
        .optional
        .push(crate::emacs_core::intern::intern("named-front-extra"));
    callee.params = params.into();
    ev.obarray
        .set_symbol_function_id(symbol.as_symbol_id().unwrap(), Value::make_bytecode(callee));
    assert!(fuse(&ev, &caller(symbol)).is_none());
}

#[test]
fn named_front_off_compile_has_no_named_chains() {
    let _backend = crate::emacs_core::jit::compile::opt_mode_scope_for_test(
        crate::emacs_core::jit::compile::OptMode::Legacy,
    );
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            force_inline_for_test(None);
            force_inline2_for_test(None);
        }
    }
    let _reset = Reset;
    force_inline_for_test(Some(true));
    force_inline2_for_test(Some(Inline2Mode::Off));
    let mut ev = Context::new();
    let symbol = Value::symbol("named-front-off");
    ev.obarray.set_symbol_function_id(
        symbol.as_symbol_id().unwrap(),
        Value::make_bytecode(function(
            vec![Op::StackRef(0), Op::Add1, Op::Return],
            vec![],
        )),
    );
    let leaf = compile::compile_bytecode_function_with(&caller(symbol), Some(&ev.obarray)).unwrap();
    assert!(leaf.chains.is_empty());
}

#[test]
fn named_front_all_preserves_static_and_hof_sites_in_a_mixed_caller() {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            force_inline2_for_test(None);
        }
    }
    let _reset = Reset;
    force_inline2_for_test(Some(Inline2Mode::All));
    let mut ev = Context::new();
    let symbol = Value::symbol("named-front-mixed");
    let target = Value::make_bytecode(function(vec![Op::StackRef(0), Op::Return], vec![]));
    ev.obarray
        .set_symbol_function_id(symbol.as_symbol_id().unwrap(), target);
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Pop,
            Op::Constant(1),
            Op::StackRef(1),
            Op::Call(1),
            Op::Pop,
            Op::Constant(2),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![symbol, target, Value::symbol("mapcar")],
    );
    let body = fuse(&ev, &f).unwrap();
    let side = body.v2.as_ref().unwrap();
    assert_eq!(side.region_kind.len(), 2);
    assert!(matches!(
        side.region_kind[0],
        inline::RegionKind::Named { .. }
    ));
    assert_eq!(side.region_kind[1], inline::RegionKind::Constant);
    assert_eq!(side.hof_at.len(), 1);
    assert_eq!(side.hof_at.values().next().unwrap().callback, target);
}

#[test]
fn named_front_all_rebases_closure_prefix_after_appended_named_targets() {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            force_inline2_for_test(None);
        }
    }
    let _reset = Reset;
    force_inline2_for_test(Some(Inline2Mode::All));
    let mut ev = Context::new();
    let symbol = Value::symbol("named-front-mixed-capture");
    let target = Value::make_bytecode(function(vec![Op::StackRef(0), Op::Return], vec![]));
    ev.obarray
        .set_symbol_function_id(symbol.as_symbol_id().unwrap(), target);
    let template = Value::make_bytecode(function(
        vec![Op::Constant(0), Op::StackRef(1), Op::Add, Op::Return],
        vec![Value::make_int(99)],
    ));
    let f = function(
        vec![
            Op::Constant(0),
            Op::StackRef(1),
            Op::Call(1),
            Op::Pop,
            Op::Constant(1),
            Op::Constant(2),
            Op::Constant(3),
            Op::Call(2),
            Op::StackRef(1),
            Op::Call(1),
            Op::Return,
        ],
        vec![
            symbol,
            Value::symbol("make-closure"),
            template,
            Value::make_int(8),
        ],
    );
    let body = fuse(&ev, &f).unwrap();
    let side = body.v2.as_ref().unwrap();
    assert_eq!(side.region_kind.len(), 2);
    assert_eq!(
        side.region_kind[1],
        inline::RegionKind::Closure {
            prefix: 1,
            const_base: 5
        }
    );
    assert_eq!(body.constants[5], Value::NIL);
    assert_eq!(body.constants[4], target);
}
