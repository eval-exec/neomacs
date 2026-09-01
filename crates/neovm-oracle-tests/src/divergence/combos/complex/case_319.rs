//! Complex combo batch 319 — `marker` ultimate: copy-marker independence,
//! marker-insertion-type, marker after kill-buffer, marker in different
//! buffers, multiple markers tracking same insert.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx319_marker_copy_independence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 5 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let* ((m1 (set-marker (make-marker) 5))
         (m2 (copy-marker m1)))
    (set-marker m1 8)
    (list (marker-position m1)
          (marker-position m2)
          (eq m1 m2))))
"##,
        expect,
    )
}

#[test]
fn div_cx319_marker_insertion_type_front_vs_rear() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil 6 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((m-front (set-marker (make-marker) 5))
        (m-rear (set-marker (make-marker) 5)))
    (set-marker-insertion-type m-front t)
    (set-marker-insertion-type m-rear nil)
    (goto-char 5)
    (insert "X")
    (list (marker-insertion-type m-front)
          (marker-insertion-type m-rear)
          (marker-position m-front)
          (marker-position m-rear))))
"##,
        expect,
    )
}

#[test]
fn div_cx319_marker_adjusts_on_insert_and_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 5 \"0123456789\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((m-ins (set-marker (make-marker) 5)))
    (goto-char 3)
    (insert "XXX")
    (let ((after-ins (marker-position m-ins)))
      (delete-region 3 6)
      (list after-ins (marker-position m-ins) (buffer-string)))))
"##,
        expect,
    )
}

#[test]
fn div_cx319_marker_after_kill_buffer_is_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx319-kill*")))
  (with-current-buffer buf (insert "content"))
  (let ((m (set-marker (make-marker) 3 buf)))
    (kill-buffer buf)
    (list (marker-buffer m) (marker-position m))))
"##,
        expect,
    )
}

#[test]
fn div_cx319_marker_in_different_buffer_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx319-mk-a*"))
      (buf-b (get-buffer-create " *neo-cx319-mk-b*")))
  (with-current-buffer buf-a (insert "AAAA"))
  (with-current-buffer buf-b (insert "BBBB"))
  (let ((m (set-marker (make-marker) 2 buf-a)))
    (set-marker m 3 buf-b)
    (let ((pos (marker-position m))
          (buf (marker-buffer m)))
      (kill-buffer buf-a)
      (kill-buffer buf-b)
      (list pos (eq buf buf-b) (marker-buffer m))))
"##,
        expect,
    )
}

#[test]
fn div_cx319_multiple_markers_track_same_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 8 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((m1 (set-marker (make-marker) 3))
        (m2 (set-marker (make-marker) 5))
        (m3 (set-marker (make-marker) 7)))
    (goto-char 4)
    (insert "XXX")
    (list (marker-position m1)
          (marker-position m2)
          (marker-position m3))))
"##,
        expect,
    )
}

#[test]
fn div_cx319_marker_buffer_narrowing_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 5 20 \"EFGHIJKLMNOPQRS\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
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
fn div_cx319_marker_set_to_zero_and_one() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "test")
  (let ((m1 (set-marker (make-marker) 0))
        (m2 (set-marker (make-marker) 1)))
    (list (marker-position m1)
          (marker-position m2)
          (markerp m1)
          (markerp m2))))
"##,
        expect,
    )
}

#[test]
fn div_cx319_marker_creation_and_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 5 8 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((m1 (set-marker (make-marker) 5))
        (m2 (set-marker (make-marker) 8 (current-buffer))))
    (list (markerp m1)
          (marker-position m1)
          (marker-position m2)
          (eq (marker-buffer m1) (current-buffer))
          (eq (marker-buffer m2) (current-buffer)))))
"##,
        expect,
    )
}

#[test]
fn div_cx319_marker_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable m2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Marker mega test buffer content here")
  (put-text-property 1 6 'face 'bold)
  (let ((m1 (set-marker (make-marker) 8))
        (m2 (set-marker (make-marker) 15))
        (m3 (copy-marker m2))
        (ov (make-overlay 4 20)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 2 25)
    (goto-char 5)
    (insert "INSERTED")
    (let ((state (list (marker-position m1)
                       (marker-position m2)
                       (marker-position m3)
                       (eq (marker-buffer m1) (current-buffer))
                       (buffer-string)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1))))
      (undo)
      (widen()
      (list state (buffer-string)
            (marker-position m1) (marker-position m2) (marker-position m3)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))))
"##,
        expect,
    )
}
