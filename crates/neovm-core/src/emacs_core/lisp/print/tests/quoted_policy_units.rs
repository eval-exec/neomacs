//! GNU print.c:2533-2568 gates all quote syntax on the live print_quoted flag.
//! Expectations come from the already refreshed GNU settings fixtures.
use super::super::*;

fn expected_strings(context: &mut crate::emacs_core::eval::Context, fixture: &str) -> Vec<Vec<u8>> {
    let expected = context.eval_str(&format!("(quote {fixture})")).unwrap();
    list_to_vec(&expected)
        .unwrap()
        .into_iter()
        .filter_map(|value| {
            value
                .as_lisp_string()
                .map(|string| string.as_bytes().to_vec())
        })
        .collect()
}

#[test]
fn stateful_quote_policy_matches_saved_gnu_on_string_and_byte_paths() {
    crate::test_utils::init_test_tracing();
    // These inputs are entirely quoted reader forms, requiring no startup Lisp.
    let mut context = crate::emacs_core::eval::Context::new();
    let expected = expected_strings(&mut context, include_str!("settings_gnu/quoted.expect"));
    assert_eq!(expected.len(), 2);
    for (source, expected) in [r#"'('a #'b `(c ,d))"#, "'(quote x)"]
        .into_iter()
        .zip(expected)
    {
        let value = context.eval_str(source).unwrap();
        let options = PrintOptions {
            print_quoted: false,
            ..PrintOptions::default()
        };
        assert_eq!(
            print_value_stateful(&value, options).as_bytes(),
            expected.as_slice()
        );
        assert_eq!(
            print_value_stateful_bytes_with_buffers(&value, None, options),
            expected
        );
        assert_eq!(
            print_value_bytes_in_context(
                &mut context,
                &value,
                options,
                PrintSettingsSource::Globals
            )
            .unwrap(),
            expected
        );
    }
}

#[test]
fn evaluator_quote_policy_refresh_affects_later_vector_children() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::emacs_core::eval::Context::new();
    let expected = expected_strings(
        &mut context,
        include_str!("settings_gnu/live_options.expect"),
    )
    .into_iter()
    .next()
    .unwrap();
    let callback = context
        .eval_str("(lambda (_object _escaping) (setq print-quoted nil) \"Q\")")
        .unwrap();
    context.set_variable("print-unreadable-function", callback);
    context.set_variable("print-quoted", Value::T);
    let value = context
        .eval_str("(vector (make-marker) '(quote a))")
        .unwrap();
    let options = crate::emacs_core::error::print_options_from_state(&context.obarray, None);
    assert_eq!(
        print_value_bytes_in_context(&mut context, &value, options, PrintSettingsSource::Globals)
            .unwrap(),
        expected
    );
}
