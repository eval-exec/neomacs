//! Combo: read-only text property + markers + overlays + undo + narrow.
//! Tests read-only text property interactions with buffer modifications.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_readonly_textprop_marker_overlay_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rtn")))
    (with-current-buffer buf
      (insert "editable-READONLY-editable")
      (put-text-property 1 9 'zone 'edit)
      (put-text-property 9 17 'zone 'ro)
      (put-text-property 9 17 'read-only t)
      (put-text-property 18 26 'zone 'edit2)
      (let* ((ov (make-overlay 1 26))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 12)))
        (narrow-to-region 1 26)
        (goto-char 9)
        (let ((inhibit-read-only t))
          (insert "XX"))
        (let ((mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k1 (get-text-property 1 'zone))
              (k2 (get-text-property 11 'zone))
              (bs (buffer-string)))
          (widen)
          (list mp os oe k1 k2 bs
                (marker-position m)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_readonly_undo_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rou")))
    (with-current-buffer buf
      (insert "AAA-BBB-CCC")
      (put-text-property 1 4 'zone 'a)
      (put-text-property 5 8 'zone 'b)
      (put-text-property 5 8 'read-only t)
      (put-text-property 9 12 'zone 'c)
      (let* ((ov (make-overlay 5 8))
             (_ (overlay-put ov 'face 'region))
             (m (make-marker))
             (_ (set-marker m 6)))
        (undo-boundary)
        (let ((inhibit-read-only t))
          (goto-char 5)
          (insert "XX-"))
        (undo-boundary)
        (let ((s (buffer-string))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k (get-text-property 5 'zone)))
          (primitive-undo 1 buffer-undo-list)
          (list s mp os oe k
                (buffer-string)
                (marker-position m)
                (overlay-start ov)))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_readonly_narrow_overlay_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rno")))
    (with-current-buffer buf
      (insert "OPEN-LOCKED-OPEN")
      (put-text-property 1 5 'part 'open1)
      (put-text-property 6 12 'part 'locked)
      (put-text-property 6 12 'read-only t)
      (put-text-property 13 17 'part 'open2)
      (let* ((ov (make-overlay 6 12))
             (_ (overlay-put ov 'priority 10))
             (m (make-marker))
             (_ (set-marker m 9)))
        (narrow-to-region 1 17)
        (goto-char (point-min))
        (forward-word)
        (let ((p1 (point))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov))
              (k1 (get-text-property 1 'part))
              (k2 (get-text-property 6 'part))
              (ro (get-text-property 6 'read-only)))
          (widen)
          (list p1 mp os oe k1 k2 ro))))
    (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_readonly_clone_marker_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rcu")))
    (with-current-buffer buf
      (insert "abc-DEF-ghi")
      (put-text-property 1 4 'kind 'low)
      (put-text-property 5 8 'kind 'up)
      (put-text-property 5 8 'read-only t)
      (put-text-property 9 12 'kind 'low2)
      (let* ((ov (make-overlay 1 12))
             (_ (overlay-put ov 'face 'highlight))
             (m (make-marker))
             (_ (set-marker m 6))
             (clone (clone-buffer "rcu-clone")))
        (with-current-buffer clone
          (let ((inhibit-read-only t))
            (undo-boundary)
            (goto-char 5)
            (insert "XX-"))
          (undo-boundary)
          (let ((mp (marker-position m))
                (os (overlay-start ov))
                (oe (overlay-end ov))
                (s (buffer-string)))
            (primitive-undo 1 buffer-undo-list)
            (list mp os oe s
                  (buffer-string)
                  (marker-position m)))))
      (kill-buffer clone)
      (kill-buffer buf)))"#,
        expect,
    );
}

#[test]
fn combo_readonly_multiple_zones_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer "rmz")))
    (with-current-buffer buf
      (insert "AB-CD-EF-GH")
      (put-text-property 1 3 'z 'a)
      (put-text-property 4 6 'z 'b)
      (put-text-property 4 6 'read-only t)
      (put-text-property 7 9 'z 'c)
      (put-text-property 7 9 'read-only t)
      (put-text-property 10 12 'z 'd)
      (let* ((ov (make-overlay 4 9))
             (_ (overlay-put ov 'face 'bold))
             (m (make-marker))
             (_ (set-marker m 5)))
        (undo-boundary)
        (let ((inhibit-read-only t))
          (goto-char 4)
          (insert "XX")
          (goto-char 9)
          (insert "YY"))
        (undo-boundary)
        (let ((s (buffer-string))
              (mp (marker-position m))
              (os (overlay-start ov))
              (oe (overlay-end ov)))
          (primitive-undo 1 buffer-undo-list)
          (list s mp os oe
                (buffer-string)
                (marker-position m)))))
    (kill-buffer buf)))"#,
        expect,
    );
}
