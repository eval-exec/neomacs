use super::{FixedMin2, NativeFn, NoEvalPolicy, SubrArity, SubrSpec, no_eval_policy};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::Value;
use crate::tagged::header::SubrFn;

mod subrs;
mod target_filtered;

fn zero(_ctx: &mut Context) -> crate::emacs_core::error::EvalResult {
    Ok(Value::NIL)
}

fn two(_ctx: &mut Context, left: Value, _right: Value) -> crate::emacs_core::error::EvalResult {
    Ok(left)
}

fn vector(_ctx: &mut Context, arguments: Vec<Value>) -> crate::emacs_core::error::EvalResult {
    Ok(arguments.into_iter().next().unwrap_or(Value::NIL))
}

const OPTIONAL_TWO_SLOT: SubrSpec = SubrSpec::fixed2("test-optional-two-slot", two, FixedMin2::One);

#[test]
fn compiled_subr_batch_is_the_executable_declaration_catalog() {
    assert_eq!(
        subrs::SUBRS.owner(),
        "neovm_core::emacs_core::subr::tests::subrs"
    );
    assert!(subrs::SUBRS.source_file().ends_with("tests/subrs.rs"));
    assert_eq!(subrs::SUBRS.specs().len(), 1);

    let mut ctx = Context::new();
    subrs::register_subrs(&mut ctx);
    let value = ctx
        .eval_str("(test-batch-zero)")
        .expect("the catalog should install its declaration");
    assert!(value.is_nil());
}

#[test]
fn target_filtered_batch_can_represent_no_subrs_on_this_target() {
    assert!(target_filtered::SUBRS.specs().is_empty());

    let mut ctx = Context::new();
    target_filtered::register_subrs(&mut ctx);
}

#[test]
fn vector_abi_does_not_imply_unbounded_lisp_arity() {
    let spec = SubrSpec::new(
        "test-fixed-vector",
        NativeFn::ContextVec(vector),
        SubrArity::new(1, Some(2)),
    );

    assert!(matches!(spec.function(), Some(SubrFn::Many(_))));
    assert_eq!(spec.arity(), SubrArity::new(1, Some(2)));
}

#[test]
fn fixed_native_function_derives_its_maximum_arity() {
    let spec = OPTIONAL_TWO_SLOT;

    assert!(matches!(spec.function(), Some(SubrFn::A2(_))));
    assert_eq!(spec.arity(), SubrArity::new(1, Some(2)));
}

#[test]
fn registering_a_spec_installs_its_declared_metadata_and_function() {
    let mut ctx = Context::new();
    ctx.register_subr(SubrSpec::fixed0("test-zero", zero));

    let value = ctx
        .eval_str("(list (subr-arity (symbol-function 'test-zero)) (test-zero))")
        .expect("registered subr should be callable from Lisp");

    assert_eq!(format!("{value}"), "((0 . 0) nil)");
}

#[test]
fn registered_spec_is_authoritative_even_for_a_known_compatibility_name() {
    let mut ctx = Context::new();
    ctx.register_subr(SubrSpec::fixed0("message", zero));

    let arity = ctx
        .eval_str("(subr-arity (symbol-function 'message))")
        .expect("subr-arity should observe the descriptor");

    assert_eq!(format!("{arity}"), "(0 . 0)");
}

#[test]
fn registering_a_spec_replaces_its_previous_no_eval_policy() {
    let mut ctx = Context::new();
    let name = "test-authoritative-no-eval-policy";
    ctx.register_subr(SubrSpec::fixed0(name, zero).requires_eval_state());
    assert_eq!(
        no_eval_policy(intern(name)),
        NoEvalPolicy::RequiresEvalState
    );

    ctx.register_subr(SubrSpec::fixed0(name, zero));
    assert_eq!(no_eval_policy(intern(name)), NoEvalPolicy::Native);
}
