//! Divergence tests: text property intervals + overlay priority + face combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_text_property_interval_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 10 1) (10 21 bridge) (21 30 3) (30 nil nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAAAAAAAABBBBBBBBBBCCCCCCCCCC")
  (put-text-property 1 10 'level 1)
  (put-text-property 11 20 'level 2)
  (put-text-property 21 30 'level 3)
  (put-text-property 10 21 'level 'bridge)
  (let ((props nil)
        (pos 1))
    (while (< pos 31)
      (let ((next (next-single-property-change pos 'level))
            (val (get-text-property pos 'level)))
        (push (list pos next val) props)
        (setq pos (or next 31))))
    (nreverse props))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_stacking_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t (100 200 300))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (let ((ov1 (make-overlay 1 10))
        (ov2 (make-overlay 3 8))
        (ov3 (make-overlay 5 7)))
    (overlay-put ov1 'priority 100)
    (overlay-put ov2 'priority 200)
    (overlay-put ov3 'priority 300)
    (overlay-put ov1 'face 'bold)
    (overlay-put ov2 'face 'italic)
    (overlay-put ov3 'face 'underline)
    (let ((at-5 (sort (overlays-at 5)
                      (lambda (a b)
                        (< (or (overlay-get a 'priority) 0)
                           (or (overlay-get b 'priority) 0)))))
          (at-2 (overlays-at 2))
          (at-9 (overlays-at 9)))
      (list (= (length at-5) 3)
            (= (length at-2) 1)
            (= (length at-9) 1)
            (mapcar (lambda (ov) (overlay-get ov 'priority)) at-5))))) "#,
        expect,
    );
}

#[test]
fn divergence_textprop_front_sticky_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (nil nil t nil #(\"AAAXA-BBBYB\" 0 3 (sticky-front t) 5 9 (sticky-front nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB")
  (put-text-property 1 4 'sticky-front t)
  (put-text-property 5 9 'sticky-front nil)
  (goto-char 4)
  (insert "X")
  (let ((p4 (get-text-property 4 'sticky-front))
        (p5 (get-text-property 5 'sticky-front)))
    (goto-char 10)
    (insert "Y")
    (list p4 p5
          (get-text-property 1 'sticky-front)
          (get-text-property 11 'sticky-front)
          (buffer-string)))) "#,
        expect,
    );
}

#[test]
fn divergence_remove_list_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil nil t bold t bold t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (put-text-property 1 10 'face 'bold)
  (put-text-property 1 10 'invisible t)
  (put-text-property 1 10 'intangible t)
  (remove-list-of-text-properties 3 7 '(face invisible))
  (let ((f3 (get-text-property 3 'face))
        (i3 (get-text-property 3 'invisible))
        (t3 (get-text-property 3 'intangible))
        (f8 (get-text-property 8 'face))
        (i8 (get-text-property 8 'invisible))
        (f1 (get-text-property 1 'face))
        (i1 (get-text-property 1 'invisible)))
    (list f3 i3 t3 f8 i8 f1 i1
          (null f3) (null i3) (eq t3 t)
          (eq f8 'bold) (eq i8 t)
          (eq f1 'bold) (eq i1 t)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_invisible_text_adjustment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"AAAA-BBBB-CCCC-DDDD-EEEE\" 2 t 5 9 10 14 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((ov1 (make-overlay 5 9))
        (ov2 (make-overlay 10 14)))
    (overlay-put ov1 'invisible t)
    (overlay-put ov2 'invisible t)
    (list (buffer-string)
          (length (overlays-in 1 20))
          (= (length (overlays-in 1 20)) 2)
          (overlay-start ov1) (overlay-end ov1)
          (overlay-start ov2) (overlay-end ov2)
          (buffer-size)))) "#,
        expect,
    );
}

#[test]
fn divergence_textprop_category_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold nil test-cat-xxx nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (put 'test-cat-xxx 'face 'bold)
  (put 'test-cat-xxx 'invisible nil)
  (insert "ABCDEFGHIJ")
  (put-text-property 1 5 'category 'test-cat-xxx)
  (let ((f1 (get-text-property 1 'face))
        (f3 (get-text-property 3 'face))
        (f6 (get-text-property 6 'face))
        (c1 (get-text-property 1 'category))
        (i1 (get-text-property 1 'invisible)))
    (list f1 f3 f6 c1 i1
          (eq f1 'bold)
          (eq f3 'bold)
          (null f6)
          (eq c1 'test-cat-xxx)
          (null i1)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_before_string_after_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 1 6 \"MIDDLE\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "MIDDLE")
  (let ((ov (make-overlay 1 6)))
    (overlay-put ov 'before-string "[")
    (overlay-put ov 'after-string "]")
    (let ((before (overlay-get ov 'before-string))
          (after (overlay-get ov 'after-string)))
      (list (string= before "[")
            (string= after "]")
            (overlay-start ov) (overlay-end ov)
            (buffer-string)
            (= (buffer-size) 6))))) "#,
        expect,
    );
}

#[test]
fn divergence_textprop_search_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 10 1 10 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBBB-CCCC-DDDD-EEEE")
  (put-text-property 1 4 'group 'a)
  (put-text-property 10 14 'group 'c)
  (let ((p1 (text-property-any 1 21 'group 'a))
        (p2 (text-property-any 1 21 'group 'c))
        (p3 (text-property-not-all 1 4 'group 'b))
        (p4 (text-property-not-all 10 14 'group nil)))
    (list p1 p2 p3 p4
          (= p1 1)
          (= p2 10)
          (= p3 1)
          (= p4 10)))) "#,
        expect,
    );
}

#[test]
fn divergence_set_text_properties_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t italic t nil t nil t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (put-text-property 1 10 'old t)
  (set-text-properties 3 7 '(new t face italic))
  (let ((o3 (get-text-property 3 'old))
        (n3 (get-text-property 3 'new))
        (f3 (get-text-property 3 'face))
        (o1 (get-text-property 1 'old))
        (n1 (get-text-property 1 'new))
        (o8 (get-text-property 8 'old))
        (n8 (get-text-property 8 'new)))
    (list o3 n3 f3 o1 n1 o8 n8
          (null o3) (eq n3 t) (eq f3 'italic)
          (eq o1 t) (null n1)
          (eq o8 t) (null n8)))) "#,
        expect,
    );
}

#[test]
fn divergence_overlay_move_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 4 10 14 movable t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD-EEEE")
  (let ((ov (make-overlay 1 4)))
    (overlay-put ov 'tag 'movable)
    (let ((s1 (overlay-start ov))
          (e1 (overlay-end ov)))
      (move-overlay ov 10 14)
      (let ((s2 (overlay-start ov))
            (e2 (overlay-end ov))
            (tag (overlay-get ov 'tag)))
        (list s1 e1 s2 e2 tag
              (= s1 1) (= e1 4)
              (= s2 10) (= e2 14)
              (eq tag 'movable)))))) "#,
        expect,
    );
}
