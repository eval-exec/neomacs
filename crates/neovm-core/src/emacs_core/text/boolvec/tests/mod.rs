//! Bool-vector operations against a bit-by-bit model, with GNU's error data
//! and destination semantics (`data.c:3709-4016`).

use super::*;
use crate::emacs_core::error::{FlowKind, FlowResultExt as _};

const LENGTHS: [usize; 10] = [0, 1, 7, 8, 63, 64, 65, 127, 128, 1000];

/// A deterministic bit stream (xorshift64*).
struct Bits(u64);

impl Bits {
    fn next(&mut self) -> bool {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 63 == 1
    }

    fn take(&mut self, n: usize) -> Vec<bool> {
        (0..n).map(|_| self.next()).collect()
    }
}

/// A bool-vector of `bits`.
fn make(bits: &[bool]) -> Value {
    bool_vector_from_bits(bits)
}

/// The bits of a bool-vector, read one at a time.
fn bits_of(value: &Value) -> Vec<bool> {
    let view = BoolVectorView::of(value).expect("a bool-vector");
    (0..view.len()).map(|i| view.get(i)).collect()
}

/// The packed words keep the bits past the end zero.
fn assert_trailing_zero(value: &Value) {
    if let Some(obj) = value.as_bool_vector_obj() {
        if let Some(&last) = obj.words().last() {
            assert_eq!(
                last & !BoolVectorObj::last_word_mask(obj.nbits),
                0,
                "bits past {} are set",
                obj.nbits
            );
        }
    }
}

fn signal_parts(result: EvalResult) -> (String, Vec<Value>) {
    match result.kinded() {
        Err(FlowKind::Signal(signal)) => (signal.symbol_name().to_string(), signal.data.clone()),
        other => panic!("expected a signal, got {other:?}"),
    }
}

#[test]
fn bool_vectors_answer_the_predicates_and_reads() {
    crate::test_utils::init_test_tracing();
    let mut rng = Bits(0x9e37_79b9_7f4a_7c15);
    for n in LENGTHS {
        let bits = rng.take(n);
        let bv = make(&bits);
        assert!(is_bool_vector(&bv), "{n}");
        assert!(bv.is_bool_vector_obj());
        assert!(!bv.is_vector());
        assert_eq!(bool_vector_length(&bv), Some(n as i64));
        assert_eq!(bits_of(&bv), bits);
        for (i, &bit) in bits.iter().enumerate() {
            assert_eq!(bool_vector_ref_value(&bv, i), Some(Value::bool_val(bit)));
        }
        assert_eq!(bool_vector_ref_value(&bv, n), None);
        assert_trailing_zero(&bv);
    }
    // A vector with the old in-band tag in slot 0 is a plain vector.
    let tagged = Value::vector(vec![
        Value::symbol("--bool-vector--"),
        Value::fixnum(1),
        Value::fixnum(1),
    ]);
    assert!(!is_bool_vector(&tagged));
    assert_eq!(bool_vector_length(&tagged), None);
    assert!(!is_bool_vector(&Value::vector(vec![Value::NIL; 3])));
    assert!(!is_bool_vector(&Value::fixnum(3)));
    assert_eq!(bool_vector_length(&Value::NIL), None);
}

#[test]
fn set_and_fill_update_in_place() {
    crate::test_utils::init_test_tracing();
    {
        for n in LENGTHS {
            let bv = make(&vec![false; n]);
            let mut model = vec![false; n];
            for i in (0..n).step_by(3) {
                assert!(bool_vector_set(&bv, i, true));
                model[i] = true;
            }
            assert!(
                !bool_vector_set(&bv, n, true),
                "out of range stores nothing"
            );
            assert_eq!(bits_of(&bv), model);
            assert!(bool_vector_fill(&bv, true));
            assert_eq!(bits_of(&bv), vec![true; n]);
            assert_trailing_zero(&bv);
            assert!(bool_vector_fill(&bv, false));
            assert_eq!(bits_of(&bv), vec![false; n]);
        }
    }
}

