//! Oracle parity tests for GNU `subr.el` `field-at-pos`.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_field_at_pos_boundary_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el:field-at-pos reads the field at `field-beginning`; when that
    // raw field is `boundary`, it returns the field before `field-end`.
    let form = r#"
(with-temp-buffer
  (insert "aaXbb")
  (put-text-property 1 3 'field 'left)
  (put-text-property 3 4 'field 'boundary)
  (put-text-property 4 6 'field 'right)
  (list
   (mapcar #'field-at-pos (number-sequence 1 5))
   (mapcar (lambda (p)
             (list p
                   (field-beginning p)
                   (field-end p)
                   (get-char-property p 'field)))
           (number-sequence 1 5))))
"#;
    let expect = expect_test::expect![[
        r#""OK ((left left left boundary right) ((1 1 3 left) (2 1 3 left) (3 1 3 boundary) (4 3 4 right) (5 4 6 right)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_field_bounds_escape_and_limit_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "aaXbbZcc")
  (put-text-property 1 3 'field 'left)
  (put-text-property 3 4 'field 'boundary)
  (put-text-property 4 6 'field 'right)
  (put-text-property 7 9 'field 'tail)
  (list
   (mapcar (lambda (p)
             (list p
                   (field-beginning p)
                   (field-beginning p t)
                   (field-end p)
                   (field-end p t)))
           (number-sequence 1 9))
   (list
    (field-beginning 5 nil 4)
    (field-beginning 5 t 2)
    (field-end 2 nil 4)
    (field-end 3 t 5)
    (field-end 3 t 8))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((1 1 1 3 3) (2 1 1 3 3) (3 1 1 3 6) (4 3 1 4 6) (5 4 4 6 6) (6 4 4 6 7) (7 6 6 7 9) (8 7 7 9 9) (9 7 7 9 9)) (4 4 3 5 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_field_string_and_delete_field_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "aaXbbZcc")
  (put-text-property 1 3 'field 'left)
  (put-text-property 1 3 'face 'bold)
  (put-text-property 3 4 'field 'boundary)
  (put-text-property 4 6 'field 'right)
  (put-text-property 7 9 'field 'tail)
  (let ((field-with-props (field-string 1))
        (field-no-props (field-string-no-properties 1))
        (delete-result (delete-field 3)))
    (list
     field-with-props
     (text-properties-at 0 field-with-props)
     field-no-props
     (text-properties-at 0 field-no-props)
     delete-result
     (buffer-string)
     (condition-case err
         (field-string 99)
       (error (list (car err) (cdr err)))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (#(\"aa\" 0 2 (face bold field left)) (face bold field left) \"aa\" nil nil #(\"XbbZcc\" 0 1 (field boundary) 1 3 (field right) 4 6 (field tail)) (args-out-of-range (99)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_constrain_to_field_boundary_motion_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "aaXbb\ncc")
  (put-text-property 1 3 'field 'left)
  (put-text-property 3 4 'field 'boundary)
  (put-text-property 4 6 'field 'right)
  (put-text-property 7 9 'field 'tail)
  (put-text-property 2 3 'capture t)
  (let ((inhibit-field-text-motion nil))
    (goto-char 8)
    (list
     (constrain-to-field 5 2)
     (constrain-to-field 5 2 t)
     (constrain-to-field 8 2 nil t)
     (constrain-to-field 8 2 nil nil)
     (constrain-to-field 5 2 nil nil 'capture)
     (let ((inhibit-field-text-motion t))
       (constrain-to-field 8 2))
     (list (constrain-to-field nil 2) (point)))))
"#;
    let expect = expect_test::expect![[r#""OK (3 3 8 3 3 8 (3 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
