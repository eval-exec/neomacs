//! GNU print.c:1593-1610, 2388-2389: immutable runtime output retains
//! the same charset pruning as Lisp printers, including error DATA printing.
use super::super::*;

#[test]
fn immutable_error_string_path_preserves_gnu_charset_policy() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::emacs_core::Context::new();
    context.eval_str("(set-charset-priority 'unicode)").unwrap();
    let fixture = context
        .eval_str(&format!(
            "(quote {})",
            include_str!("settings_gnu/charset.expect")
        ))
        .unwrap();
    // Own fixture payloads before subsequent Lisp allocations can collect.
    let expected: Vec<String> = list_to_vec(&fixture)
        .unwrap()
        .into_iter()
        .skip(1)
        .map(|value| {
            String::from_utf8(value.as_lisp_string().unwrap().as_bytes().to_vec()).unwrap()
        })
        .collect();
    // Keep this test on the immutable legacy String arm, rather than the
    // stateful route used when any graph/limit option is active.
    for name in [
        "print-circle",
        "print-level",
        "print-length",
        "print-escape-nonascii",
    ] {
        context.obarray.set_symbol_value(name, Value::NIL);
    }
    for ((text, charset, policy, other_property), expected) in [
        ("a", "ascii", Value::symbol("default"), false),
        ("a", "ascii", Value::symbol("default"), true),
        ("é", "ascii", Value::NIL, false),
        ("a", "ascii", Value::T, false),
        ("é", "ascii", Value::symbol("default"), false),
        ("é", "unicode", Value::symbol("default"), false),
    ]
    .into_iter()
    .zip(expected)
    {
        context
            .obarray
            .set_symbol_value("print-charset-text-property", policy);
        let mut props = vec![Value::symbol("charset"), Value::symbol(charset)];
        if other_property {
            props.extend([Value::symbol("foo"), Value::symbol("bar")]);
        }
        let value = Value::string_with_text_properties(
            text,
            vec![StringTextPropertyRun {
                start: 0,
                end: 1,
                plist: Value::list(props),
            }],
        );
        assert_eq!(
            crate::emacs_core::error::print_value_in_state(&context, &value),
            expected
        );
    }
}
