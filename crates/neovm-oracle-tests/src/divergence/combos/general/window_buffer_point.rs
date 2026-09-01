//! Divergence tests: window + buffer + point + marker spatial combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_window_point_sync_across_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 t 8 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (current-buffer))
        (buf2 (generate-new-buffer " test-wp-xxx")))
    (with-current-buffer buf2
      (insert "BBBBBBBBBB"))
    (insert "AAAAAAAAAA")
    (goto-char 5)
    (with-current-buffer buf2
      (goto-char 8))
    (list (with-current-buffer buf1 (point))
          (= (with-current-buffer buf1 (point)) 5)
          (with-current-buffer buf2 (point))
          (= (with-current-buffer buf2 (point)) 8)
          (kill-buffer buf2)))) "#,
        expect,
    );
}

#[test]
fn divergence_temporary_buffer_set_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "MAIN")
  (let ((m (copy-marker 2)))
    (with-temp-buffer
      (insert "TEMP")
      (set-marker m (point-max)))
    (list (marker-position m)
          (buffer-string)
          (= (marker-position m) 2)
          (string= (buffer-string) "MAIN")))) "#,
        expect,
    );
}

#[test]
fn divergence_marker_insertion_type_across_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 2 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (current-buffer))
        (buf2 (generate-new-buffer " test-mk-xxx")))
    (with-current-buffer buf1 (insert "AAAA"))
    (with-current-buffer buf2 (insert "BBBB"))
    (let ((m1 (with-current-buffer buf1 (copy-marker 2 t)))
          (m2 (with-current-buffer buf2 (copy-marker 2 nil))))
      (with-current-buffer buf1
        (goto-char 2)
        (insert "XX"))
      (with-current-buffer buf2
        (goto-char 2)
        (insert "YY"))
      (let ((result (list (marker-position m1)
                          (marker-position m2)
                          (marker-insertion-type m1)
                          (marker-insertion-type m2))))
        (kill-buffer buf2)
        result)))) "#,
        expect,
    );
}

#[test]
fn divergence_save_excursion_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12 nil \"BCDXXEFGHI\" 3 9 test)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'tag 'test)
    (save-excursion
      (goto-char 5)
      (insert "XX")
      (narrow-to-region 2 12))
    (list (point)
          (= (point) 1)
          (buffer-string)
          (overlay-start ov) (overlay-end ov)
          (overlay-get ov 'tag)))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_list_order_after_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((b1 (current-buffer))
        (b2 (generate-new-buffer " test-blo1-xxx"))
        (b3 (generate-new-buffer " test-blo2-xxx")))
    (with-current-buffer b2 (insert "B2"))
    (with-current-buffer b3 (insert "B3"))
    (set-buffer b1)
    (let ((bl (buffer-list)))
      (list (eq (car bl) b1)
            (memq b2 bl)
            (memq b3 bl)
            (>= (length bl) 3)
            (kill-buffer b2)
            (kill-buffer b3)))) "#,
        expect,
    );
}

#[test]
fn divergence_point_min_max_after_multiple_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 8 4 7 1 11 t t t t t nil 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((m (copy-marker 5)))
    (narrow-to-region 3 8)
    (let ((min1 (point-min)) (max1 (point-max)))
      (narrow-to-region 4 7)
      (let ((min2 (point-min)) (max2 (point-max)))
        (widen)
        (let ((min3 (point-min)) (max3 (point-max)))
          (list min1 max1 min2 max2 min3 max3
                (= min1 3) (= max1 8)
                (= min2 4) (= max2 7)
                (= min3 1) (= max3 10)
                (marker-position m))))))) "#,
        expect,
    );
}

#[test]
fn divergence_kill_buffer_undo_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((5 . 7) 9 nil (1 . 9) (t . 0)) ((5 . 7) 9 nil (1 . 9) (t . 0)) t 10 10 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ORIGINAL")
  (undo-boundary)
  (goto-char 5)
  (insert "XX")
  (let ((ul1 buffer-undo-list)
        (bs1 (buffer-size)))
    (let ((buf (current-buffer)))
      (with-temp-buffer
        (insert "TEMP")
        (set-buffer buf))
      (list ul1 buffer-undo-list
            (eq ul1 buffer-undo-list)
            bs1 (buffer-size)
            (= bs1 (buffer-size)))))) "#,
        expect,
    );
}

#[test]
fn divergence_get_buffer_create_existing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t \" test-gbc-xxx\" t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((name " test-gbc-xxx"))
    (let ((b1 (get-buffer-create name)))
      (with-current-buffer b1 (insert "DATA"))
      (let ((b2 (get-buffer name))
            (b3 (get-buffer-create name)))
        (list (eq b1 b2)
              (eq b1 b3)
              (eq b2 b3)
              (buffer-name b1)
              (= (with-current-buffer b1 (buffer-size)) 4)
              (kill-buffer b1)))))) "#,
        expect,
    );
}

#[test]
fn divergence_marker_after_revert_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 1 \"XYZ\" t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((m1 (copy-marker 3))
        (m2 (copy-marker 7)))
    (erase-buffer)
    (insert "XYZ")
    (list (marker-position m1)
          (marker-position m2)
          (buffer-string)
          (= (buffer-size) 3)
          (null (marker-position m1))
          (null (marker-position m2))))) "#,
        expect,
    );
}

#[test]
fn divergence_window_buffer_swap_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 5 nil nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf1 (current-buffer))
        (buf2 (generate-new-buffer " test-wbs1-xxx"))
        (buf3 (generate-new-buffer " test-wbs2-xxx")))
    (with-current-buffer buf2 (insert "BBBB"))
    (with-current-buffer buf3 (insert "CCCC"))
    (insert "AAAA")
    (set-buffer buf2)
    (let ((p2 (point)))
      (set-buffer buf3)
      (let ((p3 (point)))
        (set-buffer buf1)
        (list p2 p3
              (= p2 1) (= p3 1)
              (eq (current-buffer) buf1)
              (kill-buffer buf2)
              (kill-buffer buf3)))))) "#,
        expect,
    );
}
