//! P4.1 Stage 0 pins: the `cconv-make-interpreted-closure` hook's knob,
//! statistics and effect snapshot.

use crate::emacs_core::eval::{CconvMemoEvent, CconvMemoMode, Context, parse_cconv_memo_knob};
use crate::emacs_core::print::print_value;
use crate::emacs_core::value::Value;

fn startup(mode: CconvMemoMode) -> Context {
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.cconv_memo.set_mode(mode);
    eval
}

fn eval_ok(eval: &mut Context, src: &str) -> Value {
    eval.eval_str(src)
        .unwrap_or_else(|err| panic!("{src}: {err:?}"))
}

fn printed(eval: &mut Context, src: &str) -> String {
    let value = eval_ok(eval, src);
    print_value(&value)
}

fn count(eval: &Context, event: CconvMemoEvent) -> u64 {
    eval.cconv_memo.stats().count(event)
}

#[test]
fn knob_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(parse_cconv_memo_knob(None), CconvMemoMode::Off);
    for off in ["", "0", "off", "OFF", "no", "false"] {
        assert_eq!(
            parse_cconv_memo_knob(Some(off)),
            CconvMemoMode::Off,
            "{off}"
        );
    }
    assert_eq!(parse_cconv_memo_knob(Some("stats")), CconvMemoMode::Stats);
    assert_eq!(parse_cconv_memo_knob(Some(" Stats ")), CconvMemoMode::Stats);
    assert_eq!(parse_cconv_memo_knob(Some("bogus")), CconvMemoMode::Off);
}

/// The filter is the dumped `cconv-make-interpreted-closure` in a GNU `-Q`
/// equivalent image, so the hook sees every lexical closure creation.
#[test]
fn stats_classify_trimming_and_untrimmed_creations() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Stats);
    // An environment with a lexical variable: the trimming path, identity.
    eval_ok(&mut eval, "(let ((cm-x 1)) (lambda () cm-x))");
    assert_eq!(count(&eval, CconvMemoEvent::Call), 1);
    assert_eq!(count(&eval, CconvMemoEvent::Trim), 1);
    assert_eq!(count(&eval, CconvMemoEvent::RunIdentity), 1);
    assert_eq!(count(&eval, CconvMemoEvent::RunEffect), 0);

    // `(t)` only: no lexical variable, cconv.el returns the closure as is.
    eval_ok(&mut eval, "(lambda () 1)");
    assert_eq!(count(&eval, CconvMemoEvent::Call), 2);
    assert_eq!(count(&eval, CconvMemoEvent::NoLexvars), 1);
    assert_eq!(count(&eval, CconvMemoEvent::Trim), 1);

    // A macro in the body is expanded: the body is rewritten.
    eval_ok(&mut eval, "(let ((cm-y 2)) (lambda () (when cm-y 3)))");
    assert_eq!(count(&eval, CconvMemoEvent::Trim), 2);
    assert_eq!(count(&eval, CconvMemoEvent::RunRewritten), 1);

    // GNU's quirk (cconv.el:647-669): an uninitialized let variable that is
    // read makes the analysis call the unloaded `byte-compile-warn-x`.
    let err = eval.eval_str("(let ((cm-z 1)) (lambda () (let ((cm-w)) (list cm-w cm-z))))");
    assert!(err.is_err(), "{err:?}");
    assert_eq!(count(&eval, CconvMemoEvent::RunError), 1);

    let report = eval.cconv_memo_report();
    assert!(report.contains(" trim=3 "), "{report}");
}

#[test]
fn stats_mode_builds_the_same_closures_as_off() {
    crate::test_utils::init_test_tracing();
    let forms = [
        "(let ((a 1) (b 2)) (lambda (c) (list a c)))",
        "(let ((a 1)) (defvar cm-dyn) (let ((cm-dyn 2)) (lambda () (list a cm-dyn))))",
        "(let ((a 1) (b 2)) (lambda () 'nothing))",
        "(let ((a 1)) (lambda (x) \"doc\" (+ a x)))",
    ];
    let mut off = startup(CconvMemoMode::Off);
    let mut stats = startup(CconvMemoMode::Stats);
    for form in forms {
        assert_eq!(printed(&mut off, form), printed(&mut stats, form), "{form}");
    }
    assert_eq!(count(&off, CconvMemoEvent::Call), 0);
    assert_eq!(count(&stats, CconvMemoEvent::Trim), forms.len() as u64);
}

#[test]
fn effect_snapshot_sees_what_a_run_can_change() {
    crate::test_utils::init_test_tracing();
    let mut eval = startup(CconvMemoMode::Off);
    let quiet = eval.cconv_effect_snapshot();
    eval_ok(&mut eval, "(list 1 2 (car '(3)))");
    assert_eq!(
        eval.cconv_effect_snapshot(),
        quiet,
        "a pure run moves nothing"
    );

    for (what, src) in [
        ("message", "(message \"cm effect\")"),
        ("buffer creation", "(generate-new-buffer \"cm-effect\")"),
        (
            "buffer text",
            "(with-current-buffer \"*scratch*\" (insert \"x\"))",
        ),
        ("gensym", "(gensym)"),
        ("intern", "(intern \"cm-effect-fresh-symbol-1\")"),
        ("fset", "(defalias 'cm-effect-fn #'car)"),
        ("match data", "(string-match \"b\" \"abc\")"),
    ] {
        let before = eval.cconv_effect_snapshot();
        eval_ok(&mut eval, src);
        assert_ne!(eval.cconv_effect_snapshot(), before, "{what}");
    }

    let before = eval.cconv_effect_snapshot();
    eval.note_load_effect();
    assert_ne!(eval.cconv_effect_snapshot(), before, "load");
}
