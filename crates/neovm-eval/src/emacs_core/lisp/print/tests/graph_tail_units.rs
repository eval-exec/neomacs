//! GNU print.c:2740-2763: byte-sink Brent detector uses cons identity.
use super::super::*;

#[test]
fn byte_sink_circular_tail_matches_gnu_brent_identity() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::runtime_startup_context();
    let fixture = context
        .eval_str(&format!("(quote {})", include_str!("graph_tail.expect")))
        .unwrap();
    // Own expected bytes before another evaluator call can collect Lisp values.
    let expected: Vec<Vec<u8>> = list_to_vec(&fixture)
        .unwrap()
        .into_iter()
        .map(|value| value.as_lisp_string().unwrap().as_bytes().to_vec())
        .collect();
    for (source, expected) in [
        "(let ((x (list 'a 'a 'a 'a))) (setcdr (nthcdr 3 x) (cdr x)) x)",
        "(let ((x (list 'a 'b 'a 'b 'a 'b))) (setcdr (nthcdr 5 x) (nthcdr 2 x)) x)",
    ]
    .into_iter()
    .zip(expected)
    {
        let value = context.eval_str(source).unwrap();
        assert_eq!(print_value_bytes(&value).as_slice(), expected.as_slice());
    }
}
