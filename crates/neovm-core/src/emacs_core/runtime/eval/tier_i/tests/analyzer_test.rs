//! T4: the analyzer's compiled trees (golden) and its coverage report; the
//! `analyze` mode never runs compiled code.

use crate::emacs_core::eval::{Context, TierIEvent, TierIMode};

fn context(mode: TierIMode, threshold: u32) -> Context {
    let mut eval = crate::test_utils::runtime_startup_context();
    eval.tier_i.set_mode(mode);
    eval.tier_i.set_threshold(threshold);
    eval.tier_i.clear_for_test();
    eval
}

fn eval_ok(eval: &mut Context, src: &str) {
    eval.eval_str(src)
        .unwrap_or_else(|err| panic!("{src}: {err:?}"));
}

fn describe(eval: &Context, name: &str) -> String {
    eval.tier_i_describe_function(name)
        .unwrap_or_else(|| panic!("{name} is not compiled"))
}

#[test]
fn golden_scopes_slots_and_islands() {
    crate::test_utils::init_test_tracing();
    let mut eval = context(TierIMode::Analyze, 1);
    eval_ok(
        &mut eval,
        "(let ((internal-make-interpreted-closure-function nil))
           (defvar ti-a-dyn 0)
           (defmacro ti-a-mac (x) x)
           (defun ti-a1 (a &optional b &rest r)
             (let ((c (+ a 1)) (a b))
               (if c (setq a c ti-a-dyn a) (list a b r))
               (let* ((d a) (a d)) (cond ((eq a d) 'same) (t (ti-a-mac a))))
               (while nil)
               (catch 'k (unwind-protect (prog1 a b) (ignore c)))
               (condition-case e (car a) (error e))
               (lambda (z) (list z a))
               (ti-a-undefined a)))
           (ignore-errors (ti-a1 1)))",
    );
    assert_eq!(
        describe(&eval, "ti-a1"),
        "[a@0 b@1 r@2] (let [c@3=(call + a@0 1) a@4=b@1] \
         (if c@3 (setq a@4.0 c@3 ti-a-dyn a@4.0) (call list a@4.0 b@1 r@2)) \
         (let* [d@5=a@4.0 a@6=d@5] (cond ((call eq a@6.4.0 d@5) (handler 'same)) (t !(ti-a-mac a)))) \
         (while nil) \
         (catch (handler 'k) (unwind-protect (prog1 a@4.0 b@1) ...)) \
         (condition-case (call car a@4.0) ...) \
         !(lambda (z) (list z a)) \
         (call ti-a-undefined a@4.0))"
    );
    let stats = eval.tier_i.stats();
    assert_eq!(stats.count(TierIEvent::Run), 0, "analyze never runs code");
    assert!(stats.count(TierIEvent::Compiled) >= 1, "{}", stats.report());
}

#[test]
fn malformed_special_forms_are_left_to_the_tree_walker() {
    crate::test_utils::init_test_tracing();
    let mut eval = context(TierIMode::Analyze, 1);
    eval_ok(
        &mut eval,
        "(let ((internal-make-interpreted-closure-function nil))
           (defun ti-a2 (x)
             (condition-case nil (if x) (error nil))
             (condition-case nil (let ((a 1 2)) a) (error nil))
             (condition-case nil (setq x) (error nil))
             (condition-case nil (let* x) (error nil))
             ((lambda (y) y) x))
           (ti-a2 1))",
    );
    assert_eq!(
        describe(&eval, "ti-a2"),
        "[x@0] (condition-case !(if x) ...) (condition-case !(let ((a 1 2)) a) ...) \
         (condition-case !(setq x) ...) (condition-case !(let* x) ...) !((lambda (y) y) x)"
    );
}

#[test]
fn analyze_reports_coverage() {
    crate::test_utils::init_test_tracing();
    let mut eval = context(TierIMode::Analyze, 1);
    eval_ok(
        &mut eval,
        "(progn (defun ti-cov (x) (let ((y x)) (when y (list y x)))) (ti-cov 1))",
    );
    let report = eval.tier_i_report_lines(5).join("\n");
    assert!(
        report.contains("tier-i coverage (all compiled bodies): forms="),
        "{report}"
    );
    assert!(report.contains(" ti-cov forms="), "{report}");
}
