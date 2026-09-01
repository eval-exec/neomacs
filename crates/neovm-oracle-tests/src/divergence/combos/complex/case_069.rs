//! Complex combo batch 69 — regex / match data deep: groups, backreferences,
//! char-class syntax, case-fold, multibyte ranges, regexp-opt/quote, and
//! match-data preservation across narrowing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx69_regex_groups_and_backrefs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (0 0 19 0 5 6 11 12 15 \"hello world foo bar\" \"hello\" \"world\" \"foo\" \"bar\" (0 19 0 5 6 11 12 15 16 19))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "hello world foo bar"))
  (list (string-match "\\(\\w+\\) \\(\\w+\\) \\(\\w+\\) \\(\\w+\\)" s)
        (match-beginning 0) (match-end 0)
        (match-beginning 1) (match-end 1)
        (match-beginning 2) (match-end 2)
        (match-beginning 3) (match-end 3)
        (match-string 0 s)
        (match-string 1 s)
        (match-string 2 s)
        (match-string 3 s)
        (match-string 4 s)
        (match-data)))
"##,
        expect,
    );
}

#[test]
fn div_cx69_backreference_match_word_repeat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"hello\" nil 0 \"123\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list
 (string-match "\\(\\w+\\) \\1" "hello hello")
 (match-string 1 "hello hello")
 (string-match "\\(\\w+\\) \\1" "hello world")
 (string-match "\\([0-9]+\\)-\\1" "123-123")
 (match-string 1 "123-123"))
"##,
        expect,
    );
}

#[test]
fn div_cx69_char_class_syntax_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"a\" \"abc\" \"123\" \" \" \"!!!\" \"var_123\" \"abcXYZ\" \"abcXYZ\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (pat-and-str)
          (let ((pat (car pat-and-str))
                (str (cadr pat-and-str)))
            (if (string-match pat str)
                (match-string 0 str)
              :no-match)))
        '(("[:alpha:]+" "abc123")
          ("[[:alpha:]]+" "abc123")
          ("[[:digit:]]+" "abc123def456")
          ("[[:space:]]+" "a b  c")
          ("[[:punct:]]+" "hello!!!")
          ("[[:alnum:]_]+" "var_123!!!")
          ("[[:upper:]]+" "abcXYZ")
          ("[[:lower:]]+" "abcXYZ")))
"##,
        expect,
    );
}

#[test]
fn div_cx69_case_fold_search_vs_case_fold_in_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 0 0 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "Hello WORLD foo"))
  (list
   (let ((case-fold-search nil)) (string-match "hello" s) (match-beginning 0))
   (let ((case-fold-search t))   (string-match "hello" s) (match-beginning 0))
   (let ((case-fold-search nil)) (string-match "\\chello" s) (match-beginning 0))
   (let ((case-fold-search nil)) (string-match "\\Chello" s) (match-beginning 0))
   (let ((case-fold-search t))   (string-match "WORLD" s) (match-beginning 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx69_multibyte_ranges_in_char_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 \"世界\" 12 \"é\" 17 \"123\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "hello 世界 café 世界 123"))
  (list (string-match "[一-鿿]+" s)
        (match-string 0 s)
        (string-match "[à-ÿ]+" s)
        (match-string 0 s)
        (string-match "[0-9]+" s)
        (match-string 0 s)))
"##,
        expect,
    );
}

