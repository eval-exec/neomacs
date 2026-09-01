//! Complex/combo divergence probes for overlays, text properties & faces.
//!
//! Each test combines several features at once (overlay + text-property face
//! precedence, before/after-string + editing, invisible/intangible/field +
//! navigation, modification-hooks + undo, evaporate under delete, display
//! property, propertized-string round-trips). These interactions surface
//! divergences that single-feature focused tests miss.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- overlay vs text-property face precedence --------------------------------

#[test]
fn div_combo_overlay_vs_textprop_face_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 3 7 'face 'bold)
  (let ((ov (make-overlay 4 6)))
    (overlay-put ov 'face 'italic)
    (list (get-text-property 4 'face)
          (get-char-property 4 'face)
          (get-char-property 5 'face)
          (get-char-property 7 'face))))
"##,
        expect,
    );
}

#[test]
fn div_combo_overlapping_overlays_priority_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (italic italic italic underline)""#]];
    // get-char-property resolves the highest-priority overlay.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefgh")
  (let ((o1 (make-overlay 2 6)) (o2 (make-overlay 3 7)) (o3 (make-overlay 4 8)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (overlay-put o3 'face 'underline)
    (overlay-put o1 'priority 1)
    (overlay-put o2 'priority 3)
    (overlay-put o3 'priority 2)
    (list (get-char-property 4 'face)
          (get-char-property 5 'face)
          (get-char-property 6 'face)
          (get-char-property 7 'face))))
"##,
        expect,
    );
}

#[test]
fn div_combo_face_and_font_lock_face_both() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic bold italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 4 'face 'bold)
  (put-text-property 2 4 'font-lock-face 'italic)
  (list (get-text-property 2 'face)
        (get-text-property 2 'font-lock-face)
        (get-char-property 2 'face)
        (get-char-property 2 'font-lock-face)))
"##,
        expect,
    );
}

// --- before/after-string + editing + multibyte ------------------------------

#[test]
fn div_combo_before_string_with_face_insert_near() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 8 \"caXfé世界\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (let ((ov (make-overlay 3 5)))
    (overlay-put ov 'face 'bold)
    (overlay-put ov 'before-string (propertize ">>" 'face 'italic)))
  (goto-char 3)
  (insert "X")
  (list (length (overlays-at 4))
        (point-max)
        (buffer-substring-no-properties 1 (point-max))))
"##,
        expect,
    );
}

#[test]
fn div_combo_before_string_does_not_change_point_max() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 \"café\" 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café")
  (let ((ov (make-overlay 2 3)))
    (overlay-put ov 'before-string "世界"))
  (list (point-max) (buffer-string) (length (overlays-at 2))))
"##,
        expect,
    );
}

#[test]
fn div_combo_after_string_with_embedded_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"Z\" 0 1 (face bold mouse-face highlight)) (face bold mouse-face highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (let ((ov (make-overlay 2 3)))
    (overlay-put ov 'after-string (propertize "Z" 'face 'bold 'mouse-face 'highlight)))
  (let* ((ov (car (overlays-at 2)))
         (as (overlay-get ov 'after-string)))
    (list as (text-properties-at 0 as))))
"##,
        expect,
    );
}

// --- propertized string preservation across ops -----------------------------

#[test]
fn div_combo_concat_preserves_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((face bold) nil 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s1 (propertize "ab" 'face 'bold))
      (s2 "cd"))
  (let ((r (concat s1 s2)))
    (list (text-properties-at 0 r) (text-properties-at 2 r) (length r))))
"##,
        expect,
    );
}

#[test]
fn div_combo_substring_preserves_offset_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((face bold) (face bold) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (copy-sequence "abcdef"))
       (_ (put-text-property 1 4 'face 'bold s))
       (sub (substring s 2 5)))
  (list (text-properties-at 0 sub) (text-properties-at 1 sub) (text-properties-at 2 sub)))
"##,
        expect,
    );
}

#[test]
fn div_combo_propertized_string_prin1_read_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (face bold mouse-face highlight) 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "café" 'face 'bold 'mouse-face 'highlight))
       (p (prin1-to-string s))
       (back (car (read-from-string p))))
  (list (equal s back) (text-properties-at 0 back) (length p)))
