//! Divergence tests: deep text property interval splitting/merging/inheritance.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_text_prop_insert_in_middle_splits() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold nil bold bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (propertize "AAAA" 'face 'bold))
  (goto-char 3)
  (insert "BB")
  (list (get-text-property 1 'face (current-buffer))
        (get-text-property 3 'face (current-buffer))
        (get-text-property 5 'face (current-buffer))
        (get-text-property 6 'face (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_text_prop_rear_nonsticky() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert (propertize "ABC" 'rear-nonsticky t 'face 'bold))
  (goto-char (point-max))
  (insert "DEF")
  (list (get-text-property 3 'face (current-buffer))
        (get-text-property 4 'face (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_text_prop_front_sticky() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (italic italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "XX")
  (goto-char 1)
  (insert (propertize "YY" 'front-sticky t 'face 'italic))
  (list (get-text-property 1 'face (current-buffer))
        (get-text-property 2 'face (current-buffer))
        (get-text-property 3 'face (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_text_prop_remove_text_property_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold nil nil italic italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAAAA")
  (put-text-property 1 4 'face 'bold (current-buffer))
  (put-text-property 4 7 'face 'italic (current-buffer))
  (remove-text-properties 3 5 '(face nil) (current-buffer))
  (list (get-text-property 2 'face (current-buffer))
        (get-text-property 3 'face (current-buffer))
        (get-text-property 4 'face (current-buffer))
        (get-text-property 5 'face (current-buffer))
        (get-text-property 6 'face (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_next_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 7 4 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (put-text-property 1 4 'face 'bold (current-buffer))
  (put-text-property 4 7 'face 'italic (current-buffer))
  (put-text-property 7 10 'face 'underline (current-buffer))
  (list (next-property-change 1 (current-buffer))
        (next-property-change 4 (current-buffer))
        (next-single-property-change 1 'face (current-buffer))
        (next-single-property-change 4 'face (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_previous_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 7 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGHIJ")
  (put-text-property 1 4 'face 'bold (current-buffer))
  (put-text-property 4 7 'face 'italic (current-buffer))
  (list (previous-property-change 7 (current-buffer))
        (previous-property-change 10 (current-buffer))
        (previous-single-property-change 7 'face (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_text_prop_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold nil 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World")
  (put-text-property 1 6 'face 'bold (current-buffer))
  (let ((sub (buffer-substring 3 9)))
    (list (get-text-property 0 'face sub)
          (get-text-property 3 'face sub)
          (length sub))))"#,
        expect,
    );
}

#[test]
fn divergence_add_text_properties_appends() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold italic highlight highlight)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGH")
  (put-text-property 1 5 'face 'bold (current-buffer))
  (add-text-properties 3 7 '(face italic mouse-face highlight) (current-buffer))
  (list (get-text-property 2 'face (current-buffer))
        (get-text-property 4 'face (current-buffer))
        (get-text-property 5 'mouse-face (current-buffer))
        (get-text-property 6 'mouse-face (current-buffer))))"#,
        expect,
    );
}

#[test]
fn divergence_erase_buffer_keeps_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil 1 6 \"World\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello")
  (put-text-property 1 6 'face 'bold (current-buffer))
  (erase-buffer)
  (insert "World")
  (list (get-text-property 1 'face (current-buffer))
        (point-min)
        (point-max)
        (buffer-string)))"#,
        expect,
    );
}

#[test]
fn divergence_set_text_properties_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold highlight italic nil highlight)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "ABCDEFGH")
  (put-text-property 1 9 'face 'bold (current-buffer))
  (put-text-property 1 9 'mouse-face 'highlight (current-buffer))
  (set-text-properties 3 6 '(face italic) (current-buffer))
  (list (get-text-property 2 'face (current-buffer))
        (get-text-property 2 'mouse-face (current-buffer))
        (get-text-property 4 'face (current-buffer))
        (get-text-property 4 'mouse-face (current-buffer))
        (get-text-property 7 'mouse-face (current-buffer))))"#,
        expect,
    );
}
