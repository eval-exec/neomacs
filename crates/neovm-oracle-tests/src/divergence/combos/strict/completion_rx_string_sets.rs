//! Strict combo oracle probes: completion, rx, string-width/format padding,
//! cl-lib set operations, sort stability, pcase, hash-table access, string
//! unibyte/multibyte byte counting, char-fold, and text-property search.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

// --- completion (try-completion / all-completions / test-completion) -------

#[test]
fn div_crs_completion_plain_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"f\" (\"foo\" \"fort\" \"farm\") \"foo\" nil (\"b\" \"a\" \"c\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (try-completion "f" '("foo" "bar" "fort" "farm"))
      (all-completions "f" '("foo" "bar" "fort" "farm"))
      (try-completion "fo" '("foo" "bar"))
      (try-completion "z" '("foo" "bar"))
      (all-completions "" '("b" "a" "c") nil)
      (test-completion "foo" '("foo" "bar")))
"##,
        expect,
    );
}

#[test]
fn div_crs_completion_alist_and_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function make-obarray)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (try-completion "ba" '(("bar" . 1) ("baz" . 2) ("foo" . 3)))
      (all-completions "ba" '(("bar" . 1) ("baz" . 2) ("foo" . 3)))
      (let ((ob (make-obarray 17)))
        (intern "foo" ob)
        (intern "bar" ob)
        (intern "fort" ob)
        (sort (all-completions "f" ob) #'string<)))
"##,
        expect,
    );
}

// --- rx macro and rx-to-string --------------------------------------------

#[test]
fn div_crs_rx_to_string_and_macro() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:a[[:digit:]]+\\\\)\" \"\\\\(?:bar\\\\|foo\\\\)\" \"[0-9]\\\\{3\\\\}\" \"\\\\(?:ab?c\\\\)\" \"^[A-Z]+$\" \"\\\\([[:digit:]]+\\\\)-\\\\([[:digit:]]+\\\\)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (rx-to-string '(seq "a" (1+ digit)))
      (rx-to-string '(or "foo" "bar"))
      (rx-to-string '(repeat 3 (any "0-9")))
      (rx-to-string '(seq "a" (opt "b") "c"))
      (rx bol (1+ (any "A-Z")) eol)
      (rx (group (+ digit)) "-" (group (+ digit))))
"##,
        expect,
    );
}

// --- truncate-string-to-width (basic, CJK, ellipsis) ----------------------

#[test]
fn div_crs_truncate_string_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"abcd\" \"abc…\" \"abcdefg\" \"日\" \"日本\" \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (truncate-string-to-width "abcdefg" 4)
      (truncate-string-to-width "abcdefg" 4 nil nil t)
      (truncate-string-to-width "abcdefg" 10)
      (truncate-string-to-width "日本語" 2)
      (truncate-string-to-width "日本語" 4)
      (truncate-string-to-width "abc" 5))
"##,
        expect,
    );
}

// --- format %s field width with multibyte content -------------------------

#[test]
fn div_crs_format_width_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"   ab|\" \"ab   |\" \" 日本|\" \"日本 |\" \"abcde|\" \"   abc|\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%5s|" "ab")
      (format "%-5s|" "ab")
      (format "%5s|" "日本")
      (format "%-5s|" "日本")
      (format "%2s|" "abcde")
      (format "%6s|" "abc"))
"##,
        expect,
    );
}

// --- cl-lib set operations and dedup --------------------------------------

#[test]
fn div_crs_cl_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-intersection)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-intersection '(1 2 3 4) '(3 4 5 6))
      (cl-union '(1 2 3) '(3 4 5))
      (cl-set-difference '(1 2 3 4) '(3 4))
      (cl-set-exclusive-or '(1 2 3) '(3 4 5))
      (cl-remove-duplicates '(1 2 1 3))
      (delete-dups '(1 2 1 3 2))
      (delete-consecutive-dups '(1 1 2 2 3 1 1)))
"##,
        expect,
    );
}

// --- sort stability and predicate edge cases ------------------------------

#[test]
fn div_crs_sort_stability_and_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (sort (list 3 1 2 5 4) #'<)
      (sort (list '(1 . 1) '(1 . 2) '(1 . 3))
            (lambda (a b) (< (car a) (car b))))
      (sort (list 5 3 8 3 1 3) #'<)
      (sort (list 1 2 3 4) (lambda (_a _b) nil))
      (cl-sort (list 3 1 2) #'>))
"##,
        expect,
    );
}

// --- pcase (or, and, pred, app, backquote, `it`) --------------------------

#[test]
fn div_crs_pcase_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable it)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (pcase 3 (1 'one) (2 'two) (_ 'other))
      (pcase '(1 2) (`(,a ,b) (list a b)))
      (pcase 5 ((or 1 2 3) 'low) (_ 'high))
      (pcase 5 ((and (pred numberp) (pred (> 4))) 'gt4) (_ 'no))
      (pcase '(1 2 3) ((app length 3) 'three) (_ 'other))
      (pcase "abc" ((pred stringp) (length it)) (_ 'no)))
"##,
        expect,
    );
}

// --- hash-table access (deterministic: sorted keys) -----------------------

#[test]
fn div_crs_hash_table_keys_sorted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function hash-table-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((h (make-hash-table :test 'equal)))
  (puthash "a" 1 h)
  (puthash "b" 2 h)
  (puthash "c" 3 h)
  (list (sort (hash-table-keys h) #'string<)
        (hash-table-count h)
        (gethash "b" h)
        (gethash "z" h 'missing)
        (hash-table-p h)))
"##,
        expect,
    );
}

// --- string byte counting / unibyte<->multibyte ---------------------------

#[test]
fn div_crs_string_unibyte_multibyte_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t 3 6 2 6 2 \"é\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (multibyte-string-p "abc")
      (multibyte-string-p "日本")
      (string-bytes "abc")
      (string-bytes "日本")
      (length "日本")
      (length (string-as-unibyte "日本"))
      (length (encode-coding-string "é" 'utf-8))
      (encode-coding-string "é" 'utf-8-emacs))
"##,
        expect,
    );
}

// --- char-fold ------------------------------------------------------------

#[test]
fn div_crs_char_fold_to_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:a[\u{300}-\u{304}\u{306}-\u{30a}\u{30c}\u{30f}\u{311}\u{323}\u{325}\u{328}]\\\\|[aªà-åāăąǎǟǡǻȁȃȧᵃḁạảấầẩẫậắằẳẵặₐⓐａ𝐚𝑎𝒂𝒶𝓪𝔞𝕒𝖆𝖺𝗮𝘢𝙖𝚊]\\\\)\" \"\\\\(?:e\u{301}\\\\|é\\\\)\" \"[1¹₁①１𜳱𝟏𝟙𝟣𝟭𝟷🯱]\" 140)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-fold-to-regexp "a")
      (char-fold-to-regexp "é")
      (char-fold-to-regexp "1")
      (length (char-fold-to-regexp "aa")))
"##,
        expect,
    );
}

// --- text-property search: any / not-all / char-property-change -----------

#[test]
fn div_crs_text_property_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5 5 9 5 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaaabbbbcccc")
  (put-text-property 1 5 'face 'a)
  (put-text-property 5 9 'face 'b)
  (put-text-property 9 13 'face 'c)
  (list (text-property-any 1 13 'face 'b)
        (text-property-not-all 1 13 'face 'a)
        (next-single-property-change 1 'face)
        (previous-single-property-change 13 'face)
        (next-char-property-change 1)
        (previous-char-property-change 13)))
"##,
        expect,
    );
}