"##,
        expect,
    );
}

#[test]
fn div_combo_buffer_substring_with_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((mouse-face highlight face bold) nil #(\"hell\" 0 1 (face bold) 1 3 (mouse-face highlight face bold) 3 4 (mouse-face highlight)) \"hell\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 4 'face 'bold)
  (put-text-property 2 5 'mouse-face 'highlight)
  (let ((full (buffer-substring 1 5))
        (bare (buffer-substring-no-properties 1 5)))
    (list (text-properties-at 2 full)
          (text-properties-at 2 bare)
          full bare)))
"##,
        expect,
    );
}

// --- overlay lifecycle under editing / narrowing ----------------------------

#[test]
fn div_combo_overlay_moves_with_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'face 'bold)
    (goto-char 4)
    (insert "X")
    (list (overlay-start ov) (overlay-end ov))))
"##,
        expect,
    );
}

#[test]
fn div_combo_overlay_clipping_under_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 4 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (make-overlay 3 7)
  (narrow-to-region 4 6)
  (list (length (overlays-in (point-min) (point-max)))
        (length (overlays-at 5))
        (point-min) (point-max)))
"##,
        expect,
    );
}

#[test]
fn div_combo_overlay_evaporate_under_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((ov (make-overlay 3 5)))
    (overlay-put ov 'evaporate t)
    (delete-region 3 5)
    (list (overlay-start ov) (overlay-end ov) (overlayp ov) (length (overlays-at 3)))))
"##,
        expect,
    );
}

#[test]
fn div_combo_overlay_survives_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (let ((ov (make-overlay 2 5)))
    (overlay-put ov 'face 'bold)
    (goto-char 3)
    (insert "X")
    (undo)
    (list (overlay-start ov) (overlay-end ov) (buffer-string) (length (overlays-at 2)))))
"##,
        expect,
    );
}

// --- invisible / intangible / field + navigation ----------------------------

#[test]
fn div_combo_intangible_point_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'intangible t)
  (goto-char 1)
  (let ((p1 (progn (forward-char) (point)))
        (p2 (progn (forward-char) (point)))
        (p3 (progn (forward-char) (point))))
    (list p1 p2 p3)))
"##,
        expect,
    );
}

#[test]
fn div_combo_invisible_count_lines_and_forward_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a\nb\nc\nd\n")
  (put-text-property 3 5 'invisible t)
  (list (count-lines 1 (point-max))
        (progn (goto-char 1) (forward-line 2) (point))))
"##,
        expect,
    );
}

#[test]
fn div_combo_field_property_line_beginning() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integerp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\n")
  (put-text-property 7 12 'field 'myfield)
  (goto-char 9)
  (let ((inhibit-field-text-motion nil))
    (list (line-beginning-position)
          (line-beginning-position t)
          (constrain-to-field 1 (point) t))))
"##,
        expect,
    );
}

#[test]
fn div_combo_read_only_property_insert_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 4 'read-only t)
  (let ((inhibit-read-only nil))
    (goto-char 2)
    (condition-case err (progn (insert "X") 'inserted) (error (car err)))))
"##,
        expect,
    );
}

// --- display property & modification hooks ----------------------------------

#[test]
fn div_combo_display_property_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"b\" 0 1 (display #(\"XYZ\" 0 3 (face bold)))) #(\"XYZ\" 0 3 (face bold)) \"b\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (put-text-property 2 3 'display (propertize "XYZ" 'face 'bold))
  (list (buffer-substring 2 3)
        (get-text-property 2 'display)
        (buffer-substring-no-properties 2 3)))
"##,
        expect,
    );
}

