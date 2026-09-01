//! Divergence tests: combinatorial stress - narrow+undo+overlay+marker.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_combo_narrow_undo_overlay_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (5 3 7 \"ABCXEFGHIJ\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (insert "ABCDEFGHIJ")
  (let ((m (set-marker (make-marker) 5))
        (ov (make-overlay 3 7)))
    (overlay-put ov 'face 'bold)
    (narrow-to-region 2 9)
    (goto-char 4)
    (insert "XX")
    (undo-boundary)
    (delete-region 5 7)
    (list (marker-position m)
          (overlay-start ov)
          (overlay-end ov)
          (buffer-string))
    (widen)
    (list (marker-position m)
          (overlay-start ov)
          (overlay-end ov)
          (buffer-string))))"#,
        expect,
    );
}

#[test]
fn divergence_combo_insert_delete_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"0123456789\" \"012345678\" \"012345678\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (dotimes (i 10)
    (goto-char (point-max))
    (insert (format "%d" i))
    (undo-boundary))
  (let ((full (buffer-string)))
    (dotimes (_ 5) (undo))
    (let ((after5 (buffer-string)))
      (dotimes (_ 5) (undo))
      (list full after5 (buffer-string)))))"#,
        expect,
    );
}

#[test]
fn divergence_combo_save_excursion_kill_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"ABXYZFGHIJ\" 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (save-excursion
    (goto-char 3)
    (kill-region 3 6))
  (list (buffer-string)
        (car kill-ring))
  (goto-char 3)
  (insert "XYZ")
  (list (buffer-string)
        (point)))"#,
        expect,
    );
}

#[test]
fn divergence_combo_prop_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Changes to be undone are outside visible portion of buffer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (setq buffer-undo-list nil)
  (insert "ABCDEFGHIJ")
  (put-text-property 1 6 'face 'bold)
  (narrow-to-region 3 8)
  (goto-char 4)
  (insert "XXX")
  (list (get-text-property 4 'face)
        (buffer-string))
  (undo)
  (list (get-text-property 4 'face)
        (buffer-string))
  (widen)
  (list (get-text-property 4 'face)
        (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_combo_many_overlays_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((2 10 14) (5 11 18) 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (make-string 20 ?X))
  (let ((ovs (list (make-overlay 2 5)
                   (make-overlay 8 12)
                   (make-overlay 15 19))))
    (dolist (ov ovs)
      (overlay-put ov 'face 'bold))
    (goto-char 6)
    (insert "YYY")
    (delete-region 10 14)
    (list (mapcar #'overlay-start ovs)
          (mapcar #'overlay-end ovs)
          (length (overlays-in 1 25)))))"#,
        expect,
    );
}

#[test]
fn divergence_combo_marker_insert_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument markerp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((m1 (set-marker (make-marker) 5))
        (m2 (set-marker-insertion-type (set-marker (make-marker) 5) t)))
    (goto-char 5)
    (insert "XX")
    (list (marker-position m1)
          (marker-position m2)
          (marker-insertion-type m1)
          (marker-insertion-type m2))))"#,
        expect,
    );
}

#[test]
fn divergence_combo_hash_table_lambda_closure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable counter)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((ht (make-hash-table :test 'eq))
        (counter 0)
        (inc (lambda () (setq counter (1+ counter)))))
  (puthash 'fn inc ht)
  (funcall (gethash 'fn ht))
  (funcall (gethash 'fn ht))
  (funcall (gethash 'fn ht))
  (list counter (hash-table-count ht)))"#,
        expect,
    );
}

#[test]
fn divergence_combo_read_eval_print_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a b) 12 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((forms '("(+ 1 2)" "(* 3 4)" "(list 'a 'b)"))
        results)
  (dolist (f forms)
    (push (eval (read f)) results))
  results)"#,
        expect,
    );
}

#[test]
fn divergence_combo_setq_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4 3 4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let (a b c)
  (setq a 1 b 2 c 3)
  (list a b c
        (setq a (1+ b) b (1+ c) c (1+ a))
        a b c))"#,
        expect,
    );
}

#[test]
fn divergence_combo_condition_case_nested_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (inner-caught (error \"inner error\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(condition-case outer
  (condition-case inner
      (error "inner error")
    (error (list 'inner-caught inner)))
  (error (list 'outer-caught outer)))"#,
        expect,
    );
}
