//! Complex combo batch 225 — `face` inheritance chains deep:
//! `:inherit` resolution, `face-all-attributes`, `face-attribute` with
//! `inherit` flag, face-remapping across frame/buffer scopes.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx225_face_inheritance_chain_resolution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (unspecified unspecified italic unspecified unspecified t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx225-base
        '((((type graphic)) :foreground "red" :weight bold)
          (t :foreground "yellow"))
        "Base face")
      (defface neo-cx225-mid
        '((t :inherit neo-cx225-base :slant italic))
        "Mid face inheriting base")
      (defface neo-cx225-leaf
        '((t :inherit neo-cx225-mid :underline t))
        "Leaf face inheriting mid")
      (list (face-attribute 'neo-cx225-base :weight)
            (face-attribute 'neo-cx225-mid :weight)
            (face-attribute 'neo-cx225-mid :slant)
            (face-attribute 'neo-cx225-leaf :weight)
            (face-attribute 'neo-cx225-leaf :slant)
            (face-attribute 'neo-cx225-leaf :underline)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx225_face_attribute_with_inherit_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx225-r1 '((((type graphic)) :foreground "blue")) "r1")
      (defface neo-cx225-r2 '((t :inherit neo-cx225-r1 :background "yellow")) "r2")
      (list (face-attribute 'neo-cx225-r2 :foreground nil nil nil)
            (face-attribute 'neo-cx225-r2 :foreground 'inherit)
            (face-attribute 'neo-cx225-r2 :background nil nil nil)
            (face-attribute 'neo-cx225-r2 :background 'inherit)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx225_face_all_attributes_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t t (:family . \"default\") (:height . 1) (:weight . normal))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((attrs (face-all-attributes 'default (selected-frame))))
  (list (consp attrs)
        (> (length attrs) 5)
        (assq :family attrs)
        (assq :height attrs)
        (assq :weight attrs)))
"##,
        expect,
    );
}

#[test]
fn div_cx225_face_documentation_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Basic default face.\" \"Basic bold face.\" \"Basic italic face.\" \"Basic underlined face.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (face-documentation 'default)
          (face-documentation 'bold)
          (face-documentation 'italic)
          (face-documentation 'underline))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx225_face_list_all_known() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (t t (default) (bold default) (italic bold default) (highlight link-visited link shadow variable-pitch-text variable-pitch fixed-pitch-serif fixed-pitch underline bold-italic italic bold default))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((faces (face-list)))
  (list (consp faces)
        (> (length faces) 10)
        (memq 'default faces)
        (memq 'bold faces)
        (memq 'italic faces)
        (memq 'highlight faces)))
"##,
        expect,
    );
}

#[test]
fn div_cx225_face_underline_attribute_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:color \"red\" :style wave))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (defface neo-cx225-ul '((t :underline (:color "red" :style wave))) "ul")
  (error (list :errored (car e))))
(condition-case e
    (list (face-attribute 'neo-cx225-ul :underline))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx225_face_attribute_height_integer_or_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t :int)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((h (face-attribute 'default :height)))
  (list (or (integerp h) (floatp h))
        (> h 0)
        (if (floatp h) :float :int)))
"##,
        expect,
    );
}

#[test]
fn div_cx225_set_face_attribute_temporary_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (normal bold normal)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx225-set '((t :weight normal)) "set")
      (let ((before (face-attribute 'neo-cx225-set :weight)))
        (set-face-attribute 'neo-cx225-set nil :weight 'bold)
        (let ((after (face-attribute 'neo-cx225-set :weight)))
          (set-face-attribute 'neo-cx225-set nil :weight 'normal)
          (list before after (face-attribute 'neo-cx225-set :weight)))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx225_face_spec_attr_in_frame_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"unspecified-fg\" \"unspecified-bg\" \"default\" 1 normal normal)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((frame (selected-frame)))
  (list (face-attribute 'default :foreground frame)
        (face-attribute 'default :background frame)
        (face-attribute 'default :family frame)
        (face-attribute 'default :height frame)
        (face-attribute 'default :weight frame)
        (face-attribute 'default :slant frame)))
"##,
        expect,
    );
}

#[test]
fn div_cx225_face_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (defface neo-cx225-mega '((t :foreground "purple" :weight bold)) "mega")
      (with-temp-buffer
        (buffer-enable-undo)
        (insert "Face inheritance mega test buffer content")
        (put-text-property 1 6 'face 'neo-cx225-mega)
        (put-text-property 8 14 'face 'bold)
        (let ((m (set-marker (make-marker) 8))
              (ov (make-overlay 4 14)))
          (overlay-put ov 'face 'neo-cx225-mega)
          (overlay-put ov 'evaporate t)
          (narrow-to-region 2 18)
          (let ((state (list (face-attribute 'neo-cx225-mega :weight)
                             (face-attribute 'neo-cx225-mega :foreground)
                             (get-char-property 5 'face)
                             (get-char-property 10 'face)
                             (buffer-string)
                             (marker-position m)
                             (overlay-start ov) (overlay-end ov)
                             (text-properties-at 1)
                             (text-properties-at 5))))
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
