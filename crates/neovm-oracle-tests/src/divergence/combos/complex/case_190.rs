//! Complex combo batch 190 — `regexp` / `regexp-quote` / `regexp-opt` /
//! `string-match` with special characters, anchors, and multibyte.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx190_regexp_quote_special_chars_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\".\" \"\\\\.\" 0 \".\") (\"+\" \"\\\\+\" 0 \"+\") (\"*\" \"\\\\*\" 0 \"*\") (\"?\" \"\\\\?\" 0 \"?\") (\"(\" \"(\" 0 \"(\") (\")\" \")\" 0 \")\") (\"[\" \"\\\\[\" 0 \"[\") (\"]\" \"]\" 0 \"]\") (\"{\" \"{\" 0 \"{\") (\"}\" \"}\" 0 \"}\") (\"|\" \"|\" 0 \"|\") (\"^\" \"\\\\^\" 0 \"^\") (\"$\" \"\\\\$\" 0 \"$\") (\"\\\\\" \"\\\\\\\\\" 0 \"\\\\\") (\"a.b*c?\" \"a\\\\.b\\\\*c\\\\?\" 0 \"a.b*c?\") (\"12+34\" \"12\\\\+34\" 0 \"12+34\") (\"café\" \"café\" 0 \"café\") (\"世界\" \"世界\" 0 \"世界\") (\"😀\" \"😀\" 0 \"😀\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s)
          (let ((q (regexp-quote s)))
            (list s q (string-match q s) (match-string 0 s))))
        '("." "+" "*" "?" "(" ")" "[" "]" "{" "}" "|"
          "^" "$" "\\" "a.b*c?" "12+34"
          "café" "世界" "😀"))
"##,
        expect,
    );
}

#[test]
fn div_cx190_regexp_opt_with_words_symbols() {
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
fn div_cx190_regexp_opt_with_special_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"[abc]\" t) (\"\\\\(?:alp\\\\(?:ha\\\\(?:bet\\\\)?\\\\|ine\\\\)\\\\)\" t) (\"\\\\(?:a\\\\.b\\\\|c\\\\+d\\\\|e\\\\*f\\\\)\" t) (\"\\\\(?:1\\\\(?:23?\\\\)?\\\\)\" t) (\"\\\\(?:café\\\\|naïve\\\\|résumé\\\\)\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((inputs '(("a" "b" "c")
                ("alpha" "alphabet" "alpine")
                ("a.b" "c+d" "e*f")
                ("1" "12" "123")
                ("café" "naïve" "résumé"))))
  (mapcar (lambda (words)
            (let ((opt (regexp-opt words)))
              (list opt (stringp opt))))
          inputs))
"##,
        expect,
    );
}

#[test]
fn div_cx190_string_match_with_groups_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"name\" \"世界\" 0 4 5 7 (0 7 0 4 5 7))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "name:世界 value:42"))
  (list (string-match "\\(\\w+\\):\\(\\w+\\)" s)
        (match-string 1 s)
        (match-string 2 s)
        (match-beginning 1) (match-end 1)
        (match-beginning 2) (match-end 2)
        (match-data)))
"##,
        expect,
    );
}

#[test]
fn div_cx190_looking_at_with_anchored_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"Hello\" nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Hello World")
  (goto-char 1)
  (list (looking-at "Hello")
        (match-string 0)
        (looking-at "World")
        (looking-at-p "Hello")))
"##,
        expect,
    );
}

#[test]
fn div_cx190_re_search_forward_backward_with_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaa 111 bbb 222 ccc 333 ddd")
  (goto-char 1)
  (re-search-forward "[0-9]+" nil t)
  (let ((first-num (match-string 0)))
    (re-search-forward "[0-9]+" nil t)
    (let ((second-num (match-string 0)))
      (re-search-backward "[a-z]+" nil t)
      (let ((back-str (match-string 0)))
        (list first-num second-num back-str (point)))))
"##,
        expect,
    );
}

#[test]
fn div_cx190_char_class_with_syntax_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 \"hello\" 12 \"world\" 18 \"!\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello_world 123 !@#")
  (goto-char 1)
  (list (re-search-forward "\\sw+" nil t) (match-string 0)
        (re-search-forward "\\sw+" nil t) (match-string 0)
        (re-search-forward "\\s." nil t) (match-string 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx190_regexp_with_shy_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"alpha-123\" \"123\" 0 9 6 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "alpha-123-beta-456"))
  (string-match "\\(?:\\w+\\)-\\([0-9]+\\)" s)
  (list (match-string 0 s)
        (match-string 1 s)
        (match-beginning 0) (match-end 0)
        (match-beginning 1) (match-end 1)))
"##,
        expect,
    );
}

#[test]
fn div_cx190_word_search_forward_lax_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "the quick brown fox the theory")
  (goto-char 1)
  (word-search-forward "the" t)
  (let ((first (point)))
    (word-search-forward "the" t)
    (let ((second (point)))
      (list first second))))
"##,
        expect,
    );
}

#[test]
fn div_cx190_regexp_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "name:alpha age:42 city:Tokyo name:beta age:30")
  (put-text-property 1 5 'face 'bold)
  (let ((m (set-marker (make-marker) 12))
        (ov (make-overlay 5 20)))
    (overlay-put ov 'face 'region)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 45)
    (goto-char 1)
    (let ((matches
           (cl-loop for i from 0 below 3
                    while (re-search-forward "\\(\\w+\\):\\(\\w+\\)" nil t)
                    collect (list (match-string 1) (match-string 2)))))
      (let ((state (list matches
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1)))))))
"##,
        expect,
    );
}
