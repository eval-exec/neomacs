//! Strict combo oracle probes: numeric/string formatting, regex replace,
//! coding, hashing, cl-lib accumulators, and narrowed-buffer text-property
//! combos.  These target edge cases that single-feature files tend to miss:
//! %g cutoffs, %f rounding, bignum stringification, split-string trimming,
//! replace-regexp-in-string fixed-case/subexp/function, format-spec flags,
//! CJK char/string widths, secure-hash family, regexp-opt grouping, cl-loop
//! accumulators, and cl-destructuring-bind/&rest.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- %g / %e / %f precision and rounding -----------------------------------

#[test]
fn div_fsn_g_cutoffs_and_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"0.001\" \"0.0001\" \"1e-05\" \"1\" \"100000\" \"1e+06\" \"1e-05\" \"1.23457e+08\" \"0.1\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%g" 1e-3)
      (format "%g" 1e-4)
      (format "%g" 1e-5)
      (format "%g" 1.0)
      (format "%g" 100000.0)
      (format "%g" 1000000.0)
      (format "%g" 0.00001)
      (format "%g" 123456789.0)
      (format "%.10g" 0.1))
"##,
        expect,
    );
}

#[test]
fn div_fsn_f_e_rounding_and_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"0\" \"2\" \"2\" \"4\" \"1.00\" \"2.67\" \"0.000000e+00\" \"1.235e+05\" \"000003.142\" \"-3.14e+00\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%.0f" 0.5)
      (format "%.0f" 1.5)
      (format "%.0f" 2.5)
      (format "%.0f" 3.5)
      (format "%.2f" 1.005)
      (format "%.2f" 2.675)
      (format "%e" 0.0)
      (format "%.3e" 123456.789)
      (format "%010.3f" 3.14159)
      (format "%+.2e" -3.14159))
"##,
        expect,
    );
}

#[test]
fn div_fsn_integer_format_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"+42\" \" 42\" \"00042\" \"42   |\" \"0100\" \"0xff\" \"deadbeef\" \"0XFF\" \"100\" \"101010\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%+d" 42)
      (format "% d" 42)
      (format "%05d" 42)
      (format "%-5d|" 42)
      (format "%#o" 64)
      (format "%#x" 255)
      (format "%x" 3735928559)
      (format "%#X" 255)
      (format "%o" 64)
      (format "%b" 42))
"##,
        expect,
    );
}

// --- bignum stringification and string-to-number --------------------------

#[test]
fn div_fsn_bignum_and_base_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 36)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (number-to-string 100000000000000000000)
      (format "%d" 1000000000000000000000)
      (format "%x" 1000000000000000000)
      (format "%o" 1000000000000000000)
      (string-to-number "100000000000000000000")
      (string-to-number "ff" 16)
      (string-to-number "1010" 2)
      (string-to-number "z" 36)
      (string-to-number "1e3")
      (string-to-number "  -17  "))
"##,
        expect,
    );
}

#[test]
fn div_fsn_char_format_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp (65 66 67))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%c" 65)
      (format "%c" 128578)
      (char-to-string 128578)
      (string (list 65 66 67))
      (format "%c" 945))
"##,
        expect,
    );
}

// --- split-string trimming / omit-nulls ------------------------------------

#[test]
fn div_fsn_split_string_trim_and_omit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (split-string "  a  b  c  " " +" t)
      (split-string "  a  b  c  " " +" nil)
      (split-string ",,a,,b,," "," nil t)
      (split-string ",,a,,b,," "," nil nil)
      (split-string "aa||bb||cc" "|")
      (split-string "aa||bb||cc" "|" t)
      (split-string "Remove trailing
" "\n+" t))
"##,
        expect,
    );
}

// --- replace-regexp-in-string: fixed-case, function, subexp ---------------

#[test]
fn div_fsn_replace_regexp_in_string_modes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"a#b#c#\" \"******\" \"Hello World\" \"bar bar bar\" \"Bar bar BAR\" \"a[1]b[2]\" \"f0000\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (replace-regexp-in-string "[0-9]+" "#" "a1b22c333")
      (replace-regexp-in-string "[a-z]" "*" "AbCdEf")
      (replace-regexp-in-string "\\b\\w" 'upcase "hello world")
      (replace-regexp-in-string "foo" "bar" "Foo foo FOO" t)
      (replace-regexp-in-string "foo" "bar" "Foo foo FOO")
      (replace-regexp-in-string "\\([0-9]\\)" "[\\1]" "a1b2")
      (replace-regexp-in-string "o" "0" "foooo"))
"##,
        expect,
    );
}

