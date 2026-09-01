//! Complex combo batch 392 — `marker`/`undo`/`narrow`/`excursion` ultimate:
//! copy-marker, insertion-type, marker after kill-buffer, marker in different
//! buffers, undo-boundary, undo-list inspection, undo-amalgamating, narrow/
//! widen/save-restriction/save-excursion interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx392_marker_copy_insertion_type_kill_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 5 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let* ((m1 (set-marker (make-marker) 5))
         (m2 (copy-marker m1)))
    (set-marker m1 8)
    (list (marker-position m1) (marker-position m2) (eq m1 m2))))
"##,
        expect,
    )
}

#[test]
fn div_cx392_marker_insertion_type_front_rear() {
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
    (list (marker-insertion-type m-front) (marker-insertion-type m-rear)
          (marker-position m-front) (marker-position m-rear))))
"##,
        expect,
    )
}

#[test]
fn div_cx392_marker_after_kill_buffer_is_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx392-kill*")))
  (with-current-buffer buf (insert "content"))
  (let ((m (set-marker (make-marker) 3 buf)))
    (kill-buffer buf)
    (list (marker-buffer m) (marker-position m))))
"##,
        expect,
    )
}

#[test]
fn div_cx392_undo_list_capture_boundary_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"123\" \"1\" \"1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "1")
  (undo-boundary)
  (insert "2")
  (undo-boundary)
  (insert "3")
  (let ((after-3 (buffer-string)))
    (undo)
    (let ((after-undo-1 (buffer-string)))
      (undo)
      (let ((after-undo-2 (buffer-string)))
        (list after-3 after-undo-1 after-undo-2)))))
"##,
        expect,
    )
}

#[test]
fn div_cx392_undo_after_insert_delete_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"o world\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello world")
  (undo-boundary)
  (delete-region 1 5)
  (let ((before-undo (buffer-string)))
    (undo)
    (list before-undo (buffer-string))))
"##,
        expect,
    )
}

#[test]
fn div_cx392_undo_amalgamating_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (let ((before (length buffer-undo-list)))
        (undo-amalgamating-change
          (insert "a") (insert "b") (insert "c"))
        (list (> (length buffer-undo-list) before) (buffer-string))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx392_narrow_widen_point_min_max() {
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
fn div_cx392_save_restriction_and_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 10 5 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (narrow-to-region 5 10)
  (let ((narrowed-min (point-min)) (narrowed-max (point-max)))
    (save-restriction
      (widen)
      (list (point-min) (point-max)))
    (list narrowed-min narrowed-max (point-min) (point-max))))
"##,
        expect,
    )
}

#[test]
fn div_cx392_save_excursion_across_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx392-a*"))
      (buf-b (get-buffer-create " *neo-cx392-b*")))
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
fn div_cx392_marker_undo_narrow_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable m2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Marker/undo/narrow mega test buffer content here")
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
    (undo-boundary)
    (delete-region 5 13)
    (let ((state (list (marker-position m1) (marker-position m2) (marker-position m3)
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
