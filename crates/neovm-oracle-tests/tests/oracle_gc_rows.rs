//! GNU alloc.c returns the completed sweep's owned-object snapshot.
use crate::common;

fn assert_rows(form: &str, expected: expect_test::Expect) {
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
fn gc_rows_startup_shapes_and_owned_allocation_deltas() {
    // Large ordinary GNU vectors contribute length + one header word;
    // string-bytes counts the logical 87 bytes, without sdata padding.
    let form = r#"
(let ((gc-cons-threshold most-positive-fixnum) (gc-cons-percentage 0.0)
      (post-gc-hook nil) (holder (make-vector 6 nil))
      (before nil) (after nil))
  (garbage-collect)
  (setq before (garbage-collect))
  (aset holder 0 (make-vector 10000 nil))
  (aset holder 1 (make-vector 10001 nil))
  (aset holder 2 (make-vector 10002 nil))
  (aset holder 3 (make-string 87 ?x))
  (aset holder 4 (make-string 87 ?x))
  (aset holder 5 (make-string 87 ?x))
  (setq after (garbage-collect))
  (list
   (mapcar #'length before)
   (list
    (- (nth 2 (assq 'vectors after)) (nth 2 (assq 'vectors before)))
    (- (nth 2 (assq 'vector-slots after)) (nth 2 (assq 'vector-slots before)))
    (- (nth 2 (assq 'strings after)) (nth 2 (assq 'strings before)))
    (- (nth 2 (assq 'string-bytes after)) (nth 2 (assq 'string-bytes before))))))
"#;
    assert_rows(
        form,
        expect_test::expect![[r#""OK ((4 4 4 3 3 4 4 4 3) (3 30006 3 261))""#]],
    );
}

#[test]
fn gc_rows_post_hook_symbol_appears_in_the_next_completed_snapshot() {
    let form = r#"
(let ((gc-cons-threshold most-positive-fixnum) (gc-cons-percentage 0.0)
      (post-gc-hook nil) (created nil) (baseline 0) (current 0) (next 0))
  ;; Warm every row label before taking the symbol baseline.
  (garbage-collect)
  (setq baseline (nth 2 (assq 'symbols (garbage-collect))))
  (setq post-gc-hook
        (list (lambda ()
                (setq created (intern "b68-gc-rows-hook-created-7e843f23")))))
  (setq current (nth 2 (assq 'symbols (garbage-collect))))
  (setq next (nth 2 (assq 'symbols (garbage-collect))))
  (list (- current baseline) (- next baseline)))
"#;
    assert_rows(form, expect_test::expect![[r#""OK (0 1)""#]]);
}
