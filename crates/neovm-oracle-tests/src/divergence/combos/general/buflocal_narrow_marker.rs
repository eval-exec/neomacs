//! Divergence tests: buffer-local state + narrowing + marker deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_buflocal_var_narrow_widen_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((buf-val global-val buf-val) (narrowed-val global-val) \"ABCDEFGHIJ\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq test-bl-narrow-xxx 'global-val)
  (make-variable-buffer-local 'test-bl-narrow-xxx)
  (setq test-bl-narrow-xxx 'buf-val)
  (insert "ABCDEFGHIJ")
  (narrow-to-region 3 8)
  (let ((v1 (list test-bl-narrow-xxx (default-value 'test-bl-narrow-xxx)
                   (buffer-local-value 'test-bl-narrow-xxx (current-buffer)))))
    (setq test-bl-narrow-xxx 'narrowed-val)
    (widen)
    (let ((v2 (list test-bl-narrow-xxx (default-value 'test-bl-narrow-xxx))))
      (list v1 v2 (buffer-string))))) "#,
        expect,
    );
}

#[test]
fn divergence_markers_spanning_narrow_boundary_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 6 15 20 25) 1 6 11 16 21 \"AAAA-XXBB-CCCC-DDDD-EEEE\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((m1 (set-marker (make-marker) 1))
        (m2 (set-marker (make-marker) 6))
        (m3 (set-marker (make-marker) 11))
        (m4 (set-marker (make-marker) 16))
        (m5 (set-marker (make-marker) 21)))
    (narrow-to-region 6 16)
    (goto-char (point-min))
    (insert "XXXX")
    (let ((pos-inside (list (marker-position m1) (marker-position m2)
                            (marker-position m3) (marker-position m4)
                            (marker-position m5))))
      (delete-region 8 12)
      (widen)
      (list pos-inside
            (marker-position m1) (marker-position m2)
            (marker-position m3) (marker-position m4)
            (marker-position m5)
            (buffer-string))))) "#,
        expect,
    );
}

#[test]
fn divergence_multi_buffer_markers_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (5 \"BUF2YYY-CONTENTXXX\" \"BUF2YYY-CONTENTXXX\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "BUF1-CONTENT")
  (let ((m1 (set-marker (make-marker) 5 (current-buffer)))
        (buf2 (generate-new-buffer "*test-mbuf2*")))
    (with-current-buffer buf2
      (insert "BUF2-CONTENT")
      (let ((m2 (set-marker (make-marker) 5 buf2))))
      (insert "XXX"))
    (with-current-buffer buf2
      (goto-char 5)
      (insert "YYY"))
    (let ((p1 (marker-position m1)))
      (with-current-buffer buf2
        (let ((p2 (marker-position
                    (car (delq nil (mapcar (lambda (m) (and (marker-buffer m) m))
                                           (list m1))))))))
          (list p1 (buffer-string)
                (with-current-buffer buf2 (buffer-string))
                (kill-buffer buf2)))))) "#,
        expect,
    );
}

#[test]
fn divergence_save_excursion_restriction_window_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((21 \"LINE-3\" \"LINE-1\\nLINE-2\\nLINE-3\\nLINE-4\\nLINE-5\") 15 8 25 \"LINE-2\\nLINE-3\\nLIN\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "LINE-1\nLINE-2\nLINE-3\nLINE-4\nLINE-5")
  (goto-char 15)
  (narrow-to-region 8 25)
  (let ((result
         (save-window-excursion
           (save-excursion
             (save-restriction
               (widen)
               (goto-char 1)
               (re-search-forward "LINE-3")
               (list (point) (match-string 0) (buffer-string)))))))
    (list result
          (point) (point-min) (point-max)
          (buffer-string)))) "#,
        expect,
    );
}

#[test]
fn divergence_buflocal_tab_width_indent_in_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (18 18 8 8 \"ne\ttwo\tth\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq tab-width 4)
  (setq-local tab-width 8)
  (insert "\tone\ttwo\tthree")
  (narrow-to-region 3 12)
  (list (current-column)
        (save-excursion (goto-char (point-max)) (current-column))
        tab-width
        (default-value 'tab-width)
        (buffer-string))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_narrow_marker_reconcile() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((5 11 9 \"XX-BBBB-CCC\") 5 11 9 \"AAAAXX-BBBB-CCCC-DDDD\" bold)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (let ((ov (make-overlay 5 9))
        (m (set-marker (make-marker) 7)))
    (overlay-put ov 'face 'bold)
    (narrow-to-region 5 14)
    (goto-char (point-min))
    (insert "XX")
    (let ((inside (list (overlay-start ov) (overlay-end ov)
                        (marker-position m) (buffer-string))))
      (widen)
      (list inside
            (overlay-start ov) (overlay-end ov)
            (marker-position m)
            (buffer-string)
            (overlay-get ov 'face))))) "#,
        expect,
    );
}

#[test]
fn divergence_kill_buffer_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 #<killed buffer> t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((buf (generate-new-buffer "*test-kill-mk*"))
       (m (with-current-buffer buf
            (insert "content")
            (set-marker (make-marker) 5 buf))))
  (list (marker-position m)
        (marker-buffer m)
        (kill-buffer buf)
        (marker-position m)
        (marker-buffer m))) "#,
        expect,
    );
}

#[test]
fn divergence_buffer_locals_list_after_multiple_setq_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((test-bvl1-xxx . 1) (test-bvl2-xxx . 2) (test-bvl3-xxx . 3) t test-bvl2-xxx nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq-local test-bvl1-xxx 1)
  (setq-local test-bvl2-xxx 2)
  (setq-local test-bvl3-xxx 3)
  (let ((locals (buffer-local-variables)))
    (list (assq 'test-bvl1-xxx locals)
          (assq 'test-bvl2-xxx locals)
          (assq 'test-bvl3-xxx locals)
          (>= (length locals) 3)
          (kill-local-variable 'test-bvl2-xxx)
          (assq 'test-bvl2-xxx (buffer-local-variables))))) "#,
        expect,
    );
}

#[test]
fn divergence_undo_with_buffer_locals() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 11 49)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq test-undo-bl-xxx 'initial)
  (make-variable-buffer-local 'test-undo-bl-xxx)
  (setq test-undo-bl-xxx 'modified)
  (insert "text")
  (undo-boundary)
  (setq test-undo-bl-xxx 'changed-again)
  (insert "more")
  (let ((v1 test-undo-bl-xxx))
    (primitive-undo 1 buffer-undo-list)
    (list v1 test-undo-bl-xxx (buffer-string))))) "#,
        expect,
    );
}

#[test]
fn divergence_with_temp_buffer_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (13 \"XXX-BBBB-CCCC-DDDD\" 13 \"AAAAXXX-BBBB-CCCC-DDDD-EEEE-FFFF\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE-FFFF")
  (let ((m (set-marker (make-marker) 10)))
    (narrow-to-region 5 20)
    (goto-char (point-min))
    (insert "XXX")
    (let ((p (marker-position m))
          (s (buffer-string)))
      (widen)
      (list p s (marker-position m) (buffer-string))))) "#,
        expect,
    );
}
