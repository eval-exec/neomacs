//! Strict combo oracle probes, batch 143: map-merge/into over hash tables,
//! text-property-remove with overlapping ranges, alist-get with default,
//! setq-local + default-value + kill-local-variable combo, and
//! process-adaptive-read-buffering with large output.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v7_map_merge_into_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((h1 (make-hash-table :test 'equal))
      (h2 (make-hash-table :test 'equal)))
  (puthash 'a 1 h1)
  (puthash 'b 2 h1)
  (puthash 'b 3 h2)
  (puthash 'c 4 h2)
  (let ((merged (map-merge h1 h2)))
    (list (map-elt merged 'a)
          (map-elt merged 'b)
          (map-elt merged 'c)
          (hash-table-count merged)
          (map-elt h1 'b)
          (map-elt h2 'a))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function map-merge)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v7_text_property_remove_overlapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "0123456789")
  (add-text-properties 1 6 '(face bold weight heavy))
  (add-text-properties 3 8 '(face italic color red))
  (let ((before (list (get-text-property 3 'face)
                      (get-text-property 3 'weight)
                      (get-text-property 3 'color)
                      (text-properties-at 3))))
    (remove-text-properties 3 8 '(face))
    (let ((after-remove (list (get-text-property 3 'face)
                              (get-text-property 3 'weight)
                              (get-text-property 3 'color)
                              (text-properties-at 3))))
      (remove-list-of-text-properties 3 8 '(weight color))
      (let ((after-remove-list (list (get-text-property 3 'face)
                                     (get-text-property 3 'weight)
                                     (get-text-property 3 'color)
                                     (text-properties-at 3))))
        (list before after-remove after-remove-list)))))
"##;
    let expect = expect_test::expect![[
        r#""OK ((italic heavy red (color red)) (nil heavy red (color red)) (nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v7_alist_get_default_and_assoc_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((al '(("a" . 1) ("b" . 2) (c . 3))))
  (list (alist-get "a" al)
        (alist-get "a" al nil nil #'equal)
        (alist-get "z" al)
        (alist-get "z" al 'default)
        (alist-get 'c al)
        (alist-get 'c al nil nil #'eq)
        (alist-get 'd al 'missing)
        (setf (alist-get 'd al) 4)
        (alist-get 'd al)
        (length al)))
"##;
    let expect = expect_test::expect![[r#""OK (nil 1 nil default 3 3 missing 4 4 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v7_setq_local_default_kill_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (generate-new-buffer " *probe-sldk*")))
  (unwind-protect
      (with-current-buffer b
        (defvar probe-sldk-var 'global)
        (make-variable-buffer-local 'probe-sldk-var)
        (list (default-value 'probe-sldk-var)
              probe-sldk-var
              (local-variable-p 'probe-sldk-var)
              (progn (setq-local probe-sldk-var 'local)
                     probe-sldk-var)
              (default-value 'probe-sldk-var)
              (progn (setq-default probe-sldk-var 'new-default)
                     (default-value 'probe-sldk-var))
              probe-sldk-var
              (progn (kill-local-variable 'probe-sldk-var)
                     probe-sldk-var)
              (local-variable-p 'probe-sldk-var)
              (default-value 'probe-sldk-var)))
    (kill-buffer b)))
"##;
    let expect = expect_test::expect![[
        r#""OK (global global nil local global new-default local new-default nil new-default)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v7_process_large_output_buffering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((buf (generate-new-buffer " *probe-plo*")))
  (let ((proc (make-process :name "probe-plo"
                            :command (list shell-file-name shell-command-switch
                                           "seq 1 100")
                            :buffer buf
                            :sentinel (lambda (&rest _) nil))))
    (set-process-query-on-exit-flag proc nil)
    (accept-process-output proc 1)
    (accept-process-output proc 1)
    (let ((lines (with-current-buffer buf (count-lines (point-min) (point-max))))
      (kill-buffer buf)
      lines)))
"##;
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
