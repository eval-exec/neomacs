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
    assert_core_completions_match_canonical_descriptors(&mut eval);
    assert_core_reverse_matches_canonical_descriptor(&mut eval);
    assert_core_subrp_matches_canonical_descriptor(&mut eval);
    assert_core_hash_table_count_matches_canonical_descriptor(&mut eval);
    assert_core_string_to_multibyte_matches_canonical_descriptor(&mut eval);
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

fn assert_core_completions_match_canonical_descriptors(context: &mut Context) {
    use crate::emacs_core::builtins::misc_eval::{
        ALL_COMPLETIONS_SUBR, TEST_COMPLETION_SUBR, TRY_COMPLETION_SUBR,
    };
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::error::FlowRef;
    use neovm_value::value::Value;

    let collection = context
        .eval_str("(setq core-r0-completion-list '(\"alpha\" \"alpine\" \"beta\"))")
        .expect("root the ordinary completion collection");
    let roots = context.save_specpdl_roots();
    for (name, expected) in [
        ("try-completion", TRY_COMPLETION_SUBR),
        ("all-completions", ALL_COMPLETIONS_SUBR),
        ("test-completion", TEST_COMPLETION_SUBR),
    ] {
        let id = intern(name);
        let actual = lookup_global_subr_entry(id).expect("core registered completion entry");
        let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
            (expected.function(), actual.function)
        else {
            panic!("{name} retains the production ContextVec ABI");
        };
        assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
        assert_eq!(expected.name(), name);
        assert_eq!(expected.arity(), SubrArity::new(2, Some(3)));
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

        let input = Value::string(if name == "test-completion" {
            "alpha"
        } else {
            "al"
        });
        context.push_specpdl_root(input);
        let result =
            actual_fn(context, vec![input, collection]).expect("actual core completion body");
        context.push_specpdl_root(result);
        match name {
            "try-completion" => assert_eq!(result.as_utf8_str(), Some("alp")),
            "all-completions" => assert_eq!(
                result,
                Value::list(vec![Value::string("alpha"), Value::string("alpine")])
            ),
            "test-completion" => {
                assert_eq!(result, Value::T);
                assert_eq!(
                    actual_fn(context, vec![Value::string("al"), collection]).unwrap(),
                    Value::NIL
                );
            }
            _ => unreachable!(),
        }
        let error = actual_fn(context, vec![input, collection, Value::NIL, Value::NIL])
            .expect_err("completion entries retain the legacy three-argument maximum");
        let FlowRef::Signal(signal) = error.kind() else {
            panic!("completion wrong arity is a signal");
        };
        assert_eq!(signal.symbol, intern("wrong-number-of-arguments"));

        // GNU's scanning entries bypass commandp's function cell; test-completion
        // invokes that cell normally. The real lower classifier must serve the
        // scanning route even while the Lisp-visible function is rebound.
        let table = context
            .eval_str(
                r#"(progn
                     (setq core-r0-completion-obarray (obarray-make))
                     (intern "core-r0-non-command" core-r0-completion-obarray)
                     (setq core-r0-original-commandp (symbol-function 'commandp))
                     (fset 'commandp (lambda (_) t))
                     core-r0-completion-obarray)"#,
            )
            .expect("root the obarray and temporarily rebind commandp");
        let result = actual_fn(
            context,
            vec![
                Value::string("core-r0-non-command"),
                table,
                Value::symbol("commandp"),
            ],
        );
        context
            .eval_str("(fset 'commandp core-r0-original-commandp)")
            .expect("restore commandp before checking the pointer result");
        assert_eq!(
            result.expect("actual core completion predicate route"),
            if name == "test-completion" {
                Value::T
            } else {
                Value::NIL
            },
            "only scanning completions bypass the rebound commandp function cell"
        );
    }
    context.restore_specpdl_roots(roots);
}

fn assert_core_reverse_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::misc_pure::REVERSE_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::value::Value;

    let expected = REVERSE_SUBR;
    let id = intern("reverse");
    let actual = lookup_global_subr_entry(id).expect("core registered reverse");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("reverse retains the production ContextVec ABI");
    };
    assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
    assert_eq!(expected.name(), "reverse");
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
            .get_property("reverse", "disabled")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(expected.leaf_spec().is_none());
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());

    let elements = vec![Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)];
    let input = Value::vector(elements.clone());
    let returned = actual_fn(context, vec![input]).expect("core reverse body");
    assert_ne!(returned.bits(), input.bits());
    assert_eq!(
        returned.as_vector_data().expect("reversed vector").to_vec(),
        vec![Value::fixnum(3), Value::fixnum(2), Value::fixnum(1)]
    );
    assert_eq!(
        input.as_vector_data().expect("original vector").to_vec(),
        elements
    );
}

