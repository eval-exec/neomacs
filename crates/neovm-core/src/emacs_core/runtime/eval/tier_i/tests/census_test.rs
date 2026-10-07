//! T0: the census of interpreted closure calls.

use crate::emacs_core::eval::{Context, TierIEvent, TierIMode};

fn context(mode: TierIMode) -> Context {
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.tier_i.set_mode(mode);
    eval.tier_i.clear_for_test();
    eval
}

fn eval_ok(eval: &mut Context, src: &str) {
    eval.eval_str(src)
        .unwrap_or_else(|err| panic!("{src}: {err:?}"));
}

#[test]
fn census_counts_calls_and_names_the_hottest_bodies() {
    crate::test_utils::init_test_tracing();
    let mut eval = context(TierIMode::Census);
    eval_ok(
        &mut eval,
        "(progn
           (defun ti-c-small (x) x)
           (defun ti-c-big (x) (let ((a (car x)) (b (cdr x))) (if a (list a b) (list b a))))
           (dotimes (_ 50) (ti-c-small 1))
           (dotimes (_ 20) (ti-c-big '(1 . 2))))",
    );
    let lines = eval.tier_i_report_lines(10);
    let report = lines.join("\n");
    let big = lines
        .iter()
        .position(|line| line.ends_with(" ti-c-big"))
        .unwrap_or_else(|| panic!("{report}"));
    let small = lines
        .iter()
        .position(|line| line.ends_with(" ti-c-small"))
        .unwrap_or_else(|| panic!("{report}"));
    assert!(
        big < small,
        "ranked by work (calls x forms), not calls:\n{report}"
    );
    assert!(lines[big].contains("       20 calls"), "{report}");
    assert!(lines[small].contains("       50 calls"), "{report}");
    let stats = eval.tier_i.stats();
    assert!(stats.count(TierIEvent::Call) >= 70, "{}", stats.report());
}

#[test]
fn off_counts_nothing() {
    crate::test_utils::init_test_tracing();
    let mut eval = context(TierIMode::Off);
    eval_ok(
        &mut eval,
        "(progn (defun ti-c-off (x) x) (dotimes (_ 5) (ti-c-off 1)))",
    );
    assert_eq!(eval.tier_i.stats().count(TierIEvent::Call), 0);
}
