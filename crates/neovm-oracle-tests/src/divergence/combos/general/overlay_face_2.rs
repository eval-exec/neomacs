//! Complex/combo divergence probes (batch 2): read-only enforcement, field
//! navigation, invisible/intangible effects, overlay+editing interactions,
//! text-property ordering & stickiness.
//!
//! Follows up on the read-only-enforcement bug and text-property ordering
//! divergence found in batch 1.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- read-only enforcement (confirmed bug: Neomacs does not enforce) ---------

#[test]
fn div_combo_read_only_insert_blocked() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 4 'read-only t)
  (goto-char 2)
  (condition-case err (progn (insert "X") 'inserted) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_read_only_delete_blocked() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 4 'read-only t)
  (goto-char 2)
  (condition-case err (progn (delete-char 1) 'deleted) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_read_only_kill_region_blocked() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 4 'read-only t)
  (condition-case err (progn (kill-region 1 3) 'killed) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_read_only_overlay_blocked() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK inserted""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (let ((ov (make-overlay 1 4)))
    (overlay-put ov 'read-only t)
    (goto-char 2)
    (condition-case err (progn (insert "X") 'inserted) (error (car err)))))
"##,
        expect,
    );
}

#[test]
fn div_combo_read_only_nonnil_value_blocked() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK text-read-only""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 4 'read-only "because")
  (goto-char 2)
  (condition-case err (progn (insert "X") 'inserted) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_read_only_inhibit_allows() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK #(\"hXello\" 0 1 (read-only t) 2 4 (read-only t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (put-text-property 1 4 'read-only t)
  (let ((inhibit-read-only t))
    (goto-char 2)
    (condition-case err (progn (insert "X") (buffer-string)) (error (car err)))))
"##,
        expect,
    );
}

// --- field navigation -------------------------------------------------------

#[test]
fn div_combo_field_beginning_of_line_aware() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\n")
  (put-text-property 7 12 'field 'myfield)
  (goto-char 9)
  (let ((inhibit-field-text-motion nil))
    (beginning-of-line)
    (point)))
"##,
        expect,
    );
}

#[test]
fn div_combo_field_end_of_line_aware() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\n")
  (put-text-property 7 12 'field 'myfield)
  (goto-char 7)
  (let ((inhibit-field-text-motion nil))
    (end-of-line)
    (point)))
"##,
        expect,
    );
}

#[test]
fn div_combo_field_constrain_to_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaaabbbb")
  (put-text-property 1 5 'field 'a)
  (put-text-property 5 9 'field 'b)
  (list (constrain-to-field 5 1 nil t t)
        (constrain-to-field 1 5 nil t t)))
"##,
        expect,
    );
}

#[test]
fn div_combo_field_inhibit_disables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 7""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\n")
  (put-text-property 7 12 'field 'myfield)
  (goto-char 9)
  (let ((inhibit-field-text-motion t))
    (beginning-of-line)
    (point)))
"##,
        expect,
    );
}

// --- invisible text effects -------------------------------------------------

#[test]
fn div_combo_invisible_buffer_substring_includes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (#(\"hello world\" 6 11 (invisible t)) \"hello world\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 7 12 'invisible t)
  (list (buffer-substring 1 12)
        (buffer-substring-no-properties 1 12)))
"##,
        expect,
    );
}

#[test]
fn div_combo_invisible_overlay_kill_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 9""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line one\nline two\n")
  (put-text-property 9 17 'invisible t)
  (goto-char 9)
  (condition-case err (progn (kill-line) (point)) (error (car err))))
"##,
        expect,
    );
}

#[test]
fn div_combo_invisible_fill_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"alpha\\nbravo charlie\\ndelta echo\\nfoxtrot\\n\" 12 19 (invisible t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((fill-column 10))
    (insert "alpha bravo charlie delta echo foxtrot\n")
    (put-text-property 13 20 'invisible t)
    (fill-region (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

// --- intangible -------------------------------------------------------------

#[test]
fn div_combo_intangible_backward_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 4 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'intangible t)
  (goto-char 6)
  (let ((p1 (progn (backward-char) (point)))
        (p2 (progn (backward-char) (point)))
        (p3 (progn (backward-char) (point))))
    (list p1 p2 p3)))
"##,
        expect,
    );
}

#[test]
fn div_combo_intangible_overlay_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 6""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((ov (make-overlay 2 4)))
    (overlay-put ov 'intangible t)
    (goto-char 1)
    (forward-char 5)
    (point)))
"##,
        expect,
    );
}

// --- overlay + editing interactions -----------------------------------------

#[test]
fn div_combo_insert_between_adjacent_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4 4 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefgh")
  (let ((o1 (make-overlay 2 4)) (o2 (make-overlay 4 6)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (goto-char 4)
    (insert "X")
    (list (overlay-start o1) (overlay-end o1)
          (overlay-start o2) (overlay-end o2))))
"##,
        expect,
    );
}

#[test]
fn div_combo_overlay_advance_after_insert_at_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4)""#]];
    // Insert at overlay END: default does NOT advance (rear-sticky semantics).
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((ov (make-overlay 2 4)))
    (overlay-put ov 'face 'bold)
    (goto-char 4)
    (insert "X")
    (list (overlay-start ov) (overlay-end ov))))
"##,
        expect,
    );
}

#[test]
fn div_combo_delete_removing_overlay_extent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4 \"abdef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((ov (make-overlay 2 5)))
    (overlay-put ov 'face 'bold)
    (delete-region 3 4)
    (list (overlay-start ov) (overlay-end ov) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_combo_nested_overlays_edit_then_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "the quick fox")
  (let ((o1 (make-overlay 1 14)) (o2 (make-overlay 5 9)))
    (overlay-put o1 'face 'default)
    (overlay-put o2 'face 'bold)
    (goto-char 5)
    (insert "ZZ")
    (let ((s1 (overlay-start o1)) (e1 (overlay-end o1))
          (s2 (overlay-start o2)) (e2 (overlay-end o2)))
      (undo)
      (list s1 e1 s2 e2
            (overlay-start o1) (overlay-end o1)
            (overlay-start o2) (overlay-end o2)))))
"##,
        expect,
    );
}

// --- text property ordering & stickiness ------------------------------------

#[test]
fn div_combo_propertize_property_order_preserved() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a 1 b 2 c 3 d 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s (propertize "x" 'a 1 'b 2 'c 3 'd 4)))
  (text-properties-at 0 s))
"##,
        expect,
    );
}

#[test]
fn div_combo_put_text_property_appends_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (font-lock-face keyword mouse-face highlight face bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 1 4 'face 'bold)
  (put-text-property 1 4 'mouse-face 'highlight)
  (put-text-property 1 4 'font-lock-face 'keyword)
  (text-properties-at 2))
"##,
        expect,
    );
}

#[test]
fn div_combo_sticky_insert_between_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    // Inserting between two differently-propertized regions: which props stick?
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 4 7 'face 'italic)
  (goto-char 3)
  (insert "X")
  (list (get-text-property 3 'face) (get-text-property 4 'face)))
"##,
        expect,
    );
}

// --- font-lock-like regex face application over multibyte -------------------

#[test]
fn div_combo_font_lock_like_apply_faces_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (keyword nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(defun foo (café) \"héllo\" 世界)")
  (goto-char (point-min))
  (while (re-search-forward "[a-zé]+" nil t)
    (put-text-property (match-beginning 0) (match-end 0) 'face 'keyword))
  (list (get-text-property 3 'face)
        (get-text-property 11 'face)
        (next-single-property-change 1 'face)))
"##,
        expect,
    );
}
