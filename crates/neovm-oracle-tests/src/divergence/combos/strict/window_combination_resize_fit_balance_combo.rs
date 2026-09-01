//! Strict combo oracle probes, batch 142: window combination limit + resize,
//! fit-window-to-buffer, balance-windows geometry, and buffer-local
//! variable inheritance with make-variable-frame-local.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v6_window_combination_limit_resize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((b (get-buffer-create " *probe-wcl*")))
  (unwind-protect
      (progn
        (delete-other-windows)
        (switch-to-buffer b)
        (let ((w2 (split-window nil nil 'below)))
          (set-window-parameter w2 'combination-limit 'window-size)
          (let ((w3 (split-window w2 nil 'below)))
            (list (count-windows)
                  (window-combination-limit w2)
                  (window-total-height)
                  (window-total-height w2)
                  (window-total-height w3)
                  (condition-case err
                      (window-resize w3 -3 nil nil nil)
                    (error 'err))
                  (window-total-height w2)
                  (window-total-height w3)))))
    (kill-buffer b)
    (delete-other-windows)))
"##;
    let expect = expect_test::expect![[
        r#""ERR (error \"Combination limit is meaningful for internal windows only\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v6_buffer_local_var_frame_local_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(progn
  (defvar probe-blfi 'global)
  (make-variable-buffer-local 'probe-blfi)
  (let ((b1 (generate-new-buffer " *probe-blfi-1*"))
        (b2 (generate-new-buffer " *probe-blfi-2*")))
    (unwind-protect
        (progn
          (with-current-buffer b1 (setq probe-blfi 'buf1))
          (with-current-buffer b2 (setq probe-blfi 'buf2))
          (list (default-value 'probe-blfi)
                (buffer-local-value 'probe-blfi b1)
                (buffer-local-value 'probe-blfi b2)
                (local-variable-p 'probe-blfi b1)
                (local-variable-p 'probe-blfi b2)
                (with-current-buffer b1 (default-value 'probe-blfi))
                (setq-default probe-blfi 'changed)
                (buffer-local-value 'probe-blfi b1)
                (buffer-local-value 'probe-blfi b2)))
      (kill-buffer b1)
      (kill-buffer b2))))
"##;
    let expect = expect_test::expect![[r#""OK (global buf1 buf2 t t global changed buf1 buf2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v6_read_from_string_partial_and_trailing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (read-from-string "42 extra")
      (read-from-string "(a b c) trailing")
      (read-from-string "\"string\" junk")
      (read-from-string "  whitespace-then-sym")
      (multiple-value-bind (val pos) (read-from-string "(x) (y) (z)")
        (list val pos))
      (let ((s "#(1 2 3) extra"))
        (multiple-value-bind (val pos) (read-from-string s)
          (list val pos (substring s pos))))
      (condition-case err (read-from-string "") (end-of-file 'caught)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v6_text_property_stickiness_edge_with_deletion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "AAABBBCCC")
  (add-text-properties 1 4 '(face bold rear-nonsticky nil))
  (add-text-properties 4 7 '(face italic front-sticky nil))
  (add-text-properties 7 9 '(face underline))
  (let ((before (list (text-properties-at 3)
                      (text-properties-at 4)
                      (text-properties-at 6)
                      (text-properties-at 7))))
    (delete-region 4 7)
    (list before
          (buffer-string)
          (text-properties-at 3)
          (text-properties-at 4)
          (text-properties-at 5))))
"##;
    let expect = expect_test::expect![[
        r#""OK (((rear-nonsticky nil face bold) (front-sticky nil face italic) (front-sticky nil face italic) (face underline)) #(\"AAACCC\" 0 3 (rear-nonsticky nil face bold) 3 5 (face underline)) (rear-nonsticky nil face bold) (face underline) (face underline))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v6_cl_loop_maximize_minimize_into_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (cl-loop for x in '(3 1 4 1 5 9 2 6)
               maximize x into max-v
               minimize x into min-v
               count (= (% x 2) 0) into even-count
               sum x into total
               finally (return (list max-v min-v even-count total)))
      (cl-loop for x in '(1 2 3 4 5)
               for y = (* x 10)
               when (> y 25)
                 collect y
                 and sum y into big-sum
               end
               finally (return (list big-sum)))
      (cl-loop for i below 5
               for c = (char-to-string (+ 65 i))
               concat c into result
               finally (return result)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
