//! Complex combo batch 97 — packages / face inheritance / theme variables /
//! custom variables / `custom-initialize-default` / face attribute queries
//! across inheritance chains.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx97_face_attribute_inheritance_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ([face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] neo-cx97-base italic unspecified \"yellow\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (defface neo-cx97-base
      '((((type graphic)) :foreground "red" :weight bold)
        (t :foreground "yellow"))
      "Base face")
  (error (list :errored (car e))))
(condition-case e
    (defface neo-cx97-derived
      '((t :inherit neo-cx97-base :slant italic))
      "Derived face")
  (error (list :errored (car e))))
(list (facep 'neo-cx97-base)
      (facep 'neo-cx97-derived)
      (face-attribute 'neo-cx97-derived :inherit)
      (face-attribute 'neo-cx97-derived :slant)
      (face-attribute 'neo-cx97-base :weight)
      (face-attribute 'neo-cx97-base :foreground))
"##,
        expect,
    );
}

#[test]
fn div_cx97_face_attribute_with_inherited_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 4) 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (defface neo-cx97-r1 '((((type graphic)) :foreground "blue")) "r1")
  (error (list :errored (car e))))
(condition-case e
    (defface neo-cx97-r2 '((t :inherit neo-cx97-r1 :background "yellow")) "r2")
  (error (list :errored (car e))))
(list (face-attribute 'neo-cx97-r2 :foreground nil nil nil)
      (face-attribute 'neo-cx97-r2 :foreground 'inherit)
      (face-attribute 'neo-cx97-r2 :background nil nil nil))
"##,
        expect,
    );
}

#[test]
fn div_cx97_face_all_attributes_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t (:family . \"default\") (:height . 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((attrs (face-all-attributes 'default (selected-frame))))
  (list (consp attrs)
        (> (length attrs) 5)
        (assq :family attrs)
        (assq :height attrs)))
"##,
        expect,
    );
}

#[test]
fn div_cx97_set_face_attribute_temporary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((before (face-attribute 'neo-cx97-set-test :weight)))
      (defface neo-cx97-set-test '((t :weight normal)) "test")
      (set-face-attribute 'neo-cx97-set-test nil :weight 'bold)
      (let ((after (face-attribute 'neo-cx97-set-test :weight)))
        (set-face-attribute 'neo-cx97-set-test nil :weight 'normal)
        (list before after (face-attribute 'neo-cx97-set-test :weight))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx97_face_id_assignment_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (186 187 188 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx97-id1 '((t)) "id1")
      (defface neo-cx97-id2 '((t)) "id2")
      (defface neo-cx97-id3 '((t)) "id3")
      (list (face-id 'neo-cx97-id1)
            (face-id 'neo-cx97-id2)
            (face-id 'neo-cx97-id3)
            (< (face-id 'neo-cx97-id1) (face-id 'neo-cx97-id2))
            (< (face-id 'neo-cx97-id2) (face-id 'neo-cx97-id3))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx97_custom_variable_persistence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defcustom neo-cx97-cust :default
        "test custom var"
        :type 'symbol
        :group 'neo-cx97)
      (list (custom-variable-p 'neo-cx97-cust)
            (default-value 'neo-cx97-cust)
            (custom-variable-type 'neo-cx97-cust)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx97_face_documentation_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Basic default face.\" \"Basic bold face.\" \"Basic italic face.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (face-documentation 'default)
          (face-documentation 'bold)
          (face-documentation 'italic))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx97_face_underline_color_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((underlines '(((:color "red" :style line))
                        (:color "blue")
                        (t)
                        nil)))
      (mapcar (lambda (u)
                (face-attribute 'default :underline nil (selected-frame)))
              underlines))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx97_face_attribute_height_integer_or_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((default-height (face-attribute 'default :height)))
      (list (integerp default-height)
            (> default-height 0)
            (or (integerp default-height) (floatp default-height))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx97_face_list_all_known_faces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (t t (default) (bold default) (italic bold default))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((faces (face-list)))
  (list (consp faces)
        (> (length faces) 10)
        (memq 'default faces)
        (memq 'bold faces)
        (memq 'italic faces)))
"##,
        expect,
    );
}

#[test]
fn div_cx97_face_spec_attr_in_frame_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"unspecified-fg\" \"unspecified-bg\" \"default\" 1 normal)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (face-attribute 'default :foreground frame)
        (face-attribute 'default :background frame)
        (face-attribute 'default :family frame)
        (face-attribute 'default :height frame)
        (face-attribute 'default :weight frame)))
"##,
        expect,
    );
}

#[test]
fn div_cx97_face_attribute_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx97-mega '((t :foreground "purple" :weight bold)) "mega")
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Face test buffer content here")
        (put-text-property 1 6 'face 'neo-cx97-mega)
        (put-text-property 8 14 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'neo-cx97-mega)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (face-attribute 'neo-cx97-mega :weight)
                             (face-attribute 'neo-cx97-mega :foreground)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1)
                             (get-char-property 5 'face))))
            (undo)
            (widen)
            (list state (buffer-string) (marker-position m)
                  (overlay-start ov) (overlay-end ov)
                  (text-properties-at 1))))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}