fn assert_core_subrp_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::misc_pure::SUBRP_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::error::FlowRef;
    use neovm_value::value::Value;

    let expected = SUBRP_SUBR;
    let id = intern("subrp");
    let actual = lookup_global_subr_entry(id).expect("core registered subrp");
    let (Some(SubrFn::ManySlice(expected_fn)), Some(SubrFn::ManySlice(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("subrp retains the production ManySlice ABI");
    };
    assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
    assert_eq!(expected.name(), "subrp");
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
            .get_property("subrp", "disabled")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(expected.leaf_spec().is_none());
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());

    let primitive = context
        .eval_str("(symbol-function 'identity)")
        .expect("actual core primitive function object");
    assert_eq!(actual_fn(context, &[primitive]).unwrap(), Value::T);
    assert_eq!(
        actual_fn(context, &[Value::symbol("identity")]).unwrap(),
        Value::NIL
    );
    assert_eq!(actual_fn(context, &[Value::fixnum(7)]).unwrap(), Value::NIL);
    let error = actual_fn(context, &[]).expect_err("subrp requires its argument");
    let FlowRef::Signal(signal) = error.kind() else {
        panic!("subrp wrong arity is a signal");
    };
    assert_eq!(signal.symbol, intern("wrong-number-of-arguments"));
}

fn assert_core_hash_table_count_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::collections::HASH_TABLE_COUNT_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::error::FlowRef;
    use neovm_value::value::Value;

    let expected = HASH_TABLE_COUNT_SUBR;
    let id = intern("hash-table-count");
    let actual = lookup_global_subr_entry(id).expect("core registered hash-table-count");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("hash-table-count retains the production Many ABI");
    };
    assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
    assert_eq!(expected.name(), "hash-table-count");
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
            .get_property("hash-table-count", "disabled")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(expected.leaf_spec().is_none());
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());

    let table = context
        .eval_str("(let ((h (make-hash-table))) (puthash 'a 1 h) (puthash 'b 2 h) h)")
        .expect("actual core hash table with two entries");
    assert_eq!(actual_fn(context, vec![table]).unwrap(), Value::fixnum(2));
    let error = actual_fn(context, vec![Value::fixnum(7)])
        .expect_err("hash-table-count rejects a non-table");
    let FlowRef::Signal(signal) = error.kind() else {
        panic!("hash-table-count wrong type is a signal");
    };
    assert_eq!(signal.symbol, intern("wrong-type-argument"));
    assert_eq!(
        signal.data,
        vec![Value::symbol("hash-table-p"), Value::fixnum(7)]
    );
}

fn assert_core_string_to_multibyte_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::strings::STRING_TO_MULTIBYTE_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::error::FlowRef;
    use neovm_value::heap_types::LispString;
    use neovm_value::value::Value;

    let expected = STRING_TO_MULTIBYTE_SUBR;
    let id = intern("string-to-multibyte");
    let actual = lookup_global_subr_entry(id).expect("core registered string-to-multibyte");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("string-to-multibyte retains the production Many ABI");
    };
    assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
    assert_eq!(expected.name(), "string-to-multibyte");
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
            .get_property("string-to-multibyte", "disabled")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(expected.leaf_spec().is_none());
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());

    let unicode = Value::string("λ🙂");
    let unchanged = actual_fn(context, vec![unicode]).expect("core Unicode conversion");
    assert_eq!(unchanged.bits(), unicode.bits());
    let byte_cases: [(&[u8], &[u8]); 2] = [
        (&[0x80, 0xff], &[0xc0, 0x80, 0xc1, 0xbf]),
        (&[0xc3, 0xa9], &[0xc1, 0x83, 0xc0, 0xa9]),
    ];
    for (bytes, expected_bytes) in byte_cases {
        let input = Value::heap_string(LispString::from_unibyte_slice(bytes));
        let converted = actual_fn(context, vec![input]).expect("core byte8 conversion");
        assert_ne!(converted.bits(), input.bits());
        let string = converted.as_lisp_string().expect("multibyte string");
        assert!(string.is_multibyte());
        assert_eq!(string.as_bytes(), expected_bytes);
    }
    let error = actual_fn(context, vec![Value::fixnum(7)])
        .expect_err("string-to-multibyte rejects a non-string");
    let FlowRef::Signal(signal) = error.kind() else {
        panic!("string-to-multibyte wrong type is a signal");
    };
    assert_eq!(signal.symbol, intern("wrong-type-argument"));
    assert_eq!(
        signal.data,
        vec![Value::symbol("stringp"), Value::fixnum(7)]
    );
}
