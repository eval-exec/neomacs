//! Combo: setf on buffer positions + markers + overlays + textprop + undo.
//! Tests setf generalized variable interactions with buffer state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_setf_char_at_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ char-after\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "scu")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'seg 'a)
      (put-text-property 6 10 'seg 'b)
      (put-text-property 11 15 'seg 'c)
      (let* ((ov (make-overlay 6 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 8)))
        (undo-boundary)
        (setf (char-after 1) ?Z)
        (setf (char-after 6) ?Y)
        (undo-boundary)
        (let ((s (buffer-string))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (ka (get-text-property 1 'seg))
              (kb (get-text-property 6 'seg)))
          (primitive-undo 1 buffer-undo-list)
          (list s mp os oe ka kb
                (buffer-string)
                (marker-position m)
                (get-text-property 1 'seg)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_setf_marker_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "smt")))
    (with-current-buffer buf
      (insert "hello world test")
      (put-text-property 1 6 'w 'hello)
      (put-text-property 7 12 'w 'world)
      (put-text-property 13 17 'w 'test)
      (let* ((ov (make-overlay 7 12))
             (_ (overlay-put ov 'priority 5))
             (m (make-marker))
             (_ (set-marker m 9)))
        (undo-boundary)
        (setf (marker-position m) 14)
        (setf (overlay-start ov) 1)
        (setf (overlay-end ov) 17)
        (undo-boundary)
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property 7 'w))
              (s (buffer-string)))
          (primitive-undo 1 buffer-undo-list)
          (list mp os oe k s
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_setf_narrow_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ char-after\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "snu")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let* ((ov (make-overlay 1 15))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 8)))
        (narrow-to-region 6 10)
        (undo-boundary)
        (setf (char-after (point-min)) ?Z)
        (undo-boundary)
        (let ((s (buffer-substring (point-min) (point-max)))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property (point-min) 'zone)))
          (primitive-undo 1 buffer-undo-list)
          (widen)
          (list s mp os oe k
                (buffer-string)
                (marker-position m)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_setf_buffer_substring_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "sbs")))
    (with-current-buffer buf
      (insert "alpha-beta-gamma")
      (put-text-property 1 6 'p 'a)
      (put-text-property 7 11 'p 'b)
      (put-text-property 12 17 'p 'c)
      (let* ((ov (make-overlay 7 11))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 9)))
        (undo-boundary)
        (setf (buffer-substring 7 11) "BETA")
        (undo-boundary)
        (let ((s (buffer-string))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property 7 'p)))
          (primitive-undo 1 buffer-undo-list)
          (list s mp os oe k
                (buffer-string)
                (marker-position m)
                (overlay-start ov)
                (overlay-end ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_setf_textprop_overlay_clone() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "sto")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'kind 'a)
      (put-text-property 6 10 'kind 'b)
      (put-text-property 11 15 'kind 'c)
      (let* ((ov (make-overlay 1 10))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 6))
             (clone (clone-buffer "sto-clone")))
        (with-current-buffer clone
          (undo-boundary)
          (setf (char-after 1) ?Z)
          (setf (marker-position m) 11)
          (setf (overlay-end ov) 15)
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (s (buffer-string))
                (k (get-text-property 1 'kind)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe s k
                  (buffer-string)
                  (marker-position m)
                  (overlay-end ov)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}
