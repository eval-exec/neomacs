//! Complex combo batch 114 — `concat` / `mapconcat` / `split-string` /
//! `string-join` edge cases, with separator variations, strings as input,
//! and property preservation.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx114_concat_various_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"alphabetagamma\" \"x y z\" \"\" \"pre: 42 :post\" \"abc\" \"startabc\" \"a-b-c\" \"1,2,3\" \"10/20/30\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (concat "alpha" "beta" "gamma")
      (concat "x" " " "y" " " "z")
      (concat)
      (concat "pre: " (number-to-string 42) " :post")
      (apply #'concat '("a" "b" "c"))
      (apply #'concat "start" '("a" "b" "c"))
      (mapconcat #'identity '("a" "b" "c") "-")
      (mapconcat (lambda (n) (number-to-string n)) '(1 2 3) ",")
      (mapconcat #'number-to-string [10 20 30] "/"))
"##,
        expect,
    );
}

#[test]
fn div_cx114_split_string_with_various_separators() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\" \"d\") (\"a\" \"b\" \"c\" \"d\") (\"a\" \"b\" \"c\") (\"a\" \"b\" \"c\" \"d\") (\"a\" \"b\" \"c\") (\"\") (\"single\") (\"a\" \"b\" \"c\" \"\") (\"\" \"a\" \"b\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (split-string "a,b,c,d" ",")
      (split-string "a, b, c, d" ", ?")
      (split-string "a  b   c" "[ \t]+")
      (split-string "a-b-c-d" "-")
      (split-string "a:b:c" ":" t)
      (split-string "" ",")
      (split-string "single")
      (split-string "a,b,c," ",")
      (split-string ",a,b" ","))
"##,
        expect,
    );
}

#[test]
fn div_cx114_string_join_with_separator_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"a,b,c\" \"a b c\" \"abc\" \"single\" \"\" \"a -> b -> c\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-join '("a" "b" "c") ",")
      (string-join '("a" "b" "c") " ")
      (string-join '("a" "b" "c") "")
      (string-join '("single"))
      (string-join '())
      (string-join '("a" "b" "c") " -> "))
"##,
        expect,
    );
}

#[test]
fn div_cx114_string_trim_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"hello\" \"hello\" \"hello\" \"hello\" \"hello\" \"hello\" \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-trim "   hello   ")
      (string-trim-left "   hello")
      (string-trim-right "hello   ")
      (string-trim "\n\nhello\n\n")
      (string-trim "xxhelloxx" "x+" "x+")
      (string-trim "  hello  " "[ ]+" "[ ]+")
      (string-trim-left "-----hello" "-+")
      (string-trim-right "hello-----" "-+"))
"##,
        expect,
    );
}

#[test]
fn div_cx114_string_pad_with_spaces() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"                  hi\" \"hi                  |\" \"         a|         b\" \"hello     \" \"hello-----\" \"hello\" \"hello\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%20s" "hi")
      (format "%-20s|" "hi")
      (format "%10s|%10s" "a" "b")
      (string-pad "hello" 10)
      (string-pad "hello" 10 ?-)
      (string-pad "hello" 3)
      (string-pad "hello" 5))
"##,
        expect,
    );
}

#[test]
fn div_cx114_string_replace_with_subgroups() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 6 84)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (replace-regexp-in-string "[0-9]+" "#" "abc 123 def 456 ghi 789")
      (replace-regexp-in-string "\\(\\w+\\) \\(\\w+\\)" "\\2 \\1" "alpha beta")
      (replace-regexp-in-string " +" "_" "a  b   c    d")
      (replace-regexp-in-string "[aeiou]" "*" "alphabet" t)
      (replace-regexp-in-string "\\b\\w+\\b" (lambda (m) (upcase m)) "alpha beta")))
"##,
        expect,
    );
}

#[test]
fn div_cx114_subst_char_in_string_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 5 51)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (subst-char-in-string ?a ?X "banana")
      (subst-char-in-string ?a ?X "BANANA")
      (subst-char-in-string ?- ?_ "snake-case-var")
      (subst-char-in-string ?\s ?- "with spaces")))
"##,
        expect,
    );
}

#[test]
fn div_cx114_string_to_multibyte_and_back_round_trip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"café 世界\" t 12 \"caf� \u{16}L\" nil \"caf\\351 \u{16}L\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café 世界"))
  (list s
        (multibyte-string-p s)
        (string-bytes s)
        (string-make-unibyte s)
        (multibyte-string-p (string-make-unibyte s))
        (string-make-multibyte (string-make-unibyte s))
        (equal s (string-make-multibyte (string-make-unibyte s)))))
"##,
        expect,
    );
}

#[test]
fn div_cx114_format_with_special_specifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"\\\"hello\\\"\" \"(1 \\\"two\\\" 3)\" \"[1 2 3]\" \"symbol\" \"65\" \"A\" \"42\" \"ff\" \"100\" \"1010\" \"3.140000e+00\" \"3.140000\" \"0.0001\" \"%\" \"   42\" \"42   |\" \"00042\" \"+42\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%s" "hello")
      (format "%S" "hello")
      (format "%S" '(1 "two" 3))
      (format "%S" [1 2 3])
      (format "%S" 'symbol)
      (format "%S" ?A)
      (format "%c" 65)
      (format "%d" 42)
      (format "%x" 255)
      (format "%o" 64)
      (format "%b" 10)
      (format "%e" 3.14)
      (format "%f" 3.14)
      (format "%g" 0.0001)
      (format "%%")
      (format "%5d" 42)
      (format "%-5d|" 42)
      (format "%05d" 42)
      (format "%+d" 42))
"##,
        expect,
    );
}

#[test]
fn div_cx114_string_with_text_properties_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"alpha-beta-gamma\" 0 5 (face bold) 11 16 (face italic)) (face bold) nil nil (face italic) (face italic))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s1 (propertize "alpha" 'face 'bold))
       (s2 "beta")
       (s3 (propertize "gamma" 'face 'italic))
       (combined (concat s1 "-" s2 "-" s3)))
  (list combined
        (text-properties-at 0 combined)
        (text-properties-at 5 combined)
        (text-properties-at 6 combined)
        (text-properties-at 11 combined)
        (text-properties-at 12 combined)))
"##,
        expect,
    );
}

#[test]
fn div_cx114_compare_strings_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t -3 -4 3 t 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (compare-strings "abc" 0 3 "abc" 0 3)
      (compare-strings "abc" 0 3 "abd" 0 3)
      (compare-strings "abc" 0 3 "abcd" 0 4)
      (compare-strings "abc" 0 3 "ab" 0 2)
      (compare-strings "abc" 0 3 "ABC" 0 3 t)
      (compare-strings "abc" 0 3 "ABC" 0 3 nil))
"##,
        expect,
    );
}

#[test]
fn div_cx114_concat_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((parts (mapcar (lambda (n) (format "part-%d" n)) (number-sequence 1 5)))
       (joined (string-join parts "\n")))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert joined)
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 20)
      (let ((state (list (buffer-string)
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
    );
}
