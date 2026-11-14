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
    assert_core_bufferp_matches_canonical_descriptor(&mut eval);
    assert_core_number_to_string_matches_canonical_descriptor(&mut eval);
    assert_core_make_char_table_matches_canonical_descriptor(&mut eval);
    assert_core_standard_syntax_table_matches_canonical_descriptor(&mut eval);
    assert_core_copy_marker_matches_canonical_descriptor(&mut eval);
    assert_core_char_table_subtype_matches_canonical_descriptor(&mut eval);
    assert_core_set_char_table_range_matches_canonical_descriptor(&mut eval);
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

fn assert_core_bufferp_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::types::BUFFERP_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::value::Value;

    let expected = BUFFERP_SUBR;
    let id = intern("bufferp");
    let actual = lookup_global_subr_entry(id).expect("core registered bufferp");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("bufferp retains the production ContextVec ABI");
    };
    assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
    assert_eq!(expected.name(), "bufferp");
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
            .get_property("bufferp", "disabled")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(expected.leaf_spec().is_none());
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());

    let buffer = context
        .eval_str("(current-buffer)")
        .expect("core current buffer");
    assert!(buffer.is_buffer(), "real Context supplies a buffer object");
    assert_eq!(
        actual_fn(context, vec![buffer]).expect("core bufferp buffer"),
        Value::T
    );
    assert_eq!(
        actual_fn(context, vec![Value::NIL]).expect("core bufferp nonbuffer"),
        Value::NIL
    );
}

fn assert_core_number_to_string_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::strings::NUMBER_TO_STRING_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::error::FlowRef;
    use neovm_value::value::Value;

    let expected = NUMBER_TO_STRING_SUBR;
    let id = intern("number-to-string");
    let actual = lookup_global_subr_entry(id).expect("core registered number-to-string");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("number-to-string retains the production ContextVec ABI");
    };
    assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
    assert_eq!(expected.name(), "number-to-string");
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
            .get_property("number-to-string", "disabled")
            .unwrap_or(Value::NIL)
            .is_nil()
    );
    assert!(expected.leaf_spec().is_none());
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());

    let bignum = context
        .eval_str("18446744073709551616")
        .expect("core bignum literal");
    assert!(bignum.as_bignum().is_some());
    for (number, expected) in [
        (Value::fixnum(-42), "-42"),
        (Value::make_float(1.25), "1.25"),
        (bignum, "18446744073709551616"),
    ] {
        let result = actual_fn(context, vec![number]).expect("actual core number printer");
        let string = result.as_lisp_string().expect("number-to-string result");
        assert_eq!(string.as_bytes(), expected.as_bytes());
        assert!(!string.is_multibyte());
    }
    let formatted = context
        .eval_str("(let ((float-output-format \"%.2f\")) (number-to-string 1.23456789))")
        .expect("core dynamic float-output-format");
    assert_eq!(formatted.as_lisp_string().unwrap().as_bytes(), b"1.23");
    let error =
        actual_fn(context, vec![Value::NIL]).expect_err("number-to-string rejects a non-number");
    let FlowRef::Signal(signal) = error.kind() else {
        panic!("number-to-string wrong type is a signal");
    };
    assert_eq!(signal.symbol, intern("wrong-type-argument"));
    assert_eq!(signal.data, vec![Value::symbol("numberp"), Value::NIL]);
}

fn assert_core_make_char_table_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::misc_eval::MAKE_CHAR_TABLE_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::chartab::{
        builtin_char_table_subtype, char_table_default, ct_ref, is_char_table,
    };
    use neovm_value::value::Value;

    let expected = MAKE_CHAR_TABLE_SUBR;
    let id = intern("make-char-table");
    let actual = lookup_global_subr_entry(id).expect("core registered make-char-table");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("make-char-table retains the production ContextVec ABI");
    };
    assert!(
        std::ptr::fn_addr_eq(actual_fn, expected_fn),
        "actual={actual_fn:p} expected={expected_fn:p} delta={:#x}",
        (actual_fn as usize).abs_diff(expected_fn as usize)
    );
    assert_eq!(expected.name(), "make-char-table");
    assert_eq!(expected.arity(), SubrArity::new(1, Some(2)));
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
            .get_property("make-char-table", "disabled")
            .is_none_or(|value| value.is_nil())
    );
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());
    assert!(expected.leaf_spec().is_none());

    let purpose = Value::symbol("r0-make-char-table-purpose");
    let init = Value::fixnum(17);
    let table = actual_fn(context, vec![purpose, init]).expect("core constructor body");
    assert!(is_char_table(&table));
    assert_eq!(builtin_char_table_subtype(vec![table]).unwrap(), purpose);
    assert_eq!(char_table_default(&table), init);
    assert_eq!(ct_ref(&table, 65), init);
    let defaulted = actual_fn(context, vec![purpose]).expect("core optional initializer");
    assert!(is_char_table(&defaulted));
    assert_eq!(char_table_default(&defaulted), Value::NIL);
}

