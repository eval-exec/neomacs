//! Oracle parity tests for regexp-quote edge cases and pattern building.
//!
//! Tests regexp-quote with all metacharacters, empty string, no-metachar
//! strings, integration with string-match/re-search-forward, dynamic
//! pattern building, and use within replace-regexp-in-string.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Quoting individual metacharacters: . * + ? [ ] ^ $ \ ( ) { } |
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_individual_metacharacters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Each regex metacharacter must be properly escaped by regexp-quote.
    // The quoted version should match the literal character in string-match.
    let form = r#"(let ((metas '("." "*" "+" "?" "[" "]" "^" "$" "\\" "(" ")" "{" "}" "|")))
  (mapcar (lambda (ch)
            (let* ((quoted (regexp-quote ch))
                   ;; Quoted pattern should literally match the metachar
                   (match-result (string-match (concat "\\`" quoted "\\'") ch))
                   ;; Verify quoting added a backslash for special chars
                   (longer (> (length quoted) (length ch))))
              (list ch quoted match-result longer)))
          metas))"#;
    let expect = expect_test::expect![[
        r#""OK ((\".\" \"\\\\.\" 0 t) (\"*\" \"\\\\*\" 0 t) (\"+\" \"\\\\+\" 0 t) (\"?\" \"\\\\?\" 0 t) (\"[\" \"\\\\[\" 0 t) (\"]\" \"]\" 0 nil) (\"^\" \"\\\\^\" 0 t) (\"$\" \"\\\\$\" 0 t) (\"\\\\\" \"\\\\\\\\\" 0 t) (\"(\" \"(\" 0 nil) (\")\" \")\" 0 nil) (\"{\" \"{\" 0 nil) (\"}\" \"}\" 0 nil) (\"|\" \"|\" 0 nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Quoting strings with no metacharacters (should be identity)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_no_metacharacters() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Strings containing only letters, digits, and non-metachar punctuation
    // should pass through regexp-quote unchanged.
    let form = r#"(let ((safe-strings '("hello" "world" "abc123" "foobar"
                                        "UPPERCASE" "MiXeD" "a" "Z"
                                        "with spaces" "line1\nline2"
                                        "tab\there" "0123456789"
                                        "hyphen-ated" "under_scored"
                                        "at@sign" "hash#tag" "percent%"
                                        "ampersand&" "tilde~" "comma,"
                                        "semicolon;" "colon:" "bang!"
                                        "slash/" "equals=" "less<"
                                        "greater>" "quote'" "double\"")))
  (mapcar (lambda (s)
            (let ((q (regexp-quote s)))
              (list s (string= s q)
                    ;; Quoted version should still match the original
                    (if (string-match (regexp-quote s) s) t nil))))
          safe-strings))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"hello\" t t) (\"world\" t t) (\"abc123\" t t) (\"foobar\" t t) (\"UPPERCASE\" t t) (\"MiXeD\" t t) (\"a\" t t) (\"Z\" t t) (\"with spaces\" t t) (\"line1\\nline2\" t t) (\"tab\there\" t t) (\"0123456789\" t t) (\"hyphen-ated\" t t) (\"under_scored\" t t) (\"at@sign\" t t) (\"hash#tag\" t t) (\"percent%\" t t) (\"ampersand&\" t t) (\"tilde~\" t t) (\"comma,\" t t) (\"semicolon;\" t t) (\"colon:\" t t) (\"bang!\" t t) (\"slash/\" t t) (\"equals=\" t t) (\"less<\" t t) (\"greater>\" t t) (\"quote'\" t t) (\"double\\\"\" t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Quoting the empty string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Empty string quoted should remain empty
  (regexp-quote "")
  (string= (regexp-quote "") "")
  (length (regexp-quote ""))
  ;; Empty regexp matches at the beginning of any string
  (string-match (regexp-quote "") "anything")
  (string-match (regexp-quote "") "")
  ;; Using empty regexp-quote in concat still works
  (string-match (concat "foo" (regexp-quote "") "bar") "foobar"))"#;
    let expect = expect_test::expect![[r#""OK (\"\" t 0 0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Using regexp-quote result in string-match and re-search-forward
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_in_search_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that regexp-quote produces patterns usable in all search functions.
    let form = r#"(list
  ;; string-match with quoted metachar-heavy string
  (let ((needle "price is $100.00 (USD)")
        (haystack "The price is $100.00 (USD) total."))
    (string-match (regexp-quote needle) haystack))
  ;; string-match-p (no match-data side effect)
  (let ((needle "[tag]")
        (haystack "Use [tag] for labels."))
    (if (string-match-p (regexp-quote needle) haystack) t nil))
  ;; re-search-forward in buffer
  (with-temp-buffer
    (insert "line with foo.bar and baz*qux here")
    (goto-char (point-min))
    (let ((found (re-search-forward (regexp-quote "foo.bar") nil t)))
      (list found (match-string 0))))
  ;; re-search-forward: ensure . is not treated as any-char
  (with-temp-buffer
    (insert "fooXbar foo.bar")
    (goto-char (point-min))
    ;; Without quoting, "foo.bar" would match "fooXbar" first
    (let ((pos-quoted (progn
                        (goto-char (point-min))
                        (re-search-forward (regexp-quote "foo.bar") nil t)))
          (pos-unquoted (progn
                          (goto-char (point-min))
                          (re-search-forward "foo.bar" nil t))))
      (list pos-quoted pos-unquoted)))
  ;; looking-at with quoted pattern
  (with-temp-buffer
    (insert "^start of line$")
    (goto-char (point-min))
    (if (looking-at (regexp-quote "^start of line$")) t nil)))"#;
    let expect = expect_test::expect![[r#""OK (4 t (18 \"foo.bar\") (16 8) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building dynamic regexps from user input
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_dynamic_pattern_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate building search patterns from arbitrary user input strings
    // that contain metacharacters.
    let form = r#"(progn
  (fset 'neovm--rqp-find-all
    (lambda (needle haystack)
      "Find all occurrences of literal NEEDLE in HAYSTACK."
      (let ((pattern (regexp-quote needle))
            (pos 0)
            (results nil))
        (while (string-match pattern haystack pos)
          (setq results (cons (match-beginning 0) results))
          (setq pos (1+ (match-beginning 0))))
        (nreverse results))))

  (unwind-protect
      (list
        ;; Find literal dots
        (funcall 'neovm--rqp-find-all "." "a.b.c.d")
        ;; Find literal asterisks
        (funcall 'neovm--rqp-find-all "*" "a*b**c")
        ;; Find literal brackets
        (funcall 'neovm--rqp-find-all "[" "a[b]c[d]")
        ;; Find literal parens
        (funcall 'neovm--rqp-find-all "(" "f(x) + g(y)")
        ;; Find multi-char pattern with metas
        (funcall 'neovm--rqp-find-all "$." "cost $. and $. more $.")
        ;; Find backslash
        (funcall 'neovm--rqp-find-all "\\" "a\\b\\c")
        ;; No matches
        (funcall 'neovm--rqp-find-all "xyz" "no match here")
        ;; Overlapping-position search for meta-heavy pattern
        (funcall 'neovm--rqp-find-all "??" "a??b??c"))
    (fmakunbound 'neovm--rqp-find-all)))"#;
    let expect =
        expect_test::expect![[r#""OK ((1 3 5) (1 3 4) (1 5) (1 8) (5 12 20) (1 3) nil (1 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: escaping then using in replace-regexp-in-string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_in_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use regexp-quote to safely replace literal substrings that contain
    // regex metacharacters.
    let form = r#"(list
  ;; Replace literal "." with "!"
  (replace-regexp-in-string (regexp-quote ".") "!" "a.b.c.d")
  ;; Replace literal "*" with "x"
  (replace-regexp-in-string (regexp-quote "*") "x" "a*b*c*d")
  ;; Replace literal "$100" with "100 dollars"
  (replace-regexp-in-string (regexp-quote "$100") "100 dollars" "Price: $100 each")
  ;; Replace literal "[tag]" with "<tag>"
  (replace-regexp-in-string (regexp-quote "[tag]") "<tag>" "Use [tag] and [tag]")
  ;; Replace literal "(foo)" with "FOO"
  (replace-regexp-in-string (regexp-quote "(foo)") "FOO" "bar(foo)baz(foo)qux")
  ;; Replace literal backslash with forward slash
  (replace-regexp-in-string (regexp-quote "\\") "/" "c:\\users\\docs")
  ;; Replace literal "^" with "caret"
  (replace-regexp-in-string (regexp-quote "^") "caret" "x^2 + y^3")
  ;; Replace complex multi-meta pattern
  (replace-regexp-in-string
    (regexp-quote "f(x) = x^2 + $c")
    "FORMULA"
    "Given f(x) = x^2 + $c, compute f(3)."))"#;
    let expect = expect_test::expect![[
        r#""OK (\"a!b!c!d\" \"axbxcxd\" \"Price: 100 dollars each\" \"Use <tag> and <tag>\" \"barFOObazFOOqux\" \"c:/users/docs\" \"xcaret2 + ycaret3\" \"Given FORMULA, compute f(3).\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: combining regexp-quote with regex constructs for anchoring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_quote_combined_with_anchors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build patterns that combine quoted literals with regex anchors
    // and grouping constructs.
    let form = r#"(list
  ;; Match literal at start of string
  (if (string-match (concat "\\`" (regexp-quote "$price")) "$price: 50")
      (match-string 0) nil)
  ;; Match literal at end of string
  (if (string-match (concat (regexp-quote "(end)") "\\'") "done (end)")
      (match-string 0) nil)
  ;; Match literal between word boundaries (with capture group)
  (if (string-match (concat "\\(" (regexp-quote "foo.bar") "\\)") "use foo.bar now")
      (match-string 1) nil)
  ;; Build alternation of quoted literals
  (let ((pattern (concat "\\("
                         (mapconcat #'regexp-quote '("a+b" "c*d" "e?f") "\\|")
                         "\\)")))
    (let ((results nil)
          (text "compute a+b and c*d or e?f here"))
      (let ((pos 0))
        (while (string-match pattern text pos)
          (setq results (cons (match-string 1 text) results))
          (setq pos (match-end 0))))
      (nreverse results)))
  ;; Negative test: unquoted "." matches any char, quoted "." matches only "."
  (list
    (string-match "a.c" "abc")   ;; matches (. = any char)
    (string-match "a.c" "a.c")   ;; matches
    (string-match (regexp-quote "a.c") "abc")  ;; nil (literal dot required)
    (string-match (regexp-quote "a.c") "a.c")) ;; matches
  ;; Multiple sequential quoted fragments
  (let ((pat (concat (regexp-quote "[") "[0-9]+" (regexp-quote "]"))))
    (if (string-match pat "value [42] found")
        (match-string 0) nil)))"#;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 0 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
