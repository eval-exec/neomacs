//! Complex combo batch 195 — `buffer` / `point` / `region` / `mark` /
//! `narrow` / `widen` / `save-excursion` / `save-restriction` / `goto-char`
//! / `forward-char` / `backward-char` / `line-beginning-position`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx195_point_and_motion_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 6 nil 6 0 7 7 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello\nWorld\nFoo")
  (goto-char 1)
  (list (point)
        (line-beginning-position)
        (line-end-position)
        (forward-char 5)
        (point)
        (forward-line 1)
        (point)
        (line-beginning-position)
        (line-end-position)))
"##,
        expect,
    );
}

#[test]
fn div_cx195_region_and_mark_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 10 3 nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (push-mark 3)
  (goto-char 10)
  (list (region-beginning)
        (region-end)
        (mark)
        (use-region-p)
        (region-active-p)
        (deactivate-mark)
        (region-active-p)))
"##,
        expect,
    );
}

#[test]
fn div_cx195_save_excursion_restores_point_and_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx195-se*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert "0123456789")
    (goto-char 3))
  (let ((origin-buffer (current-buffer)))
    (save-excursion
      (set-buffer buf)
      (goto-char 7)
      (insert "X"))
    (list (eq (current-buffer) origin-buffer)
          (with-current-buffer buf (buffer-string))
          (with-current-buffer buf (point))))
  (kill-buffer buf))
"##,
        expect,
    );
}

#[test]
fn div_cx195_save_restriction_restores_bounds() {
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
    );
}

#[test]
fn div_cx195_narrow_then_motion_respects_bounds() {
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
    );
}

#[test]
fn div_cx195_exchange_point_and_mark_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 2 2 8 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (push-mark 2)
  (goto-char 8)
  (let ((p-before (point))
        (m-before (mark)))
    (exchange-point-and-mark)
    (let ((p-after (point))
          (m-after (mark)))
      (list p-before m-before p-after m-after
            (= p-before m-after)
            (= m-before p-after)))))
"##,
        expect,
    );
}

#[test]
fn div_cx195_point_min_max_after_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 11 nil 3 7 nil 1 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (list (point-min) (point-max)
        (narrow-to-region 3 7)
        (point-min) (point-max)
        (widen)
        (point-min) (point-max)))
"##,
        expect,
    );
}

#[test]
fn div_cx195_count_lines_with_narrowing() {
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
    );
}

#[test]
fn div_cx195_what_line_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"Line 3\" 3 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\nline4\n")
  (goto-char 1)
  (forward-line 2)
  (list (what-line)
        (line-number-at-pos)
        (current-column)))
"##,
        expect,
    );
}

#[test]
fn div_cx195_motion_with_marker_overlay_undo_narrow_mega() {
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
    (let ((state (list (point) (mark)
                       (region-beginning) (region-end)
                       (point-min) (point-max)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (count-lines (point-min) (point))
                       (buffer-string)
                       (text-properties-at 1))))
      (save-excursion
        (save-restriction
          (widen)
          (goto-char 1)
          (insert "BEFORE")))
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
