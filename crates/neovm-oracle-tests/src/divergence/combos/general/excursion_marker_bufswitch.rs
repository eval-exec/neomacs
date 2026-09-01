//! Divergence tests: save-excursion + marker + buffer-switch + undo combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_save_excursion_marker_after_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (28 nil 13 t #(\"AAAAXXX-BBBB-CCCC-DDDD-EEEE\" 0 3 (zone a) 12 16 (zone c)) a t c t nil 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((m (copy-marker 10 t)))
    (put-text-property 1 4 'zone 'a)
    (put-text-property 10 14 'zone 'c)
    (save-excursion
      (goto-char 5)
      (insert "XXX")
      (overlay-put (make-overlay 5 8) 'inserted t))
    (list (point)
          (= (point) 1)
          (marker-position m)
          (> (marker-position m) 10)
          (buffer-string)
          (get-text-property 1 'zone)
          (eq (get-text-property 1 'zone) 'a)
          (get-text-property 13 'zone)
          (eq (get-text-property 13 'zone) 'c)
          (= (buffer-size) 28)
          (length (overlays-in 5 8))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_switch_with_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (current-buffer))
        (buf2 (generate-new-buffer " test-bs-xxx")))
    (with-current-buffer buf1
      (insert "BUF1-CONTENT")
      (put-text-property 1 5 'src 'buf1)
      (narrow-to-region 5 12)
      (undo-boundary)
      (goto-char (point-min))
      (insert "XX"))
    (with-current-buffer buf2
      (insert "BUF2-CONTENT")
      (put-text-property 1 5 'src 'buf2))
    (let ((s1 (with-current-buffer buf1 (buffer-string)))
          (s2 (with-current-buffer buf2 (buffer-string)))
          (z1 (with-current-buffer buf1 (get-text-property 1 'src))))
      (with-current-buffer buf1
        (primitive-undo 1 buffer-undo-list)
        (widen))
      (list s1 s2 z1
            (with-current-buffer buf1 (buffer-string))
            (string= (with-current-buffer buf1 (buffer-string)) "BUF1-CONTENT")
            (eq (with-current-buffer buf2 (get-text-property 1 'src)) 'buf2)
            (kill-buffer buf2))))) "#,
        expect,
    );
}

#[test]
fn divergence_save_restriction_overlay_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
  (let ((ov (make-overlay 10 14)))
    (overlay-put ov 'tag 'middle)
    (put-text-property 5 9 'zone 'b)
    (narrow-to-region 5 20)
    (save-restriction
      (widen)
      (goto-char 1)
      (insert "XX")
      (let ((s-wide (buffer-string))
            (ov-pos (list (overlay-start ov) (overlay-end ov))))
        (narrow-to-region 7 22)
        (list s-wide ov-pos
              (buffer-string)
              (overlay-start ov) (overlay-end ov)
              (overlay-get ov 'tag)
              (get-text-property 7 'zone)
              (eq (get-text-property 7 'zone) 'b))))) "#,
        expect,
    );
}

#[test]
fn divergence_save_excursion_through_multiple_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (current-buffer))
        (buf2 (generate-new-buffer " test-se1-xxx"))
        (buf3 (generate-new-buffer " test-se2-xxx")))
    (with-current-buffer buf1 (insert "ONE"))
    (with-current-buffer buf2 (insert "TWO"))
    (with-current-buffer buf3 (insert "THREE"))
    (with-current-buffer buf1
      (goto-char 2)
      (let ((p1 (point)))
        (save-excursion
          (set-buffer buf2)
          (goto-char 2)
          (insert "X")
          (set-buffer buf3)
          (goto-char 2)
          (insert "Y"))
        (list p1
              (= p1 2)
              (point)
              (= (point) 2)
              (with-current-buffer buf1 (buffer-string))
              (string= (with-current-buffer buf1 (buffer-string)) "ONE")
              (with-current-buffer buf2 (buffer-string))
              (string= (with-current-buffer buf2 (buffer-string)) "TXWO")
              (with-current-buffer buf3 (buffer-string))
              (string= (with-current-buffer buf3 (buffer-string)) "THREE")
              (kill-buffer buf2)
              (kill-buffer buf3))))) "#,
        expect,
    );
}

#[test]
fn divergence_marker_across_buffer_kill_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "MAIN-CONTENT")
  (let ((m1 (copy-marker 5 t))
        (m2 (copy-marker 10 nil)))
    (put-text-property 1 4 'part 'start)
    (put-text-property 5 8 'part 'mid)
    (undo-boundary)
    (goto-char 5)
    (insert "XXXX")
    (let ((p1 (marker-position m1))
          (p2 (marker-position m2))
          (v1 (get-text-property 1 'part)))
      (primitive-undo 1 buffer-undo-list)
      (list p1 p2 v1
            (marker-position m1)
            (marker-position m2)
            (= (marker-position m1) 5)
            (= (marker-position m2) 10)
            (get-text-property 1 'part)
            (eq (get-text-property 1 'part) 'start)
            (get-text-property 5 'part)
            (eq (get-text-property 5 'part) 'mid)
            (buffer-string)
            (string= (buffer-string) "MAIN-CONTENT")))) "#,
        expect,
    );
}

#[test]
fn divergence_temp_buffer_undo_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"OUTER\" t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "OUTER")
  (let ((outer-undo buffer-undo-list))
    (with-temp-buffer
      (insert "INNER")
      (undo-boundary)
      (goto-char 3)
      (insert "XX"))
    (list (buffer-string)
          (string= (buffer-string) "OUTER")
          (eq buffer-undo-list outer-undo)
          (= (buffer-size) 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_save_excursion_with_narrow_and_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (22 nil \"-BBBB-CCCC-DDDD\" 17 t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((m (copy-marker 15 t)))
    (narrow-to-region 5 20)
    (save-excursion
      (save-restriction
        (widen)
        (goto-char 1)
        (insert "XX")))
    (list (point)
          (= (point) 5)
          (buffer-string)
          (marker-position m)
          (> (marker-position m) 15)
          (= (point-min) 5)
          (= (point-max) 20)
          (get-text-property 7 'zone)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_in_killed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " test-oik-xxx")))
    (with-current-buffer buf
      (insert "TEMPORARY")
      (let ((ov (make-overlay 1 9)))
        (overlay-put ov 'data 'test)
        (let ((start (overlay-start ov))
              (data (overlay-get ov 'data)))
          (kill-buffer buf)
          (list start data
                (= start 1)
                (eq data 'test)
                (null (buffer-name buf))
                (not (buffer-live-p buf))))))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_locals_with_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-blt-xxx 0)
  (make-variable-buffer-local 'test-blt-xxx)
  (setq test-blt-xxx 42)
  (let ((v1 test-blt-xxx))
    (with-temp-buffer
      (setq test-blt-xxx 99)
      (let ((v2 test-blt-xxx))
        (list v1 v2
              (= v1 42)
              (= v2 99)
              test-blt-xxx
              (= test-blt-xxx 42))))) "#,
        expect,
    );
}

#[test]
fn divergence_switch_buffer_preserves_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (current-buffer))
        (buf2 (generate-new-buffer " test-sbo-xxx")))
    (with-current-buffer buf1
      (insert "CONTENT1")
      (let ((ov (make-overlay 1 8)))
        (overlay-put ov 'src 'buf1)
        (put-text-property 1 8 'label 'first)))
    (with-current-buffer buf2
      (insert "CONTENT2")
      (let ((ov (make-overlay 1 8)))
        (overlay-put ov 'src 'buf2)
        (put-text-property 1 8 'label 'second)))
    (let ((ovs1 (with-current-buffer buf1 (overlays-in 1 8)))
          (ovs2 (with-current-buffer buf2 (overlays-in 1 8))))
      (list (length ovs1) (length ovs2)
            (= (length ovs1) 1) (= (length ovs2) 1)
            (overlay-get (car ovs1) 'src)
            (eq (overlay-get (car ovs1) 'src) 'buf1)
            (overlay-get (car ovs2) 'src)
            (eq (overlay-get (car ovs2) 'src) 'buf2)
            (with-current-buffer buf1 (get-text-property 1 'label))
            (eq (with-current-buffer buf1 (get-text-property 1 'label)) 'first)
            (with-current-buffer buf2 (get-text-property 1 'label))
            (eq (with-current-buffer buf2 (get-text-property 1 'label)) 'second)
            (kill-buffer buf2)))) "#,
        expect,
    );
}