fn assert_core_standard_syntax_table_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::misc_pure::STANDARD_SYNTAX_TABLE_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::chartab::{builtin_char_table_subtype, is_char_table};
    use neovm_value::tagged::header::SubrDispatchKind;
    use neovm_value::value::Value;

    let expected = STANDARD_SYNTAX_TABLE_SUBR;
    let id = intern("standard-syntax-table");
    let actual = lookup_global_subr_entry(id).expect("core registered standard-syntax-table");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("standard-syntax-table retains the production ContextVec ABI");
    };
    assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
    assert_eq!(expected.name(), "standard-syntax-table");
    assert_eq!(expected.arity(), SubrArity::new(0, Some(0)));
    assert_eq!(
        (actual.min_args, actual.max_args),
        (expected.arity().min(), expected.arity().max())
    );
    assert_eq!(actual.dispatch_kind, expected.dispatch_kind());
    assert_eq!(expected.dispatch_kind(), SubrDispatchKind::Builtin);
    assert!(actual.interactive_spec.is_none());
    assert!(expected.interactive_spec().is_none());
    assert_eq!(no_eval_policy(id), expected.no_eval_policy());
    assert_eq!(no_eval_policy(id), NoEvalPolicy::Native);
    assert_eq!(expected.command_default(), CommandDefault::Enabled);
    assert!(expected.leaf_spec().is_none());
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());
    assert_eq!(
        context
            .obarray_view()
            .get_property("standard-syntax-table", "disabled")
            .unwrap_or(Value::NIL),
        Value::NIL
    );

    let table = actual_fn(context, vec![]).expect("core standard syntax body");
    assert!(is_char_table(&table));
    assert_eq!(
        builtin_char_table_subtype(vec![table]).unwrap(),
        Value::symbol("syntax-table")
    );
    let again = actual_fn(context, vec![]).expect("core cached standard syntax");
    assert_eq!(
        again.bits(),
        table.bits(),
        "the standard table is canonical"
    );
}

