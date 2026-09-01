//! Complex combo batch 125 — `chars` / `syntax` / `category` text
//! properties for `syntax-table` interaction, `category-set` operations,
//! and `with-syntax-table` scoping.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx125_with_syntax_table_local_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx125-st*")))
  (with-current-buffer buf
    (let ((outer-st (syntax-table))
          (outer-char-at (char-syntax ?_)))
      (with-syntax-table (make-syntax-table)
        (modify-syntax-entry ?_ "w")
        (let ((inner-st (syntax-table))
              (inner-char-at (char-syntax ?_)))
          (list outer-char-at inner-char-at
                (eq outer-st (syntax-table))
                (not (eq outer-st inner-st)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx125_syntax_table_per_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf-a (get-buffer-create " *neo-cx125-a*"))
      (buf-b (get-buffer-create " *neo-cx125-b*")))
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
    );
}

#[test]
fn div_cx125_category_table_define_and_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ct (make-category-table)))
      (define-category ?l "letter" ct)
      (define-category ?d "digit" ct)
      (define-category ?s "space" ct)
      (modify-category-entry ?a ?l ct)
      (modify-category-entry ?b ?l ct)
      (modify-category-entry ?0 ?d ct)
      (modify-category-entry ?\s ?s ct)
      (list (category-docstring ?l ct)
            (category-docstring ?d ct)
            (char-category-set ?a ct)
            (char-category-set ?0 ct)
            (char-category-set ?\s ct)))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx125_category_set_mnemonics_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-number-of-arguments)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((ct (make-category-table)))
      (define-category ?1 "cat-1" ct)
      (define-category ?2 "cat-2" ct)
      (define-category ?3 "cat-3" ct)
      (modify-category-entry ?a ?1 ct)
      (modify-category-entry ?a ?2 ct)
      (modify-category-entry ?a ?3 ct)
      (let ((cs (char-category-set ?a ct)))
        (list cs
              (category-set-mnemonics cs))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx125_syntax_class_matrix_default_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 119) (122 119) (65 119) (90 119) (48 119) (57 119) (95 95) (45 95) (40 40) (41 41) (91 40) (93 41) (123 95) (125 95) (34 34) (39 39) (96 39) (59 60) (44 39) (46 95) (92 92) (63 95) (33 95) (35 39) (36 95) (37 95) (38 95) (42 95) (43 95) (60 95) (62 95) (64 95) (47 95) (124 95) (126 95))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (char-syntax c)))
        '(?a ?z ?A ?Z ?0 ?9 ?_ ?-
          ?\( ?\) ?\[ ?\] ?\{ ?\}
          ?\" ?\' ?\` ?\; ?, ?.
          ?\\ ?? ?! ?# ?$ ?% ?& ?* ?+ ?< ?> ?@ ?/ ?| ?~))
"##,
        expect,
    );
}

#[test]
fn div_cx125_modify_syntax_entry_string_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 95 46 34 60 62)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((st (copy-syntax-table (syntax-table))))
    (set-syntax-table st)
    (modify-syntax-entry ?_ "w")
    (modify-syntax-entry ?- "_")
    (modify-syntax-entry ?! ".")
    (modify-syntax-entry ?\" "\"")
    (modify-syntax-entry ?\; "<")
    (modify-syntax-entry ?\n ">")
    (list (char-syntax ?_)
          (char-syntax ?-)
          (char-syntax ?!)
          (char-syntax ?\")
          (char-syntax ?\;)
          (char-syntax ?\n))))
"##,
        expect,
    );
}

#[test]
fn div_cx125_parse_partial_sexp_full_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 0 nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(defun foo (a b)\n  \"docstring with \\\"escape\\\"\"\n  (+ a b)) ; comment")
  (let ((p1 (parse-partial-sexp 1 5))
        (p2 (parse-partial-sexp 1 20))
        (p3 (parse-partial-sexp 1 40))
        (p4 (parse-partial-sexp 1 60)))
    (list (nth 0 p1) (nth 0 p4)
          (nth 3 p2) (nth 8 p2)
          (nth 4 p4) (nth 32 p4))))
"##,
        expect,
    );
}

#[test]
fn div_cx125_scan_lists_nested_parens() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (35 8 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "outer (mid (inner deep) mid-again) outer-tail")
  (goto-char 7)   ; at "("
  (list (scan-lists (point) 1 0)
        (scan-lists (point) 1 -1)
        (scan-lists (point) 2 0)
        (scan-lists (point) -1 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx125_forward_comment_line_and_block_styles() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 8 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before ; line comment\nalpha\nbefore2 /* block\nmulti-line */ after")
  (goto-char 8)
  (forward-comment 1)
  (let ((after-line (point)))
    (forward-comment 1)
    (let ((after-alpha (point)))
      (forward-comment 1)
      (list after-line after-alpha (point)))))
"##,
        expect,
    );
}

#[test]
fn div_cx125_syntax_ppss_cached_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 nil nil nil nil nil 0 nil nil nil nil) (1 1 12 nil nil nil 0 nil nil (1) nil) (1 1 17 nil nil nil 0 nil nil (1) nil) (2 31 36 nil nil nil 0 nil nil (1 31) nil) (3 58 59 nil nil nil 0 nil nil (1 31 58) nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(defun foo ()\n  \"docstring\"\n  (let ((x 1)) ; comment\n    (+ x 1)))")
  (list (syntax-ppss 1)
        (syntax-ppss 15)
        (syntax-ppss 30)
        (syntax-ppss 45)
        (syntax-ppss 60)))
"##,
        expect,
    );
}

#[test]
fn div_cx125_word_motion_with_custom_syntax() {
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
      (forward-word -1)
      (list p1 p2 (point) (char-after)))))
"##,
        expect,
    );
}

#[test]
fn div_cx125_syntax_with_marker_overlay_undo_narrow_mega() {
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
                       (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (nth 0 (syntax-ppss (point)))
                       (nth 3 (syntax-ppss (point)))
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