#[test]
fn div_combo_modification_hooks_on_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 ((3 4) (2 4)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let (fired)
    (let ((hook (lambda (beg end &rest _) (push (list beg end) fired))))
      (put-text-property 2 4 'modification-hooks (list hook))
      (put-text-property 2 4 'insert-in-front-hooks (list hook))
      (goto-char 3)
      (insert "X"))
    (list (length fired) fired)))
"##,
        expect,
    );
}

// --- multiple text properties + change search across mix --------------------

#[test]
fn div_combo_mixed_props_change_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 3 4 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 4 6 'mouse-face 'highlight)
  (put-text-property 7 9 'font-lock-face 'keyword)
  (list (next-single-property-change 1 'face)
        (next-single-property-change 1 'mouse-face)
        (next-property-change 1)
        (next-property-change 3)
        (text-property-any 1 10 'font-lock-face 'keyword)))
"##,
        expect,
    );
}

// --- category text property + overlay priority combos -----------------------

#[test]
fn div_combo_category_text_property_with_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"bc\" 0 2 (face bold mouse-face highlight)) (face bold mouse-face highlight) (face bold mouse-face highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (set-text-properties 2 4 '(face bold mouse-face highlight))
  (let ((sub (buffer-substring 2 4)))
    (list sub (text-properties-at 0 sub) (text-properties-at 1 sub))))
"##,
        expect,
    );
}

#[test]
fn div_combo_overlay_priority_negative_and_window() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 0 -10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (let ((o1 (make-overlay 2 5)) (o2 (make-overlay 2 5)) (o3 (make-overlay 2 5)))
    (overlay-put o1 'priority -10)
    (overlay-put o2 'priority 0)
    (overlay-put o3 'priority 5)
    (mapcar (lambda (o) (overlay-get o 'priority)) (overlays-at 3))))
"##,
        expect,
    );
}

// --- propertize + mapcar + multibyte ----------------------------------------

#[test]
fn div_combo_propertize_each_char_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 (face bold) (face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((result (mapcar (lambda (c)
                         (text-properties-at 0 (propertize (char-to-string c) 'face 'bold)))
                       "café")))
  (list (length result) (nth 0 result) (nth 3 result)))
"##,
        expect,
    );
}

// --- narrowing + overlay + buffer-substring across boundary -----------------

#[test]
fn div_combo_narrow_overlay_substring_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"DEFG\" nil nil 4 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'face 'bold))
  (narrow-to-region 4 8)
  (list (buffer-string)
        (text-properties-at 0 (buffer-string))
        (text-properties-at 3 (buffer-string))
        (point-min) (point-max)))
"##,
        expect,
    );
}

// --- overlay before-string spanning multibyte + position tracking -----------

#[test]
fn div_combo_overlay_before_string_position_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9 4 7 \"caféY世界x\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界x")
  (let ((ov (make-overlay 4 6)))
    (overlay-put ov 'before-string ">>")
    (overlay-put ov 'face 'bold))
  (goto-char 5)
  (insert "Y")
  (let ((ov (car (overlays-at 5))))
    (list (point-max)
          (overlay-start ov) (overlay-end ov)
          (buffer-substring-no-properties 1 (point-max)))))
"##,
        expect,
    );
}

// --- face inheritance + text-property face combo ----------------------------

#[test]
fn div_combo_face_inherit_and_property_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (unspecified unspecified neo-combo-child)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defface neo-combo-parent '((t :foreground "green" :weight bold)) "doc")
  (defface neo-combo-child '((t :inherit neo-combo-parent :slant italic)) "doc")
  (with-temp-buffer
    (insert "hello world")
    (put-text-property 3 7 'face 'neo-combo-child)
    (list (face-attribute 'neo-combo-child :foreground)
          (face-attribute 'neo-combo-child :weight)
          (get-text-property 3 'face))))
"##,
        expect,
    );
}
