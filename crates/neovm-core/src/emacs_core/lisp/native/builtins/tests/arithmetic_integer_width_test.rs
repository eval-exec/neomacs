//! `integer-width` bounds every bignum an arithmetic primitive returns.
//!
//! GNU checks the bit count of each new bignum in `make_bignum_bits`
//! (`src/bignum.c:92`), which every arithmetic result reaches through
//! `make_integer_mpz`: more than `integer-width` bits (and more than twice
//! the machine word, the floor `timefns.c` relies on) signals a bare
//! `overflow-error`.  `expt` and `ash` additionally refuse results whose
//! limb count GMP cannot hold (`emacs_mpz_pow_ui`, `emacs_mpz_mul_2exp`).
//! `integer-width` is a `DEFVAR_INT` that C reads directly, so `let`,
//! `setq` and `set-default` take effect as soon as they are made.
//! Reading a numeral (`make_bignum_str`) does not check.
//!
//! Every expectation below is GNU Emacs 32.0.50's answer to the same form,
//! from `emacs -Q --batch` with each form wrapped in
//! `(condition-case e FORM (error e))`.

use crate::emacs_core::{Context, format_eval_result};
use std::time::{Duration, Instant};

/// Evaluate each `(form, GNU answer)` in one context, in order, so a
/// binding that leaked out of an earlier form shows up in a later one.
fn check(cases: &[(&str, &str)]) {
    crate::test_utils::init_test_tracing();
    let mut ev = Context::new();
    for &(form, want) in cases {
        let start = Instant::now();
        let got = format_eval_result(&ev.eval_str(&format!("(condition-case e {form} (error e))")));
        assert_eq!(got, format!("OK {want}"), "{form}");
        assert!(
            start.elapsed() < Duration::from_secs(20),
            "{form} took {:?}",
            start.elapsed()
        );
    }
}

const OVERFLOW: &str = "(overflow-error)";

/// The ticket's runaways: before the check these allocated without bound
/// (repeated squaring) or asked for a 2^40-bit number.
#[test]
fn bignum_runaway_signals_overflow_quickly() {
    check(&[
        ("(expt 2 (expt 2 40))", OVERFLOW),
        ("(expt 3 (expt 2 40))", OVERFLOW),
        ("(expt -3 (expt 2 40))", OVERFLOW),
        ("(ash 1 (expt 2 40))", OVERFLOW),
        ("(let ((x 3)) (while t (setq x (* x x))))", OVERFLOW),
        (
            "(let ((x 3)) (while t (setq x (+ x x x x x x x x x x x x x x x x))))",
            OVERFLOW,
        ),
    ]);
}

#[test]
fn integer_width_default_limit() {
    check(&[
        ("integer-width", "65536"),
        ("(= (expt 2 65535) (ash 1 65535))", "t"),
        ("(= (ash -1 65535) (- (expt 2 65535)))", "t"),
        ("(expt 2 65536)", OVERFLOW),
        ("(- (expt 2 65536))", OVERFLOW),
        ("(expt 2 100000)", OVERFLOW),
        ("(expt 3 1000000)", OVERFLOW),
        ("(ash 1 65536)", OVERFLOW),
        ("(* (expt 2 40000) (expt 2 40000))", OVERFLOW),
    ]);
}

