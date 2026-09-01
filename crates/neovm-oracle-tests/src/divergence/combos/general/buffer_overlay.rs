//! Divergence tests: complex buffer + overlay + textprop combinations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_overlay_textprop_priority_conflict() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (default default bold bold bold bold bold default default default)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (put-text-property 1 11 'face 'default)
  (put-text-property 3 8 'face 'bold)
  (let ((ov1 (make-overlay 2 6))
        (ov2 (make-overlay 5 10)))
    (overlay-put ov1 'face 'italic)
    (overlay-put ov2 'face 'underline)
    (overlay-put ov1 'priority 5)
    (overlay-put ov2 'priority 10)
    (let ((faces nil))
      (dotimes (i 10)
        (push (get-text-property (1+ i) 'face) faces))
      (nreverse faces)))) ",
        expect,
    );
}

#[test]
fn divergence_overlay_props_after_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((3 10 original) 3 7 original \"AB12EFGHIJ\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'test 'original)
    (goto-char 3)
    (insert \"123\")
    (let ((p1 (list (overlay-start ov) (overlay-end ov) (overlay-get ov 'test))))
      (delete-region 5 8)
      (list p1
            (overlay-start ov)
            (overlay-end ov)
            (overlay-get ov 'test)
            (buffer-string))))) ",
        expect,
    );
}

#[test]
fn divergence_textprop_survive_replace_in_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 23 23)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"foo X-MARKER-HERE bar baz\")
  (put-text-property 1 26 'category 'test-cat)
  (put-text-property 5 20 'face 'highlight)
  (let ((ov (make-overlay 4 21)))
    (overlay-put ov 'intangible t)
    (goto-char 1)
    (re-search-forward \"X-MARKER-HERE\")
    (replace-match \"REPLACED\")
    (list (buffer-string)
          (get-text-property 1 'category)
          (get-text-property 5 'face)
          (get-text-property 15 'face)
          (get-text-property 23 'category)
          (overlay-get ov 'intangible)
          (overlay-start ov)
          (overlay-end ov)))) ",
        expect,
    );
}

#[test]
fn divergence_narrow_overlay_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"AAA-BBBB-CCCC-DDDD\")
  (let ((ov (make-overlay 5 9)))
    (overlay-put ov 'face 'bold)
    (narrow-to-region 5 14)
    (list (buffer-string)
          (point-min) (point-max)
          (overlay-start ov) (overlay-end ov)
          (get-text-property 1 'face)
          (length (overlays-in (point-min) (point-max))))
    (widen)
    (list (buffer-string)
          (point-min) (point-max)
          (overlay-start ov) (overlay-end ov)))) ",
        expect,
    );
}

#[test]
fn divergence_invisible_overlay_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (19 8 19 19 \"before HIDDEN-TEXT after\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"before HIDDEN-TEXT after\")
  (let ((ov (make-overlay 8 19)))
    (overlay-put ov 'invisible t)
    (goto-char 1)
    (let ((pos (re-search-forward \"HIDDEN-TEXT\" nil t)))
      (list pos
            (when pos (match-beginning 0))
            (when pos (match-end 0))
            (point)
            (buffer-substring-no-properties 1 (point-max)))))) ",
        expect,
    );
}

#[test]
fn divergence_copy_region_with_props_to_new_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 5 12)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"AAA BBB CCC DDD\")
  (put-text-property 1 4 'weight 'heavy)
  (put-text-property 5 8 'weight 'medium)
  (put-text-property 9 12 'weight 'light)
  (let ((src-props (list (get-text-property 1 'weight)
                         (get-text-property 5 'weight)
                         (get-text-property 9 'weight)))
        (dst (generate-new-buffer \"*test-copy*\")))
    (with-current-buffer dst
      (insert-buffer-substring (current-buffer) 5 12)
      (let ((dst-props (list (get-text-property 1 'weight)
                             (get-text-property 4 'weight))))
        (kill-buffer dst)
        (list src-props dst-props (buffer-string)))))) ",
        expect,
    );
}

#[test]
fn divergence_overlay_evaporation_on_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"AACCDD\" nil nil evap 0)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"AABBCCDD\")
  (let ((ov (make-overlay 3 5)))
    (overlay-put ov 'test 'evap)
    (overlay-put ov 'evaporate t)
    (delete-region 3 5)
    (list (buffer-string)
          (overlay-start ov)
          (overlay-end ov)
          (overlay-get ov 'test)
          (length (overlays-in 1 7))))) ",
        expect,
    );
}

#[test]
fn divergence_overlay_chain_insert_behind_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"FRONT-XXMIDDLEYY-BACK\" 4 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defvar test-hook-log-xxx nil)
  (insert \"FRONT-MIDDLE-BACK\")
  (let ((ov (make-overlay 7 13)))
    (overlay-put ov 'insert-in-front-hooks
                 (list (lambda (ov after-p beg end &optional len)
                         (push (list 'front after-p beg end len) test-hook-log-xxx))))
    (overlay-put ov 'insert-behind-hooks
                 (list (lambda (ov after-p beg end &optional len)
                         (push (list 'behind after-p beg end len) test-hook-log-xxx))))
    (goto-char 7)
    (insert \"XX\")
    (goto-char 15)
    (insert \"YY\")
    (list (buffer-string)
          (length test-hook-log-xxx)
          (>= (length test-hook-log-xxx) 2)))) ",
        expect,
    );
}

#[test]
fn divergence_multiple_overlays_face_merging() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 t 1 10 5)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"ABCDEFGHIJ\")
  (let ((ov1 (make-overlay 1 6))
        (ov2 (make-overlay 4 11))
        (ov3 (make-overlay 3 8)))
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face 'italic)
    (overlay-put ov3 'face 'underline)
    (overlay-put ov1 'priority 1)
    (overlay-put ov2 'priority 10)
    (overlay-put ov3 'priority 5)
    (let ((ov-at-4 (car (overlays-at 4)))
          (count (length (overlays-at 4))))
      (list count
            (>= count 2)
            (overlay-get ov1 'priority)
            (overlay-get ov2 'priority)
            (overlay-get ov3 'priority))))) ",
        expect,
    );
}

#[test]
fn divergence_textprop_field_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (col1 col2 nil nil 8 7)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"field1\\tfield2\\tfield3\\n\")
  (insert \"data1\\tdata2\\tdata3\")
  (put-text-property 1 7 'field 'col1)
  (put-text-property 8 14 'field 'col2)
  (put-text-property 15 21 'field 'col3)
  (goto-char 1)
  (let ((f1 (get-text-property (point) 'field)))
    (forward-char 10)
    (let ((f2 (get-text-property (point) 'field)))
      (end-of-line)
      (let ((f3 (get-text-property (point) 'field)))
        (forward-line 1)
        (let ((f4 (get-text-property (point) 'field)))
          (list f1 f2 f3 f4
                (text-property-any 1 22 'field 'col2)
                (text-property-not-all 1 22 'field 'col1))))))) ",
        expect,
    );
}
