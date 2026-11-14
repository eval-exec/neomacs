//! Private inline-subr registration fixture for the re-homed core test.
//!
//! Core supplies the real canonical Context factory. The classification
//! state and original assertions stay inside eval, with no new public type.

use crate::emacs_core::eval::Context;
use neovm_value::intern::intern;

pub(crate) fn run_inline_subr_kinds_follow_the_gnu_inline_opcode_shape_fixture(
    make_context: fn() -> Context,
) {
    use crate::emacs_core::eval::{InlineSubrKind, inline_subr};
    crate::test_utils::init_test_tracing();
    let mut eval = make_context();
    let kind = |name: &str| inline_subr(intern(name)).kind;
    assert_eq!(kind("point"), InlineSubrKind::Point);
    assert_eq!(kind("point-min"), InlineSubrKind::PointMin);
    assert_eq!(kind("point-max"), InlineSubrKind::PointMax);
    assert_eq!(kind("current-buffer"), InlineSubrKind::CurrentBuffer);
    assert_eq!(kind("aset"), InlineSubrKind::Generic);
    assert_eq!(kind("fillarray"), InlineSubrKind::Generic);
    for name in [
        "car",
        "char-after",
        "goto-char",
        "widen",
        "bolp",
        "get-char-property",
    ] {
        assert_eq!(kind(name), InlineSubrKind::Direct, "{name}");
    }
    for name in [
        "re-search-forward",
        "put-text-property",
        "maphash",
        "garbage-collect",
        "no-such-subr-xyz",
    ] {
        assert_eq!(kind(name), InlineSubrKind::Generic, "{name}");
    }

    assert_core_identity_matches_canonical_descriptor(&mut eval);
}

fn assert_core_identity_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::misc_pure::IDENTITY_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::value::Value;

    let expected = IDENTITY_SUBR;
    let id = intern("identity");
    let actual = lookup_global_subr_entry(id).expect("core registered identity");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("identity retains the production ContextVec ABI");
    };
    assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
    assert_eq!(expected.name(), "identity");
    assert_eq!(expected.arity(), SubrArity::new(1, Some(1)));
    assert_eq!(
        (actual.min_args, actual.max_args),
        (expected.arity().min(), expected.arity().max())
    );
    assert_eq!(actual.dispatch_kind, expected.dispatch_kind());
    assert!(actual.interactive_spec.is_none());
    assert!(expected.interactive_spec().is_none());
    assert_eq!(no_eval_policy(id), expected.no_eval_policy());
    assert_eq!(no_eval_policy(id), NoEvalPolicy::Native);
    assert_eq!(expected.command_default(), CommandDefault::Enabled);
    assert!(
        context
            .obarray_view()
            .get_property("identity", "disabled")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(expected.leaf_spec().is_none());
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());

    let object = Value::vector(vec![Value::fixnum(17)]);
    let returned = actual_fn(context, vec![object]).expect("core identity body");
    assert_eq!(returned.bits(), object.bits());
}