#[test]
fn integer_width_binding_lowers_the_limit_and_is_restored() {
    check(&[
        // 128 bits always fit, whatever `integer-width' says.
        (
            "(let ((integer-width 10)) (= (expt 2 127) (ash 1 127)))",
            "t",
        ),
        ("(let ((integer-width 10)) (expt 2 128))", OVERFLOW),
        (
            "(let ((integer-width 10)) (= (- (expt 2 127)) (ash -1 127)))",
            "t",
        ),
        ("(let ((integer-width 10)) (- (expt 2 128)))", OVERFLOW),
        (
            "(let ((integer-width 200)) (= (expt 2 199) (ash 1 199)))",
            "t",
        ),
        ("(let ((integer-width 200)) (expt 2 200))", OVERFLOW),
        // A negative width is a huge unsigned one in GNU's comparison.
        (
            "(let ((integer-width -1)) (= (expt 2 1000) (ash 1 1000)))",
            "t",
        ),
        (
            "(list (condition-case e (let ((integer-width 10)) (expt 2 1000)) (error e)) \
             integer-width (= (expt 2 1000) (ash 1 1000)))",
            "((overflow-error) 65536 t)",
        ),
        ("(progn (setq integer-width 10) (expt 2 200))", OVERFLOW),
        (
            "(progn (setq integer-width 65536) (= (expt 2 200) (ash 1 200)))",
            "t",
        ),
        (
            "(let ((integer-width 10)) (set-default 'integer-width 20) integer-width)",
            "20",
        ),
        ("integer-width", "65536"),
    ]);
}

/// Each primitive that builds a bignum checks it; one that hands back an
/// operand, or a result of at most 128 bits, does not.
#[test]
fn integer_width_checks_every_new_bignum() {
    const X: &str = "(let ((x (expt 2 200))) (let ((integer-width 10)) ";
    let cases: Vec<(String, &str)> = [
        ("(+ x 0)", OVERFLOW),
        ("(+ 0 x 0)", OVERFLOW),
        ("(- x)", OVERFLOW),
        ("(- x 1)", OVERFLOW),
        ("(- x 1 1)", OVERFLOW),
        ("(* x 1)", OVERFLOW),
        ("(* 1 x 1)", OVERFLOW),
        ("(* x x)", OVERFLOW),
        ("(1+ x)", OVERFLOW),
        ("(1- x)", OVERFLOW),
        ("(/ x 1)", OVERFLOW),
        ("(/ x 2)", OVERFLOW),
        ("(/ x)", "0"),
        ("(% x 3)", "1"),
        ("(mod x 3)", "1"),
        ("(eq x (abs x))", "t"),
        ("(abs (- x))", OVERFLOW),
        ("(logand x x)", OVERFLOW),
        ("(logior x 1)", OVERFLOW),
        ("(logxor x 0)", OVERFLOW),
        ("(logand x 255)", "0"),
        ("(lognot x)", OVERFLOW),
        ("(ash x -1)", OVERFLOW),
        ("(ash x -100)", "1267650600228229401496703205376"),
        ("(= x (ash x 0))", "t"),
        ("(eq x (max x 1))", "t"),
        ("(eq x (truncate x))", "t"),
        ("(floor x 1)", OVERFLOW),
        ("(round x 3)", OVERFLOW),
        ("(truncate x (expt 2 150))", OVERFLOW),
        ("(ceiling x 2.0)", OVERFLOW),
    ]
    .into_iter()
    .map(|(body, want)| (format!("{X}{body}))"), want))
    .collect();
    let mut cases: Vec<(&str, &str)> = cases.iter().map(|(f, w)| (f.as_str(), *w)).collect();
    cases.extend([
        // `mod' fixes the remainder's sign up into a 199-bit result.
        (
            "(let ((y (- (expt 2 199)))) (let ((integer-width 10)) (mod 1 y)))",
            OVERFLOW,
        ),
        (
            "(let ((integer-width 10)) \
             (* most-positive-fixnum most-positive-fixnum most-positive-fixnum))",
            OVERFLOW,
        ),
        (
            "(let ((integer-width 10)) \
             (= (* 4611686018427387904 4611686018427387904) (ash 1 124)))",
            "t",
        ),
        ("(let ((integer-width 10)) (ash 1 200))", OVERFLOW),
        (
            "(let ((integer-width 10)) (= (ash 1 127) (expt 2 127)))",
            "t",
        ),
        ("(let ((integer-width 10)) (truncate 1e300))", OVERFLOW),
        ("(let ((integer-width 10)) (floor 1e300 1))", OVERFLOW),
        ("(let ((integer-width 10)) (round 1e300))", OVERFLOW),
        (
            "(let ((integer-width 10)) \
             (= (truncate 1e38) 99999999999999997748809823456034029568))",
            "t",
        ),
        // Reading a numeral builds its bignum without the check.
        (
            "(let ((n (1- (expt 10 100)))) (let ((integer-width 10)) \
             (= (string-to-number (make-string 100 ?9)) n)))",
            "t",
        ),
        (
            "(let ((n (1- (expt 10 100)))) (let ((integer-width 10)) \
             (= (car (read-from-string (make-string 100 ?9))) n)))",
            "t",
        ),
    ]);
    check(&cases);
}

