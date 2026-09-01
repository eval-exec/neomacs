//! Advanced oracle parity tests for `string-search`: START-POS argument,
//! case sensitivity, empty string edge cases, boundary searches, repeated
//! searches to find all occurrences, combined with substring extraction,
//! and comparison with `string-match` behavior.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// START-POS: systematic exploration of offset behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_adv_start_pos_systematic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exhaustively test START-POS at every position in a string with
    // multiple occurrences, verifying each successive find.
    let form = r#"(let ((haystack "abcabcabc"))
  (list
    ;; Find "abc" starting from each valid position
    (string-search "abc" haystack 0)
    (string-search "abc" haystack 1)
    (string-search "abc" haystack 2)
    (string-search "abc" haystack 3)
    (string-search "abc" haystack 4)
    (string-search "abc" haystack 5)
    (string-search "abc" haystack 6)
    (string-search "abc" haystack 7)
    (string-search "abc" haystack 8)
    ;; START-POS exactly at string length
    (string-search "abc" haystack 9)
    ;; Single char search at every position
    (string-search "b" haystack 0)
    (string-search "b" haystack 1)
    (string-search "b" haystack 2)
    (string-search "b" haystack 4)
    (string-search "b" haystack 5)
    (string-search "b" haystack 8)))"#;
    let expect = expect_test::expect![[r#""OK (0 3 3 3 6 6 6 nil nil nil 1 1 4 4 7 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_search_adv_bignum_start_pos_error_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs fns.c:Fstring_search validates START-POS with CHECK_FIXNUM,
    // so bignums signal `fixnump` before range checks.
    let form = r#"(string-search "x" "xyz" 1000000000000000000000000000000)"#;
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument fixnump 1000000000000000000000000000000)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_string_search_start_pos_range_signal_data_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/fns.c:Fstring_search signals `args-out-of-range` with
    // exactly START-POS as the signal datum when START-POS is negative or past
    // the haystack length.
    let form = r#"
(list
 (condition-case err
     (string-search "a" "abc" -1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (string-search "" "abc" -1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (string-search "" "abc" 4)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((args-out-of-range (-1)) (args-out-of-range (-1)) (args-out-of-range (4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_search_text_properties_ignored() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let* ((plain "alpha beta alpha")
       (haystack (copy-sequence plain))
       (needle (copy-sequence "beta"))
       (empty (copy-sequence ""))
       (missing (copy-sequence "gamma")))
  (add-text-properties 0 (length haystack) '(face bold mouse-face highlight) haystack)
  (add-text-properties 0 (length needle) '(category marked) needle)
  (add-text-properties 0 (length missing) '(category absent) missing)
  (list
   ;; GNU src/fns.c:Fstring_search says text properties are ignored.
   (string-search needle haystack)
   (string-search needle haystack 6)
   (string-search "alpha" haystack)
   (string-search "alpha" haystack 1)
   (string-search missing haystack)
   (string-search empty haystack)
   (string-search empty haystack (length haystack))))"#;

    let expect = expect_test::expect![[r#""OK (6 6 0 11 nil 0 16)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case sensitivity behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_adv_case_sensitivity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-search is always case-sensitive (unlike string-match which
    // respects case-fold-search). Verify this thoroughly.
    let form = r#"(list
  ;; Exact case matches
  (string-search "Hello" "Hello World")
  (string-search "hello" "Hello World")
  (string-search "HELLO" "Hello World")
  (string-search "hELLO" "Hello World")
  ;; Mixed case in haystack, searching for each variant
  (string-search "abc" "ABCabcAbcABC")
  (string-search "ABC" "ABCabcAbcABC")
  (string-search "Abc" "ABCabcAbcABC")
  (string-search "aBc" "ABCabcAbcABC")
  ;; Verify case-fold-search does NOT affect string-search
  (let ((case-fold-search t))
    (string-search "hello" "HELLO WORLD"))
  (let ((case-fold-search nil))
    (string-search "hello" "HELLO WORLD"))
  ;; Both bindings should return nil for string-search
  (let ((case-fold-search t))
    (string-search "HELLO" "HELLO WORLD")))"#;
    let expect = expect_test::expect![[r#""OK (0 nil nil nil 3 0 6 nil nil nil 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty string needle and haystack edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_adv_empty_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty needle in non-empty haystack: returns START-POS (default 0)
  (string-search "" "hello")
  (string-search "" "hello" 0)
  (string-search "" "hello" 3)
  (string-search "" "hello" 5)
  ;; Empty needle in empty haystack
  (string-search "" "")
  (string-search "" "" 0)
  ;; Non-empty needle in empty haystack
  (string-search "a" "")
  (string-search "abc" "")
  ;; Empty needle with various START-POS values
  (string-search "" "xy" 0)
  (string-search "" "xy" 1)
  (string-search "" "xy" 2)
  ;; Single character haystack
  (string-search "" "x" 0)
  (string-search "" "x" 1)
  (string-search "x" "x" 0)
  (string-search "x" "x" 1))"#;
    let expect = expect_test::expect![[r#""OK (0 0 3 5 0 0 nil nil 0 1 2 0 1 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_string_search_adv_raw_byte_multibyte_conversion_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:Fstring_search has dedicated conversion branches for
    // unibyte non-ASCII needles in multibyte haystacks and multibyte raw-byte
    // needles in unibyte haystacks; these are not plain byte-slice searches.
    let form = r#"
(let ((unibyte-e9 (unibyte-string #xe9))
      (multibyte-eacute "é")
      (raw-byte-e9 (string-to-multibyte (unibyte-string #xe9))))
  (list
   (multibyte-string-p unibyte-e9)
   (multibyte-string-p multibyte-eacute)
   (multibyte-string-p raw-byte-e9)
   (string-search multibyte-eacute unibyte-e9)
   (string-search raw-byte-e9 unibyte-e9)
   (string-search unibyte-e9 multibyte-eacute)
   (string-search unibyte-e9 raw-byte-e9)
   (string-search "é" "xéy")
   (string-search (unibyte-string #xe9) "xéy")))
"#;
    let expect = expect_test::expect![[r#""OK (nil t t nil 0 nil 0 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Repeated searches to find all occurrences (tokenizer pattern)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_adv_find_all_occurrences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a while loop to find all positions of a substring, collecting
    // positions into a list, mimicking a simple tokenizer.
    let form = r#"(let ((find-all
         (lambda (needle haystack)
           "Return list of all positions where NEEDLE occurs in HAYSTACK."
           (let ((positions nil)
                 (start 0)
                 (nlen (length needle))
                 (pos nil))
             (while (setq pos (string-search needle haystack start))
               (setq positions (cons pos positions))
               (setq start (+ pos nlen)))
             (nreverse positions)))))
  (list
    ;; Multiple non-overlapping occurrences
    (funcall find-all "ab" "ababababab")
    ;; Single occurrence
    (funcall find-all "xyz" "abcxyzdef")
    ;; No occurrences
    (funcall find-all "zzz" "abcdef")
    ;; Needle at very start and very end
    (funcall find-all "xx" "xxmiddlexx")
    ;; Needle is the entire haystack
    (funcall find-all "hello" "hello")
    ;; Adjacent matches of single char
    (funcall find-all "a" "aaaa")
    ;; Real-world: find all comma positions for CSV parsing
    (funcall find-all "," "one,two,,four,five")
    ;; Longer needle with multiple matches
    (funcall find-all "the" "the cat on the mat ate the rat")))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 2 4 6 8) (3) nil (0 8) (0) (0 1 2 3) (3 7 8 13) (0 11 23))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Combined with substring extraction for parsing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_adv_with_substring_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use string-search + substring to implement split, field extraction,
    // and a simple URL parser.
    let form = r#"(let ((split-by
         (lambda (sep str)
           "Split STR by SEP, return list of parts."
           (let ((result nil)
                 (start 0)
                 (slen (length sep))
                 (pos nil))
             (while (setq pos (string-search sep str start))
               (setq result (cons (substring str start pos) result))
               (setq start (+ pos slen)))
             (setq result (cons (substring str start) result))
             (nreverse result)))))
  (list
    ;; Basic split
    (funcall split-by "," "a,b,c,d")
    ;; Split with multi-char separator
    (funcall split-by "::" "one::two::three")
    ;; Split with empty parts (consecutive separators)
    (funcall split-by "," "a,,b,,,c")
    ;; Split where sep is not present
    (funcall split-by ";" "no semicolons here")
    ;; Split empty string
    (funcall split-by "," "")
    ;; Parse URL-like structure: scheme://host:port/path
    (let* ((url "https://example.com:8080/api/v1/data")
           (scheme-end (string-search "://" url))
           (scheme (substring url 0 scheme-end))
           (after-scheme (substring url (+ scheme-end 3)))
           (path-start (string-search "/" after-scheme))
           (host-port (substring after-scheme 0 path-start))
           (path (substring after-scheme path-start))
           (colon-pos (string-search ":" host-port))
           (host (substring host-port 0 colon-pos))
           (port (substring host-port (1+ colon-pos))))
      (list scheme host port path))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\" \"d\") (\"one\" \"two\" \"three\") (\"a\" \"\" \"b\" \"\" \"\" \"c\") (\"no semicolons here\") (\"\") (\"https\" \"example.com\" \"8080\" \"/api/v1/data\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Comparison with string-match behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_adv_vs_string_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Compare string-search (literal substring) with string-match (regexp).
    // They should agree on position for literal patterns, but string-match
    // treats special chars as regex.
    let form = r#"(list
  ;; Both should find "hello" at position 0
  (string-search "hello" "hello world")
  (string-match "hello" "hello world")
  ;; Both find "world" at position 6
  (string-search "world" "hello world")
  (string-match "world" "hello world")
  ;; string-search finds literal "a.b", string-match treats . as any char
  (string-search "a.b" "axb a.b")
  (string-match "a\\.b" "axb a.b")
  ;; string-search for "[a]" finds the literal brackets
  (string-search "[a]" "test [a] done")
  ;; string-match with regexp-quote to get literal behavior
  (string-match (regexp-quote "[a]") "test [a] done")
  ;; Both with START parameter
  (string-search "ab" "ababab" 2)
  (string-match "ab" "ababab" 2)
  ;; string-search returns nil, string-match returns nil
  (string-search "xyz" "abcdef")
  (string-match "xyz" "abcdef")
  ;; Verify positions match for simple literal searches at various offsets
  (let ((results nil)
        (text "the quick brown fox jumps over the lazy dog"))
    (dolist (word '("the" "fox" "dog" "over" "zzz"))
      (setq results
        (cons (list word
                    (string-search word text)
                    (string-match (regexp-quote word) text))
              results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK (0 0 6 6 4 4 5 5 2 2 nil nil ((\"the\" 0 0) (\"fox\" 16 16) (\"dog\" 40 40) (\"over\" 26 26) (\"zzz\" nil nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Boundary conditions: start/end of string, single-char strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_adv_boundary_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Needle at the very start
  (string-search "abc" "abcdef")
  ;; Needle at the very end
  (string-search "def" "abcdef")
  ;; Needle is entire string
  (string-search "abcdef" "abcdef")
  ;; Needle one char longer than haystack
  (string-search "abcdefg" "abcdef")
  ;; Single char needle, single char haystack - match
  (string-search "a" "a")
  ;; Single char needle, single char haystack - no match
  (string-search "b" "a")
  ;; START-POS at last valid position
  (string-search "f" "abcdef" 5)
  (string-search "e" "abcdef" 5)
  ;; START-POS 0 with needle at position 0
  (string-search "a" "abcdef" 0)
  ;; Repeated chars: finding each subsequent one
  (let ((results nil)
        (s "aababcabcd")
        (pos 0))
    (while (and pos (< pos (length s)))
      (setq pos (string-search "abc" s pos))
      (when pos
        (setq results (cons pos results))
        (setq pos (1+ pos))))
    (nreverse results))
  ;; Overlapping potential: "aa" in "aaaa" (non-overlapping search)
  (let ((results nil)
        (s "aaaaa")
        (pos 0))
    (while (setq pos (string-search "aa" s pos))
      (setq results (cons pos results))
      (setq pos (+ pos 2)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[r#""OK (0 3 0 nil 0 nil 5 nil 0 (3 6) (0 2))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
