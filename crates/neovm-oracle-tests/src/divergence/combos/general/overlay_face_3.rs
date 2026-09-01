//! Complex/combo divergence probes (batch 3): combined overlay+text-property
//! resolution (next-char-property-change, char-property-range-p), face merging
//! (add-face-text-property), syntax-table text-property override of parse
//! motion, point-entered/point-left hooks, line-prefix/wrap-prefix + fill,
//! window-specific overlays, keymap text-property/overlay precedence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- combined overlay + text-property resolution ----------------------------

#[test]
fn div_combo_next_char_property_change_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4 6 12 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 2 4 'face 'bold)
  (let ((ov (make-overlay 6 8)))
    (overlay-put ov 'face 'italic))
  (list (next-char-property-change 1)
        (next-char-property-change 3)
        (next-char-property-change 5)
        (next-char-property-change 8)
        (next-single-char-property-change 1 'face)))
"##,
        expect,
    );
}

#[test]
fn div_combo_previous_char_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9 7 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 2 4 'face 'bold)
  (let ((ov (make-overlay 7 9)))
    (overlay-put ov 'face 'italic))
  (list (previous-char-property-change 12)
        (previous-char-property-change 8)
        (previous-single-char-property-change 12 'face)))
"##,
        expect,
    );
}

#[test]
fn div_combo_char_property_range_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 2 5 'face 'bold)
  (let ((ov (make-overlay 4 7)))
    (overlay-put ov 'face 'italic))
  (multiple-value-bind (from to val) (char-property-range-p 3 'face)
    (list from to val)))
"##,
        expect,
    );
}

// --- face merging via add-face-text-property --------------------------------

#[test]
fn div_combo_add_face_text_property_merge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold (italic bold) italic nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (add-face-text-property 1 5 'bold nil)
  (add-face-text-property 3 8 'italic nil)
  (list (get-text-property 2 'face)
        (get-text-property 3 'face)
        (get-text-property 6 'face)
        (get-text-property 9 'face)))
"##,
        expect,
    );
}

#[test]
fn div_combo_add_face_text_property_append() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (add-face-text-property 1 3 'bold t)
  (add-face-text-property 1 3 'italic t)
  (get-text-property 2 'face))
"##,
        expect,
    );
}

// --- syntax-table text property override ------------------------------------

#[test]
fn div_combo_syntax_table_property_forward_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3)""#]];
    // Override the syntax of "(" to be word-constituent via text property.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ab(cd)ef")
  (put-text-property 3 4 'syntax-table (string-to-syntax "w"))
  (goto-char 1)
  (let ((p1 (progn (forward-word) (point))))
    (goto-char 1)
    (forward-sexp 1)
    (list p1 (point))))
"##,
        expect,
    );
}

#[test]
fn div_combo_syntax_table_property_forward_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 4""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc.def")
  (put-text-property 4 5 'syntax-table (string-to-syntax "_"))
  (goto-char 1)
  (progn (forward-word) (point)))
"##,
        expect,
    );
}

// --- point-entered / point-left hooks ---------------------------------------

#[test]
fn div_combo_point_entered_hook_fires() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let (entered)
    (put-text-property 3 5 'point-entered (lambda (&rest _) (push 'entered entered)))
    (goto-char 4)
    (list (length entered) entered)))
"##,
        expect,
    );
}

#[test]
fn div_combo_point_left_hook_fires() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let (left)
    (put-text-property 3 5 'point-left (lambda (&rest _) (push 'left left)))
    (goto-char 3)
    (goto-char 7)
    (list (length left) left)))
"##,
        expect,
    );
}

// --- line-prefix / wrap-prefix text property + fill ------------------------

