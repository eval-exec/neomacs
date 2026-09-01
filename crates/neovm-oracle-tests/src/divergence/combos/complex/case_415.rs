//! Complex combo batch 415 — 20 probes into syntax, sexp navigation,
//! region/mark operations, Unicode properties, bidi, whitespace ops,
//! and string editing functions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// modify-syntax-entry with complex flags and forward-comment.
#[test]
fn div_cx415_modify_syntax_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((st (make-syntax-table)))
    (modify-syntax-entry ?/ ". 124b" st)
    (modify-syntax-entry ?* ". 23" st)
    (modify-syntax-entry ?\n "> b" st)
    (set-syntax-table st))
  (insert "/* comment */ (code)")
  (goto-char 1)
  (list (forward-comment 1) (point)))
"##,
        expect,
    );
}

/// parse-sexp-lookup-properties with syntax text properties.
#[test]
fn div_cx415_parse_sexp_lookup_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((parse-sexp-lookup-properties t))
    (insert "a(b)c")
    (put-text-property 3 4 'syntax-table (string-to-syntax ")"))
    (goto-char 1)
    (list (forward-sexp 1) (point))))
"##,
        expect,
    );
}

/// forward-sexp / backward-sexp with multibyte content.
#[test]
fn div_cx415_forward_backward_sexp_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 17 nil 23 nil 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(café (世界) test) (αβγ)")
  (goto-char 1)
  (list (forward-sexp 1) (point)
        (forward-sexp 1) (point)
        (backward-sexp 1) (point)))
"##,
        expect,
    );
}

/// beginning-of-defun / end-of-defun in emacs-lisp-mode.
#[test]
fn div_cx415_beginning_end_of_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 2 nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun a (x) (* x 2))\n(defun b (y) (+ y 3))")
  (goto-char (point-max))
  (list (beginning-of-defun -1) (line-number-at-pos)
        (end-of-defun 1) (line-number-at-pos)))
"##,
        expect,
    );
}

/// push-mark / pop-mark / exchange-point-and-mark.
#[test]
fn div_cx415_push_pop_mark() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (goto-char 3)
  (push-mark 8)
  (list (mark t) (point))
  (exchange-point-and-mark)
  (list (mark t) (point))
  (pop-mark)
  (list (point)))
"##,
        expect,
    );
}

/// region-active-p / use-region-p / region-bounds.
#[test]
fn div_cx415_region_active_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t 5 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "sample text")
  (transient-mark-mode 1)
  (push-mark 5 nil t)
  (goto-char 8)
  (list (region-active-p)
        (use-region-p)
        (region-beginning)
        (region-end)))
"##,
        expect,
    );
}

/// zap-to-char / zap-up-to-char: deleting up to a character.
#[test]
fn div_cx415_zap_to_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"world foo bar\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world foo bar")
  (goto-char 1)
  (zap-to-char 1 ?\s)
  (buffer-string))
"##,
        expect,
    );
}

/// delete-pair / insert-pair: balancing pairs.
#[test]
fn div_cx415_delete_insert_pair() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Not before matching pair\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(hello [world] {foo})")
  (goto-char 2)
  (delete-pair)
  (buffer-string))
"##,
        expect,
    );
}

/// just-one-space / delete-horizontal-space / fixup-whitespace.
#[test]
fn div_cx415_whitespace_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a b   c\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a     b   c")
  (goto-char 3)
  (just-one-space)
  (buffer-string))
"##,
        expect,
    );
}

/// cycle-spacing: cycling through spacing options.
#[test]
fn div_cx415_cycle_spacing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a b\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a       b")
  (goto-char 3)
  (cycle-spacing 1)
  (buffer-string))
"##,
        expect,
    );
}

/// get-char-code-property: Unicode property lookup.
#[test]
fn div_cx415_get_char_code_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"LATIN CAPITAL LETTER A\" Lu \"CJK IDEOGRAPH-4E16\" Lo)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (get-char-code-property ?A 'name)
      (get-char-code-property ?A 'general-category)
      (get-char-code-property ?世 'name)
      (get-char-code-property ?世 'general-category))
"##,
        expect,
    );
}

/// bidi-string-mark-left-to-right / string-mark-left-to-right.
#[test]
fn div_cx415_bidi_string_mark_ltr() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"bidi-string\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'bidi-string)
  (let ((rtl "العربية"))
    (list (string-mark-left-to-right rtl)
          (bidi-string-mark-left-to-right rtl))))
"##,
        expect,
    );
}

/// get-unicode-property-internal: internal Unicode data access.
#[test]
fn div_cx415_get_unicode_property_internal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (wrong-type-argument wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (get-unicode-property-internal 'uppercase-p ?a) (error (car e)))
      (condition-case e (get-unicode-property-internal 'lowercase-p ?A) (error (car e))))
"##,
        expect,
    );
}

/// mark-word / mark-sexp / mark-paragraph.
#[test]
fn div_cx415_mark_word_sexp_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(hello world) foo bar\nbaz qux")
  (goto-char 2)
  (mark-word 1)
  (list (mark t) (point)))
"##,
        expect,
    );
}

/// narrow-to-defun / widen.
#[test]
fn div_cx415_narrow_to_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 17 \"(defun a (x) x)\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun a (x) x)\n(defun b (y) y)\n")
  (goto-char 15)
  (narrow-to-defun)
  (list (point-min) (point-max) (buffer-string)))
"##,
        expect,
    );
}

/// transpose-sexps / transpose-words.
#[test]
fn div_cx415_transpose_sexps_words() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"world hello\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (goto-char 7)
  (transpose-words 1)
  (buffer-string))
"##,
        expect,
    );
}

/// up-list / down-list / backward-up-list.
#[test]
fn div_cx415_up_down_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 4 nil 5 nil 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a (b c) d)")
  (goto-char 6)
  (list (backward-up-list 1) (point)
        (down-list 1) (point)
        (up-list 1) (point)))
"##,
        expect,
    );
}

/// delete-horizontal-space / fixup-whitespace edge cases.
#[test]
fn div_cx415_delete_horizontal_fixup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\\n \t b\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a \t \n \t b")
  (goto-char 3)
  (delete-horizontal-space)
  (buffer-string))
"##,
        expect,
    );
}

/// mark-defun / mark-whole-buffer.
#[test]
fn div_cx415_mark_defun_whole_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (24 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo (x) (* x 2))")
  (goto-char 10)
  (mark-defun)
  (list (mark t) (point)))
"##,
        expect,
    );
}

/// string-suffix-p / string-prefix-p with multibyte.
#[test]
fn div_cx415_string_prefix_suffix_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-prefix-p "caf" "café")
        (string-prefix-p "CAF" "café" t)
        (string-suffix-p "tion" "position")
        (string-suffix-p "αγ" "αβγ")))
"##,
        expect,
    );
}
