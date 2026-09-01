//! Comprehensive oracle parity tests for string comparison operations.
//!
//! Tests `string=`/`string-equal`, `string<`/`string-lessp`, `string>`/`string-greaterp`,
//! `string-collate-lessp`, `string-collate-equalp`, `compare-strings` with all params,
//! `string-version-lessp`, `string-prefix-p`/`string-suffix-p` with IGNORE-CASE,
//! locale-dependent comparisons, and mixed case comparisons.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string= / string-equal: exact equality with various types
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_equal_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic equal strings
  (string= "hello" "hello")
  ;; Different strings
  (string= "hello" "world")
  ;; Empty strings
  (string= "" "")
  ;; One empty, one not
  (string= "" "a")
  ;; Case sensitivity: not equal
  (string= "Hello" "hello")
  ;; string-equal is alias for string=
  (string-equal "foo" "foo")
  (string-equal "foo" "bar")
  ;; Symbols compared as their names
  (string= 'hello "hello")
  (string= 'abc 'abc)
  ;; Unicode strings
  (string= "cafe\u0301" "cafe\u0301")
  ;; Different unicode normalization yields different
  (string= "\u00e9" "e\u0301")
  ;; Strings with special characters
  (string= "line1\nline2" "line1\nline2")
  ;; Very long matching strings
  (string= (make-string 1000 ?a) (make-string 1000 ?a))
  ;; Very long differing strings
  (string= (make-string 1000 ?a) (make-string 1000 ?b)))"#;
    let expect = expect_test::expect![[r#""OK (t nil t nil nil t nil t t t nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_equal_ignore_case_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; GNU subr.el defines this via compare-strings with IGNORE-CASE non-nil.
  (string-equal-ignore-case "Hello" "hello")
  (string-equal-ignore-case "Hello" "HELLO")
  (string-equal-ignore-case "Hello" "Hell")
  (string-equal-ignore-case "" "")
  (string-equal-ignore-case "Straße" "straße")
  ;; GNU documents that unibyte strings are converted to multibyte.
  (string-equal-ignore-case (string-as-unibyte "ABC") "abc")
  (string-equal-ignore-case "abc" (string-as-unibyte "ABC"))
  ;; Non-string operands reach compare-strings' CHECK_STRING.
  (condition-case err
      (string-equal-ignore-case 'abc "abc")
    (error (list (car err) (cadr err))))
  (condition-case err
      (string-equal-ignore-case "abc" 'abc)
    (error (list (car err) (cadr err)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t nil t t t t (wrong-type-argument stringp) (wrong-type-argument stringp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string< / string-lessp: lexicographic less-than
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_lessp_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic ordering
  (string< "abc" "abd")
  (string< "abd" "abc")
  ;; Equal strings: not less
  (string< "same" "same")
  ;; Prefix is less than longer string
  (string< "ab" "abc")
  (string< "abc" "ab")
  ;; Empty string is less than any non-empty
  (string< "" "a")
  (string< "a" "")
  ;; Case: uppercase letters are less than lowercase in ASCII
  (string< "A" "a")
  (string< "a" "A")
  ;; Numeric characters
  (string< "1" "2")
  (string< "9" "10")
  ;; string-lessp is alias
  (string-lessp "alpha" "beta")
  ;; Symbols work too
  (string< 'abc 'abd)
  ;; Mixed symbol and string
  (string< 'abc "abd"))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t nil t nil t nil t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string> / string-greaterp: lexicographic greater-than
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_greaterp_ordering() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic ordering
  (string> "xyz" "abc")
  (string> "abc" "xyz")
  ;; Equal strings: not greater
  (string> "same" "same")
  ;; Longer string is greater than its prefix
  (string> "abc" "ab")
  (string> "ab" "abc")
  ;; Empty vs non-empty
  (string> "a" "")
  (string> "" "a")
  ;; string-greaterp is alias
  (string-greaterp "z" "a")
  ;; Consistent with string<: exactly one of <, =, > is true
  (let ((a "hello") (b "world"))
    (list (string< a b)
          (string= a b)
          (string> a b)))
  ;; Symbols
  (string> 'zebra 'alpha))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t nil t nil t (t nil nil) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_string_greaterp_symbol_and_error_contract() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU subr.el defines string-greaterp by reversing the arguments to
    // string-lessp.  GNU src/fns.c:string-lessp accepts symbols by their print
    // names and rejects non-symbol, non-string inputs before comparison.
    let form = r#"(list
 (string-greaterp "b" "a")
 (string-greaterp "a" "b")
 (string-greaterp "a" "a")
 (string-greaterp :beta :alpha)
 (string-greaterp :alpha "beta")
 (condition-case e
     (string-greaterp 1 "a")
   (error (list (car e) (cadr e))))
 (condition-case e
     (string-greaterp "a")
   (error (list (car e) (cadr e)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil nil t nil (wrong-type-argument stringp) (wrong-number-of-arguments (2 . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// compare-strings: all parameters (STR1 START1 END1 STR2 START2 END2 IGNORE-CASE)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_compare_strings_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Full strings match: returns t
  (compare-strings "hello" nil nil "hello" nil nil)
  ;; Full strings differ: returns position+1 (negative if s1 < s2)
  (compare-strings "abc" nil nil "abd" nil nil)
  (compare-strings "abd" nil nil "abc" nil nil)
  ;; Substring comparison via START1/END1
  (compare-strings "hello world" 6 11 "world" nil nil)
  ;; Substring comparison via START2/END2
  (compare-strings "world" nil nil "hello world" 6 11)
  ;; Both substrings
  (compare-strings "xxhelloxx" 2 7 "yyhelloyy" 2 7)
  ;; Case-insensitive: IGNORE-CASE = t
  (compare-strings "Hello" nil nil "hello" nil nil t)
  (compare-strings "HELLO" nil nil "hello" nil nil t)
  ;; Case-sensitive (default): not equal
  (compare-strings "Hello" nil nil "hello" nil nil nil)
  ;; Partial overlap: first 3 chars match, differ at 4th
  (compare-strings "abcx" nil nil "abcy" nil nil)
  ;; Length mismatch: shorter string is "less"
  (compare-strings "ab" nil nil "abc" nil nil)
  ;; START at 0 is same as nil
  (compare-strings "hello" 0 nil "hello" 0 nil)
  ;; END = length is same as nil
  (compare-strings "hello" nil 5 "hello" nil 5)
  ;; Case-insensitive with mixed ASCII
  (compare-strings "FoO bAr" nil nil "foo bar" nil nil t))"#;
    let expect = expect_test::expect![[r#""OK (t -3 3 t t t t t -1 -4 -3 t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-version-lessp: version-aware comparison
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_version_lessp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Numeric parts compared numerically
  (string-version-lessp "file1" "file2")
  (string-version-lessp "file2" "file10")
  (string-version-lessp "file10" "file2")
  ;; Version numbers
  (string-version-lessp "1.2.3" "1.2.10")
  (string-version-lessp "1.2.10" "1.2.3")
  (string-version-lessp "1.9" "1.10")
  ;; Same version
  (string-version-lessp "1.0" "1.0")
  ;; Leading zeros
  (string-version-lessp "1.01" "1.1")
  ;; Mixed alpha-numeric
  (string-version-lessp "abc2def" "abc10def")
  ;; Pure alphabetic falls back to string<
  (string-version-lessp "alpha" "beta")
  ;; Empty strings
  (string-version-lessp "" "1")
  ;; Real-world versions
  (string-version-lessp "emacs-27.1" "emacs-28.2")
  (string-version-lessp "v2.0.0" "v10.0.0"))"#;
    let expect = expect_test::expect![[r#""OK (t t nil t nil t nil nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-prefix-p / string-suffix-p with IGNORE-CASE
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_prefix_suffix_ignore_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; string-prefix-p basic
  (string-prefix-p "hel" "hello")
  (string-prefix-p "world" "hello")
  ;; string-prefix-p: exact match is a prefix
  (string-prefix-p "hello" "hello")
  ;; Empty prefix always matches
  (string-prefix-p "" "anything")
  ;; Prefix longer than string
  (string-prefix-p "hello world" "hello")
  ;; Case sensitive by default
  (string-prefix-p "HEL" "hello")
  ;; Case insensitive
  (string-prefix-p "HEL" "hello" t)
  (string-prefix-p "Hello" "HELLO WORLD" t)
  ;; string-suffix-p basic
  (string-suffix-p "llo" "hello")
  (string-suffix-p "world" "hello")
  ;; Exact match is a suffix
  (string-suffix-p "hello" "hello")
  ;; Empty suffix always matches
  (string-suffix-p "" "anything")
  ;; Suffix longer than string
  (string-suffix-p "hello world" "hello")
  ;; Case sensitive by default
  (string-suffix-p "LLO" "hello")
  ;; Case insensitive
  (string-suffix-p "LLO" "hello" t)
  (string-suffix-p ".TXT" "readme.txt" t))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t nil nil t t t nil t t nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_string_prefix_suffix_strict_edge_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU lisp/subr.el:string-prefix-p and string-suffix-p call `length`
    // before `compare-strings`.  Sequence-like non-strings can therefore
    // return nil when the affix is too long, but signal once compare-strings
    // is reached.
    let form = r#"
(let ((capture (lambda (form)
                 (condition-case err
                     (eval form)
                   (error (cons (car err) (cdr err)))))))
  (list
   (string-prefix-p "abcd" ["a" "b" "c"])
   (string-suffix-p "abcd" ["a" "b" "c"])
   (funcall capture '(string-prefix-p "a" ["a" "b"]))
   (funcall capture '(string-suffix-p "b" ["a" "b"]))
   (funcall capture '(string-prefix-p "" 42))
   (funcall capture '(string-suffix-p "" 42))
   (funcall capture '(string-prefix-p nil "abc"))
   (funcall capture '(string-suffix-p nil "abc"))
   (string-prefix-p "A" "abc" '(non-nil-ignore-case))
   (string-suffix-p "C" "abc" '(non-nil-ignore-case))
   (funcall capture '(string-prefix-p "x"))
   (funcall capture '(string-suffix-p "x" "x" nil 'extra))))
"#;
    let expect = expect_test::expect![[
        r#""OK (nil nil (wrong-type-argument stringp [\"a\" \"b\"]) (wrong-type-argument stringp [\"a\" \"b\"]) (wrong-type-argument sequencep 42) (wrong-type-argument sequencep 42) (wrong-type-argument stringp nil) (wrong-type-argument stringp nil) t t (wrong-number-of-arguments (2 . 3) 1) (wrong-number-of-arguments (2 . 3) 4))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Mixed case and locale-sensitive comparisons
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_mixed_case_locale() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Case-folding: downcase comparison
  (string= (downcase "Hello World") (downcase "hello world"))
  ;; Case-folding: upcase comparison
  (string= (upcase "Hello World") (upcase "HELLO WORLD"))
  ;; Sorting a list of strings case-insensitively
  (sort (list "Banana" "apple" "Cherry" "date")
        (lambda (a b) (string< (downcase a) (downcase b))))
  ;; Mixed-case binary search: find position in sorted list
  (let ((sorted '("Alpha" "Beta" "gamma" "DELTA")))
    (sort (copy-sequence sorted)
          (lambda (a b) (string< (downcase a) (downcase b)))))
  ;; compare-strings case-insensitive on various pairs
  (list
   (compare-strings "straße" nil nil "STRASSE" nil nil t)
   (compare-strings "Abc" nil nil "aBC" nil nil t)
   (compare-strings "Z" nil nil "a" nil nil t))
  ;; Comprehensive ordering: verify transitivity
  (let ((a "abc") (b "abd") (c "abe"))
    (list (string< a b)
          (string< b c)
          (string< a c)))  ;; all should be t
  ;; Unicode strings ordering
  (string< "café" "caff"))"#;
    let expect = expect_test::expect![[
        r#""OK (t t (\"apple\" \"Banana\" \"Cherry\" \"date\") (\"Alpha\" \"Beta\" \"DELTA\" \"gamma\") (5 t 1) (t t t) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-collate-lessp / string-collate-equalp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_collate_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; string-collate-lessp uses locale-aware ordering
  ;; With nil locale, uses current locale
  (string-collate-lessp "abc" "abd")
  (string-collate-lessp "abd" "abc")
  ;; Equal strings
  (string-collate-lessp "same" "same")
  ;; string-collate-equalp: locale-aware equality
  (string-collate-equalp "hello" "hello")
  (string-collate-equalp "hello" "world")
  ;; With explicit locale "POSIX" (basic ASCII ordering)
  (string-collate-lessp "a" "b" "POSIX")
  (string-collate-lessp "A" "a" "POSIX")
  (string-collate-equalp "abc" "abc" "POSIX")
  ;; IGNORE-CASE parameter
  (string-collate-equalp "Hello" "hello" nil t)
  (string-collate-lessp "A" "b" nil t))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil t nil t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_collate_argument_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; GNU accepts symbols for S1/S2 by comparing their print names.
  (string-collate-lessp 'alpha 'beta "POSIX")
  (string-collate-equalp 'same "same" "POSIX")
  ;; GNU's fns.c checks that non-nil LOCALE is a string before calling str_collate.
  (condition-case err
      (string-collate-lessp "a" "b" 42)
    (error (list (car err) (cadr err))))
  (condition-case err
      (string-collate-equalp "a" "a" 'not-a-locale)
    (error (list (car err) (cadr err)))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t (wrong-type-argument stringp) (wrong-type-argument stringp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_collate_invalid_locale_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; GNU's str_collate opens explicit locale strings with newlocale.
  ;; Invalid locale names signal `error' instead of falling back to byte comparison.
  (condition-case err
      (string-collate-lessp "a" "b" "neomacs-invalid-locale")
    (error (car err)))
  (condition-case err
      (string-collate-equalp "a" "a" "neomacs-invalid-locale")
    (error (car err))))"#;
    let expect = expect_test::expect![[r#""OK (error error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases and comprehensive combination tests
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Null bytes in strings
  (string< "a\0b" "a\0c")
  (string= "a\0b" "a\0b")
  ;; Single character comparisons
  (string< "a" "b")
  (string< "z" "a")
  (string= "x" "x")
  ;; Numbers as strings (lexicographic, not numeric)
  (string< "9" "10")
  (string< "100" "99")
  ;; Whitespace comparisons
  (string< " " "a")
  (string< "\t" " ")
  ;; Newline vs space
  (string< "\n" " ")
  ;; compare-strings with zero-length substrings
  (compare-strings "hello" 3 3 "world" 2 2)
  ;; string-prefix-p and string-suffix-p on single chars
  (list
   (string-prefix-p "a" "abc")
   (string-suffix-p "c" "abc")
   (string-prefix-p "x" "abc")
   (string-suffix-p "x" "abc"))
  ;; Comprehensive: sorting with all comparison functions agree
  (let ((a "apple") (b "banana"))
    (list (string< a b)
          (not (string> a b))
          (not (string= a b))
          (< (compare-strings a nil nil b nil nil) 0))))"#;
    let expect =
        expect_test::expect![[r#""OK (t t t nil t nil t t t t t (t t nil nil) (t t t t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sorting strings using various comparison predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_comparison_sort_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((words '("banana" "Apple" "cherry" "date" "ELDERBERRY" "fig")))
  (list
   ;; Sort case-sensitive (uppercase < lowercase in ASCII)
   (sort (copy-sequence words) #'string<)
   ;; Sort case-insensitive
   (sort (copy-sequence words)
         (lambda (a b) (string< (downcase a) (downcase b))))
   ;; Sort by length, then alphabetically for ties
   (sort (copy-sequence words)
         (lambda (a b)
           (or (< (length a) (length b))
               (and (= (length a) (length b))
                    (string< a b)))))
   ;; Sort version-style strings
   (sort (list "v1.10" "v1.2" "v2.0" "v1.1" "v10.0")
         #'string-version-lessp)
   ;; Reverse sort
   (sort (copy-sequence words) #'string>)
   ;; Check stability: equal elements preserve relative order
   ;; (using case-insensitive compare where "Apple" == "apple" never occurs here)
   (sort (list "a" "b" "a" "c" "b") #'string<)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Apple\" \"ELDERBERRY\" \"banana\" \"cherry\" \"date\" \"fig\") (\"Apple\" \"banana\" \"cherry\" \"date\" \"ELDERBERRY\" \"fig\") (\"fig\" \"date\" \"Apple\" \"banana\" \"cherry\" \"ELDERBERRY\") (\"v1.1\" \"v1.2\" \"v1.10\" \"v2.0\" \"v10.0\") (\"fig\" \"date\" \"cherry\" \"banana\" \"ELDERBERRY\" \"Apple\") (\"a\" \"a\" \"b\" \"b\" \"c\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