#[test]
fn div_combo_line_prefix_text_property_with_fill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"alpha bravo\\ncharlie delta\\necho\\n\" 0 11 (line-prefix \"> \") 11 12 (line-prefix \"> \") 12 25 (line-prefix \"> \") 25 26 (line-prefix \"> \") 26 31 (line-prefix \"> \"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((fill-column 14))
    (insert "alpha bravo charlie delta echo\n")
    (put-text-property 1 (point-max) 'line-prefix "> ")
    (fill-region (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_combo_wrap_prefix_text_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"alpha bravo\\ncharlie\\ndelta echo\\n\" 0 11 (wrap-prefix \"  \") 11 12 (wrap-prefix \"  \") 12 19 (wrap-prefix \"  \") 19 20 (wrap-prefix \"  \") 20 31 (wrap-prefix \"  \"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((fill-column 12))
    (insert "alpha bravo charlie delta echo\n")
    (put-text-property 1 (point-max) 'wrap-prefix "  ")
    (fill-region (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

// --- keymap text property & overlay keymap precedence ----------------------

#[test]
fn div_combo_local_map_text_property_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (my-action (keymap (97 . my-action)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map "a" 'my-action)
  (with-temp-buffer
    (insert "hello")
    (put-text-property 1 3 'local-map map)
    (list (lookup-key map "a")
          (get-text-property 1 'local-map))))
"##,
        expect,
    );
}

#[test]
fn div_combo_overlay_keymap_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((keymap (120 . overlay-action)) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map "x" 'overlay-action)
  (with-temp-buffer
    (insert "hello")
    (let ((ov (make-overlay 1 4)))
      (overlay-put ov 'keymap map)
      (list (overlay-get ov 'keymap)
            (eq (overlay-get ov 'keymap) map)))))
"##,
        expect,
    );
}

// --- window-specific overlays ----------------------------------------------

#[test]
fn div_combo_overlay_window_specific() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1)""#]];
    // An overlay bound to a specific window is invisible elsewhere.
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (let ((ov (make-overlay 2 5)))
    (overlay-put ov 'face 'bold)
    (overlay-put ov 'window (selected-window)))
  (list (length (overlays-at 3))
        (length (overlays-in 1 11))))
"##,
        expect,
    );
}

// --- buffer-display-table interaction ---------------------------------------

#[test]
fn div_combo_buffer_display_table_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" \"a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello\n")
  (let ((dt (make-display-table)))
    (aset dt ?a [?X])
    (setq buffer-display-table dt))
  (list (buffer-substring 1 6) (char-to-string ?a)))
"##,
        expect,
    );
}

// --- composition + overlay --------------------------------------------------

#[test]
fn div_combo_compose_region_then_overlay_face() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((2 4 [] t nil 0) bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcde")
  (compose-region 2 4 "")
  (let ((ov (make-overlay 2 4)))
    (overlay-put ov 'face 'bold))
  (list (find-composition 2 nil nil t)
        (get-char-property 3 'face)))
"##,
        expect,
    );
}

// --- text-property search ignoring overlays --------------------------------

#[test]
fn div_combo_text_property_search_vs_char_property_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-property-range-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefgh")
  (put-text-property 2 4 'face 'bold)
  (let ((ov (make-overlay 5 7)))
    (overlay-put ov 'face 'italic))
  (list (next-single-property-change 1 'face)
        (next-single-char-property-change 1 'face)
        (text-property-any 1 8 'face 'bold)
        (char-property-range-p 5 'face)))
"##,
        expect,
    );
}

// --- modification-hooks + overlay modification-hooks -----------------------

#[test]
fn div_combo_overlay_modification_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 ((t 3) (nil 3)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let (fired)
    (let ((ov (make-overlay 2 5)))
      (overlay-put ov 'modification-hooks (list (lambda (o beg end &rest _) (push (list beg end) fired))))
      (goto-char 3)
      (insert "X"))
    (list (length fired) fired)))
"##,
        expect,
    );
}

// --- face remap relative ----------------------------------------------------

#[test]
fn div_combo_face_remap_add_relative_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case err
    (let ((cookie (face-remap-add-relative 'default :weight 'bold)))
      (list (consp cookie) (length cookie)))
  (error (list 'errored (car err))))
"##,
        expect,
    );
}

// --- invisible overlay + buffer-substring-filter ---------------------------

#[test]
fn div_combo_invisible_overlay_vs_textprop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (#(\"hello world\" 1 4 (invisible t)) \"hello world\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 2 5 'invisible t)
  (let ((ov (make-overlay 7 10)))
    (overlay-put ov 'invisible t))
  (list (buffer-substring 1 12)
        (buffer-substring-no-properties 1 12)))
"##,
        expect,
    );
}
