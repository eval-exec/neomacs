//! `memory-use-counts` counts one per object handed out, at the moment Lisp
//! reads it, and `garbage-collect-maybe` tests the exact bytes consed since
//! the last collection — whatever allocation regions the heap keeps open
//! behind them (the cons and float bump regions are charged whole when
//! granted and refunded at close; the Lisp-visible views subtract the
//! unused tail). Byte-compiled loops, so no interpreter environment conses
//! enter the delta; each delta includes the 7 conses of the first
//! `memory-use-counts` list, as GNU's does.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn memory_use_counts_cons_delta_is_exact_in_a_compiled_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(funcall
 (byte-compile
  (lambda ()
    (let* ((before (car (memory-use-counts)))
           (acc nil))
      (dotimes (i 1000) (setq acc (cons i acc)))
      (list (length acc) (- (car (memory-use-counts)) before))))))
"##;
    let expect = expect_test::expect![[r#""OK (1000 1007)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn memory_use_counts_cons_delta_is_exact_across_region_sizes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(funcall
 (byte-compile
  (lambda ()
    (mapcar (lambda (n)
              (let* ((before (car (memory-use-counts)))
                     (acc nil))
                (dotimes (i n) (setq acc (cons i acc)))
                (- (car (memory-use-counts)) before)))
            '(0 1 127 128 129 1000)))))
"##;
    let expect = expect_test::expect![[r#""OK (7 8 134 135 136 1007)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn memory_use_counts_float_delta_is_exact_in_a_compiled_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(funcall
 (byte-compile
  (lambda ()
    (let* ((before (nth 1 (memory-use-counts)))
           (f 1.0)
           (keep nil))
      (dotimes (i 1000)
        (setq f (* f 1.0001))
        (when (= (% i 100) 0) (setq keep (cons f keep))))
      (list (length keep) (- (nth 1 (memory-use-counts)) before))))))
"##;
    let expect = expect_test::expect![[r#""OK (10 1000)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn garbage_collect_maybe_sees_only_what_was_consed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (garbage-collect)
  (cons 1 2)
  (list (garbage-collect-maybe 1) (garbage-collect-maybe 0)))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