#[test]
fn bytes_follow_gnu_order() {
    crate::test_utils::init_test_tracing();
    {
        // Bit i is bit i%8 of byte i/8; bytes past the end are ignored.
        let bv = bool_vector_from_bytes(10, &[0b1000_0101, 0b1111_1110, 0xff]);
        assert_eq!(
            bits_of(&bv),
            [
                true, false, true, false, false, false, false, true, false, true
            ]
        );
        let view = BoolVectorView::of(&bv).unwrap();
        assert_eq!(view.byte(0), 0b1000_0101);
        assert_eq!(view.byte(1), 0b0000_0010, "bits past the end read as zero");
        assert_trailing_zero(&bv);
    }
}

/// Every set operation against the model, fresh and into a destination,
/// with the destination distinct from and equal to an operand.
#[test]
fn set_operations_match_the_model() {
    crate::test_utils::init_test_tracing();
    type Op = fn(Vec<Value>) -> EvalResult;
    let ops: [(&str, Op, fn(bool, bool) -> bool); 4] = [
        ("xor", builtin_bool_vector_exclusive_or, |a, b| a ^ b),
        ("union", builtin_bool_vector_union, |a, b| a | b),
        ("intersection", builtin_bool_vector_intersection, |a, b| {
            a & b
        }),
        (
            "set-difference",
            builtin_bool_vector_set_difference,
            |a, b| a & !b,
        ),
    ];
    let mut rng = Bits(42);
    for n in LENGTHS {
        let a_bits = rng.take(n);
        let b_bits = rng.take(n);
        for (name, op, model) in ops {
            let expected: Vec<bool> = a_bits
                .iter()
                .zip(&b_bits)
                .map(|(&a, &b)| model(a, b))
                .collect();
            {
                {
                    let a = make(&a_bits);
                    let b = make(&b_bits);
                    // Fresh result.
                    let fresh = op(vec![a, b]).unwrap();
                    assert_eq!(bits_of(&fresh), expected, "{name} {n}");
                    assert_trailing_zero(&fresh);
                    // An explicit nil destination allocates too.
                    let fresh = op(vec![a, b, Value::NIL]).unwrap();
                    assert_eq!(bits_of(&fresh), expected);
                    {
                        // Into a destination: returned when it changed...
                        let dest = make(&vec![false; n]);
                        let changed = expected.iter().any(|&bit| bit);
                        let result = op(vec![a, b, dest]).unwrap();
                        if changed {
                            assert!(
                                result.bits() == dest.bits(),
                                "{name}: returns the destination"
                            );
                        } else {
                            assert!(result.is_nil(), "{name}: unchanged destination is nil");
                        }
                        assert_eq!(bits_of(&dest), expected);
                        assert_trailing_zero(&dest);
                        // ...and nil when it already held the result.
                        assert!(op(vec![a, b, dest]).unwrap().is_nil());
                    }
                    // The destination may be an operand.
                    let a_copy = make(&a_bits);
                    let result = op(vec![a_copy, b, a_copy]).unwrap();
                    assert_eq!(bits_of(&a_copy), expected);
                    assert!(result.is_nil() || result.bits() == a_copy.bits());
                }
            }
        }
    }
}