fn assert_core_copy_marker_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::misc_eval::COPY_MARKER_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_edit::buffer::{EmacsBytePos, LispCharPos1};
    use neovm_edit::emacs_core::marker::{make_registered_buffer_marker, marker_logical_fields};
    use neovm_value::value::Value;

    let expected = COPY_MARKER_SUBR;
    let id = intern("copy-marker");
    let actual = lookup_global_subr_entry(id).expect("core registered copy-marker");
    let (Some(SubrFn::ManySlice(expected_fn)), Some(SubrFn::ManySlice(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("copy-marker retains the production ContextSlice ABI");
    };
    assert!(std::ptr::fn_addr_eq(actual_fn, expected_fn));
    assert_eq!(expected.name(), "copy-marker");
    assert_eq!(expected.arity(), SubrArity::new(0, Some(2)));
    assert_eq!(
        (actual.min_args, actual.max_args),
        (expected.arity().min(), expected.arity().max())
    );
    assert_eq!(actual.dispatch_kind, expected.dispatch_kind());
    assert_eq!(
        expected.dispatch_kind(),
        neovm_value::tagged::header::SubrDispatchKind::Builtin
    );
    assert!(actual.interactive_spec.is_none());
    assert!(expected.interactive_spec().is_none());
    assert_eq!(no_eval_policy(id), expected.no_eval_policy());
    assert_eq!(no_eval_policy(id), NoEvalPolicy::Native);
    assert!(expected.leaf_spec().is_none());
    assert_eq!(expected.command_default(), CommandDefault::Enabled);
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());
    assert!(
        context
            .obarray_view()
            .get_property("copy-marker", "disabled")
            .is_none_or(|value| value.is_nil())
    );

    let detached = actual_fn(context, &[]).expect("registered no-argument marker copy");
    let second = actual_fn(context, &[Value::NIL]).expect("registered nil marker copy");
    assert!(detached.is_marker() && second.is_marker());
    assert_ne!(
        detached.bits(),
        second.bits(),
        "each copy allocates a fresh marker"
    );
    assert_eq!(marker_logical_fields(&detached), Some((None, None, false)));
    let advancing = actual_fn(context, &[Value::NIL, Value::T])
        .expect("registered detached marker with insertion type");
    assert_eq!(marker_logical_fields(&advancing), Some((None, None, true)));

    let current = context
        .edit
        .buffers
        .current_buffer_id()
        .expect("current buffer");
    let source_buffer = context
        .edit
        .buffers
        .create_buffer("*registered-copy-marker-source*");
    assert_ne!(
        source_buffer, current,
        "copy must keep a noncurrent source buffer"
    );
    context
        .edit
        .buffers
        .get_mut(source_buffer)
        .unwrap()
        .insert("a中b");
    let source = make_registered_buffer_marker(
        &mut context.edit.buffers,
        source_buffer,
        LispCharPos1::new(2),
        true,
    );
    let copied = actual_fn(context, &[source]).expect("registered marker source copy");
    let typed = actual_fn(context, &[source, Value::T]).expect("registered typed marker copy");
    assert_ne!(copied.bits(), source.bits());
    assert_ne!(typed.bits(), source.bits());
    assert_ne!(typed.bits(), copied.bits());
    assert_eq!(
        marker_logical_fields(&copied),
        Some((Some(source_buffer), Some(LispCharPos1::new(2)), false)),
        "an omitted insertion type is nil, not inherited from the source"
    );
    assert_eq!(
        marker_logical_fields(&typed),
        Some((Some(source_buffer), Some(LispCharPos1::new(2)), true))
    );
    // All three marker objects must be on the actual source buffer's chain.
    let buffer = context.edit.buffers.get_mut(source_buffer).unwrap();
    buffer.goto_emacs_byte_pos(EmacsBytePos::new(1));
    buffer.insert("X");
    assert_eq!(
        marker_logical_fields(&source).unwrap().1,
        Some(LispCharPos1::new(3))
    );
    assert_eq!(
        marker_logical_fields(&copied).unwrap().1,
        Some(LispCharPos1::new(2))
    );
    assert_eq!(
        marker_logical_fields(&typed).unwrap().1,
        Some(LispCharPos1::new(3))
    );

    assert_eq!(context.edit.buffers.current_buffer_id(), Some(current));
    let full_end = context.edit.buffers.get(current).unwrap().z_lisp_char_pos();
    let numeric = actual_fn(context, &[Value::fixnum(Value::MOST_POSITIVE_FIXNUM)])
        .expect("registered numeric copy in the current buffer");
    assert_eq!(
        marker_logical_fields(&numeric),
        Some((Some(current), Some(full_end), false))
    );
}

fn assert_core_char_table_subtype_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::misc_pure::CHAR_TABLE_SUBTYPE_SUBR;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::chartab::make_char_table_value;
    use neovm_value::value::Value;

    let expected = CHAR_TABLE_SUBTYPE_SUBR;
    let id = intern("char-table-subtype");
    let actual = lookup_global_subr_entry(id).expect("core registered char-table-subtype");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("char-table-subtype retains the production ContextVec ABI");
    };
    assert!(
        std::ptr::fn_addr_eq(actual_fn, expected_fn),
        "actual={actual_fn:p} expected={expected_fn:p} delta={:#x}",
        (actual_fn as usize).abs_diff(expected_fn as usize)
    );
    assert_eq!(expected.name(), "char-table-subtype");
    assert_eq!(expected.arity(), SubrArity::new(1, Some(1)));
    assert_eq!(
        expected.dispatch_kind(),
        neovm_value::tagged::header::SubrDispatchKind::Builtin
    );
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
            .get_property("char-table-subtype", "disabled")
            .is_none_or(|value| value.is_nil())
    );
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());
    assert!(expected.leaf_spec().is_none());

    let purpose = Value::symbol("round4-subtype-purpose");
    let table = make_char_table_value(purpose, Value::fixnum(23));
    let result = actual_fn(context, vec![table]).expect("registered subtype body");
    assert_eq!(result, purpose);
    let nil_purpose = make_char_table_value(Value::NIL, Value::NIL);
    assert_eq!(actual_fn(context, vec![nil_purpose]).unwrap(), Value::NIL);
    let bad = Value::fixnum(9);
    let error = actual_fn(context, vec![bad]).unwrap_err();
    let signal = error.as_signal().expect("wrong-type signal");
    assert_eq!(signal.symbol, intern("wrong-type-argument"));
    assert_eq!(signal.data, vec![Value::symbol("char-table-p"), bad]);
    let error = actual_fn(context, vec![]).unwrap_err();
    let signal = error.as_signal().expect("arity signal");
    assert_eq!(signal.symbol, intern("wrong-number-of-arguments"));
    assert_eq!(
        signal.data,
        vec![Value::symbol("char-table-subtype"), Value::fixnum(0)]
    );
}

