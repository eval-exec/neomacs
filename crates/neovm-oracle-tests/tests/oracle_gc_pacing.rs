//! Serial automatic GC budgets: GNU alloc.c consing_threshold/maybe_gc.
use crate::common;

fn assert_pacing(form: &str, expected: expect_test::Expect) {
    // Measure ordinary pacing even when the runner profiles GC with overrides.
    common::assert_oracle_parity_with_env_expect(
        form,
        &[
            ("NEOVM_GC_STRESS", "0"),
            ("NEOVM_GC_THRESHOLD_CAP_BYTES", ""),
            ("NEOVM_GC_LIVE_GROWTH_PERCENT", ""),
        ],
        expected,
    );
}

#[test]
fn string_gc_pacing_retained_substrings() {
    // All large inputs are generated; the source itself is deliberately small.
    let form = r#"
(let ((gc-cons-threshold 800000) (gc-cons-percentage 1.0)
      (post-gc-hook nil) (source (make-string 87 ?x))
      (holder nil) (i 0) (before 0))
  (garbage-collect)
  (setq before gcs-done holder (make-vector 400000 nil))
  (while (< i 400000)
    (aset holder i (substring source 0 87))
    (setq i (1+ i)))
  (list (- gcs-done before) (length holder) (length (aref holder 0))))
"#;
    assert_pacing(form, expect_test::expect![[r#""OK (3 400000 87)""#]]);
}

#[test]
fn gcpace_settings_changes_adjust_pending_budget() {
    let form = r#"
(let ((gc-cons-threshold most-positive-fixnum) (gc-cons-percentage 0.0)
      (post-gc-hook nil) (retained (make-list 200000 nil)) (answers nil))
  (dolist (setting '(threshold percentage))
    (setq gc-cons-threshold (if (eq setting 'threshold) 1000000 80000)
          gc-cons-percentage (if (eq setting 'threshold) 0.0 1.0))
    (garbage-collect)
    (let ((before gcs-done))
      (make-list 6000 nil)
      (if (eq setting 'threshold)
          (setq gc-cons-threshold 80000)
        (setq gc-cons-percentage 0.0))
      (setq answers (cons (- gcs-done before) answers))))
  (list (nreverse answers) (length retained)))
"#;
    assert_pacing(form, expect_test::expect![[r#""OK ((1 1) 200000)""#]]);
}

#[test]
fn gcpace_threshold_floor_and_larger_budget() {
    let form = r#"
(let ((gc-cons-threshold most-positive-fixnum) (gc-cons-percentage 0.0)
      (post-gc-hook nil) (retained (make-list 200000 nil)) (answers nil))
  (dolist (threshold '(0 -1 80000 160000))
    (setq gc-cons-threshold threshold)
    (garbage-collect)
    (let ((before gcs-done) (i 0))
      (while (< i 4)
        (make-list 6000 nil)
        (setq i (1+ i)))
      (setq answers (cons (- gcs-done before) answers))))
  (list (nreverse answers) (length retained)))
"#;
    assert_pacing(form, expect_test::expect![[r#""OK ((4 4 4 2) 200000)""#]]);
}

#[test]
fn gcpace_threshold_counts_after_cons_batches() {
    let form = r#"
(let ((gc-cons-threshold most-positive-fixnum) (gc-cons-percentage 0.0)
      (post-gc-hook nil) (retained (make-list 200000 nil)))
  (setq gc-cons-threshold 80000)
  (garbage-collect)
  (let ((before gcs-done) (i 0))
    (while (< i 4)
      (make-list 6000 nil)
      (setq i (1+ i)))
    (list (- gcs-done before) (length retained))))
"#;
    assert_pacing(form, expect_test::expect![[r#""OK (4 200000)""#]]);
}

#[test]
fn gcpace_percentage_controls_cons_budget() {
    let form = r#"
(let ((gc-cons-threshold most-positive-fixnum) (gc-cons-percentage 0.0)
      (post-gc-hook nil) (retained (make-list 200000 nil)) (answers nil))
  (dolist (percentage '(0.0 1.0 1))
    (setq gc-cons-percentage percentage gc-cons-threshold 80000)
    (garbage-collect)
    (let ((before gcs-done) (i 0))
      (while (< i 4)
        (make-list 6000 nil)
        (setq i (1+ i)))
      (setq answers (cons (- gcs-done before) answers))))
  (list (nreverse answers) (length retained)))
"#;
    assert_pacing(form, expect_test::expect![[r#""OK ((4 0 4) 200000)""#]]);
}

#[test]
fn gcpace_percentage_float_edges() {
    let form = r#"
(let ((gc-cons-threshold most-positive-fixnum) (gc-cons-percentage 0.0)
      (post-gc-hook nil) (retained (make-list 200000 nil)) (answers nil))
  (dolist (percentage '(1.0e+INF 0.0e+NaN -0.5 nil))
    (setq gc-cons-percentage percentage gc-cons-threshold 80000)
    (garbage-collect)
    (let ((before gcs-done) (i 0))
      (while (< i 4)
        (make-list 6000 nil)
        (setq i (1+ i)))
      (setq answers (cons (- gcs-done before) answers))))
  (list (nreverse answers) (length retained)))
"#;
    assert_pacing(form, expect_test::expect![[r#""OK ((0 4 4 4) 200000)""#]]);
}

#[test]
fn gcpace_post_gc_hook_counts_automatic_cycles() {
    let form = r#"
(let ((gc-cons-threshold most-positive-fixnum) (gc-cons-percentage 0.0)
      (post-gc-hook nil) (retained (make-list 200000 nil)) (calls 0))
  (setq gc-cons-threshold 80000)
  (garbage-collect)
  (setq post-gc-hook (list (lambda () (setq calls (1+ calls)))))
  (let ((before gcs-done) (i 0))
    (while (< i 4)
      (make-list 6000 nil)
      (setq i (1+ i)))
    (list (- gcs-done before) calls (length retained))))
"#;
    assert_pacing(form, expect_test::expect![[r#""OK (4 4 200000)""#]]);
}
