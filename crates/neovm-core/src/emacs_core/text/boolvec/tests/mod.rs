//! Bool-vector representations and operations against a bit-by-bit model,
//! in both representations (and mixed), with GNU's error data and
//! destination semantics (`data.c:3709-4016`).

use super::*;
use crate::emacs_core::error::Flow;

const LENGTHS: [usize; 10] = [0, 1, 7, 8, 63, 64, 65, 127, 128, 1000];
const REPRS: [BoolVectorRepr; 2] = [BoolVectorRepr::Legacy, BoolVectorRepr::Packed];

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

/// A bool-vector of `bits` in representation `repr`.
fn make(repr: BoolVectorRepr, bits: &[bool]) -> Value {
    set_bool_vector_repr_for_test(Some(repr));
    let value = bool_vector_from_bits(bits);
    set_bool_vector_repr_for_test(None);
    value
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
    match result {
        Err(Flow::Signal(signal)) => (signal.symbol_name().to_string(), signal.data.clone()),
        other => panic!("expected a signal, got {other:?}"),
    }
}

#[test]
fn both_representations_answer_the_predicates_and_reads() {
    crate::test_utils::init_test_tracing();
    let mut rng = Bits(0x9e37_79b9_7f4a_7c15);
    for repr in REPRS {
        for n in LENGTHS {
            let bits = rng.take(n);
            let bv = make(repr, &bits);
            assert!(is_bool_vector(&bv), "{repr:?} {n}");
            assert_eq!(bool_vector_length(&bv), Some(n as i64));
            assert_eq!(bv.is_bool_vector_obj(), repr == BoolVectorRepr::Packed);
            assert_eq!(bits_of(&bv), bits);
            for (i, &bit) in bits.iter().enumerate() {
                assert_eq!(bool_vector_ref_value(&bv, i), Some(Value::bool_val(bit)));
            }
            assert_eq!(bool_vector_ref_value(&bv, n), None);
            assert_trailing_zero(&bv);
        }
    }
    assert!(!is_bool_vector(&Value::vector(vec![Value::NIL; 3])));
    assert!(!is_bool_vector(&Value::fixnum(3)));
    assert_eq!(bool_vector_length(&Value::NIL), None);
}

