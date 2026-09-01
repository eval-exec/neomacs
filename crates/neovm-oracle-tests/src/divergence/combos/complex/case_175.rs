//! Complex combo batch 175 — `image` / `glyphless-char` /
//! `composition` / `char-scripts` display engine queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx175_compose_region_find_composition() {
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
    );
}

#[test]
fn div_cx175_compose_string_returns_composed() {
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
    );
}

#[test]
fn div_cx175_glyphless_char_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'glyphless-char-display-control)
          (consp glyphless-char-display)
          (fboundp 'glyphless-char-display))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx175_char_script_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (mapcar (lambda (c) (list c (char-script c)))
            '(?a ?A ?0 ?α ?世 ?界 ?日 ?ä ?☺))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx175_compose_text_property_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "test content")
  (compose-region 1 5 "")
  (let ((prop-at-1 (get-text-property 1 'composition))
        (prop-at-3 (get-text-property 3 'composition))
        (prop-at-6 (get-text-property 6 'composition)))
    (list (consp prop-at-1)
          (eq prop-at-1 prop-at-3)
          prop-at-6)))
"##,
        expect,
    );
}

#[test]
fn div_cx175_decompose_region() {
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
    );
}

#[test]
fn div_cx175_char_script_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'char-script-table)
          (char-table-p (if (fboundp 'char-script-table)
                            (char-script-table)
                          nil)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx175_auto_composition_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'auto-composition-mode)
          (boundp 'auto-composition-mode)
          (boundp 'auto-composition-functions))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx175_compose_last_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'compose-last-chars)
          (boundp 'composition-function-table))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx175_glyphless_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "Composition mega test buffer content")
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
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}
