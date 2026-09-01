/// Batch 531: face-attribute, face-all-attributes, face-font, color-related deep.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx531_face_attribute_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (face-attribute 'bold :weight nil 'default)
      (face-attribute 'italic :slant nil 'default))
"##,
        expect,
    );
}

#[test]
fn div_cx531_face_all_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:family . unspecified)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(car (face-all-attributes 'bold))
"##,
        expect,
    );
}

#[test]
fn div_cx531_face_documentation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"Basic bold face.\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(face-documentation 'bold)
"##,
        expect,
    );
}

#[test]
fn div_cx531_face_font() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(face-font 'default)
"##,
        expect,
    );
}

#[test]
fn div_cx531_color_name_to_rgb() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1.0 0.0 0.0) (0.0 0.0 1.0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (color-name-to-rgb "red") (color-name-to-rgb "blue"))
"##,
        expect,
    );
}

#[test]
fn div_cx531_color_rgb_to_hex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r##""OK (\"#ffff00000000\" \"#0000ffff0000\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (color-rgb-to-hex 1.0 0 0) (color-rgb-to-hex 0 1.0 0))
"##,
        expect,
    );
}

#[test]
fn div_cx531_color_distance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (327669 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (color-distance "red" "blue") (color-distance "red" "#ff0000"))
"##,
        expect,
    );
}

#[test]
fn div_cx531_color_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((65535 0 0) (65535 0 0) (65535 65535 65535))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (color-values "red") (color-values "#ff0000") (color-values "alice blue"))
"##,
        expect,
    );
}

#[test]
fn div_cx531_color_gray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (color-gray-p "gray50") (color-gray-p "red"))
"##,
        expect,
    );
}

#[test]
fn div_cx531_color_supported() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument framep t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (color-supported-p "red" nil t) (color-supported-p "#ff0000" t nil))
"##,
        expect,
    );
}

#[test]
fn div_cx531_defined_colors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((colors (defined-colors))) (list (listp colors) (> (length colors) 10)))
"##,
        expect,
    );
}

#[test]
fn div_cx531_face_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"red\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-face 'cx531-test-face)))
  (set-face-attribute f nil :foreground "red" :inherit 'bold)
  (face-attribute f :foreground nil 'default))
"##,
        expect,
    );
}

#[test]
fn div_cx531_face_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"green\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-face 'cx531-set-face)))
  (face-spec-set f '((t (:foreground "green"))) nil)
  (face-attribute f :foreground nil 'default))
"##,
        expect,
    );
}

#[test]
fn div_cx531_face_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"unspecified-fg\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-face 'cx531-reset-face)))
  (face-spec-set f '((t (:foreground "blue"))) nil)
  (face-spec-reset-face f)
  (face-attribute f :foreground nil 'default))
"##,
        expect,
    );
}

#[test]
fn div_cx531_color_name_to_rgb_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1.0 0.0 0.0) (1.0 1.0 1.0) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (color-name-to-rgb "red") (color-name-to-rgb "gainsboro") (color-name-to-rgb "teal"))
"##,
        expect,
    );
}
