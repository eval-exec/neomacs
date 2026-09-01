//! Complex combo batch 116 — `completion-at-point-functions` / `capf`
//! behavior with various sources, `completion-extra-properties`, and
//! metadata-driven completions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx116_completion_extra_properties_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((closure (t) (s) (format \" [%s]\" (length s))) (closure (t) (s status) (message \"exiting %s\" s)) no (:annotation-function (closure (t) (s) (format \" [%s]\" (length s))) :exit-function (closure (t) (s status) (message \"exiting %s\" s)) :display-sort-function (closure (t) (comps) (sort comps #'string<)) :company-kind (closure (t) (s) 'text) :exclusive no))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((props (list :annotation-function (lambda (s) (format " [%s]" (length s)))
                       :exit-function (lambda (s status) (message "exiting %s" s))
                       :display-sort-function (lambda (comps) (sort comps #'string<))
                       :company-kind (lambda (s) 'text)
                       :exclusive 'no)))
      (list (plist-get props :annotation-function)
            (plist-get props :exit-function)
            (plist-get props :exclusive)
            (plist-member props :annotation-function)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_in_region_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"alpha\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "alpha")
      (let ((completion-at-point-functions
             (list (lambda ()
                     (list (point-min) (point)
                           '("alpha" "alphabet" "alpine" "amplitude")
                           :exclusive t)))))
        (list (completion-at-point)
              (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_styles_basic_partial_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"alp\" (\"alpha\" \"alphabet\" \"alpine\") nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("alpha" "alphabet" "alpine" "amplitude" "antelope"))
      (completion-styles '(basic partial-completion substring)))
  (list (try-completion "al" coll)
        (all-completions "al" coll)
        (try-completion "apt" coll)
        (all-completions "apt" coll)))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_flex_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((coll '("alpha" "alphabet" "alpine" "amplitude"))
          (completion-styles '(flex)))
      (list (try-completion "ap" coll)
            (all-completions "ap" coll)
            (try-completion "aht" coll)
            (all-completions "aht" coll)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_metadata_format_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((md (completion-metadata "al" '("alpha" "alphabet" "alpine")
                                    '(category string))))
      (list (plist-get md :category)
            (plist-get md :annotation-function)
            (plist-get md :display-sort-function)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_all_completions_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((coll '("alpha" "alphabet" "alpine"))
           (result (completion-all-completions "al" coll nil 2)))
      (list (consp result)
            (> (length result) 0)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_all_sorted_completions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((coll '("alpha" "alphabet" "alpine" "amplitude" "antelope")))
      (list (completion-all-sorted-completions 1 3 coll nil)
            (try-completion "al" coll)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_escape_callback_invoke() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (called)
      (let ((completion-wrap-exit-function
             (lambda (s status) (push (cons s status) called))))
        (list called)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_table_dynamic_with_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alp\" (\"alpha\" \"alphabet\" \"alpine\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dynamic-table
       (completion-table-dynamic
        (lambda (str)
          (all-completions str '("alpha" "alphabet" "alpine"))))))
  (list (try-completion "al" dynamic-table)
        (all-completions "al" dynamic-table)))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_table_in_turn_combines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alpha\" \"be\" 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((combined (completion-table-in-turn
                     '("alpha" "alphabet")
                     '("beta" "berry"))))
      (list (try-completion "a" combined)
            (try-completion "b" combined)
            (length (all-completions "" combined))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_table_with_quoting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((combined (completion-table-merge
                     '("alpha" "alphabet")
                     '("amplitude" "antelope"))))
      (list (try-completion "a" combined)
            (length (all-completions "a" combined))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx116_completion_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((coll '("alpha" "alphabet" "alpine" "amplitude" "antelope")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "capf mega test buffer content")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (try-completion "al" coll)
                         (all-completions "al" coll)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
