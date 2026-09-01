//! Strict combo oracle probes, batch 284: alloc / gc / pure CORE variable
//! sweep (cons counters, gc thresholds). Any nil-in-Neomacs/t-in-GNU is a
//! missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cons_cells_floats_vectors_consed_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'cons-cells-consed)
      (boundp 'floats-consed)
      (boundp 'vector-cells-consed)
      (boundp 'symbols-consed)
      (boundp 'string-chars-consed)
      (boundp 'intervals-consed)
      (boundp 'misc-objects-consed)
      (boundp 'pure-bytes-used)
      (boundp 'gcs-done)
      (boundp 'gc-elapsed)
      (boundp 'memory-full)
      (boundp 'memory-signal-data))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_gc_threshold_percentage_messages_pure_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'gc-cons-threshold)
      (boundp 'gc-cons-percentage)
      (boundp 'garbage-collection-messages)
      (boundp 'purify-flag)
      (boundp 'max-specpdl-size)
      (boundp 'max-lisp-eval-depth)
      (boundp 'pre-redisplay-function)
      (boundp 'redisplay--pre-redisplay-functions)
      (boundp 'garbage-collect)
      (boundp 'memory-info)
      (boundp 'memory-use-counts)
      (boundp 'purecopy))
"##;
    let expect = expect_test::expect![[r#""OK (t t t t t t t nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_misc_alloc_pure_marker_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'mark)
      (boundp 'mark-marker)
      (boundp 'global-mark-ring)
      (boundp 'global-mark-ring-max)
      (boundp 'mark-ring-max)
      (boundp 'set-marker)
      (boundp 'copy-marker)
      (boundp 'marker-buffer)
      (boundp 'marker-position)
      (boundp 'marker-insertion-type)
      (boundp 'set-marker-insertion-type)
      (boundp 'marker-react-to-insertion))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil t t t nil nil nil nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
