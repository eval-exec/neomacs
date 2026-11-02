//! The default profile's HOF-only front: a call-heavy body whose
//! `mapc`/`mapcar` site has an admitted literal callback compiles with the
//! callback loop inlined; any other body, and every body with
//! `NEOVM_JIT_HOF_INLINE=off`, compiles without a v2 front.

use super::*;
use crate::emacs_core::bytecode::ByteCodeFunctionExt;
use crate::emacs_core::intern::intern;
use crate::emacs_core::jit::compile;
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::LambdaParams;

fn function(arity: usize, ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: (0..arity)
            .map(|i| intern(&format!("hof-front-arg-{i}")))
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

/// `(lambda (x) (1+ x))`: an admitted callback.
fn add1_callback() -> Value {
    Value::make_bytecode(function(
        1,
        vec![Op::StackRef(0), Op::Add1, Op::Return],
        vec![],
    ))
}

/// `(lambda (x) (identity x))`: a callback with a call, never admitted.
fn calling_callback() -> Value {
    Value::make_bytecode(function(
        1,
        vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
        vec![Value::symbol("identity")],
    ))
}

/// `(lambda (l) (mapcar CALLBACK l))`
fn caller(callback: Value) -> ByteCodeFunction {
    function(
        1,
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::StackRef(2),
            Op::Call(2),
            Op::Return,
        ],
        vec![Value::symbol("mapcar"), callback],
    )
}

/// The production front: Opt backend from the default profile, no INLINE2,
/// profit gate on. Threading: scalar compiler overrides on this test thread.
struct Production(Box<dyn std::any::Any>);

impl Production {
    fn enter(hof_inline: bool) -> Self {
        compile::force_inline2_for_test(Some(Inline2Mode::Off));
        compile::force_profit_gate_for_test(true);
        compile::force_hof_inline_for_test(Some(hof_inline));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
        Self(Box::new(compile::opt_mode_scope_for_test(OptMode::Opt)))
    }
}

impl Drop for Production {
    fn drop(&mut self) {
        compile::force_inline2_for_test(None);
        compile::force_hof_inline_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
    }
}

fn inlines_mapcar(leaf: &CompiledLeaf) -> bool {
    leaf.inline_deps().contains(&intern("mapcar"))
}

#[test]
fn default_front_inlines_an_admitted_mapcar_callback() {
    let _production = Production::enter(true);
    let mut ctx = Context::new();
    let callback = add1_callback();
    let source = caller(callback);
    ctx.push_vm_root_frame();
    ctx.push_vm_frame_root(callback);
    let leaf = compile::compile_bytecode_function_with(&source, Some(&ctx.obarray))
        .expect("the HOF-only front makes the call-heavy caller profitable");
    assert!(inlines_mapcar(&leaf));
    let list = Value::list(vec![Value::make_int(1), Value::make_int(41)]);
    ctx.push_vm_frame_root(list);
    let NativeRun::Ok(bits) = leaf.call(&mut ctx as *mut Context as *mut u8, &[list]) else {
        panic!("an all-fixnum list maps natively");
    };
    assert_eq!(print_value(&Value::from_bits(bits)), "(2 42)");
}

#[test]
fn default_front_leaves_a_calling_callback_on_legacy() {
    let _production = Production::enter(true);
    let ctx = Context::new();
    let source = caller(calling_callback());
    match compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)) {
        Ok(leaf) => assert!(!inlines_mapcar(&leaf)),
        Err(error) => assert!(matches!(error, CompileError::NotProfitable), "{error:?}"),
    }
}

#[test]
fn hof_inline_off_keeps_the_legacy_front() {
    let _production = Production::enter(false);
    let ctx = Context::new();
    let source = caller(add1_callback());
    match compile::compile_bytecode_function_with(&source, Some(&ctx.obarray)) {
        Ok(leaf) => assert!(!inlines_mapcar(&leaf)),
        Err(error) => assert!(matches!(error, CompileError::NotProfitable), "{error:?}"),
    }
}
