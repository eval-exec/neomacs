//! Face & color divergence probes.
//!
//! Probes face lifecycle (make-face/defface/set-face-attribute/face-attribute),
//! inheritance, face-bold/italic/underline-p, face-all-attributes, face
//! documentation, face-list size, and color functions (color-defined-p,
//! defined-colors, color-values, color-rgb-to-hex, color-distance). Faces and
//! colors are resolved against the batch tty frame, identical in both engines.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_face_make_face_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] \"neo-face-x\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((f (make-face 'neo-face-x)))
  (list (facep f) (facep 'neo-face-x) (face-name f) (integerp (face-id f))))
"#,
        expect,
    );
}

#[test]
fn div_face_set_attribute_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"red\" bold italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((f (make-face 'neo-face-y)))
  (set-face-attribute f nil :foreground "red" :weight 'bold :slant 'italic)
  (list (face-attribute f :foreground)
        (face-attribute f :weight)
        (face-attribute f :slant)))
"#,
        expect,
    );
}

#[test]
fn div_face_defface_attribute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] \"blue\" bold)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(progn
  (defface neo-defface-x '((t :foreground "blue" :weight bold)) "doc")
  (list (facep 'neo-defface-x)
        (face-attribute 'neo-defface-x :foreground)
        (face-attribute 'neo-defface-x :weight)))
"#,
        expect,
    );
}

#[test]
fn div_face_inheritance_resolves() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (unspecified bold neo-parent-face)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(progn
  (defface neo-parent-face '((t :foreground "green")) "doc")
  (defface neo-child-face '((t :inherit neo-parent-face :weight bold)) "doc")
  (list (face-attribute 'neo-child-face :foreground)
        (face-attribute 'neo-child-face :weight)
        (face-attribute 'neo-child-face :inherit)))
"#,
        expect,
    );
}

#[test]
fn div_face_bold_italic_underline_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((bold extra-bold ultra-bold) (italic oblique) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((f (make-face 'neo-biu-face)))
  (set-face-attribute f nil :weight 'bold :slant 'italic :underline t)
  (list (face-bold-p f) (face-italic-p f) (face-underline-p f)))
"#,
        expect,
    );
}

#[test]
fn div_face_default_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"unspecified-fg\" \"unspecified-bg\" normal bold italic)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (face-attribute 'default :foreground)
      (face-attribute 'default :background)
      (face-attribute 'default :weight)
      (face-attribute 'bold :weight)
      (face-attribute 'italic :slant))
"#,
        expect,
    );
}

#[test]
fn div_face_all_attributes_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:family . unspecified) (:foundry . unspecified) (:width . unspecified) (:height . unspecified) (:weight . bold) (:slant . unspecified) (:underline . unspecified) (:overline . unspecified) (:extend . unspecified) (:strike-through . unspecified) (:box . unspecified) (:inverse-video . unspecified) (:foreground . \"red\") (:background . unspecified) (:stipple . unspecified) (:inherit . unspecified))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((f (make-face 'neo-all-attr)))
  (set-face-attribute f nil :foreground "red" :weight 'bold)
  (face-all-attributes f (selected-frame)))
"#,
        expect,
    );
}

#[test]
fn div_face_documentation_builtins() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Basic bold face.\" \"Basic default face.\" \"Basic face for highlighting.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (face-documentation 'bold)
      (face-documentation 'default)
      (face-documentation 'highlight))
"#,
        expect,
    );
}

#[test]
fn div_face_list_count_and_known_faces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] 186)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (facep 'default) (facep 'bold) (facep 'region)
      (facep 'font-lock-keyword-face)
      (length (face-list)))
"#,
        expect,
    );
}

#[test]
fn div_color_defined_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list (color-defined-p \"red\") (color-defined-p \"blue\") (color-defined-p \"nonexistent\") (color-defined-p \"#ff0000\") (color-defined-p \"#000000\"))",
        expect,
    );
}

#[test]
fn div_color_values_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((65535 0 0) (0 65535 0) (0 0 0) (65535 65535 65535))""#]];
    crate::common::assert_oracle_parity_expect(
        "(list (color-values \"red\") (color-values \"#00ff00\") (color-values \"black\") (color-values \"white\"))",
        expect,
    );
}

#[test]
fn div_color_rgb_hex_distance() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r##""OK (\"#ffff00000000\" \"#7fff7fff7fff\" 327669 589805)""##]];
    crate::common::assert_oracle_parity_expect(
        "(list (color-rgb-to-hex 1 0 0) (color-rgb-to-hex 0.5 0.5 0.5) (color-distance \"red\" \"blue\") (color-distance \"#000000\" \"#ffffff\"))",
        expect,
    );
}

#[test]
fn div_color_defined_colors_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (8 (\"red\" \"green\" \"yellow\" \"blue\" \"magenta\" \"cyan\" \"white\") (\"black\" \"red\" \"green\" \"yellow\" \"blue\" \"magenta\" \"cyan\" \"white\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(list (length (defined-colors))
      (member "red" (defined-colors))
      (member "black" (defined-colors)))
"#,
        expect,
    );
}

#[test]
fn div_face_unspecified_and_reset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((f (make-face 'neo-unspec-face)))
  (list (eq (face-attribute f :foreground) 'unspecified)
        (eq (face-attribute f :weight) 'unspecified)
        (set-face-attribute f nil :foreground nil)
        (eq (face-attribute f :foreground) 'unspecified)))
"#,
        expect,
    );
}
