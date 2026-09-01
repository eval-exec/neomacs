//! Advanced oracle parity tests for `split-string` usage patterns.
//!
//! Covers: regex separators, OMIT-NULLS parameter, TRIM parameter,
//! multi-character delimiters, captured groups in separators,
//! CSV parsing with escaping, and tokenization of code snippets.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// split-string with regex separator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_regex_separator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Split on various regex patterns: multiple whitespace, punctuation runs,
    // alternations, character classes
    let form = r#"(list
  ;; Split on one or more whitespace chars
  (split-string "hello   world\t\tfoo\nbar" "[ \t\n]+")
  ;; Split on any punctuation run
  (split-string "one...two---three___four" "[[:punct:]]+")
  ;; Split on digits (numbers as separators)
  (split-string "abc123def456ghi" "[0-9]+")
  ;; Split on alternation pattern: comma or semicolon with optional spaces
  (split-string "a, b; c ,d;e" "[ \t]*[,;][ \t]*")
  ;; Split on word boundary approximation: transition from letter to non-letter
  (split-string "camelCaseWord" "\\(?:[a-z]\\)\\(\\)" ))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" \"world\" \"foo\" \"bar\") (\"one\" \"two\" \"three\" \"four\") (\"abc\" \"def\" \"ghi\") (\"a\" \"b\" \"c\" \"d\" \"e\") (\"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\" \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// split-string with OMIT-NULLS parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_omit_nulls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate difference between omit-nulls t vs nil
    // Leading/trailing separators produce empty strings when omit-nulls is nil
    let form = r#"(list
  ;; Default (omit-nulls nil): empty strings preserved at boundaries
  (split-string ",a,,b,c," ",")
  ;; omit-nulls t: empty strings removed
  (split-string ",a,,b,c," "," t)
  ;; Leading/trailing whitespace with omit-nulls
  (split-string "  hello  world  " " " t)
  (split-string "  hello  world  " " " nil)
  ;; Separator appears at every position
  (split-string ":::" ":" t)
  (split-string ":::" ":" nil)
  ;; No matches for separator: entire string as single element
  (split-string "noseparator" "," t)
  (split-string "noseparator" "," nil))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"\" \"a\" \"\" \"b\" \"c\" \"\") (\"a\" \"b\" \"c\") (\"hello\" \"world\") (\"\" \"\" \"hello\" \"\" \"world\" \"\" \"\") nil (\"\" \"\" \"\" \"\") (\"noseparator\") (\"noseparator\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// split-string with TRIM parameter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_trim() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The TRIM parameter is a regex applied to trim leading/trailing from each piece
    let form = r#"(list
  ;; Trim whitespace from each split piece
  (split-string " a , b , c " "," t "[ \t]+")
  ;; Trim specific characters (brackets, quotes)
  (split-string "[one] | [two] | [three]" " *| *" t "[][]")
  ;; Trim digits from edges of each piece
  (split-string "123abc456,789def012,345ghi678" "," nil "[0-9]+")
  ;; Trim combined with omit-nulls
  (split-string "  ,  ,hello,  ,world,  " "," t "[ \t]+")
  ;; Trim that removes everything from some pieces
  (split-string "123,456,abc,789" "," nil "[0-9]+"))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\") (\"one\" \"two\" \"three\") (\"abc\" \"def\" \"ghi\") (\"hello\" \"world\") (\"\" \"\" \"abc\" \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Splitting on multi-character delimiters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_multi_char_delimiters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Split on literal multi-char string "->"
  (split-string "a->b->c->d" "->")
  ;; Split on "::" (C++ scope operator)
  (split-string "std::vector::iterator::value_type" "::")
  ;; Split on " -- " (em dash separator)
  (split-string "first -- second -- third" " -- ")
  ;; Split on CRLF line endings
  (split-string "line1\r\nline2\r\nline3" "\r\n")
  ;; Split on ellipsis
  (split-string "start...middle...end" "\\.\\.\\.")
  ;; Split on HTML-like tag
  (split-string "hello<br>world<br>again" "<br>"))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"b\" \"c\" \"d\") (\"std\" \"vector\" \"iterator\" \"value_type\") (\"first\" \"second\" \"third\") (\"line1\" \"line2\" \"line3\") (\"start\" \"middle\" \"end\") (\"hello\" \"world\" \"again\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// split-string edge cases and special patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty string input
  (split-string "" "," t)
  (split-string "" ",")
  ;; Separator longer than input
  (split-string "ab" "abcdef")
  ;; Input equals separator exactly
  (split-string "," ",")
  (split-string "," "," t)
  ;; Separator is a single char matching entire input
  (split-string "aaaa" "a+" t)
  (split-string "aaaa" "a+")
  ;; Unicode content split on ASCII separator
  (split-string "alpha,beta,gamma" "," t)
  ;; Default separator (split-string with just one arg uses whitespace)
  (split-string "  hello   world   "))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (\"\") (\"ab\") (\"\" \"\") nil nil (\"\" \"\") (\"alpha\" \"beta\" \"gamma\") (\"hello\" \"world\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: CSV-like parser using split-string with post-processing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_csv_parser() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Parse simple CSV rows: split on commas, then handle quoting by
    // detecting quoted fields and stripping quotes, trimming whitespace
    let form = r#"(progn
  (fset 'neovm--test-parse-csv-field
    (lambda (field)
      ;; Strip leading/trailing whitespace
      (let ((trimmed field))
        (when (string-match "\\`[ \t]*\\(.*?\\)[ \t]*\\'" trimmed)
          (setq trimmed (match-string 1 trimmed)))
        ;; If quoted, strip quotes and unescape doubled quotes
        (if (and (> (length trimmed) 1)
                 (= (aref trimmed 0) ?\")
                 (= (aref trimmed (1- (length trimmed))) ?\"))
            (let ((inner (substring trimmed 1 (1- (length trimmed)))))
              (replace-regexp-in-string "\"\"" "\"" inner))
          trimmed))))
  (fset 'neovm--test-parse-csv-row
    (lambda (row)
      (let ((fields (split-string row "," nil))
            (result nil))
        (dolist (f fields)
          (setq result (cons (funcall 'neovm--test-parse-csv-field f) result)))
        (nreverse result))))
  (unwind-protect
      (list
        (funcall 'neovm--test-parse-csv-row "name, age, city")
        (funcall 'neovm--test-parse-csv-row "\"Smith\", 42, \"New York\"")
        (funcall 'neovm--test-parse-csv-row "plain,123,  spaced  ")
        (funcall 'neovm--test-parse-csv-row "\"has \"\"quotes\"\"\", value, end")
        (funcall 'neovm--test-parse-csv-row "single"))
    (fmakunbound 'neovm--test-parse-csv-field)
    (fmakunbound 'neovm--test-parse-csv-row)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"name\" \"age\" \"city\") (\"Smith\" \"42\" \"New York\") (\"plain\" \"123\" \"spaced\") (\"has \\\"quotes\\\"\" \"value\" \"end\") (\"single\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: tokenize a mini-language snippet using split-string + match-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_split_string_tokenizer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Tokenize a simple expression language by splitting on whitespace,
    // then classifying each token via regex matching
    let form = r#"(progn
  (fset 'neovm--test-classify-token
    (lambda (tok)
      (cond
        ((string-match "\\`[0-9]+\\'" tok)
         (list 'number (string-to-number tok)))
        ((string-match "\\`[0-9]+\\.[0-9]+\\'" tok)
         (list 'float tok))
        ((string-match "\\`\"\\(.*\\)\"\\'" tok)
         (list 'string (match-string 1 tok)))
        ((string-match "\\`[a-zA-Z_][a-zA-Z0-9_]*\\'" tok)
         (let ((kw (member tok '("if" "then" "else" "let" "in" "fn" "return"))))
           (if kw
               (list 'keyword tok)
             (list 'ident tok))))
        ((string-match "\\`[-+*/=<>!]+\\'" tok)
         (list 'operator tok))
        ((string-match "\\`[(){}\\[\\],;]\\'" tok)
         (list 'punct tok))
        (t (list 'unknown tok)))))
  (fset 'neovm--test-tokenize
    (lambda (code)
      ;; First normalize: add spaces around punctuation and operators
      (let ((s code))
        (setq s (replace-regexp-in-string "\\([(){}\\[\\],;]\\)" " \\1 " s))
        (setq s (replace-regexp-in-string "\\([-+*/=<>!]+\\)" " \\1 " s))
        (let ((tokens (split-string s "[ \t\n]+" t))
              (result nil))
          (dolist (tok tokens)
            (setq result (cons (funcall 'neovm--test-classify-token tok) result)))
          (nreverse result)))))
  (unwind-protect
      (list
        (funcall 'neovm--test-tokenize "let x = 42")
        (funcall 'neovm--test-tokenize "fn add(a, b) return a + b")
        (funcall 'neovm--test-tokenize "if x > 0 then y = x * 2 else y = 0"))
    (fmakunbound 'neovm--test-classify-token)
    (fmakunbound 'neovm--test-tokenize)))"#;
    let expect = expect_test::expect![[
        r#""OK (((keyword \"let\") (ident \"x\") (operator \"=\") (number 42)) ((keyword \"fn\") (unknown \"add(a,\") (unknown \"b)\") (keyword \"return\") (ident \"a\") (operator \"+\") (ident \"b\")) ((keyword \"if\") (ident \"x\") (operator \">\") (number 0) (keyword \"then\") (ident \"y\") (operator \"=\") (ident \"x\") (operator \"*\") (number 2) (keyword \"else\") (ident \"y\") (operator \"=\") (number 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
