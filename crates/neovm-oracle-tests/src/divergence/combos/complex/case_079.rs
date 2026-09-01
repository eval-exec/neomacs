//! Complex combo batch 79 — strings / multibyte / byte operations deep:
//! `format` with mixed multibyte, `string-make-unibyte`/`string-make-multibyte`,
//! `encode-coding-string` matrix, `string-to-multibyte`, `byte-to-string`,
//! and `string-make-unibyte` with non-ASCII.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx79_string_make_unibyte_with_multibyte_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\" t 4 5 \"caf�\" 4 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café"))
  (list s
        (multibyte-string-p s)
        (length s)
        (string-bytes s)
        (string-make-unibyte s)
        (length (string-make-unibyte s))
        (multibyte-string-p (string-make-unibyte s))))
"##,
        expect,
    );
}

#[test]
fn div_cx79_string_make_multibyte_with_unibyte_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"���\" nil \"\\200\\201\\202\" t 3 6 4194176 eight-bit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((uni (unibyte-string #x80 #x81 #x82))
       (mb (string-make-multibyte uni)))
  (list uni
        (multibyte-string-p uni)
        mb
        (multibyte-string-p mb)
        (length mb)
        (string-bytes mb)
        (aref mb 0)
        (char-charset (aref mb 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx79_string_to_multibyte_vs_make_multibyte_divergence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\\200\\303\\251\" \"\\200\\303\\251\" t nil 3 3 6 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((uni (unibyte-string #x80 #xc3 #xa9))   ; é as latin-1
       (stmb (string-to-multibyte uni))
       (smmb (string-make-multibyte uni)))
  (list stmb
        smmb
        (equal stmb smmb)
        (eq stmb smmb)
        (length stmb)
        (length smmb)
        (string-bytes stmb)
        (string-bytes smmb)))
"##,
        expect,
    );
}

#[test]
fn div_cx79_byte_to_string_and_string_to_list_byte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"A\" \"�\" (65 66 67) [65 66 67] (65 66 67) [65 66 67] \"�\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (byte-to-string 65)
      (byte-to-string 255)
      (string-to-list "ABC")
      (string-to-vector "ABC")
      (append "ABC" nil)
      (vconcat "ABC")
      (string-make-unibyte (byte-to-string 255)))
"##,
        expect,
    );
}

#[test]
fn div_cx79_encode_coding_string_per_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 11 nil) (utf-8-with-signature 14 nil) (latin-1 6 nil) (iso-8859-1 6 nil) (iso-8859-9 6 nil) (utf-16 14 nil) (utf-16le 12 nil) (utf-16be 12 nil) (big5 8 nil) (gb2312 9 nil) (no-conversion 11 nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café世界"))
  (mapcar (lambda (cs)
            (condition-case e
                (let ((enc (encode-coding-string s cs)))
                  (list cs (string-bytes enc) (multibyte-string-p enc)))
              (error (list cs :err (car e)))))
          '(utf-8 utf-8-with-signature
            latin-1 iso-8859-1 iso-8859-9
            utf-16 utf-16le utf-16be
            big5 gb2312 no-conversion)))
"##,
        expect,
    );
}

#[test]
fn div_cx79_decode_coding_string_per_coding_with_invalid_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 7 9 (ascii ascii ascii ascii ascii eight-bit unicode-bmp)) (latin-1 8 11 (ascii ascii ascii ascii ascii unicode-bmp unicode-bmp unicode-bmp)) (iso-8859-1 8 11 (ascii ascii ascii ascii ascii unicode-bmp unicode-bmp unicode-bmp)) (no-conversion 8 8 (ascii ascii ascii ascii ascii unicode-bmp unicode-bmp unicode-bmp)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string #x68 #x65 #x6c #x6c #x6f #xff #xc3 #xa9)))
  (mapcar (lambda (cs)
            (condition-case e
                (let ((dec (decode-coding-string raw cs t)))
                  (list cs (length dec) (string-bytes dec)
                        (mapcar #'char-charset (string-to-list dec))))
              (error (list cs :err (car e)))))
          '(utf-8 latin-1 iso-8859-1 no-conversion)))
"##,
        expect,
    );
}

#[test]
fn div_cx79_format_with_field_width_and_multibyte_padding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"               hello|\" \"hello               |\" \"                café|\" \"café                |\" \"                世界|\" \"世界                |\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%20s|" "hello")
      (format "%-20s|" "hello")
      (format "%20s|" "café")
      (format "%-20s|" "café")
      (format "%20s|" "世界")
      (format "%-20s|" "世界"))
"##,
        expect,
    );
}

