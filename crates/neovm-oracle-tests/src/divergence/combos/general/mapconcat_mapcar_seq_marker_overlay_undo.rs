//! Deep combo: mapconcat × mapcar × mapc × seq-map × seq-filter ×
//! seq-reduce × marker × overlay × textprop × undo × buffer-local × narrow.
//!
//! Stresses sequence operations with buffer state: mapping over sequences,
//! filtering, reducing, and concatenating results while preserving
//! markers, overlays, text properties, and undo. Sequence operations
//! are tricky because they involve callback execution that must interact
//! correctly with the buffer's edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_mapconcat_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((result (mapconcat 'upcase '("aaa" "bbb" "ccc") "-")))
          (goto-char 5)
          (insert (format "-<%s>-" result))
          (let ((after (list (buffer-string)
                             result
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'zone)
                             (get-text-property 6 'zone))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone)
                                  (get-text-property 11 'zone))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_mapcar_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mapc")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let ((result (mapcar (lambda (x) (* x 2)) '(1 2 3 4 5))))
          (goto-char 5)
          (insert (format "-<%S>-" result))
          (let ((after (list (buffer-string)
                             result
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'grp)
                             (get-text-property 6 'grp)
                             (get-text-property 12 'grp))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'grp)
                                  (get-text-property 6 'grp)
                                  (get-text-property 11 'grp)
                                  (get-text-property 16 'grp))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_seq_filter_reduce_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-sfr")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (let* ((filtered (seq-filter 'oddp '(1 2 3 4 5 6 7)))
               (reduced (seq-reduce (lambda (acc x) (+ acc x)) filtered 0))
               (mapped (seq-map (lambda (x) (* x x)) filtered)))
          (goto-char 5)
          (insert (format "-<%d:%S>-" reduced mapped))
          (let ((after (list (buffer-string)
                             filtered reduced mapped
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'zone)
                             (get-text-property 6 'zone))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'zone)
                                  (get-text-property 6 'zone)
                                  (get-text-property 11 'zone))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_mapcar_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mcnar")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 6 20)))
        (overlay-put ov 'zone 'middle)
        (undo-boundary)
        (narrow-to-region 6 20)
        (let ((result (mapcar 'upcase '("xx" "yy" "zz"))))
          (goto-char (point-min))
          (insert (format "%S-" result))
          (widen)
          (let ((after (list (buffer-string)
                             result
                             (marker-position m1)
                             (marker-position m2)
                             (overlay-start ov) (overlay-end ov)
                             (get-text-property 1 'sect)
                             (get-text-property 6 'sect)
                             (get-text-property 16 'sect)
                             (get-text-property 21 'sect))))
            (primitive-undo 1 buffer-undo-list)
            (let ((restored (list (buffer-string)
                                  (marker-position m1)
                                  (marker-position m2)
                                  (overlay-start ov) (overlay-end ov)
                                  (get-text-property 1 'sect)
                                  (get-text-property 6 'sect)
                                  (get-text-property 11 'sect)
                                  (get-text-property 16 'sect)
                                  (get-text-property 21 'sect))))
              (kill-buffer buf)
              (list after restored)))))))) "#,
        expect,
    );
}

#[test]
fn combo_mapc_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-mcbl"))
        (acc nil))
    (with-current-buffer buf
      (make-local-variable 'mapc-local)
      (setq mapc-local 'buffer-specific)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (undo-boundary)
        (mapc (lambda (x)
                (with-current-buffer buf
                  (push (format "%s:%S" x mapc-local) acc)))
              '(a b c))
        (setq acc (nreverse acc))
        (goto-char 5)
        (insert (format "-<%S>-" acc))
        (let ((after (list (buffer-string)
                           acc
                           mapc-local
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                mapc-local
                                (marker-position m1)
                                (marker-position m2)
                                (overlay-start ov) (overlay-end ov)
                                (get-text-property 1 'zone)
                                (get-text-property 6 'zone)
                                (get-text-property 11 'zone))))
            (kill-buffer buf)
            (list after restored))))))) "#,
        expect,
    );
}