#[test]
fn not_subsetp_and_counts_match_the_model() {
    crate::test_utils::init_test_tracing();
    let mut rng = Bits(7);
    for n in LENGTHS {
        let a_bits = rng.take(n);
        let b_bits: Vec<bool> = a_bits.iter().map(|&a| a || rng.next()).collect();
        {
            let a = make(&a_bits);
            let b = make(&b_bits);
            let not: Vec<bool> = a_bits.iter().map(|&x| !x).collect();
            let fresh = builtin_bool_vector_not(vec![a]).unwrap();
            assert_eq!(bits_of(&fresh), not);
            assert_trailing_zero(&fresh);
            // `bool-vector-not` returns its destination unconditionally.
            let dest = make(&not);
            assert!(builtin_bool_vector_not(vec![a, dest]).unwrap().bits() == dest.bits());
            assert_eq!(bits_of(&dest), not);
            assert_trailing_zero(&dest);

            assert!(builtin_bool_vector_subsetp(vec![a, b]).unwrap().is_t());
            let strict = a_bits != b_bits;
            assert_eq!(
                builtin_bool_vector_subsetp(vec![b, a]).unwrap().is_t(),
                !strict
            );

            let pop = a_bits.iter().filter(|&&x| x).count() as i64;
            assert_eq!(
                builtin_bool_vector_count_population(vec![a]).unwrap(),
                Value::fixnum(pop)
            );
            for start in [0, 1, n / 2, n.saturating_sub(1), n] {
                if start > n {
                    continue;
                }
                for target in [false, true] {
                    let expected = a_bits[start.min(n)..]
                        .iter()
                        .take_while(|&&bit| bit == target)
                        .count() as i64;
                    let got = builtin_bool_vector_count_consecutive(vec![
                        a,
                        Value::bool_val(target),
                        Value::fixnum(start as i64),
                    ])
                    .unwrap();
                    assert_eq!(got, Value::fixnum(expected), "n={n} start={start}");
                }
            }
        }
    }
}

#[test]
fn count_consecutive_runs_across_words() {
    crate::test_utils::init_test_tracing();
    {
        let mut bits = vec![true; 200];
        bits[150] = false;
        let bv = make(&bits);
        for (start, expected) in [(0, 150), (3, 147), (64, 86), (150, 0), (151, 49), (200, 0)] {
            let got =
                builtin_bool_vector_count_consecutive(vec![bv, Value::T, Value::fixnum(start)])
                    .unwrap();
            assert_eq!(got, Value::fixnum(expected), "start {start}");
        }
        let zeros = make(&[false; 70]);
        let got = builtin_bool_vector_count_consecutive(vec![zeros, Value::NIL, Value::fixnum(3)])
            .unwrap();
        assert_eq!(got, Value::fixnum(67), "the pad bits never count");
    }
}

#[test]
fn errors_carry_gnu_data() {
    crate::test_utils::init_test_tracing();
    {
        let a = make(&[true; 3]);
        let b = make(&[true; 4]);
        let c = make(&[true; 5]);
        // A length mismatch between the operands: two sizes with no
        // destination, three with one.
        let (sym, data) = signal_parts(builtin_bool_vector_union(vec![a, b]));
        assert_eq!(sym, "wrong-length-argument");
        assert_eq!(data, vec![Value::fixnum(3), Value::fixnum(4)]);
        let (_, data) = signal_parts(builtin_bool_vector_union(vec![a, b, c]));
        assert_eq!(
            data,
            vec![Value::fixnum(3), Value::fixnum(4), Value::fixnum(5)]
        );
        // A destination of the wrong length.
        let a2 = make(&[false; 3]);
        let (_, data) = signal_parts(builtin_bool_vector_intersection(vec![a, a2, c]));
        assert_eq!(
            data,
            vec![Value::fixnum(3), Value::fixnum(3), Value::fixnum(5)]
        );
        // `subsetp` runs the driver with B as its destination.
        let (_, data) = signal_parts(builtin_bool_vector_subsetp(vec![a, b]));
        assert_eq!(
            data,
            vec![Value::fixnum(3), Value::fixnum(4), Value::fixnum(4)]
        );
        // `not` compares only A and its destination.
        let (_, data) = signal_parts(builtin_bool_vector_not(vec![a, b]));
        assert_eq!(data, vec![Value::fixnum(3), Value::fixnum(4)]);
        // Type errors.
        let (sym, data) = signal_parts(builtin_bool_vector_union(vec![a, Value::fixnum(1)]));
        assert_eq!(sym, "wrong-type-argument");
        assert_eq!(data, vec![Value::symbol("bool-vector-p"), Value::fixnum(1)]);
        let plain = Value::vector(vec![Value::NIL; 3]);
        let (_, data) = signal_parts(builtin_bool_vector_union(vec![a, a2, plain]));
        assert_eq!(data, vec![Value::symbol("bool-vector-p"), plain]);
        let (sym, data) = signal_parts(builtin_bool_vector_count_consecutive(vec![
            a,
            Value::T,
            Value::fixnum(4),
        ]));
        assert_eq!(sym, "args-out-of-range");
        assert_eq!(data, vec![a, Value::fixnum(4)]);
        let (_, data) = signal_parts(builtin_bool_vector_count_consecutive(vec![
            a,
            Value::T,
            Value::fixnum(-1),
        ]));
        assert_eq!(data, vec![Value::symbol("wholenump"), Value::fixnum(-1)]);
        let (_, data) = signal_parts(builtin_make_bool_vector(vec![
            Value::fixnum(-1),
            Value::NIL,
        ]));
        assert_eq!(data, vec![Value::symbol("wholenump"), Value::fixnum(-1)]);
    }
}

