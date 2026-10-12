//! GNU print.c:2330, 2463-2468, 2530-2568: object-depth domains.
use super::super::*;
static_assertions::assert_not_impl_any!(PrintShorthandGuard<'static, 'static>: Send, Sync);

#[test]
fn object_depth_domains_match_gnu_on_direct_string_and_byte_sinks() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    let fixture = context
        .eval_str(&format!("(quote {})", include_str!("graph_level.expect")))
        .unwrap();
    // Own expected bytes before another evaluator call can collect Lisp values.
    let expected: Vec<Vec<u8>> = list_to_vec(&fixture)
        .unwrap()
        .into_iter()
        .map(|value| value.as_lisp_string().unwrap().as_bytes().to_vec())
        .collect();
    for ((source, level), expected) in [
        (r#"(propertize "abc" 'face 'bold)"#, 1),
        (r#"'(\` (a (\, b)))"#, 1),
        (r#"'(\` (a (\, (b c))))"#, 3),
        ("'(a (b))", -1),
        (r#"(list (propertize "abc" 'face 'bold))"#, 2),
    ]
    .into_iter()
    .zip(expected)
    {
        let value = context.eval_str(source).unwrap();
        let options = PrintOptions::new(false, false, Some(level), None);
        let expected = expected.as_slice();
        assert_eq!(
            print_value_with_options(&value, options).as_bytes(),
            expected
        );
        assert_eq!(
            print_value_bytes_with_options(&value, options).as_slice(),
            expected
        );
    }
}
