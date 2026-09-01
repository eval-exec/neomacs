//! Oracle parity tests for `string-replace`, `string-search`, and `number-sequence`.
//!
//! Tests all parameters, edge cases, and real-world combination patterns
//! for these three primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// string-replace: basic, multiple occurrences, no match
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_basic_and_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic single replacement
  (string-replace "world" "Emacs" "hello world")
  ;; Multiple occurrences
  (string-replace "o" "0" "foo boo moo")
  ;; No match — returns original string
  (string-replace "xyz" "abc" "hello world")
  ;; Replacement longer than original
  (string-replace "a" "AAA" "abracadabra")
  ;; Replacement shorter than original
  (string-replace "abc" "x" "abcdefabcghi"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello Emacs\" \"f00 b00 m00\" \"hello world\" \"AAAbrAAAcAAAdAAAbrAAA\" \"xdefxghi\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-replace: empty string FROM and TO
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_empty_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Replacing empty string inserts TOSTRING before every char and at end.
    // Replacing with empty string removes all occurrences.
    let form = r#"(list
  ;; FROM is empty: inserts TO between every character
  (string-replace "" "-" "abc")
  ;; TO is empty: removes all occurrences of FROM
  (string-replace "l" "" "hello world")
  ;; Both empty: should return original
  (string-replace "" "" "hello")
  ;; FROM empty on empty string
  (string-replace "" "x" "")
  ;; Entire string is the match
  (string-replace "hello" "" "hello"))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-length-argument 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-replace: case sensitivity and special characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_replace_case_and_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Case-sensitive: should NOT replace
  (string-replace "HELLO" "bye" "hello world")
  ;; Newlines in search and replacement
  (string-replace "\n" " " "line1\nline2\nline3")
  ;; Replace with newline
  (string-replace " " "\n" "one two three")
  ;; Backslash in strings
  (string-replace "\\" "/" "path\\to\\file")
  ;; Overlapping pattern: string-replace is non-greedy, left-to-right
  (string-replace "aa" "b" "aaaa"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"hello world\" \"line1 line2 line3\" \"one\\ntwo\\nthree\" \"path/to/file\" \"bb\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-search: all 3 params (STRING, HAYSTACK, START-POS)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic search
  (string-search "world" "hello world")
  ;; Search from start-pos (skip first occurrence)
  (string-search "o" "foo bar boo" 2)
  ;; START-POS at exact match position
  (string-search "bar" "foo bar baz" 4)
  ;; START-POS past the match — not found
  (string-search "foo" "foo bar" 4)
  ;; Not found returns nil
  (string-search "xyz" "hello world")
  ;; Search for empty string returns START-POS (or 0)
  (string-search "" "hello" 3)
  ;; START-POS = 0 explicit
  (string-search "he" "hello" 0)
  ;; Search at end of string
  (string-search "d" "world" 4))"#;
    let expect = expect_test::expect![[r#""OK (6 2 4 nil nil 3 0 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-search: edge cases and multibyte
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Search in empty haystack
  (string-search "a" "")
  ;; Empty needle in empty haystack
  (string-search "" "")
  ;; Needle longer than haystack
  (string-search "abcdef" "abc")
  ;; Needle equals haystack
  (string-search "exact" "exact")
  ;; Multiple occurrences: returns first
  (string-search "ab" "ababab")
  ;; Case-sensitive
  (string-search "Hello" "hello world")
  ;; START-POS equals length of string
  (string-search "" "abc" 3))"#;
    let expect = expect_test::expect![[r#""OK (nil 0 nil 0 0 nil 3)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// number-sequence: all params (FROM, TO, INC)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Basic ascending
  (number-sequence 1 5)
  ;; With explicit increment
  (number-sequence 0 10 2)
  ;; Increment of 3
  (number-sequence 1 10 3)
  ;; FROM = TO: single-element list
  (number-sequence 5 5)
  ;; Negative increment (descending)
  (number-sequence 10 1 -1)
  ;; Negative range with step -2
  (number-sequence 10 0 -2)
  ;; Large step that overshoots: still includes FROM
  (number-sequence 1 3 10))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 2 3 4 5) (0 2 4 6 8 10) (1 4 7 10) (5) (10 9 8 7 6 5 4 3 2 1) (10 8 6 4 2 0) (1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// number-sequence: float increment and edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_float_and_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Float increment
  (number-sequence 0.0 1.0 0.25)
  ;; Negative numbers
  (number-sequence -5 -1)
  ;; Negative to positive
  (number-sequence -3 3)
  ;; FROM only (nil TO) produces single element list
  (number-sequence 42 42)
  ;; Zero increment with FROM=TO should return (FROM)
  (number-sequence 7 7 0)
  ;; Descending with float step
  (number-sequence 1.0 0.0 -0.5))"#;
    let expect = expect_test::expect![[
        r#""OK ((0.0 0.25 0.5 0.75 1.0) (-5 -4 -3 -2 -1) (-3 -2 -1 0 1 2 3) (42) (7) (1.0 0.5 0.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: combine string-search + substring for parsing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_search_substring_parsing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse a simple key=value format using string-search and substring.
    let form = r#"(let ((parse-kv
         (lambda (str)
           (let ((eq-pos (string-search "=" str)))
             (if eq-pos
                 (cons (substring str 0 eq-pos)
                       (substring str (1+ eq-pos)))
               (cons str nil))))))
  (let ((split-lines
         (lambda (str)
           (let ((result nil)
                 (start 0)
                 (pos nil))
             (while (setq pos (string-search "\n" str start))
               (setq result (cons (substring str start pos) result)
                     start (1+ pos)))
             (when (< start (length str))
               (setq result (cons (substring str start) result)))
             (nreverse result)))))
    (let ((input "name=Alice\nage=30\ncity=Boston\nactive=true"))
      (let ((lines (funcall split-lines input)))
        (mapcar parse-kv lines)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"name\" . \"Alice\") (\"age\" . \"30\") (\"city\" . \"Boston\") (\"active\" . \"true\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