#[test]
fn make_bool_vector_and_bool_vector_build_packed_bool_vectors() {
    crate::test_utils::init_test_tracing();
    {
        let made = builtin_make_bool_vector(vec![Value::fixnum(70), Value::T]).unwrap();
        let listed = builtin_bool_vector(vec![Value::T, Value::NIL, Value::symbol("x")]).unwrap();
        assert!(made.is_bool_vector_obj());
        assert_eq!(bits_of(&made), vec![true; 70]);
        assert_trailing_zero(&made);
        assert_eq!(bits_of(&listed), [true, false, true]);
        assert!(builtin_bool_vector_p(vec![made]).unwrap().is_t());
        let copy = copy_bool_vector(&made).unwrap();
        assert!(copy.bits() != made.bits());
        assert_eq!(bits_of(&copy), bits_of(&made));
    }
    assert!(
        builtin_bool_vector_p(vec![Value::vector(vec![])])
            .unwrap()
            .is_nil()
    );
}

/// Evaluate `src` in a fresh evaluator and print the result.
fn eval_printed(src: &str) -> String {
    let mut ctx = crate::emacs_core::eval::Context::new();
    let value = ctx.eval_str(src).unwrap_or_else(|e| panic!("{src}: {e:?}"));
    crate::emacs_core::print::print_value(&value)
}

/// Where the old tagged encoding diverged from GNU, bool-vectors answer as
/// GNU 31.1 does (`bvsem.el` R2 and `muc.el` on GNU: `vectorp` nil,
/// `type-of` `bool-vector`, 17 vector cells for 1000 bits), and a vector
/// whose slot 0 is the old tag is a plain vector (`bvsem.el` R1 on GNU).
#[test]
fn bool_vectors_answer_as_gnu_where_the_tagged_encoding_diverged() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_printed(
            "(let ((b (make-bool-vector 5 t)))
               (list (vectorp b) (type-of b) (cl-type-of b) (arrayp b) (sequencep b)
                     (vector-or-char-table-p b)
                     (let* ((before (nth 2 (memory-use-counts)))
                            (x (make-bool-vector 1000 t)))
                       (and x (- (nth 2 (memory-use-counts)) before)))))"
        ),
        "(nil bool-vector bool-vector t t nil 17)"
    );
    assert_eq!(
        eval_printed(
            "(let ((fake (vector (intern \"--bool-vector--\") 3 1 0 1))
                   (fake-ct (vector (intern \"--char-table--\") nil nil nil 0)))
               (list (bool-vector-p fake) (vectorp fake) (length fake) (aref fake 0)
                     (char-table-p fake-ct) (vectorp fake-ct) (length fake-ct)))"
        ),
        "(nil t 5 --bool-vector-- nil t 5)"
    );
}
