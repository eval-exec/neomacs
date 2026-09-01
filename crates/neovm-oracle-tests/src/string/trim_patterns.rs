//! Advanced oracle parity tests for `string-trim`, `string-trim-left`,
//! and `string-trim-right` with custom TRIM-LEFT/TRIM-RIGHT regexp arguments.
//!
//! Covers: default whitespace trimming, custom character classes, multiple
//! whitespace types (tabs, newlines, spaces), trimming special characters,
//! combined patterns, empty strings, and strings that are entirely trimmed.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Default whitespace trimming with all three functions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_trim_default_whitespace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test default behavior (no custom regexp): trims whitespace from
    // beginning and/or end, including spaces, tabs, newlines, carriage returns
    let form = r####"(list
  ;; Basic space trimming
  (string-trim "  hello  ")
  (string-trim-left "  hello  ")
  (string-trim-right "  hello  ")
  ;; Tab trimming
  (string-trim "\thello\t")
  (string-trim-left "\thello\t")
  (string-trim-right "\thello\t")
  ;; Newline trimming
  (string-trim "\nhello\n")
  (string-trim-left "\nhello\n")
  (string-trim-right "\nhello\n")
  ;; Mixed whitespace: spaces, tabs, newlines, carriage returns
  (string-trim " \t\n\r hello \t\n\r ")
  (string-trim-left " \t\n\r hello \t\n\r ")
  (string-trim-right " \t\n\r hello \t\n\r ")
  ;; No whitespace to trim
  (string-trim "hello")
  (string-trim-left "hello")
  (string-trim-right "hello")
  ;; Empty string
  (string-trim "")
  (string-trim-left "")
  (string-trim-right "")
  ;; String of only whitespace
  (string-trim "   ")
  (string-trim-left "   ")
  (string-trim-right "   ")
  ;; Internal whitespace preserved
  (string-trim "  hello   world  "))"####;
    let expect = expect_test::expect![[
        r#""OK (\"hello\" \"hello  \" \"  hello\" \"hello\" \"hello\t\" \"\thello\" \"hello\" \"hello\\n\" \"\\nhello\" \"hello\" \"hello \t\\n\\r \" \" \t\\n\\r hello\" \"hello\" \"hello\" \"hello\" \"\" \"\" \"\" \"\" \"\" \"\" \"hello   world\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Custom character class trimming
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_trim_custom_character_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use custom TRIM-LEFT and TRIM-RIGHT regexp arguments to trim
    // specific character classes: digits, specific letters, punctuation
    let form = r####"(list
  ;; Trim leading/trailing digits
  (string-trim "123hello456" "[0-9]+")
  (string-trim-left "123hello456" "[0-9]+")
  (string-trim-right "123hello456" "[0-9]+")
  ;; Trim leading/trailing underscores
  (string-trim "___name___" "_+")
  (string-trim-left "___name___" "_+")
  (string-trim-right "___name___" "_+")
  ;; Trim leading/trailing dashes
  (string-trim "---title---" "-+")
  (string-trim-left "---title---" "-+")
  (string-trim-right "---title---" "-+")
  ;; Trim leading/trailing dots
  (string-trim "...text..." "\\.+")
  (string-trim-left "...text..." "\\.+")
  (string-trim-right "...text..." "\\.+")
  ;; Trim leading/trailing vowels
  (string-trim "aeiouhelloaeiou" "[aeiou]+")
  (string-trim-left "aeiouhelloaeiou" "[aeiou]+")
  (string-trim-right "aeiouhelloaeiou" "[aeiou]+")
  ;; Trim leading/trailing hash marks
  (string-trim "###comment###" "#+")
  (string-trim-left "###comment###" "#+")
  (string-trim-right "###comment###" "#+"))"####;
    let expect = expect_test::expect![[
        r####""OK (\"hello456\" \"hello456\" \"123hello\" \"name___\" \"name___\" \"___name\" \"title---\" \"title---\" \"---title\" \"text...\" \"text...\" \"...text\" \"helloaeiou\" \"helloaeiou\" \"aeiouhell\" \"comment###\" \"comment###\" \"###comment\")""####
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Asymmetric trimming: different patterns for left and right
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_trim_asymmetric_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // string-trim accepts separate TRIM-LEFT and TRIM-RIGHT patterns
    // allowing different trimming rules on each side
    let form = r####"(list
  ;; Trim digits from left, letters from right
  (string-trim "123middle_abc" "[0-9]+" "[a-z]+")
  ;; Trim spaces from left, dashes from right
  (string-trim "   content---" "[ ]+" "-+")
  ;; Trim open brackets from left, close brackets from right
  (string-trim "((([data]))" "[(]+" "[)]+")
  ;; Trim asterisks from left, exclamation from right
  (string-trim "***bold!!!!" "[*]+" "!+")
  ;; Trim whitespace from left only (empty right pattern matches nothing)
  (string-trim "  text  " "[ \t\n]+" "\\`\\'")
  ;; Trim hashes from left, percent from right
  (string-trim "##value%%" "#+$" "%+$")
  ;; Both sides same character but different amounts
  (string-trim "xxhelloxxxxx" "x+" "x+")
  ;; Left pattern matches multiple char types, right matches single
  (string-trim "  \t123data!!!" "[ \t0-9]+" "!+"))"####;
    let expect = expect_test::expect![[
        r###""OK (\"middle_\" \"content\" \"[data]\" \"bold\" \"text  \" \"##value\" \"hello\" \"data\")""###
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Trimming with complex regexp patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_trim_complex_regexp_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use more complex regexp patterns as trim arguments:
    // alternations, character ranges, special sequences
    let form = r####"(list
  ;; Trim mixed whitespace and punctuation from both ends
  (string-trim " . , hello . , " "[[:space:][:punct:]]+")
  ;; Trim anything that's not alphanumeric
  (string-trim "!!!hello world!!!" "[^a-zA-Z0-9]+")
  ;; Trim leading XML/HTML-like tags (simplified)
  (string-trim-left "<p>hello" "<[^>]*>")
  ;; Trim trailing newlines and carriage returns specifically
  (string-trim-right "hello\r\n\r\n" "[\r\n]+")
  ;; Trim zero-width: pattern that matches empty string is fine
  (string-trim "hello" "[ ]*")
  ;; Trim BOM-like leading bytes (represented as chars)
  (string-trim-left (concat (string 65279) "hello") (string 65279))
  ;; Trim leading zeros (but not all digits)
  (string-trim-left "000042" "0+")
  ;; Trim trailing whitespace and semicolons
  (string-trim-right "value;  ;" "[; \t]+"))"####;
    let expect = expect_test::expect![[
        r#""OK (\"hello . ,\" \"hello world!!!\" \"hello\" \"hello\" \"hello\" \"hello\" \"42\" \"value\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: entirely trimmed, no match, special strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_trim_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test edge cases where trimming results in empty strings,
    // patterns that don't match, single-character strings, etc.
    let form = r####"(list
  ;; Entire string is trimmed away
  (string-trim "12345" "[0-9]+")
  (string-trim "   " "[ ]+")
  (string-trim "aaa" "a+")
  ;; Pattern doesn't match anything
  (string-trim "hello" "[0-9]+")
  (string-trim-left "hello" "[0-9]+")
  (string-trim-right "hello" "[0-9]+")
  ;; Single character string
  (string-trim " " "[ ]+")
  (string-trim "a" "[a]+")
  (string-trim "x" "[^x]+")
  ;; String with only internal matches (nothing to trim from ends)
  (string-trim "a123b" "[0-9]+")
  ;; Newline-heavy string
  (string-trim "\n\n\nhello\nworld\n\n\n" "\n+")
  ;; Tab-heavy string
  (string-trim "\t\t\tdata\t\t\t" "\t+")
  ;; Mixed: trim left spaces, right nothing
  (string-trim-left "   hello" " +")
  (string-trim-right "hello   " " +")
  ;; Result type is always a string
  (stringp (string-trim "  x  "))
  (stringp (string-trim "xxx" "x+")))"####;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"\" \"\" \"hello\" \"hello\" \"hello\" \"\" \"\" \"x\" \"a123b\" \"hello\\nworld\" \"data\" \"hello\" \"hello\" t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Comprehensive multi-step string cleaning pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_trim_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a string-cleaning pipeline using string-trim functions
    // to process various formats: paths, code comments, CSV fields, etc.
    let form = r####"(progn
  ;; Clean a file path: trim slashes and whitespace
  (fset 'neovm--test-clean-path
    (lambda (path)
      (string-trim (string-trim path "[ \t]+") "/+")))

  ;; Clean a code comment: trim comment chars and whitespace
  (fset 'neovm--test-clean-comment
    (lambda (comment)
      (string-trim
       (string-trim-left
        (string-trim-left comment "[ \t]*")
        "[;#/]+[ ]*"))))

  ;; Clean a CSV field: trim whitespace and optional surrounding quotes
  (fset 'neovm--test-clean-csv-field
    (lambda (field)
      (let ((trimmed (string-trim field "[ \t]+")))
        (if (and (> (length trimmed) 1)
                 (= (aref trimmed 0) ?\")
                 (= (aref trimmed (1- (length trimmed))) ?\"))
            (substring trimmed 1 (1- (length trimmed)))
          trimmed))))

  (unwind-protect
      (list
       ;; Path cleaning
       (funcall 'neovm--test-clean-path "  /usr/local/bin/  ")
       (funcall 'neovm--test-clean-path "///home///")
       (funcall 'neovm--test-clean-path "  relative/path  ")
       ;; Comment cleaning
       (funcall 'neovm--test-clean-comment ";; This is a Lisp comment")
       (funcall 'neovm--test-clean-comment "# This is a shell comment")
       (funcall 'neovm--test-clean-comment "// This is a C comment")
       (funcall 'neovm--test-clean-comment "  ;;; Triple semicolon  ")
       ;; CSV field cleaning
       (funcall 'neovm--test-clean-csv-field "  hello  ")
       (funcall 'neovm--test-clean-csv-field "  \"quoted value\"  ")
       (funcall 'neovm--test-clean-csv-field "plain")
       (funcall 'neovm--test-clean-csv-field "  \"\"  "))
    (fmakunbound 'neovm--test-clean-path)
    (fmakunbound 'neovm--test-clean-comment)
    (fmakunbound 'neovm--test-clean-csv-field)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"usr/local/bin/\" \"home///\" \"relative/path\" \"This is a Lisp comment\" \"This is a shell comment\" \"This is a C comment\" \"Triple semicolon\" \"hello\" \"quoted value\" \"plain\" \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// string-trim consistency: trim = trim-left + trim-right
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_string_trim_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that string-trim with a single pattern is equivalent to
    // applying string-trim-left then string-trim-right (and vice versa)
    let form = r####"(let ((test-cases
       '(("  hello  " "[ ]+")
         ("123abc456" "[0-9]+")
         ("---text---" "-+")
         ("aaabbbccc" "[ac]+")
         ("\n\ndata\n\n" "\n+")
         ("xxxmiddlexxx" "x+"))))
  (let ((results nil))
    (dolist (tc test-cases)
      (let* ((str (nth 0 tc))
             (pat (nth 1 tc))
             ;; Method 1: string-trim directly
             (trimmed (string-trim str pat))
             ;; Method 2: trim-left then trim-right
             (lr (string-trim-right (string-trim-left str pat) pat))
             ;; Method 3: trim-right then trim-left
             (rl (string-trim-left (string-trim-right str pat) pat)))
        (setq results
              (cons (list str pat trimmed lr rl
                          (string= trimmed lr)
                          (string= trimmed rl)
                          (string= lr rl))
                    results))))
    (let ((all-consistent t))
      (dolist (r results)
        (unless (and (nth 5 r) (nth 6 r) (nth 7 r))
          (setq all-consistent nil)))
      (list
       all-consistent
       (length results)
       ;; Show actual trimmed values for verification
       (mapcar (lambda (r) (list (nth 0 r) (nth 2 r))) results)))))"####;
    let expect = expect_test::expect![[
        r#""OK (nil 6 ((\"xxxmiddlexxx\" \"middlexxx\") (\"\\n\\ndata\\n\\n\" \"data\") (\"aaabbbccc\" \"bbbccc\") (\"---text---\" \"text---\") (\"123abc456\" \"abc456\") (\"  hello  \" \"hello\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
