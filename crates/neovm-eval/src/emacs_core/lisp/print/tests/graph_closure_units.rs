//! GNU print.c:2250-2265, 2611-2613: interpreted closure slot limits.
use super::super::*;
static_assertions::assert_not_impl_any!(PrintDepthGuard<'static, 'static>: Send, Sync);

#[test]
fn closure_slot_limits_match_gnu_on_direct_string_and_byte_sinks() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    let fixture = context
        .eval_str(&format!("(quote {})", include_str!("graph_closure.expect")))
        .unwrap();
    // Own expected bytes before another evaluator call can collect Lisp values.
    let expected: Vec<Vec<u8>> = list_to_vec(&fixture)
        .unwrap()
        .into_iter()
        .map(|value| value.as_lisp_string().unwrap().as_bytes().to_vec())
        .collect();
    for (source, options, expected) in [
        (
            r##"(car (read-from-string "#[(x y) ((list x) y) (t)]"))"##,
            PrintOptions::new(false, false, None, Some(1)),
            expected[0].clone(),
        ),
        (
            r##"(car (read-from-string "#[(x) ((list x)) (t)]"))"##,
            PrintOptions::new(false, false, Some(1), None),
            expected[1].clone(),
        ),
    ] {
        let closure = context.eval_str(source).unwrap();
        let expected = expected.as_slice();
        assert_eq!(
            print_value_with_options(&closure, options).as_bytes(),
            expected
        );
        assert_eq!(
            print_value_bytes_with_options(&closure, options).as_slice(),
            expected
        );
    }
}
