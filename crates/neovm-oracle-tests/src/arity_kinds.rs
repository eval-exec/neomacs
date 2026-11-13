//! All-kinds `wrong-number-of-arguments` oracle (lead 2026-11-08 14:3xZ,
//! item 3; the all-kinds arity oracle owed after the arity-precondition r2).
//!
//! The arity error DATA is not the same for every callee kind, and GNU's
//! shapes are observable:
//!
//! * subr: `xsignal2 (Qwrong_number_of_arguments, fun, make_fixnum (nargs))`
//!   (`src/eval.c:3311`) -- the function object and the argument count.
//! * byte-code: `(mandatory <= nargs && (rest || nargs <= nonrest))` else
//!   `Fsignal (Qwrong_number_of_arguments, list2 (Fcons (mandatory,
//!   nonrest), nargs))` (`src/bytecode.c:537-541`) -- a CONS of the
//!   mandatory/nonrest counts, not the function object.
//! * interpreted closure: `apply_lambda` signals
//!   `(fun, make_fixnum (nargs))` when a required argument is missing
//!   (`src/eval.c:3424`) and the symbol-designator path signals for an
//!   invalid lambda list shape (`src/eval.c:3438`); the data names the
//!   closure object (or the source list for a quoted lambda).
//!
//! The byte-code data must not move when the same function is executed by a
//! different tier: the JIT/native leaves keep `bytecode.c`'s check, so the
//! hot/JIT test runs the same form under a knob matrix (interpreted, default,
//! immediate tier-up, forced deopt) and requires one answer from all of them.
//! GNU runs once per expectation: the matrix is Neomacs-only.
//!
//! The expectations are pinned from GNU 31.1 (a360712c9d27, pinned checkout);
//! in live/verify modes the harness also runs that GNU and compares. Each
//! kind is checked with TOO FEW and TOO MANY arguments.

use crate::common::{
    JitTierWitness, assert_oracle_parity_expect,
    assert_oracle_parity_under_envs_expect_with_witness,
    return_if_neovm_enable_oracle_proptest_not_set,
};

/// `car` is a one-argument subr: zero arguments and two arguments both take
/// the subr arity path (`eval.c:3305-3311`).
#[test]
fn arity_subr_too_few_and_too_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((wrong-number-of-arguments car 0) (wrong-number-of-arguments car 2))""#
    ]];
    assert_oracle_parity_expect(
        "(list (condition-case e (car) (error e))
               (condition-case e (car 1 2) (error e)))",
        expect,
    );
}

/// A byte-compiled two-argument function: the error data is GNU's
/// `(wrong-number-of-arguments (MANDATORY . NONREST) NARGS)` cons
/// (`bytecode.c:537-541`), not the function object.
#[test]
fn arity_byte_code_too_few_and_too_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((wrong-number-of-arguments (2 . 2) 0) (wrong-number-of-arguments (2 . 2) 3))""#
    ]];
    assert_oracle_parity_expect(
        "(progn
           (defun arity-kind-bc (a b) (+ a b))
           (byte-compile 'arity-kind-bc)
           (list (condition-case e (arity-kind-bc) (error e))
                 (condition-case e (arity-kind-bc 1 2 3) (error e))))",
        expect,
    );
}

/// Interpreted closures name the closure object; a quoted lambda list names
/// the source list (`eval.c:3424` and the symbol-designator path at :3438).
#[test]
fn arity_interpreted_closure_too_few_and_too_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((wrong-number-of-arguments (closure (t) (a b) (+ a b)) 0) (wrong-number-of-arguments (closure (t) (a b) (+ a b)) 3) (wrong-number-of-arguments (lambda (a b) (+ a b)) 1))""#
    ]];
    assert_oracle_parity_expect(
        "(let ((arity-kind-fn (lambda (a b) (+ a b))))
           (list (condition-case e (funcall arity-kind-fn) (error e))
                 (condition-case e (funcall arity-kind-fn 1 2 3) (error e))
                 (condition-case e
                          (funcall '(lambda (a b) (+ a b)) 1)
                        (error e))))",
        expect,
    );
}

/// AK-1 (lead 2026-11-09): each arm of the tier matrix must not only answer
/// like GNU, it must actually execute the tier it claims. This witness reads
/// the same run's `NEOVM_JIT_STATS_FILE` report -- outside the parity
/// payload, so it can only fail the arm, never move the expected answer:
///
/// * JIT off compiles nothing and enters no compiled leaf;
/// * both JIT arms compile a leaf and enter it natively;
/// * the forced-deopt arm actually deopts.
///
/// A knob that stops taking effect, or a form that never gets hot enough to
/// compile, turns the test RED while every answer still matches GNU. Raise
/// any of these bounds and the witness fails.
fn arity_tier_witness(env: &[(&str, &str)], jit: &JitTierWitness) {
    let knob = |name: &str| {
        env.iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| *value)
    };
    if knob("NEOVM_JIT") == Some("0") {
        assert_eq!(
            jit.compiles, 0,
            "the JIT-off arm compiled a leaf: {env:?}\n{}",
            jit.stats
        );
        assert_eq!(
            jit.native_entries, 0,
            "the JIT-off arm entered a compiled leaf: {env:?}\n{}",
            jit.stats
        );
    } else {
        assert!(
            jit.compiles > 0,
            "the JIT arm compiled nothing: {env:?}\n{}",
            jit.stats
        );
        assert!(
            jit.native_entries > 0,
            "the JIT arm never entered a compiled leaf: {env:?}\n{}",
            jit.stats
        );
        if knob("NEOVM_JIT_FORCE_DEOPT") == Some("1") {
            assert!(
                jit.deopts > 0,
                "the forced-deopt arm never deopted: {env:?}\n{}",
                jit.stats
            );
        }
    }
}

/// The same byte-code function after it is hot enough to have a compiled
/// leaf: interpreted, default, immediate-tier-up and forced-deopt Neomacs
/// must all report GNU's one answer, and each arm must show the engine
/// activity it claims (AK-1, `arity_tier_witness`).
#[test]
fn arity_byte_code_across_jit_tiers_too_few_and_too_many() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((wrong-number-of-arguments (2 . 2) 0) (wrong-number-of-arguments (2 . 2) 3))""#
    ]];
    assert_oracle_parity_under_envs_expect_with_witness(
        "(progn
           (defun arity-kind-hot (a b) (+ a b))
           (byte-compile 'arity-kind-hot)
           (dotimes (_ 2000) (arity-kind-hot 1 2))
           (list (condition-case e (arity-kind-hot) (error e))
                 (condition-case e (arity-kind-hot 1 2 3) (error e))))",
        &[
            &[("NEOVM_JIT", "0")],
            &[],
            &[("NEOVM_JIT", "1"), ("NEOVM_JIT_THRESHOLD", "1")],
            &[
                ("NEOVM_JIT", "1"),
                ("NEOVM_JIT_THRESHOLD", "1"),
                ("NEOVM_JIT_FORCE_DEOPT", "1"),
            ],
        ],
        expect,
        arity_tier_witness,
    );
}
