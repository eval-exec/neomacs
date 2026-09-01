//! Face subsystem source-audit divergences (xfaces.c vs crates/neovm-core/src/face.rs).
//!
//! Probes face attribute inheritance resolution, the face-attribute INHERIT-flag,
//! defface multi-display-spec resolution, face-spec-set, face-remap
//! (add-relative/set-base/remove), tty color support/canonicalization,
//! merge-face-attribute, the :inherit unspecified cell, :font/:box/:stipple/
//! :inverse-video defaults, and frame-font vs face :font.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_aface_inherit_list_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (unspecified unspecified (neo-fi-p1 neo-fi-p2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defface neo-fi-p1 '((t :foreground "green")) "d")
  (defface neo-fi-p2 '((t :background "blue")) "d")
  (defface neo-fi-c '((t :inherit (neo-fi-p1 neo-fi-p2))) "d")
  (list (face-attribute 'neo-fi-c :foreground)
        (face-attribute 'neo-fi-c :background)
        (face-attribute 'neo-fi-c :inherit)))
"##,
        expect,
    );
}

#[test]
fn div_aface_attribute_inherit_flag_unresolved() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (unspecified \"red\")""#]];
    // 3rd arg inherit-flag = 'unspecified -> don't resolve through inherit.
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defface neo-fa-p '((t :foreground "red")) "d")
  (defface neo-fa-c '((t :inherit neo-fa-p)) "d")
  (list (face-attribute 'neo-fa-c :foreground)
        (face-attribute 'neo-fa-c :foreground nil 'unspecified)))
"##,
        expect,
    );
}

#[test]
fn div_aface_defface_multiple_display_specs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"yellow\" unspecified)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defface neo-dm '((((class color) (background light)) :foreground "red")
                    (((class color) (background dark)) :foreground "pink")
                    (t :foreground "yellow")) "d")
  (list (face-attribute 'neo-dm :foreground)
        (face-attribute 'neo-dm :distant-foreground)))
"##,
        expect,
    );
}

#[test]
fn div_aface_face_spec_set_programmatic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"magenta\" bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (face-spec-set 'neo-fss '((t :foreground "magenta" :weight bold)) nil)
  (list (face-attribute 'neo-fss :foreground)
        (face-attribute 'neo-fss :weight)))
"##,
        expect,
    );
}

#[test]
fn div_aface_face_remap_add_relative() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t normal (:weight bold) normal)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((cookie (face-remap-add-relative 'default :weight 'bold)))
    (list (consp cookie)
          (face-attribute 'default :weight)
          (face-remap-remove-relative cookie)
          (face-attribute 'default :weight))))
"##,
        expect,
    );
}

#[test]
fn div_aface_face_remap_set_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"unspecified-fg\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (face-remap-set-base 'default :foreground "cyan")
  (list (face-attribute 'default :foreground)
        (face-remap-reset-base 'default)))
"##,
        expect,
    );
}

#[test]
fn div_aface_color_supported_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (color-supported-p "red")
      (color-supported-p "nonexistent")
      (color-supported-p "#ff0000")
      (color-supported-p "#abc"))
"##,
        expect,
    );
}

#[test]
fn div_aface_tty_color_canonicalize() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"red\" \"red\" \"nonexistent\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (tty-color-canonicalize "red")
      (tty-color-canonicalize "RED")
      (condition-case e (tty-color-canonicalize "nonexistent") (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_aface_inherit_unspecified_cell() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    // Direct face-attribute :inherit on a face with no inherit.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-face 'neo-inh-unsp)))
  (eq (face-attribute f :inherit) 'unspecified))
"##,
        expect,
    );
}

#[test]
fn div_aface_attribute_unspecified_defaults() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-face 'neo-defs-unsp)))
  (list (eq (face-attribute f :box) 'unspecified)
        (eq (face-attribute f :stipple) 'unspecified)
        (eq (face-attribute f :inverse-video) 'unspecified)
        (eq (face-attribute f :underline) 'unspecified)
        (eq (face-attribute f :overline) 'unspecified)))
"##,
        expect,
    );
}

#[test]
fn div_aface_font_attribute_tty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (unspecified nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (face-attribute 'default :font)
      (face-font 'default)
      (face-font 'bold)
      (face-font 'italic))
"##,
        expect,
    );
}

#[test]
fn div_aface_merge_face_attribute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold bold \"red\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (merge-face-attribute :weight 'bold 'extra-bold)
      (merge-face-attribute :weight 'bold nil)
      (merge-face-attribute :foreground "red" "blue"))
"##,
        expect,
    );
}

#[test]
fn div_aface_face_all_attributes_inherited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (unspecified unspecified)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defface neo-faa-p '((t :foreground "red" :weight bold)) "d")
  (defface neo-faa-c '((t :inherit neo-faa-p)) "d")
  (list (face-attribute 'neo-faa-c :weight)
        (face-attribute 'neo-faa-c :foreground)))
"##,
        expect,
    );
}

#[test]
fn div_aface_frame_parameter_font_vs_face_font() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"tty\" unspecified \"default\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (frame-parameter nil 'font)
      (face-attribute 'default :font)
      (face-attribute 'default :family))
"##,
        expect,
    );
}
