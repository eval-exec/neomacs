//! Divergence tests: overlay + textprop + narrowing + undo deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_textprop_overlay_narrow_edit_undo_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"XXX-BBBB-CCCC-DDDD\" 3 18 (category test)) 9 13 #(\"AAAA-BBBB-CCCC-DDDD-EEEE\" 0 4 (category test) 4 24 (category test)) 6 10 bold test)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (put-text-property 1 25 'category 'test)
  (let ((ov (make-overlay 6 10)))
    (overlay-put ov 'face 'bold)
    (undo-boundary)
    (narrow-to-region 5 20)
    (goto-char (point-min))
    (insert "XXX")
    (let ((s1 (buffer-string))
          (ov-start (overlay-start ov))
          (ov-end (overlay-end ov)))
      (primitive-undo 1 buffer-undo-list)
      (widen)
      (list s1 ov-start ov-end
            (buffer-string)
            (overlay-start ov) (overlay-end ov)
            (overlay-get ov 'face)
            (get-text-property 1 'category))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_spanning_narrow_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"ZZBB-C\" (5 17) \"AAAA-BBBB-CCCC-DDDD\" 5 15 tracked)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (let ((ov (make-overlay 5 15)))
    (overlay-put ov 'test 'tracked)
    (narrow-to-region 8 12)
    (goto-char (point-min))
    (undo-boundary)
    (insert "ZZ")
    (let ((s1 (buffer-string))
          (ov-pos (list (overlay-start ov) (overlay-end ov))))
      (primitive-undo 1 buffer-undo-list)
      (widen)
      (list s1 ov-pos
            (buffer-string)
            (overlay-start ov) (overlay-end ov)
            (overlay-get ov 'test))))) "#,
        expect,
    );
}

#[test]
fn divergence_invisible_overlay_narrow_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (17 13 17 \"VIS1-HIDDEN-VIS2-MORE\" 5 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "VIS1-HIDDEN-VIS2-MORE")
  (let ((ov (make-overlay 5 11)))
    (overlay-put ov 'invisible t)
    (narrow-to-region 3 17)
    (goto-char (point-min))
    (let ((pos (re-search-forward "VIS2" nil t)))
      (widen)
      (list pos
            (when pos (match-beginning 0))
            (when pos (match-end 0))
            (buffer-string)
            (overlay-start ov) (overlay-end ov))))) "#,
        expect,
    );
}

#[test]
fn divergence_modification_hooks_narrow_overlap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ABCXXDEFGHIJ\" 2 t 2 7 4 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-mh-log-xxx nil)
  (insert "ABCDEFGHIJ")
  (let ((ov1 (make-overlay 2 5))
        (ov2 (make-overlay 4 8)))
    (overlay-put ov1 'modification-hooks
                 (list (lambda (ov after &rest _)
                         (push (list 1 after) test-mh-log-xxx))))
    (overlay-put ov2 'modification-hooks
                 (list (lambda (ov after &rest _)
                         (push (list 2 after) test-mh-log-xxx))))
    (narrow-to-region 3 9)
    (goto-char 4)
    (insert "XX")
    (widen)
    (list (buffer-string)
          (length test-mh-log-xxx)
          (>= (length test-mh-log-xxx) 2)
          (overlay-start ov1) (overlay-end ov1)
          (overlay-start ov2) (overlay-end ov2)))) "#,
        expect,
    );
}

#[test]
fn divergence_rear_nonsticky_narrow_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil yes nil nil #(\"AAAXX-BBB-CCC\" 5 8 (rear-nonsticky t sticky yes)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBB-CCC")
  (put-text-property 4 7 'sticky 'yes)
  (put-text-property 4 7 'rear-nonsticky t)
  (narrow-to-region 4 10)
  (goto-char 4)
  (insert "XX")
  (let ((p1 (get-text-property 4 'sticky))
        (p2 (get-text-property 6 'sticky)))
    (widen)
    (list p1 p2
          (get-text-property 1 'sticky)
          (get-text-property 4 'sticky)
          (buffer-string)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_hook_modifies_buflocal_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 1 \"ABCDEFGH\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-bh-var-xxx 0)
  (make-variable-buffer-local 'test-bh-var-xxx)
  (insert "ABCDEFGH")
  (let ((ov (make-overlay 3 6)))
    (overlay-put ov 'insert-in-front-hooks
                 (list (lambda (ov after &rest _)
                         (when after (cl-incf test-bh-var-xxx)))))
    (undo-boundary)
    (goto-char 3)
    (insert "XX")
    (let ((v1 test-bh-var-xxx))
      (primitive-undo 1 buffer-undo-list)
      (list v1 test-bh-var-xxx (buffer-string))))) "#,
        expect,
    );
}

#[test]
fn divergence_next_single_property_change_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((4 5) (5 9) (9 10) (10 nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBBB-CCCC-DDDD-EEEE")
  (put-text-property 1 4 'group 'a)
  (put-text-property 5 9 'group 'b)
  (put-text-property 10 14 'group 'c)
  (narrow-to-region 4 14)
  (let ((changes nil)
        (pos (point-min)))
    (while (< pos (point-max))
      (let ((next (next-single-property-change pos 'group)))
        (push (list pos next) changes)
        (setq pos (or next (point-max)))))
    (widen)
    (nreverse changes))) "#,
        expect,
    );
}

#[test]
fn divergence_erase_buffer_with_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 t \"\" 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBBB-CCCC-DDDD")
  (let ((ov1 (make-overlay 1 4))
        (ov2 (make-overlay 5 9))
        (ov3 (make-overlay 10 14)))
    (overlay-put ov1 'idx 1)
    (overlay-put ov2 'idx 2)
    (overlay-put ov3 'idx 3)
    (put-text-property 1 17 'test 'original)
    (narrow-to-region 5 14)
    (let ((ov-count (length (overlays-in (point-min) (point-max)))))
      (erase-buffer)
      (widen)
      (list ov-count
            (= (buffer-size) 0)
            (buffer-string)
            (length (overlays-in 1 1)))))) "#,
        expect,
    );
}

#[test]
fn divergence_atomic_change_group_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (#(\"ORIGINAL\" 0 8 (cat orig)) orig after)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ORIGINAL")
  (let ((ov (make-overlay 1 9)))
    (overlay-put ov 'test 'before)
    (put-text-property 1 9 'cat 'orig)
    (condition-case nil
        (atomic-change-group
          (insert "-ADDED")
          (overlay-put ov 'test 'after)
          (put-text-property 1 14 'cat 'modified)
          (signal 'error nil))
      (error nil)))
  (list (buffer-string)
        (get-text-property 1 'cat)
        (let ((ovs (overlays-in 1 9)))
          (and ovs (overlay-get (car ovs) 'test))))) "#,
        expect,
    );
}

#[test]
fn divergence_table_overlays_sort_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"a-line\\nb-line\\nm-line\\nz-line\" 4 t (\"z-line\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "z-line\na-line\nm-line\nb-line")
  (let ((ovs nil))
    (goto-char 1)
    (while (not (eobp))
      (let ((start (line-beginning-position))
            (end (line-end-position)))
        (let ((ov (make-overlay start end)))
          (overlay-put ov 'line-content
                       (buffer-substring start end))
          (push ov ovs)))
      (forward-line 1))
    (sort-lines nil 1 (point-max))
    (let ((result (mapcar (lambda (ov) (overlay-get ov 'line-content)) ovs)))
      (list (buffer-string)
            (length ovs)
            (= (length ovs) 4)
            (member "z-line" result))))) "#,
        expect,
    );
}
