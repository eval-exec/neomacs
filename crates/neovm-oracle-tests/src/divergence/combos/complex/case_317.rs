//! Complex combo batch 317 — `narrow`/`widen`/`save-restriction`/
//! `save-excursion`/`point-min`/`point-max` interactions with markers,
//! overlays, and undo across multiple buffers.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx317_narrow_widen_point_min_max_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 27 nil 5 20 nil 1 27)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
  (list (point-min) (point-max)
        (narrow-to-region 5 20)
        (point-min) (point-max)
        (widen)
        (point-min) (point-max)))
"##,
        expect,
    )
}

#[test]
fn div_cx317_save_restriction_restores_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 10 5 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (narrow-to-region 5 10)
  (let ((narrowed-min (point-min))
        (narrowed-max (point-max)))
    (save-restriction
      (widen)
      (list (point-min) (point-max)))
    (list narrowed-min narrowed-max
          (point-min) (point-max))))
"##,
        expect,
    )
}

#[test]
fn div_cx317_narrow_then_motion_respects_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
  (narrow-to-region 5 20)
  (goto-char (point-min))
  (list (point)
        (point-min) (point-max)
        (forward-char 100)
        (point)
        (goto-char (point-min))
        (forward-line 0)
        (point)))
"##,
        expect,
    )
}

#[test]
fn div_cx317_narrow_with_marker_position_relative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 5 20 \"456789ABCDEF012\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF0123456789")
  (let ((m (set-marker (make-marker) 15)))
    (narrow-to-region 5 20)
    (list (marker-position m)
          (point-min) (point-max)
          (buffer-substring (point-min) (point-max)))))
"##,
        expect,
    )
}

#[test]
fn div_cx317_save_excursion_across_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx317-a*"))
      (buf-b (get-buffer-create " *neo-cx317-b*")))
  (with-current-buffer buf-a (insert "AAAA") (goto-char 3))
  (with-current-buffer buf-b (insert "BBBB") (goto-char 2))
  (let ((origin (current-buffer)))
    (save-excursion
      (set-buffer buf-a)
      (goto-char 1)
      (insert "X"))
    (list (eq (current-buffer) origin)
          (with-current-buffer buf-a (buffer-string))
          (with-current-buffer buf-a (point))))
  (kill-buffer buf-a)
  (kill-buffer buf-b))
"##,
        expect,
    )
}

#[test]
fn div_cx317_exchange_point_and_mark_in_narrowed_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 2 2 8 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (push-mark 2)
  (goto-char 8)
  (let ((p-before (point)) (m-before (mark)))
    (exchange-point-and-mark)
    (list p-before m-before (point) (mark)
          (= p-before (mark))
          (= m-before (point)))))
"##,
        expect,
    )
}

#[test]
fn div_cx317_save_excursion_persists_point_across_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (goto-char 5)
  (let ((p-before (point)))
    (save-excursion
      (goto-char 1)
      (insert "X")
      (undo))
    (list p-before (point) (buffer-string))))
"##,
        expect,
    )
}

#[test]
fn div_cx317_count_lines_with_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 nil 2 nil 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\nline4\nline5\n")
  (list (count-lines (point-min) (point-max))
        (narrow-to-region 1 13)
        (count-lines (point-min) (point-max))
        (widen)
        (count-lines (point-min) (point-max))))
"##,
        expect,
    )
}

#[test]
fn div_cx317_what_line_and_line_number_at_pos() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Line 3\" 3 0 13 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\nline4\n")
  (goto-char 1)
  (forward-line 2)
  (list (what-line)
        (line-number-at-pos)
        (current-column)
        (line-beginning-position)
        (line-end-position)))
"##,
        expect,
    )
}

#[test]
fn div_cx317_narrow_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 18))
        (ov (make-overlay 4 14)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (push-mark 3)
    (goto-char 25)
    (narrow-to-region 5 28)
    (save-excursion
      (save-restriction
        (widen)
        (goto-char 1)
        (insert "BEFORE")))
    (let ((state (list (point) (mark)
                       (region-beginning) (region-end)
                       (point-min) (point-max)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (count-lines (point-min) (point))
                       (buffer-string)
                       (text-properties-at 1))))
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    )
}
