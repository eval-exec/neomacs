//! Deep combo: defstruct × cl-defstruct × record × type-of ×
//! make-struct × struct accessors × marker × overlay × textprop ×
//! undo × buffer-local × narrow.
//!
//! Stresses struct/record operations with buffer state: defining
//! structures, creating instances, accessing fields, and type
//! checking during edits. Structs are tricky because they involve
//! type system state that must interact correctly with the buffer's
//! edit pipeline.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn combo_defstruct_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defstruct combo--point x y)
  (let ((buf (generate-new-buffer " combo-ds")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15))
            (pt (make-combo--point :x 10 :y 20)))
        (overlay-put ov 'scope 'all)
        (overlay-put ov 'struct pt)
        (undo-boundary)
        (setf (combo--point-x pt) 30)
        (goto-char 5)
        (insert "XX")
        (let ((after (list (buffer-string)
                           (combo--point-x pt)
                           (combo--point-y pt)
                           (type-of pt)
                           (overlay-get ov 'struct)
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
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_record_type_of_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((buf (generate-new-buffer " combo-rec"))
        (rec (record 'my-type 'data '(a b c))))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15)))
        (overlay-put ov 'scope 'all)
        (overlay-put ov 'record rec)
        (undo-boundary)
        (aset rec 1 'new-data)
        (goto-char 5)
        (insert "XX")
        (let* ((ov-rec (overlay-get ov 'record))
               (after (list (buffer-string)
                            (aref ov-rec 0)
                            (aref ov-rec 1)
                            (aref ov-rec 2)
                            (type-of ov-rec)
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
            (list after restored)))))) "#,
        expect,
    );
}

#[test]
fn combo_defstruct_buffer_local_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defstruct combo--config name value)
  (let ((buf (generate-new-buffer " combo-dsbl")))
    (with-current-buffer buf
      (make-local-variable 'ds-local)
      (setq ds-local 'buffer-specific)
      (insert "AAAA-BBBB-CCCC")
      (put-text-property 1 5 'zone 'a)
      (put-text-property 6 10 'zone 'b)
      (put-text-property 11 15 'zone 'c)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 15))
            (cfg (make-combo--config :name "test" :value 42)))
        (overlay-put ov 'scope 'all)
        (overlay-put ov 'config cfg)
        (undo-boundary)
        (setf (combo--config-value cfg) 100)
        (goto-char 5)
        (insert (format "-<%s:%s>-" ds-local (combo--config-value cfg)))
        (let ((after (list (buffer-string)
                           ds-local
                           (combo--config-name cfg)
                           (combo--config-value cfg)
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'zone)
                           (get-text-property 6 'zone))))
          (primitive-undo 1 buffer-undo-list)
          (let ((restored (list (buffer-string)
                                ds-local
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

#[test]
fn combo_defstruct_narrow_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defstruct combo--item id label)
  (let ((buf (generate-new-buffer " combo-dsnar")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
      (put-text-property 1 5 'sect 'a)
      (put-text-property 6 10 'sect 'b)
      (put-text-property 11 15 'sect 'c)
      (put-text-property 16 20 'sect 'd)
      (put-text-property 21 25 'sect 'e)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 6 20))
            (item (make-combo--item :id 1 :label "test")))
        (overlay-put ov 'zone 'middle)
        (overlay-put ov 'item item)
        (undo-boundary)
        (narrow-to-region 6 20)
        (setf (combo--item-label item) "modified")
        (goto-char (point-min))
        (insert (format "<%d>-" (combo--item-id item)))
        (widen)
        (let ((after (list (buffer-string)
                           (combo--item-id item)
                           (combo--item-label item)
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
            (list after restored))))))) "#,
        expect,
    );
}

#[test]
fn combo_defstruct_multi_instance_marker_overlay_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defstruct combo--node name children)
  (let ((buf (generate-new-buffer " combo-dsmi")))
    (with-current-buffer buf
      (insert "AAAA-BBBB-CCCC-DDDD")
      (put-text-property 1 5 'grp 'a)
      (put-text-property 6 10 'grp 'b)
      (put-text-property 11 15 'grp 'c)
      (put-text-property 16 20 'grp 'd)
      (let ((m1 (copy-marker 5 nil))
            (m2 (copy-marker 10 t))
            (ov (make-overlay 1 20))
            (root (make-combo--node :name "root" :children nil))
            (child1 (make-combo--node :name "c1" :children nil))
            (child2 (make-combo--node :name "c2" :children nil)))
        (setf (combo--node-children root) (list child1 child2))
        (overlay-put ov 'scope 'all)
        (overlay-put ov 'tree root)
        (undo-boundary)
        (setf (combo--node-name child1) "modified-c1")
        (goto-char 5)
        (insert (format "-<%s>-" (combo--node-name root)))
        (let ((after (list (buffer-string)
                           (combo--node-name root)
                           (length (combo--node-children root))
                           (combo--node-name (car (combo--node-children root)))
                           (combo--node-name (cadr (combo--node-children root)))
                           (marker-position m1)
                           (marker-position m2)
                           (overlay-start ov) (overlay-end ov)
                           (get-text-property 1 'grp)
                           (get-text-property 6 'grp))))
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
            (list after restored))))))) "#,
        expect,
    );
}
