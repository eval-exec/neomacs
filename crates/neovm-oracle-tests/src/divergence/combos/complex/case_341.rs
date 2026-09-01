//! Complex combo batch 341 — `syntax-table`/`parse-partial-sexp` ultimate:
//! char-syntax full matrix, with-syntax-table scoping, syntax-ppss state,
//! parse-partial-sexp nested strings/comments, scan-lists/scan-sexps error.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx341_char_syntax_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 119) (65 119) (48 119) (57 119) (95 95) (45 95) (40 40) (41 41) (91 40) (93 41) (123 95) (125 95) (34 34) (39 39) (96 39) (59 60) (44 39) (46 95) (92 92) (63 95) (33 95) (35 39) (36 95) (37 95) (38 95) (42 95) (43 95) (60 95) (62 95) (64 95) (47 95) (124 95) (126 95) (94 95))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (char-syntax c)))
        '(?a ?A ?0 ?9 ?_ ?-
          ?\( ?\) ?\[ ?\] ?\{ ?\}
          ?\" ?\' ?\` ?\; ?, ?.
          ?\\ ?? ?! ?# ?$ ?% ?& ?* ?+ ?< ?> ?@ ?/ ?| ?~ ?^))
"##,
        expect,
    )
}

#[test]
fn div_cx341_with_syntax_table_local_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 95 95 95)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((outer-at (char-syntax ?@))
      (outer-dash (char-syntax ?-)))
  (with-syntax-table (make-syntax-table)
    (modify-syntax-entry ?@ "w")
    (modify-syntax-entry ?- "_")
    (list (char-syntax ?@) (char-syntax ?-)
          outer-at outer-dash)))
"##,
        expect,
    )
}

#[test]
fn div_cx341_syntax_ppss_through_string_and_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil 34 17 0 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(defun foo ()\n  \"docstring with \\\"esc\\\"\"\n  (+ 1 2)) ; comment\n")
  (list (nth 4 (syntax-ppss 1))
        (nth 4 (syntax-ppss 30))
        (nth 4 (syntax-ppss 45))
        (nth 4 (syntax-ppss 50))
        (nth 3 (syntax-ppss 25))
        (nth 8 (syntax-ppss 25))
        (nth 0 (syntax-ppss 1))
        (nth 0 (syntax-ppss 40))))
"##,
        expect,
    )
}

#[test]
fn div_cx341_parse_partial_sexp_nested_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (scan-error \"Containing expression ends prematurely\" 47 48)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(outer (mid (inner deep) mid-again) outer-tail)")
  (goto-char 7)
  (list (scan-lists (point) 1 0)
        (scan-lists (point) 1 1)
        (scan-lists (point) 2 0)
        (scan-lists (point) -1 0)
        (scan-sexps (point) 1)
        (scan-sexps (point) 2)
        (condition-case e (scan-lists (point) 99 0) (error (car e)))))
"##,
        expect,
    )
}

#[test]
fn div_cx341_forward_comment_line_and_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 8 8 59)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before ; line comment\nalpha\nbefore2 /* block\nmulti-line */ after")
  (goto-char 8)
  (forward-comment 1)
  (let ((p1 (point)))
    (forward-comment 1)
    (let ((p2 (point)))
      (forward-comment 1)
      (list p1 p2 (point) (char-after)))))
"##,
        expect,
    )
}

#[test]
fn div_cx341_syntax_table_per_buffer_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx341-a*"))
      (buf-b (get-buffer-create " *neo-cx341-b*")))
  (with-current-buffer buf-a
    (set-syntax-table (make-syntax-table))
    (modify-syntax-entry ?@ "w"))
  (with-current-buffer buf-b
    (set-syntax-table (make-syntax-table))
    (modify-syntax-entry ?@ "."))
  (let ((at-a (with-current-buffer buf-a (char-syntax ?@)))
        (at-b (with-current-buffer buf-b (char-syntax ?@))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (list at-a at-b)))
"##,
        expect,
    )
}

#[test]
fn div_cx341_word_motion_under_custom_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 31 16 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-syntax-table (make-syntax-table))
  (modify-syntax-entry ?_ "w")
  (modify-syntax-entry ?- "w")
  (insert "snake_case_var camelCase-kebab dotted.name")
  (goto-char 1)
  (forward-word 1)
  (let ((p1 (point)))
    (forward-word 1)
    (let ((p2 (point)))
      (backward-word 1)
      (list p1 p2 (point) (char-after)))))
"##,
        expect,
    )
}

#[test]
fn div_cx341_string_to_syntax_syntax_class_to_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (syntax-class-to-char (string-to-syntax "w"))
          (syntax-class-to-char (string-to-syntax "_"))
          (syntax-class-to-char (string-to-syntax "."))
          (syntax-class-to-char (string-to-syntax "\""))
          (syntax-class-to-char (string-to-syntax "("))
          (syntax-class-to-char (string-to-syntax ")"))
          (syntax-class-to-char (string-to-syntax ";"))
          (string-to-syntax "w")
          (string-to-syntax "( "))
  (error (list :errored (car e))))
"##,
        expect,
    )
}

#[test]
fn div_cx341_up_down_list_navigation_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 16 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a (b (c (d) e) f) g)")
  (goto-char 1)
  (down-list 1)
  (down-list 1)
  (down-list 1)
  (let ((deep (point)))
    (up-list 1)
    (let ((up1 (point)))
      (up-list 1)
      (let ((up2 (point)))
        (list deep up1 up2)))))
"##,
        expect,
    )
}

#[test]
fn div_cx341_syntax_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (set-syntax-table (make-syntax-table))
  (modify-syntax-entry ?_ "w")
  (modify-syntax-entry ?- "w")
  (insert "var_name-1 (call_arg x) end_token")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 12))
        (ov (make-overlay 4 24)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 2 30)
    (goto-char 1)
    (forward-word 2)
    (forward-comment 1)
    (let ((state (list (point) (char-syntax (char-after))
                       (nth 0 (syntax-ppss (point)))
                       (nth 3 (syntax-ppss (point)))
                       (scan-lists 1 1 0)
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