/// GNU's `+`, `*`, `logand`, `logior` and `logxor` return a sole argument
/// itself (`src/data.c:3307-3348`, `3494-3528`), `ash` returns VALUE for a
/// zero COUNT, `max` one of its arguments and the rounding functions an
/// integer given no divisor: no new bignum, so no width check, however wide
/// the operand.  Every other spelling of the same value builds one and is
/// checked.
#[test]
fn integer_width_spares_an_operand_returned_unchanged() {
    let identities = [
        "(eq x (* x))",
        "(eq x (apply #'* (list x)))",
        "(eq x (funcall #'* x))",
        "(eq x (+ x))",
        "(eq x (logand x))",
        "(eq x (logior x))",
        "(eq x (logxor x))",
        "(eq x (ash x 0))",
        "(eq x (max x))",
        "(eq x (truncate x))",
        "(eq x (floor x))",
        "(eq x (ceiling x))",
        "(eq x (round x))",
    ];
    let mut cases: Vec<(String, &str)> = Vec::new();
    // Above the limit, negative, and below the limit.
    for (x, width) in [
        ("(expt 2 200)", 10),
        ("(- (expt 2 200))", 10),
        ("(expt 2 200)", 300),
    ] {
        for body in identities {
            cases.push((
                format!("(let ((x {x})) (let ((integer-width {width})) {body}))"),
                "t",
            ));
        }
    }
    let x = |body: &str| format!("(let ((x (expt 2 200))) (let ((integer-width 10)) {body}))");
    cases.extend([
        (x("(* x 1)"), OVERFLOW),
        (x("(* 1 x)"), OVERFLOW),
        (x("(apply #'* (list x 1))"), OVERFLOW),
        (x("(logand x -1)"), OVERFLOW),
        (x("(expt x 1)"), OVERFLOW),
        (x("(ash 0 x)"), "0"),
        (
            "(let ((x (expt 2 200))) (let ((integer-width 300)) (= (* x 1) x)))".to_owned(),
            "t",
        ),
        ("(let ((x 1.5)) (eq x (* x)))".to_owned(), "t"),
        (
            "(* 'a)".to_owned(),
            "(wrong-type-argument number-or-marker-p a)",
        ),
        (
            "(progn (erase-buffer) (insert \"abc\") (* (point-marker)))".to_owned(),
            "4",
        ),
    ]);
    let cases: Vec<(&str, &str)> = cases.iter().map(|(f, w)| (f.as_str(), *w)).collect();
    check(&cases);
}

