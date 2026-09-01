//! Complex combo batch 359 — `composition`/`bidi`/`char-fold` ultimate:
//! compose-region/find-composition, bidi-paragraph-direction auto/explicit,
//! char-fold search with accented chars, auto-composition-mode.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx359_compose_region_find_composition_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((1 4 t) (1 4 t) nil (7 8 t) ((3 . \"\")) ((1 . \"\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café 世界 hello")
  (compose-region 1 4 "")
  (compose-region 7 8 "")
  (list (find-composition 1)
        (find-composition 2)
        (find-composition 5)
        (find-composition 7)
        (get-text-property 1 'composition)
        (get-text-property 7 'composition)))
"##,
        expect,
    )
}

#[test]
fn div_cx359_compose_string_find_composition_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"café\" 1 4 (composition ((3 . \"\")))) 4 (composition ((3 . \"\"))) ((3 . \"\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (compose-string "café" 1 4 ""))
       (props (text-properties-at 1 s))
       (comp (get-text-property 1 'composition s)))
  (list s (length s) props comp))
"##,
        expect,
    )
}

#[test]
fn div_cx359_decompose_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (((4 . \"\")) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "test content here")
      (compose-region 1 5 "")
      (let ((before (get-text-property 1 'composition)))
        (decompose-region 1 5)
        (list before (get-text-property 1 'composition))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx359_current_bidi_paragraph_direction_all_scripts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (left-to-right right-to-left right-to-left left-to-right left-to-right left-to-right)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (with-temp-buffer (insert "Hello world") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "مرحبا بالعالم") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "שלום עולם") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "12345") (current-bidi-paragraph-direction))
 (with-temp-buffer (insert "你好世界") (current-bidi-paragraph-direction)))
"##,
        expect,
    )
}

#[test]
fn div_cx359_bidi_explicit_direction_honored() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK right-to-left""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx359-bidi-explicit*")))
  (with-current-buffer buf
    (erase-buffer)
    (insert "Hello world")
    (setq bidi-paragraph-direction 'right-to-left)
    (prog1 (current-bidi-paragraph-direction)
      (kill-buffer buf))))
"##,
        expect,
    )
}

#[test]
fn div_cx359_char_fold_search_with_accents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "café naïve résumé piñata")
      (let ((case-fold-search t))
        (goto-char 1)
        (list (search-forward "cafe" nil t)
              (search-forward "naive" nil t)
              (search-forward "resume" nil t)
              (search-forward "pinata" nil t))))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx359_char_fold_to_regexp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((re (char-fold-to-regexp "cafe")))
      (list (stringp re)
            (string-match re "cafe")
            (string-match re "café")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx359_auto_composition_mode_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'auto-composition-mode)
          (boundp 'auto-composition-mode)
          (boundp 'auto-composition-functions)
          (boundp 'composition-function-table))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx359_char_script_table_and_syntax_class_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'char-script-table)
          (char-table-p (if (fboundp 'char-script-table)
                            (char-script-table) nil))
          (syntax-class-to-char (string-to-syntax "w"))
          (syntax-class-to-char (string-to-syntax "."))
          (syntax-class-to-char (string-to-syntax "\"")))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx359_composition_bidi_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Composition/bidi mega test buffer content")
  (put-text-property 1 6 'face 'bold)
  (compose-region 5 12 "")
  (let ((m (set-marker (make-marker) 8))
        (ov (make-overlay 4 14)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 2 18)
    (let ((state (list (find-composition 5)
                       (get-text-property 5 'composition)
                       (get-text-property 1 'composition)
                       (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1))))
      (undo)
      (widen()
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))))
"##,
        expect,
    )
}