fn assert_core_set_char_table_range_matches_canonical_descriptor(context: &mut Context) {
    use crate::emacs_core::builtins::misc_eval::SET_CHAR_TABLE_RANGE_SUBR;
    use crate::emacs_core::eval::data::window_state::current_char_table_layout_revision;
    use crate::emacs_core::eval::lookup_global_subr_entry;
    use crate::emacs_core::eval::subr_abi::SubrFn;
    use crate::emacs_core::subr::{CommandDefault, NoEvalPolicy, SubrArity, no_eval_policy};
    use neovm_value::chartab::{char_table_default, ct_ref, make_char_table_value};
    use neovm_value::value::{Value, char_table_write_tick};
    let expected = SET_CHAR_TABLE_RANGE_SUBR;
    let id = intern("set-char-table-range");
    let actual = lookup_global_subr_entry(id).expect("core registered range setter");
    let (Some(SubrFn::Many(expected_fn)), Some(SubrFn::Many(actual_fn))) =
        (expected.function(), actual.function)
    else {
        panic!("range setter retains ContextVec ABI");
    };
    assert!(
        std::ptr::fn_addr_eq(actual_fn, expected_fn),
        "actual={actual_fn:p} expected={expected_fn:p} delta={:#x}",
        (actual_fn as usize).abs_diff(expected_fn as usize)
    );
    assert_eq!(expected.name(), "set-char-table-range");
    assert_eq!(expected.arity(), SubrArity::new(3, Some(3)));
    assert_eq!(
        expected.dispatch_kind(),
        neovm_value::tagged::header::SubrDispatchKind::Builtin
    );
    assert_eq!(
        (actual.min_args, actual.max_args),
        (expected.arity().min(), expected.arity().max())
    );
    assert_eq!(actual.dispatch_kind, expected.dispatch_kind());
    assert!(expected.interactive_spec().is_none());
    assert!(actual.interactive_spec.is_none());
    assert_eq!(no_eval_policy(id), expected.no_eval_policy());
    assert_eq!(expected.no_eval_policy(), NoEvalPolicy::Native);
    assert_eq!(expected.command_default(), CommandDefault::Enabled);
    assert!(
        context
            .obarray_view()
            .get_property("set-char-table-range", "disabled")
            .is_none_or(|value| value.is_nil())
    );
    assert!(expected.leaf_spec().is_none());
    assert!(crate::emacs_core::subr::leaf::subr_leaf(id).is_none());
    // These direct lower calls allocate without evaluator safepoints.
    let table = make_char_table_value(Value::symbol("round4-range-purpose"), Value::fixnum(1));
    let before_tick = char_table_write_tick();
    let before_layout = current_char_table_layout_revision();
    assert_eq!(
        actual_fn(context, vec![table, Value::NIL, Value::fixnum(42)]).unwrap(),
        Value::fixnum(42)
    );
    assert_eq!(char_table_default(&table), Value::fixnum(42));
    assert_ne!(char_table_write_tick(), before_tick);
    assert_ne!(current_char_table_layout_revision(), before_layout);
    assert_eq!(
        actual_fn(context, vec![table, Value::T, Value::fixnum(7)]).unwrap(),
        Value::fixnum(7)
    );
    assert_eq!(
        char_table_default(&table),
        Value::fixnum(42),
        "t retains the default slot"
    );
    assert_eq!(ct_ref(&table, 65), Value::fixnum(7));
    assert_eq!(ct_ref(&table, 0x3f_ffff), Value::fixnum(7));
    assert_eq!(
        actual_fn(
            context,
            vec![table, Value::fixnum(0x4e2d), Value::fixnum(9)]
        )
        .unwrap(),
        Value::fixnum(9)
    );
    assert_eq!(ct_ref(&table, 0x4e2d), Value::fixnum(9));

    let payload = Value::vector(vec![Value::fixnum(17)]);
    let range = Value::cons(Value::fixnum(126), Value::fixnum(129));
    let returned = actual_fn(context, vec![table, range, payload]).unwrap();
    assert_eq!(returned.bits(), payload.bits());
    for ch in 126..=129 {
        assert_eq!(ct_ref(&table, ch).bits(), payload.bits());
    }
    assert_eq!(ct_ref(&table, 125), Value::fixnum(7));
    assert_eq!(ct_ref(&table, 130), Value::fixnum(7));
    let reverse = Value::cons(Value::fixnum(90), Value::fixnum(65));
    assert_eq!(
        actual_fn(context, vec![table, reverse, Value::fixnum(10)]).unwrap(),
        Value::fixnum(10)
    );
    assert_eq!(ct_ref(&table, 65), Value::fixnum(7));

    let tick = char_table_write_tick();
    let error = actual_fn(context, vec![]).unwrap_err();
    let signal = error.as_signal().expect("arity signal");
    assert_eq!(signal.symbol, intern("wrong-number-of-arguments"));
    assert_eq!(
        signal.data,
        vec![Value::symbol("set-char-table-range"), Value::fixnum(0)]
    );
    assert_eq!(
        char_table_write_tick(),
        tick,
        "arity errors precede the write tick"
    );

    let bad_table = Value::fixnum(9);
    let bad_range = Value::string("invalid");
    let tick = char_table_write_tick();
    let layout = current_char_table_layout_revision();
    let error = actual_fn(context, vec![bad_table, bad_range, Value::NIL]).unwrap_err();
    let signal = error.as_signal().expect("table validation signal");
    assert_eq!(signal.symbol, intern("wrong-type-argument"));
    assert_eq!(signal.data, vec![Value::symbol("char-table-p"), bad_table]);
    assert_ne!(
        char_table_write_tick(),
        tick,
        "the original error-side tick is retained"
    );
    assert_eq!(current_char_table_layout_revision(), layout);

    for (car, cdr, offending) in [
        (Value::fixnum(-1), Value::fixnum(65), Value::fixnum(-1)),
        (Value::fixnum(65), bad_range, bad_range),
    ] {
        let pair = Value::cons(car, cdr);
        let layout = current_char_table_layout_revision();
        let error = actual_fn(context, vec![table, pair, Value::NIL]).unwrap_err();
        let signal = error.as_signal().expect("endpoint validation signal");
        assert_eq!(signal.symbol, intern("wrong-type-argument"));
        assert_eq!(signal.data, vec![Value::symbol("characterp"), offending]);
        assert_eq!(current_char_table_layout_revision(), layout);
    }

    // Exercise the real TextHost variable view, not the no-view style helper.
    let previous_style = context
        .variables()
        .value_by_name("text-quoting-style")
        .unwrap_or(Value::NIL);
    for (style, message) in [
        ("grave", "Invalid RANGE argument to `set-char-table-range'"),
        (
            "straight",
            "Invalid RANGE argument to 'set-char-table-range'",
        ),
        (
            "curve",
            "Invalid RANGE argument to \u{2018}set-char-table-range\u{2019}",
        ),
    ] {
        context.set_variable("text-quoting-style", Value::symbol(style));
        let table = make_char_table_value(Value::NIL, Value::NIL);
        for bad in [Value::fixnum(-1), Value::string("invalid")] {
            let layout = current_char_table_layout_revision();
            let error = actual_fn(context, vec![table, bad, Value::NIL]).unwrap_err();
            let signal = error.as_signal().expect("invalid-range signal");
            assert_eq!(signal.symbol, intern("error"));
            assert_eq!(signal.data.len(), 1);
            assert_eq!(signal.data[0].as_utf8_str(), Some(message));
            assert_eq!(current_char_table_layout_revision(), layout);
        }
    }
    context.set_variable("text-quoting-style", previous_style);
}
