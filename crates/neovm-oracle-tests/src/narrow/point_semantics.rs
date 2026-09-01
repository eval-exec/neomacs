//! Oracle parity tests for GNU narrowing and point-bound semantics.
//!
//! GNU implements point bounds and narrowing in `src/editfns.c`.  These tests
//! cover reversed bounds, point clamping, out-of-range errors, and marker values
//! for narrowed `point-min` / `point-max`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_narrow_to_region_swaps_reversed_bounds_and_widen_restores() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "0123456789")
  (goto-char 5)
  (let ((wide (list (point-min) (point-max) (point))))
    (narrow-to-region 8 3)
    (let ((narrowed (list (point-min) (point-max) (point)
                          (buffer-substring (point-min) (point-max)))))
      (widen)
      (list wide narrowed (point-min) (point-max) (point)))))
"#;

    let expect = expect_test::expect![[r#""OK ((1 11 5) (3 8 5 \"23456\") 1 11 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_narrow_to_region_clamps_point_to_new_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "0123456789")
  (let (before after)
    (goto-char 2)
    (narrow-to-region 4 8)
    (setq before (point))
    (widen)
    (goto-char 11)
    (narrow-to-region 4 8)
    (setq after (point))
    (list before after (point-min) (point-max))))
"#;

    let expect = expect_test::expect![[r#""OK (4 8 4 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_narrow_to_region_out_of_range_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abc")
  (list
   (condition-case err
       (narrow-to-region 0 2)
     (error (list (car err) (cdr err))))
   (condition-case err
       (narrow-to-region 1 99)
     (error (list (car err) (cdr err))))
   (condition-case err
       (narrow-to-region (copy-marker 2) (copy-marker 4))
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((args-out-of-range (0 2)) (args-out-of-range (1 99)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_point_min_max_markers_reflect_current_restriction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcdef")
  (narrow-to-region 3 6)
  (let ((min-marker (point-min-marker))
        (max-marker (point-max-marker)))
    (list
     (point-min)
     (point-max)
     (marker-position min-marker)
     (marker-position max-marker)
     (eq (marker-buffer min-marker) (current-buffer))
     (eq (marker-buffer max-marker) (current-buffer))
     (marker-insertion-type min-marker)
     (marker-insertion-type max-marker))))
"#;

    let expect = expect_test::expect![[r#""OK (3 6 3 6 t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_goto_char_clips_point_but_returns_requested_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "0123456789")
  (narrow-to-region 4 8)
  (let ((low-ret (goto-char -100))
        (low-point (point))
        (high-ret (goto-char 999))
        (high-point (point))
        (inside-ret (goto-char 6))
        (inside-point (point)))
    (list low-ret low-point high-ret high-point inside-ret inside-point
          (point-min) (point-max))))
"#;

    let expect = expect_test::expect![[r#""OK (-100 4 999 8 6 6 4 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_goto_char_marker_uses_marker_position_and_type_checks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "0123456789")
  (let ((m (copy-marker 6)))
    (narrow-to-region 4 8)
    (list
     (eq (goto-char m) m)
     (point)
     (progn
       (set-marker m 2)
       (eq (goto-char m) m))
     (point)
     (condition-case err
         (goto-char "not-position")
       (error (list (car err) (cdr err)))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t 6 t 4 (wrong-type-argument (integer-or-marker-p \"not-position\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