#[test]
fn set_and_fill_update_in_place() {
    crate::test_utils::init_test_tracing();
    for repr in REPRS {
        for n in LENGTHS {
            let bv = make(repr, &vec![false; n]);
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
    for repr in REPRS {
        // Bit i is bit i%8 of byte i/8; bytes past the end are ignored.
        set_bool_vector_repr_for_test(Some(repr));
        let bv = bool_vector_from_bytes(10, &[0b1000_0101, 0b1111_1110, 0xff]);
        set_bool_vector_repr_for_test(None);
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
/// across both representations and mixed operands.
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
            for ra in REPRS {
                for rb in REPRS {
                    let a = make(ra, &a_bits);
                    let b = make(rb, &b_bits);
                    // Fresh result.
                    let fresh = op(vec![a, b]).unwrap();
                    assert_eq!(bits_of(&fresh), expected, "{name} {n} {ra:?}/{rb:?}");
                    assert_trailing_zero(&fresh);
                    // An explicit nil destination allocates too.
                    let fresh = op(vec![a, b, Value::NIL]).unwrap();
                    assert_eq!(bits_of(&fresh), expected);
                    for rd in REPRS {
                        // Into a destination: returned when it changed...
                        let dest = make(rd, &vec![false; n]);
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
                    let a_copy = make(ra, &a_bits);
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
        for repr in REPRS {
            let a = make(repr, &a_bits);
            let b = make(repr, &b_bits);
            let not: Vec<bool> = a_bits.iter().map(|&x| !x).collect();
            let fresh = builtin_bool_vector_not(vec![a]).unwrap();
            assert_eq!(bits_of(&fresh), not);
            assert_trailing_zero(&fresh);
            // `bool-vector-not` returns its destination unconditionally.
            let dest = make(repr, &not);
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
                    assert_eq!(got, Value::fixnum(expected), "{repr:?} n={n} start={start}");
                }
            }
        }
    }
}

#[test]
fn count_consecutive_runs_across_words() {
    crate::test_utils::init_test_tracing();
    for repr in REPRS {
        let mut bits = vec![true; 200];
        bits[150] = false;
        let bv = make(repr, &bits);
        for (start, expected) in [(0, 150), (3, 147), (64, 86), (150, 0), (151, 49), (200, 0)] {
            let got =
                builtin_bool_vector_count_consecutive(vec![bv, Value::T, Value::fixnum(start)])
                    .unwrap();
            assert_eq!(got, Value::fixnum(expected), "{repr:?} start {start}");
        }
        let zeros = make(repr, &[false; 70]);
        let got = builtin_bool_vector_count_consecutive(vec![zeros, Value::NIL, Value::fixnum(3)])
            .unwrap();
        assert_eq!(got, Value::fixnum(67), "the pad bits never count");
    }
}

#[test]
fn errors_carry_gnu_data() {
    crate::test_utils::init_test_tracing();
    for repr in REPRS {
        let a = make(repr, &[true; 3]);
        let b = make(repr, &[true; 4]);
        let c = make(repr, &[true; 5]);
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
        let a2 = make(repr, &[false; 3]);
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
fn make_bool_vector_and_bool_vector_follow_the_knob() {
    crate::test_utils::init_test_tracing();
    for repr in REPRS {
        set_bool_vector_repr_for_test(Some(repr));
        let made = builtin_make_bool_vector(vec![Value::fixnum(70), Value::T]).unwrap();
        let listed = builtin_bool_vector(vec![Value::T, Value::NIL, Value::symbol("x")]).unwrap();
        set_bool_vector_repr_for_test(None);
        assert_eq!(made.is_bool_vector_obj(), repr == BoolVectorRepr::Packed);
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

/// Evaluate `src` in a fresh evaluator whose new bool-vectors take
/// representation `repr`, and print the result.
fn eval_printed(repr: BoolVectorRepr, src: &str) -> String {
    set_bool_vector_repr_for_test(Some(repr));
    let mut ctx = crate::emacs_core::eval::Context::new();
    let value = ctx
        .eval_str(src)
        .unwrap_or_else(|e| panic!("{src} under {repr:?}: {e:?}"));
    let printed = crate::emacs_core::print::print_value(&value);
    set_bool_vector_repr_for_test(None);
    printed
}

/// The Lisp surface answers alike in both representations wherever the
/// legacy encoding was already GNU's answer: element access, the sequence
/// functions, printing and reading, `equal` and `equal` tables, `value<`,
/// and category sets.
#[test]
fn the_lisp_surface_is_the_same_in_both_representations() {
    crate::test_utils::init_test_tracing();
    let forms = [
        "(let ((b (bool-vector t nil t t nil)))
           (list b (length b) (aref b 2) (aref b 1) (vconcat b) (append b nil)
                 (mapcar (lambda (x) x) b) (copy-sequence b) (reverse b)
                 (equal b (copy-sequence b)) (equal b (reverse b)) (elt b 3)
                 (bool-vector-p b) (arrayp b) (sequencep b)
                 (length< b 6) (length= b 5) (length> b 4)
                 (condition-case e (aref b 5) (error e))
                 (condition-case e (aref b -1) (error e))
                 (aset b 1 'x) b))",
        "(let ((b (make-bool-vector 9 nil)))
           (aset b 0 t) (aset b 1 t)
           (list (nreverse b) (fillarray b t) (fillarray b nil)))",
        "(let* ((b (make-bool-vector 70 t)) (s (prin1-to-string b)))
           (aset b 3 nil)
           (list s (prin1-to-string b) (equal (car (read-from-string (prin1-to-string b))) b)
                 (car (read-from-string \"#&5\\\"\\\\37\\\"\"))
                 (car (read-from-string \"#&3\\\"\\\\377\\\"\"))))",
        "(let ((h (make-hash-table :test 'equal)))
           (puthash (bool-vector t nil) 'two h)
           (puthash (make-bool-vector 200 t) 'big h)
           (list (gethash (bool-vector t nil) h) (gethash (make-bool-vector 200 t) h)
                 (gethash (bool-vector nil t) h) (hash-table-count h)
                 (= (sxhash-equal (bool-vector t nil)) (sxhash-equal (bool-vector t nil)))))",
        "(list (value< (bool-vector nil t) (bool-vector t nil))
               (value< (bool-vector t) (bool-vector t nil))
               (value< (bool-vector t nil) (bool-vector t)))",
        "(let ((s (make-category-set \"abz\")))
           (list (category-set-mnemonics s) (length s) (bool-vector-p s) (aref s ?b)))",
        "(let ((a (make-bool-vector 100 nil)) (b (make-bool-vector 100 t)))
           (aset a 99 t)
           (list (bool-vector-union a b) (bool-vector-intersection a b)
                 (bool-vector-count-consecutive b t 7)
                 (bool-vector-subsetp a b) (bool-vector-not a)))",
    ];
    for form in forms {
        assert_eq!(
            eval_printed(BoolVectorRepr::Packed, form),
            eval_printed(BoolVectorRepr::Legacy, form),
            "{form}"
        );
    }
}

/// Where the legacy encoding diverged from GNU, a packed bool-vector
/// answers as GNU 31.1 does (`bvsem.el` R2 and `muc.el` on GNU: `vectorp`
/// nil, `type-of` `bool-vector`, 17 vector cells for 1000 bits).
#[test]
fn packed_bool_vectors_answer_as_gnu_where_legacy_diverged() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        eval_printed(
            BoolVectorRepr::Packed,
            "(let ((b (make-bool-vector 5 t)))
               (list (vectorp b) (type-of b) (cl-type-of b) (arrayp b) (sequencep b)
                     (vector-or-char-table-p b)
                     (let* ((before (nth 2 (memory-use-counts)))
                            (x (make-bool-vector 1000 t)))
                       (and x (- (nth 2 (memory-use-counts)) before)))))"
        ),
        "(nil bool-vector bool-vector t t nil 17)"
    );
}