#[test]
fn div_fsn_replace_match_subexp_and_literal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function match-substring)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (progn (string-match "ell" "hello") (replace-match "ELL" nil nil "hello"))
      (progn (string-match "\\(o\\)" "foo") (replace-match "0" t t "foo" 1))
      (progn (string-match "x" "axb") (match-data t))
      (progn (string-match "\\(a\\)\\(b\\)" "xabz")
             (list (match-beginning 0) (match-end 0)
                   (match-beginning 1) (match-end 1)
                   (match-beginning 2) (match-end 2)
                   (match-substring 1)))
      (replace-regexp-in-string "\\([a-z]\\)\\1" "<\\1\\1>" "aabbc"))
"##,
        expect,
    );
}

// --- format-spec width / flags / escaping ---------------------------------

#[test]
fn div_fsn_format_spec_flags() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"x-y-z\" \" x|yz\" \"x | y\" \"100% done: yes\" \"[   x]\" \"[x   ]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format-spec "%a-%b-%c" '((?a . "x") (?b . "y") (?c . "z")))
      (format-spec "%2a|%2b" '((?a . "x") (?b . "yz")))
      (format-spec "%-2a|%2b" '((?a . "x") (?b . "y")))
      (format-spec "100%% done: %p" '((?p . "yes")))
      (format-spec "[%4a]" '((?a . "x")))
      (format-spec "[%-4a]" '((?a . "x"))))
"##,
        expect,
    );
}

// --- CJK char/string width -------------------------------------------------

#[test]
fn div_fsn_cjk_char_string_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3 2 4 0 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-width ?a)
      (string-width "abc")
      (char-width ?一)
      (string-width "a一b")
      (char-width ?\N{TIBETAN VOWEL SIGN AA})
      (string-width "日本語テスト"))
"##,
        expect,
    );
}

// --- secure-hash family ----------------------------------------------------

