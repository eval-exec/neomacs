//! Divergence tests: text property + search + undo deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_intangible_prop_search_point_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 11 23 t \"visible-INVISIBLE-text\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "visible-INVISIBLE-text")
  (put-text-property 8 16 'intangible t)
  (goto-char 1)
  (let ((p1 (point)))
    (forward-char 10)
    (let ((p2 (point)))
      (re-search-forward "text")
      (let ((p3 (point)))
        (list p1 p2 p3 (>= p3 16)
              (buffer-substring-no-properties 1 (point-max))))))) "#,
        expect,
    );
}

#[test]
fn divergence_propertize_insert_regex_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"123\" 0 3 (face bold category task)) bold task 11 15 #(\"TODO: fix #123\" 0 14 (face bold category task)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (let ((s (propertize \"TODO: fix #123\" 'face 'bold 'category 'task)))
    (insert s))
  (goto-char 1)
  (re-search-forward \"#\\\\([0-9]+\\\\)\")
  (list (match-string 1)
        (get-text-property 1 'face)
        (get-text-property 6 'category)
        (match-beginning 0) (match-end 0)
        (buffer-string))) ",
        expect,
    );
}

#[test]
fn divergence_modification_hooks_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 0 \"AB123CDEFGHIJ\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-modhook-log-xxx nil)
  (insert "ABCDEFGHIJ")
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'modification-hooks
                 (list (lambda (ov after-p beg end &optional len)
                         (push (list after-p beg end len) test-modhook-log-xxx))))
    (undo-boundary)
    (goto-char 3)
    (insert "123")
    (let ((log1 (length test-modhook-log-xxx)))
      (undo-boundary)
      (primitive-undo 1 buffer-undo-list)
      (list log1
            (length test-modhook-log-xxx)
            (buffer-string)
            (>= log1 2))))) "#,
        expect,
    );
}

#[test]
fn divergence_overlapping_props_remove_middle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold nil underline underline)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAAAAAAAA")
  (put-text-property 1 11 'face 'bold)
  (put-text-property 4 8 'face 'italic)
  (put-text-property 6 11 'face 'underline)
  (list (get-text-property 1 'face)
        (get-text-property 5 'face)
        (get-text-property 9 'face))
  (remove-text-properties 3 8 '(face nil))
  (list (get-text-property 1 'face)
        (get-text-property 5 'face)
        (get-text-property 9 'face)
        (get-text-property 10 'face))) "#,
        expect,
    );
}

#[test]
fn divergence_textprop_next_property_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1 a 4) (4 nil 5) (5 b 9) (9 nil 10) (10 c 14) (14 nil 15) (15 d 18) (18 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBBB-CCCC-DDDD")
  (put-text-property 1 4 'group 'a)
  (put-text-property 5 9 'group 'b)
  (put-text-property 10 14 'group 'c)
  (put-text-property 15 18 'group 'd)
  (let ((changes nil)
        (pos 1))
    (while (< pos (point-max))
      (let ((next (next-property-change pos (current-buffer))))
        (push (list pos (get-text-property pos 'group) next) changes)
        (setq pos (or next (point-max)))))
    (nreverse changes))) "#,
        expect,
    );
}

#[test]
fn divergence_field_property_search_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((1 f1) (2 f1) (3 f1) (4 f1) (5 f1) (6 f1) (7 f1) (9 f2) (10 f2) (11 f2) (12 f2) (13 f2) (14 f2) (15 f2) (16 f2) (18 f3) (19 f3) (20 f3) (21 f3) (22 f3) (23 f3) (24 f3) (25 f3)) 1 8 9 17)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "field1:val1\tfield2:val2\tfield3:val3")
  (put-text-property 1 8 'field 'f1)
  (put-text-property 9 17 'field 'f2)
  (put-text-property 18 26 'field 'f3)
  (let ((fields nil))
    (dotimes (i 26)
      (let ((f (get-text-property (1+ i) 'field)))
        (when (and f (not (eq f (car (car fields)))))
          (push (list (1+ i) f) fields))))
    (list (nreverse fields)
          (field-beginning 5) (field-end 5)
          (field-beginning 12) (field-end 12)))) "#,
        expect,
    );
}

#[test]
fn divergence_invisible_property_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"before\" 1 7) (#(\"HIDDEN\" 0 6 (invisible t)) 8 14) (\"middle\" 15 21) (#(\"AFTER\" 0 5 (invisible t)) 22 27) (\"end\" 28 31))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "before-HIDDEN-middle-AFTER-end")
  (put-text-property 8 14 'invisible t)
  (put-text-property 21 27 'invisible t)
  (goto-char 1)
  (let ((visible-positions nil))
    (while (re-search-forward "[A-Z]+" nil t)
      (push (list (match-string 0) (match-beginning 0) (match-end 0)) visible-positions))
    (nreverse visible-positions))) "#,
        expect,
    );
}

#[test]
fn divergence_textprops_after_multiple_replaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (4 #(\"bar-X-bar-Y-bar-Z-bar\" 3 6 (track original) 9 12 (track original) 15 18 (track original)) nil original nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (insert \"foo-X-foo-Y-foo-Z-foo\")
  (put-text-property 1 20 'track 'original)
  (goto-char 1)
  (let ((count 0))
    (while (search-forward \"foo\" nil t)
      (cl-incf count)
      (replace-match \"bar\" t))
    (list count (buffer-string)
          (get-text-property 1 'track)
          (get-text-property 10 'track)
          (get-text-property 15 'track)))) ",
        expect,
    );
}

#[test]
fn divergence_propertize_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAA-BBBB-CCCC-DDDD")
  (put-text-property 1 5 'level 1)
  (put-text-property 6 10 'level 2)
  (put-text-property 11 15 'level 3)
  (put-text-property 16 19 'level 4)
  (let ((sub1 (buffer-substring 3 12))
        (sub2 (buffer-substring-no-properties 3 12)))
    (list (get-text-property 1 'level sub1)
          (get-text-property 4 'level sub1)
          (get-text-property 7 'level sub1)
          (get-text-property 1 'level sub2)
          (= (length sub1) 9)
          (= (length sub2) 9)))) "#,
        expect,
    );
}

#[test]
fn divergence_add_face_text_property_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold italic underline 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAAAAAAAAABBBBBBBBBBCCCCCCCCCC")
  (add-face-text-property 1 11 'bold nil (current-buffer))
  (add-face-text-property 11 21 'italic nil (current-buffer))
  (add-face-text-property 21 31 'underline nil (current-buffer))
  (let ((ov (make-overlay 8 24)))
    (overlay-put ov 'face 'highlight)
    (overlay-put ov 'priority 100)
    (list (get-text-property 1 'face)
          (get-text-property 10 'face)
          (get-text-property 15 'face)
          (get-text-property 25 'face)
          (length (overlays-in 10 20))))) "#,
        expect,
    );
}