#[test]
fn div_cx69_regexp_opt_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:ap\\\\(?:ple\\\\|ricot\\\\)\\\\|b\\\\(?:anana\\\\|erry\\\\)\\\\|cherry\\\\)\" \"\\\\<\\\\(ap\\\\(?:ple\\\\|ricot\\\\)\\\\|b\\\\(?:anana\\\\|erry\\\\)\\\\|cherry\\\\)\\\\>\" \"\\\\_<\\\\(ap\\\\(?:ple\\\\|ricot\\\\)\\\\|b\\\\(?:anana\\\\|erry\\\\)\\\\|cherry\\\\)\\\\_>\" 7 \"apple\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((words '("apple" "apricot" "banana" "berry" "cherry")))
  (let ((opt-none (regexp-opt words nil))
        (opt-words (regexp-opt words 'words))
        (opt-symbols (regexp-opt words 'symbols)))
    (list opt-none opt-words opt-symbols
          (string-match opt-none "I love apple pie")
          (match-string 0 "I love apple pie"))))
"##,
        expect,
    );
}

#[test]
fn div_cx69_regexp_quote_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\.\" 0 \".\") (\"\\\\+\" 0 \"+\") (\"\\\\*\" 0 \"*\") (\"\\\\?\" 0 \"?\") (\"(\" 0 \"(\") (\")\" 0 \")\") (\"\\\\[\" 0 \"[\") (\"]\" 0 \"]\") (\"{\" 0 \"{\") (\"}\" 0 \"}\") (\"|\" 0 \"|\") (\"\\\\^\" 0 \"^\") (\"\\\\$\" 0 \"$\") (\"\\\\\\\\\" 0 \"\\\\\") (\"a\\\\.b\\\\*c\\\\?\" 0 \"a.b*c?\") (\"12\\\\+34\" 0 \"12+34\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (let ((q (regexp-quote s)))
            (list q (string-match q s) (match-string 0 s))))
        '("." "+" "*" "?" "(" ")" "[" "]" "{" "}" "|"
          "^" "$" "\\" "a.b*c?" "12+34"))
"##,
        expect,
    );
}

#[test]
fn div_cx69_looking_at_and_re_search_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"alpha\" 10 \"123\" 5 \"a\" 10 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "alpha 123\nbeta 456\ngamma 789\n")
  (goto-char 1)
  (list (looking-at "[a-z]+")
        (match-string 0)
        (re-search-forward "[0-9]+")
        (match-string 0)
        (re-search-backward "[a-z]+")
        (match-string 0)
        (re-search-forward "[0-9]+" nil t)
        (point)))
"##,
        expect,
    );
}

#[test]
fn div_cx69_match_data_save_and_restore_with_set_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 0 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (saved)
  (with-temp-buffer
    (insert "first second third")
    (string-match "\\(\\w+\\) \\(\\w+\\) \\(\\w+\\)" (buffer-string))
    (setq saved (match-data))
    (string-match "no match here" "different string")
    (set-match-data saved)
    (list (match-data)
          (match-string 1)
          (match-string 2)
          (match-string 3))))
"##,
        expect,
    );
}

#[test]
fn div_cx69_skip_chars_forward_backward_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 9 10 15 16 19)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "   hello world 123\n")
  (goto-char 1)
  (skip-chars-forward " \t")
  (let ((p1 (point)))
    (skip-syntax-forward "w")
    (let ((p2 (point)))
      (skip-chars-forward " ")
      (let ((p3 (point)))
        (skip-syntax-forward "w")
        (let ((p4 (point)))
          (skip-chars-forward " ")
          (let ((p5 (point)))
            (skip-syntax-forward "w")
            (list p1 p2 p3 p4 p5 (point))))))))
"##,
        expect,
    );
}

#[test]
fn div_cx69_replace_match_with_backref_substitution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"third second first\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "first second third")
  (goto-char 1)
  (re-search-forward "\\(\\w+\\) \\(\\w+\\) \\(\\w+\\)")
  (replace-match "\\3 \\2 \\1")
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx69_replace_regexp_in_buffer_global() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"let foo = bar;\\nlet baz = qux;\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "var foo = bar;\nvar baz = qux;\n")
  (goto-char 1)
  (while (re-search-forward "var \\(\\w+\\) = \\(\\w+\\);" nil t)
    (replace-match "let \\1 = \\2;"))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx69_regex_match_replace_marker_overlay_narrow_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "alpha 123 beta 456 gamma 789 delta 012 epsilon")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 7 11 'face 'italic)
  (let ((m (set-marker (make-marker) 12))
        (ov (make-overlay 5 20)))
    (overlay-put ov 'face 'region)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 4 45)
    (undo-boundary)
    (let ((case-fold-search nil))
      (goto-char 1)
      (while (re-search-forward "\\b[a-z]+\\b" nil t)
        (replace-match (upcase (match-string 0))))
    (let ((state (list (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (point-min) (point-max)
                       (text-properties-at 1))))
      (undo) (undo)
      (widen)
      (list state (buffer-string)
            (marker-position m)
            (overlayp ov) (overlay-start ov)
            (text-properties-at 1)
            (point-min) (point-max)))))
"##,
        expect,
    );
}