#[test]
fn div_fsn_secure_hash_family() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"900150983cd24fb0d6963f7d28e17f72\" \"a9993e364706816aba3e25717850c26c9cd0d89d\" \"23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7\" \"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad\" \"cb00753f45a35e8bb5a03d699ac65007272c32ab0eded1631a8b605a43ff5bed8086072ba1e7cc2358baeca134c825a7\" \"ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f\" \"a9993e364706816aba3e25717850c26c9cd0d89d\" \"900150983cd24fb0d6963f7d28e17f72\" \"da39a3ee5e6b4b0d3255bfef95601890afd80709\" \"d7a8fbb307d7809469ca9abcb0082e4f8d5651e46d3cdb762d02d0bf37c9e592\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (secure-hash 'md5 "abc")
      (secure-hash 'sha1 "abc")
      (secure-hash 'sha224 "abc")
      (secure-hash 'sha256 "abc")
      (secure-hash 'sha384 "abc")
      (secure-hash 'sha512 "abc")
      (sha1 "abc")
      (md5 "abc")
      (secure-hash 'sha1 "")
      (secure-hash 'sha256 "The quick brown fox jumps over the lazy dog"))
"##,
        expect,
    );
}

// --- regexp-opt grouping / regexp-quote ------------------------------------

#[test]
fn div_fsn_regexp_opt_and_quote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:ba[rz]\\\\|foo\\\\)\" \"\\\\(ba[rz]\\\\|foo\\\\)\" \"a\\\\.b\\\\*c\\\\+d\\\\?\" \"\\\\(?:cat\\\\(?:alog\\\\|egory\\\\)?\\\\)\" \"\\\\(x\\\\(?:xx?\\\\)?\\\\)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (regexp-opt '("foo" "bar" "baz"))
      (regexp-opt '("foo" "bar" "baz") t)
      (regexp-quote "a.b*c+d?")
      (regexp-opt '("cat" "category" "catalog") nil)
      (regexp-opt '("x" "xx" "xxx") t))
"##,
        expect,
    );
}

// --- base64 roundtrips -----------------------------------------------------

#[test]
fn div_fsn_base64_roundtrips() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"aGVsbG8gd29ybGQ=\" \"aGVsbG8gd29ybGQ=\" \"hello\" \"Pz8=\" \"YS9iK2M9\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (base64-encode-string "hello world")
      (base64-encode-string "hello world" t)
      (base64-decode-string (base64-encode-string "hello"))
      (base64url-encode-string "??")
      (base64url-encode-string "a/b+c="))
"##,
        expect,
    );
}

// --- cl-loop accumulators --------------------------------------------------

#[test]
fn div_fsn_cl_loop_accumulators() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-loop for x in '(1 2 3 4) sum x)
      (cl-loop for x in '(1 2 3 4) count (= (% x 2) 0))
      (cl-loop for x in '(1 2 3) maximize x into m finally (return m))
      (cl-loop for x in '(1 2 3) minimize x into m finally (return m))
      (cl-loop for x on '(1 2 3) collect (length x))
      (cl-loop for i from 1 to 10 by 2 collect i)
      (cl-loop for i from 10 downto 1 by 3 collect i)
      (cl-loop for x across [1 2 3] collect (* x x))
      (cl-loop for x in '(1 2 3) append (list x x))
      (cl-loop for x in '(1 2 3) sum (* x x)))
"##,
        expect,
    );
}

#[test]
fn div_fsn_cl_destructure_labels_coerce() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-destructuring-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-destructuring-bind (a (b c) &rest d) '(1 (2 3) 4 5) (list a b c d))
      (cl-destructuring-bind (&key a b) '(:a 1 :b 2) (list a b))
      (cl-labels ((fact (n) (if (<= n 1) 1 (* n (fact (1- n)))))) (fact 6))
      (cl-coerce "abc" 'list)
      (cl-coerce 65 'character)
      (cl-remove-duplicates '(1 2 1 3 2 4) :test #'=)
      (cl-sort (list 3 1 2) #'<)
      (cl-subseq "abcdef" 1 4)
      (cl-position 2 '(1 2 3 2 1))
      (cl-substitute 9 2 '(1 2 3 2 1)))
"##,
        expect,
    );
}

// --- string comparison / version ------------------------------------------

#[test]
fn div_fsn_compare_and_version() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 t 1 t t t t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (compare-strings "abcdef" 0 6 "abcxyz" 0 3)
      (compare-strings "abcdef" 0 6 "abcdef" 0 6)
      (compare-strings "abcdef" 0 6 "ABCDEF" 0 6)
      (compare-strings "abcdef" 0 6 "ABCDEF" 0 6 t)
      (string-version-lessp "foo2" "foo10")
      (string-version-lessp "1.0" "1.10")
      (string-lessp "abc" "abd")
      (string-lessp "abc" "abc")
      (version-list-< '(1 2 3) '(1 2 10)))
"##,
        expect,
    );
}

// --- concat / store-substring / make-string / coding ----------------------

#[test]
fn div_fsn_string_ops_and_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"abXdef\" \"*****\" \"🙂🙂🙂\" \"abbc\" \"bcd\" \"ab\" \"�\" 1 \"café\" 5 \"café\" \"�\" 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (store-substring "abcdef" 2 ?X)
      (make-string 5 ?*)
      (make-string 3 128578)
      (concat "a" (make-string 2 ?b) "c")
      (substring "abcdef" 1 -2)
      (substring-no-properties (propertize "abc" 'face 'bold) 0 2)
      (string-make-unibyte (string 200))
      (length (string-make-unibyte (string 200)))
      (encode-coding-string "café" 'utf-8)
      (length (encode-coding-string "café" 'utf-8))
      (decode-coding-string (encode-coding-string "café" 'utf-8) 'utf-8)
      (encode-coding-string (string 200) 'iso-8859-1)
      (length (encode-coding-string (string 200) 'iso-8859-1)))
"##,
        expect,
    );
}

// --- narrowed buffer + text-property combo --------------------------------

#[test]
fn div_fsn_narrow_textprop_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"cdefg\" 0 3 (weight heavy face bold)) 3 8 (weight heavy face bold) bold 6 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (add-text-properties 2 6 '(face bold weight heavy))
  (narrow-to-region 3 8)
  (list (buffer-string)
        (point-min) (point-max)
        (text-properties-at 4)
        (get-text-property 4 'face)
        (next-single-property-change 3 'face)
        (previous-single-property-change 7 'face)))
"##,
        expect,
    );
}

// --- mapcar/mapconcat over mixed types -------------------------------------

#[test]
fn div_fsn_mapcar_mapconcat_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((2 3 4) (1 3) \"a-b-c\" \"1,2,3\" (97 98 99) (\"A\" \"B\" \"C\") \"xyz\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (mapcar #'1+ '(1 2 3))
      (mapcar #'car '((1 . 2) (3 . 4)))
      (mapconcat #'identity '("a" "b" "c") "-")
      (mapconcat #'number-to-string '(1 2 3) ",")
      (mapcar #'identity "abc")
      (mapcar #'char-to-string "ABC")
      (apply #'concat (mapcar #'char-to-string "xyz")))
"##,
        expect,
    );
}
