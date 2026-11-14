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
    assert_core_classification_matches_canonical_descriptors(&mut eval);
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

fn assert_core_classification_matches_canonical_descriptors(context: &mut Context) {
    use crate::emacs_core::builtins::misc_eval::{COMMAND_MODES_SUBR, COMMANDP_SUBR};
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::error::FlowRef;
    use neovm_value::value::Value;

    let command = context
        .eval_str(
            r#"(setq core-r0-classification-command
                     '(lambda ()
                        (interactive (error "interactive SPEC must remain unevaluated")
                                     text-mode prog-mode)))"#,
        )
        .expect("root the quoted command without evaluating its interactive SPEC");
    for (name, expected, min, max) in [
        ("commandp", COMMANDP_SUBR, 1, Some(2)),
        ("command-modes", COMMAND_MODES_SUBR, 1, Some(1)),
    ] {
        let id = intern(name);
        let actual = lookup_global_subr_entry(id).expect("core registered classification entry");
        assert_eq!(expected.name(), name);
        assert_eq!(expected.arity(), SubrArity::new(min, max));
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
                .get_property(name, "disabled")
                .unwrap_or(Value::NIL)
                .is_nil()
        );
        assert!(expected.leaf_spec().is_none());
        assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());
        match (name, expected.function(), actual.function) {
            (
                "commandp",
                Some(SubrFn::ManySlice(expected_fn)),
                Some(SubrFn::ManySlice(actual_fn)),
            ) => {
                assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
                assert_eq!(actual_fn(context, &[command]).unwrap(), Value::T);
                assert_eq!(
                    actual_fn(context, &[Value::symbol("forward-char")]).unwrap(),
                    Value::T
                );
                let keyboard_macro = Value::string("x");
                assert_eq!(actual_fn(context, &[keyboard_macro]).unwrap(), Value::T);
                assert_eq!(
                    actual_fn(context, &[keyboard_macro, Value::T]).unwrap(),
                    Value::NIL
                );
                assert_eq!(actual_fn(context, &[Value::fixnum(7)]).unwrap(), Value::NIL);
                let error = actual_fn(context, &[]).expect_err("commandp requires one argument");
                let FlowRef::Signal(signal) = error.kind() else {
                    panic!("commandp wrong arity is a signal");
                };
                assert_eq!(signal.symbol, intern("wrong-number-of-arguments"));
            }
            ("command-modes", Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) => {
                assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
                assert_eq!(
                    actual_fn(context, vec![command]).unwrap(),
                    Value::list(vec![Value::symbol("text-mode"), Value::symbol("prog-mode")])
                );
                assert_eq!(
                    actual_fn(context, vec![Value::fixnum(7)]).unwrap(),
                    Value::NIL
                );
                let error = actual_fn(context, vec![])
                    .expect_err("command-modes requires exactly one argument");
                let FlowRef::Signal(signal) = error.kind() else {
                    panic!("command-modes wrong arity is a signal");
                };
                assert_eq!(signal.symbol, intern("wrong-number-of-arguments"));
            }
            _ => panic!("{name} retains its original production ABI"),
        }
    }
}
