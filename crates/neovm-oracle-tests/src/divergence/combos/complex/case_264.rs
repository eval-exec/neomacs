//! Complex combo batch 264 — `char-width` / `string-width` extreme edge
//! cases: combining marks, zero-width joiners, variation selectors,
//! emoji sequences, double-width CJK, and `current-column` / `move-to-
//! column` with complex inputs.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx264_char_width_combining_marks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument characterp (decode-char 'unicode 768))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (char-width c)))
        '(?a ?A ?0 ?  ?à ?é ?ü ?ñ
          (decode-char 'unicode #x0300)
          (decode-char 'unicode #x0301)
          (decode-char 'unicode #x0308)))
"##,
        expect,
    )
}

#[test]
fn div_cx264_char_width_variation_selectors_and_zwj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((65038 0) (65039 0) (8205 0) (8419 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (char-width c)))
        '(#xFE0E #xFE0F #x200D #x20E3))
"##,
        expect,
    )
}

#[test]
fn div_cx264_char_width_cjk_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument characterp (decode-char 'unicode 12288))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (char-width c)))
        '(?世 ?界 ?日 ?本 ?語 ?中 ?国 ?한 ?글
          ?김 ?이 ?박
          (decode-char 'unicode #x3000)))
"##,
        expect,
    )
}

#[test]
fn div_cx264_string_width_with_emoji_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4 14 12 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-width "😀")
      (string-width "🎉🌍")
      (string-width "hello 😀 world")
      (string-width "café 世界 😀")
      (string-width ""))
"##,
        expect,
    )
}

#[test]
fn div_cx264_string_width_with_combining_marks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4 4 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((e-acute-precomposed "café")
      (e-acute-decomposed (concat "caf" (string ?e (decode-char 'unicode #x0301)))))
  (list (string-width e-acute-precomposed)
        (string-width e-acute-decomposed)
        (length e-acute-precomposed)
        (length e-acute-decomposed)))
"##,
        expect,
    )
}

#[test]
fn div_cx264_current_column_with_tabs_and_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 10 16 17)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((tab-width 4))
    (insert "café\t世界\tend")
    (goto-char 1)
    (forward-char 5)
    (let ((c1 (current-column)))
      (forward-char 1)
      (let ((c2 (current-column)))
        (forward-char 2)
        (let ((c3 (current-column)))
          (forward-char 1)
          (list c1 c2 c3 (current-column)))))))
"##,
        expect,
    )
}

#[test]
fn div_cx264_move_to_column_with_cjk() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 7 9 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "你好abc世界def")
  (goto-char 1)
  (move-to-column 4)
  (let ((p1 (point)))
    (move-to-column 8)
    (let ((p2 (point)))
      (move-to-column 12)
      (list p1 p2 (point) (current-column)))))
"##,
        expect,
    )
}

#[test]
fn div_cx264_truncate_string_to_width_with_emoji() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" \"café\" \"café世…\" \"😀\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (truncate-string-to-width "hello world" 5)
      (truncate-string-to-width "café世界" 5)
      (truncate-string-to-width "café世界hello" 7 nil t "…")
      (truncate-string-to-width "😀😀😀😀" 3))
"##,
        expect,
    )
}

#[test]
fn div_cx264_window_text_pixel_width_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function window-text-pixel-width)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((win (selected-window)))
  (list (integerp (window-text-pixel-width win))
        (integerp (window-text-pixel-height win))
        (integerp (window-max-chars-per-line))))
"##,
        expect,
    )
}

#[test]
fn div_cx264_width_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((widths (list (string-width "café 世界 😀")
                    (char-width ?世)
                    (char-width ?😀))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Width mega café 世界 😀 test")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 25)
      (let ((state (list widths
                         (current-column)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