/// The arithmetic opcodes answer two integers without calling the subr;
/// that answer is bounded the same way.
#[test]
fn integer_width_bounds_the_arithmetic_opcodes() {
    let mul = r#"(make-byte-code 514 "\1\1_\207" [] 4)"#;
    let plus = r#"(make-byte-code 514 "\1\1\\\207" [] 4)"#;
    let diff = r#"(make-byte-code 514 "\1\1Z\207" [] 4)"#;
    let add1 = r#"(make-byte-code 257 "T\207" [] 2)"#;
    let sub1 = r#"(make-byte-code 257 "S\207" [] 2)"#;
    let negate = r#"(make-byte-code 257 "[\207" [] 2)"#;
    let w = |x: &str, body: String| format!("(let ((x {x})) (let ((integer-width 10)) {body}))");
    let cases = [
        (w("(expt 2 200)", format!("(funcall {mul} x x)")), OVERFLOW),
        (w("(expt 2 100)", format!("(funcall {mul} x x)")), OVERFLOW),
        (
            w(
                "(expt 2 60)",
                format!("(= (funcall {mul} x x) (expt 2 120))"),
            ),
            "t",
        ),
        (w("(expt 2 200)", format!("(funcall {plus} x 0)")), OVERFLOW),
        (w("(expt 2 200)", format!("(funcall {diff} x 1)")), OVERFLOW),
        (w("(expt 2 200)", format!("(funcall {add1} x)")), OVERFLOW),
        (w("(expt 2 200)", format!("(funcall {sub1} x)")), OVERFLOW),
        (w("(expt 2 200)", format!("(funcall {negate} x)")), OVERFLOW),
        (
            w(
                "(expt 2 127)",
                format!("(= (funcall {add1} x) (1+ (ash 1 127)))"),
            ),
            "t",
        ),
    ];
    let cases: Vec<(&str, &str)> = cases.iter().map(|(f, w)| (f.as_str(), *w)).collect();
    check(&cases);
}

/// The width the bignum constructors read is the slot of the Context active
/// on this thread, and it goes with that Context: once it is dropped, no
/// later arithmetic on the thread reads it, and dropping one Context leaves
/// the slot of the Context that is active alone.
#[test]
fn integer_width_slot_is_retired_with_its_context() {
    crate::test_utils::init_test_tracing();
    let mut a = Context::new();
    a.eval_str("(setq integer-width 10)").unwrap();
    assert!(super::integer_width_below(200));
    drop(a);
    // No Context is active: GNU's initial value, 65536.
    assert!(!super::integer_width_below(65536));
    assert!(super::integer_width_below(65537));

    let mut b = Context::new();
    b.eval_str("(setq integer-width (expt 2 62))").unwrap();
    assert!(!super::integer_width_below(1 << 20));
    let mut c = Context::new();
    c.eval_str("(setq integer-width 300)").unwrap();
    drop(b);
    // `c' is the active Context: its slot is still the one read.
    assert!(!super::integer_width_below(300));
    assert!(super::integer_width_below(301));
    assert_eq!(
        format_eval_result(&c.eval_str(
            "(list (condition-case e (expt 2 300) (error e)) (= (expt 2 299) (ash 1 299)))"
        )),
        "OK ((overflow-error) t)"
    );
    drop(c);
    assert!(!super::integer_width_below(65536));
    assert!(super::integer_width_below(65537));
}

/// A Context that moved to another thread and was dropped there leaves its
/// slot installed on the thread it came from.  The slot outlives the
/// Context, but a bignum stored in it lives in the Context's heap, which is
/// gone: the width is read from the slot without that heap.  A bignum width
/// is outside the fixnum range, so it limits nothing either way.
#[test]
fn integer_width_slot_of_a_context_dropped_elsewhere_is_read_without_its_heap() {
    crate::test_utils::init_test_tracing();
    let mut ctx = Context::new();
    ctx.eval_str("(setq integer-width (expt 2 62))").unwrap();
    assert!(!super::integer_width_below(1 << 20));
    crate::tagged::gc::clear_tagged_heap_if_installed(&ctx.tagged_heap);
    std::thread::spawn(move || {
        ctx.setup_thread_locals();
        ctx.eval_str("(setq integer-width (- (expt 2 62)))")
            .unwrap();
        assert!(!super::integer_width_below(1 << 20));
        drop(ctx);
    })
    .join()
    .expect("drop the moved Context on its new thread");
    // This thread still names the dropped Context's slot, whose bignum was
    // freed with its heap.
    assert!(!super::integer_width_below(1 << 20));
}
