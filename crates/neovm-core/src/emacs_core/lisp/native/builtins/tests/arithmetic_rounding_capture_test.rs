//! Dependency coverage for GNU's rounding validation and integer division.
//! The existing GNU-refreshed rounding families check numerical answers;
//! these cases check which operands remain captured, including error paths.

use super::*;
use crate::tagged::collection_reads::capture;
use crate::tagged::mutate::LispCollectionRevision;

const ROUNDING: [LispRounding; 4] = [
    LispRounding::Truncate,
    LispRounding::Floor,
    LispRounding::Ceiling,
    LispRounding::Round,
];

fn operands() -> (Value, Value) {
    let divisor = Integer::from(1) << 100u64;
    let numerator = &divisor * Integer::from(7) + Integer::from(3);
    (Value::make_integer(numerator), Value::make_integer(divisor))
}

fn assert_condition(result: EvalResult, condition: LispCondition, data: &[Value]) {
    let flow = result.expect_err("the rounding error path must signal");
    let signal = flow.as_signal().expect("a Lisp condition");
    assert_eq!(signal.symbol, intern(condition.name()));
    assert_eq!(signal.data.as_slice(), data);
}

#[test]
fn rounding_capture_observes_bignum_divisor_and_numerator() {
    crate::test_utils::init_test_tracing();
    let (n, d) = operands();
    let unrelated = Value::cons(Value::NIL, Value::NIL);
    for rounding in ROUNDING {
        for source in [n, d] {
            let (result, reads) = capture(|| rounding_driver(n, d, rounding));
            assert!(result.is_ok(), "{rounding:?}");
            let reads = reads.expect("read-only rounding retains its certificate");
            unrelated.set_car(Value::T);
            assert!(
                reads.unchanged(),
                "unrelated writes preserve rounding reads"
            );
            // Lisp bignums are immutable; a revision mark verifies that their
            // metadata identity was retained without changing their payload.
            LispCollectionRevision::changed(source);
            assert!(!reads.unchanged(), "{rounding:?} observes both operands");
        }
    }
}

#[test]
fn rounding_capture_keeps_numerator_first_error_and_divisor_dependencies() {
    crate::test_utils::init_test_tracing();
    let (_, d) = operands();
    let wrong_n = Value::vector(vec![Value::NIL]);
    let wrong_d = Value::vector(vec![Value::T]);
    for rounding in ROUNDING {
        for divisor in [d, wrong_d, Value::fixnum(0), Value::make_float(0.0)] {
            let (result, reads) = capture(|| rounding_driver(wrong_n, divisor, rounding));
            assert_condition(
                result,
                LispCondition::WrongTypeArgument,
                &[Value::symbol("numberp"), wrong_n],
            );
            let reads = reads.expect("numerator type validation is read-only");
            LispCollectionRevision::changed(divisor);
            assert!(
                reads.unchanged(),
                "a numerator error must precede any divisor projection"
            );
            LispCollectionRevision::changed(wrong_n);
            assert!(!reads.unchanged(), "the rejected numerator was observed");
        }

        let (result, reads) = capture(|| rounding_driver(Value::fixnum(1), wrong_d, rounding));
        assert_condition(
            result,
            LispCondition::WrongTypeArgument,
            &[Value::symbol("numberp"), wrong_d],
        );
        LispCollectionRevision::changed(wrong_d);
        assert!(
            !reads.unwrap().unchanged(),
            "a rejected divisor header remains a dependency"
        );
    }
}

#[test]
fn rounding_capture_preserves_nested_and_first_read_revisions() {
    crate::test_utils::init_test_tracing();
    let (n, d) = operands();
    for rounding in ROUNDING {
        let (inner, outer) = capture(|| {
            let (result, inner) = capture(|| rounding_driver(n, d, rounding));
            assert!(result.is_ok());
            inner.expect("nested rounding is read-only")
        });
        LispCollectionRevision::changed(d);
        assert!(!inner.unchanged(), "the nested scope observes the divisor");
        assert!(
            !outer.unwrap().unchanged(),
            "nested divisor dependencies propagate outward"
        );

        for source in [n, d] {
            let (_, reads) = capture(|| {
                assert!(rounding_driver(n, d, rounding).is_ok());
                LispCollectionRevision::changed(source);
                assert!(rounding_driver(n, d, rounding).is_ok());
            });
            assert!(
                reads.is_none(),
                "a later rounding call cannot replace its first-read revision"
            );

            let (result, reads) = capture(|| {
                LispCollectionRevision::changed(source);
                rounding_driver(n, d, rounding)
            });
            assert!(result.is_ok());
            assert!(
                reads.unwrap().unchanged(),
                "a write before the first operand read remains coherent"
            );
        }
    }
}

#[test]
fn rounding_capture_keeps_zero_divisor_and_float_dispatch_order() {
    crate::test_utils::init_test_tracing();
    let (_, d) = operands();
    // Internal bignums can be constructed directly even when their payload
    // fits a fixnum; exercise the existing bignum zero check explicitly.
    let zero_big = Value::bignum(Integer::from(0));
    for rounding in ROUNDING {
        let (result, reads) =
            capture(|| rounding_driver(Value::make_float(f64::NAN), zero_big, rounding));
        assert_condition(result, LispCondition::ArithError, &[]);
        LispCollectionRevision::changed(zero_big);
        assert!(
            !reads.unwrap().unchanged(),
            "the zero divisor is observed before float overflow handling"
        );

        for numerator in [Value::make_float(3.5), Value::make_float(f64::NAN)] {
            let (result, reads) = capture(|| rounding_driver(numerator, d, rounding));
            if numerator.xfloat().is_nan() {
                assert_condition(result, LispCondition::OverflowError, &[]);
            } else {
                assert!(result.is_ok());
            }
            LispCollectionRevision::changed(d);
            assert!(
                !reads.unwrap().unchanged(),
                "the divisor remains observed across float dispatch"
            );
        }
    }
}