#[test]
fn div_cx79_string_equal_with_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s1 (propertize "hello" 'face 'bold))
       (s2 (propertize "hello" 'face 'italic))
       (s3 "hello"))
  (list (string= s1 s2)
        (string= s1 s3)
        (equal s1 s2)
        (equal s1 s3)
        (compare-strings s1 0 5 s2 0 5)
        (compare-strings s1 0 5 s3 0 5)))
"##,
        expect,
    );
}

#[test]
fn div_cx79_string_lesp_grep_with_casefold_and_locale() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-lessp "abc" "abd")
        (string-lessp "abc" "ABC")
        (string-lessp "ABC" "abc")
        (string-version-lessp "file2.txt" "file10.txt")
        (string-version-lessp "file10.txt" "file2.txt")))
"##,
        expect,
    );
}

#[test]
fn div_cx79_string_split_join_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"alpha\" \"beta\" \"gamma\") (\"alpha\" \"beta\" \"gamma\") (\"alpha\" \"beta\" \"gamma\") (\"alpha\" \"beta\" \"gamma\") (\"alpha\" \"beta\" \"gamma\") (\"\") (\"no delimiters here\") \"alpha,beta,gamma\" \"alphabetagamma\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (split-string "alpha,beta,gamma" ",")
      (split-string "alpha, beta, gamma" ", ?")
      (split-string "alpha  beta   gamma" "[ \t]+")
      (split-string "alpha\nbeta\ngamma" "\n")
      (split-string "alpha beta gamma")
      (split-string "" ",")
      (split-string "no delimiters here" ",")
      (string-join '("alpha" "beta" "gamma") ",")
      (string-join '("alpha" "beta" "gamma")))
"##,
        expect,
    );
}

#[test]
fn div_cx79_string_replace_subst_char_in_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"abc # def #\" \"ord ith ext\" \"bXnXnX\" \"BANANA\" \"*lph*b*t\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (replace-regexp-in-string "[0-9]+" "#" "abc 123 def 456")
      (replace-regexp-in-string "\\bw\\(\\w+\\)" "\\1" "word with wext")
      (subst-char-in-string ?a ?X "banana")
      (subst-char-in-string ?a ?X "BANANA")
      (replace-regexp-in-string "[aeiou]" "*" "alphabet" t))
"##,
        expect,
    );
}

#[test]
fn div_cx79_string_operations_with_overlap_marker_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"The quick brown fox jumps over the lazy dog\" 0 8 (face bold)) 15 5 30 (face nil)) \"\" 1 nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "The quick brown fox jumps over the lazy dog")
  (put-text-property 1 9 'face 'bold)
  (let ((m (set-marker (make-marker) 15))
        (ov (make-overlay 5 30)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (perform-replace "the " "THE " t t nil)
    (let ((state (list (buffer-string)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (text-properties-at 1))))
      (undo)
      (list state
            (buffer-string)
            (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx79_trim_pad_truncate_with_multibyte_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"hello\" \"hello\" \"café\" 8 \"café\" \"café…\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-trim "   hello   ")
      (string-trim-left "   hello")
      (string-trim-right "hello   ")
      (string-trim "café   " "[ \t]+" "[ \t]+")
      (string-width "café世界")
      (truncate-string-to-width "café世界hello" 5)
      (truncate-string-to-width "café世界hello" 5 nil t "…"))
"##,
        expect,
    );
}
