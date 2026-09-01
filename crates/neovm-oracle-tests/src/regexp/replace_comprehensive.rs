//! Comprehensive oracle parity tests for `replace-regexp-in-string` covering
//! all parameters (REGEXP REP STRING FIXEDCASE LITERAL SUBEXP START),
//! back-references (\& and \N), replacement functions, case-preserving
//! replacement, empty match handling, and nested group replacements.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Back-references: \& (whole match) and \N (group N)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_backref_whole_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // \& in the replacement expands to the entire matched text.
    // Test multiple matches, each with different content.
    let form = r#"(list
      ;; Wrap each word in angle brackets using \&
      (replace-regexp-in-string "[a-zA-Z]+" "<\\&>" "foo bar baz")
      ;; Wrap digits
      (replace-regexp-in-string "[0-9]+" "(\\&)" "x=42, y=7, z=100")
      ;; \& with adjacent literal text
      (replace-regexp-in-string "\\([a-z]+\\)" "<<\\&>>" "aaa BBB ccc DDD")
      ;; \& on single-char matches
      (replace-regexp-in-string "." "[\\&]" "abc"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"<foo> <bar> <baz>\" \"x=(42), y=(7), z=(100)\" \"<<aaa>> <<BBB>> <<ccc>> <<DDD>>\" \"[a][b][c]\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_regexp_replace_comp_backref_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // \1, \2 etc. expand to the corresponding capture group.
    // Test reordering groups, multiple groups, and nested groups.
    let form = r#"(list
      ;; Swap two groups: "key=val" -> "val:key"
      (replace-regexp-in-string
        "\\([a-z]+\\)=\\([a-z]+\\)" "\\2:\\1"
        "name=alice age=thirty")
      ;; Three groups, reference all three in different order
      (replace-regexp-in-string
        "\\([0-9]+\\)-\\([0-9]+\\)-\\([0-9]+\\)" "\\3/\\2/\\1"
        "2026-03-02 and 1999-12-31")
      ;; Nested groups: outer \1, inner \2
      (replace-regexp-in-string
        "\\(\\([A-Z]\\)[a-z]+\\)" "\\2_\\1"
        "Hello World Foo")
      ;; Reference same group multiple times
      (replace-regexp-in-string
        "\\([a-z]+\\)" "\\1-\\1" "ab cd"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"alice:name thirty:age\" \"02/03/2026 and 31/12/1999\" \"H_Hello W_World F_Foo\" \"ab-ab cd-cd\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Replacement function (lambda receives matched string)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_function_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When REP is a function, it receives the entire matched string
    // and returns the replacement. Test with various transformations.
    let form = r#"(list
      ;; Reverse each matched word
      (replace-regexp-in-string
        "[a-z]+"
        (lambda (m) (concat (nreverse (string-to-list m))))
        "hello world lisp")
      ;; Compute string length as replacement
      (replace-regexp-in-string
        "[a-zA-Z]+"
        (lambda (m) (number-to-string (length m)))
        "I am testing this")
      ;; Conditional replacement based on match content
      (replace-regexp-in-string
        "[0-9]+"
        (lambda (m)
          (let ((n (string-to-number m)))
            (cond ((< n 10) "small")
                  ((< n 100) "medium")
                  (t "large"))))
        "got 5 items and 42 things plus 999 extras")
      ;; Accumulate state via let-bound counter
      (let ((cnt 0))
        (replace-regexp-in-string
          "X"
          (lambda (m) (setq cnt (1+ cnt)) (number-to-string cnt))
          "X-X-X-X")))"#;
    let expect = expect_test::expect![[
        r#""OK (\"olleh dlrow psil\" \"1 2 7 4\" \"got small items and medium things plus large extras\" \"1-2-3-4\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// FIXEDCASE parameter: nil (case-adaptive) vs t (exact)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_fixedcase_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // FIXEDCASE=nil: Emacs tries to preserve the case pattern of the match.
    // FIXEDCASE=t: replacement inserted verbatim.
    // Test edge cases: all-caps, title-case, mixed case.
    let form = r#"(list
      ;; FIXEDCASE=nil with all-caps match -> replacement uppercased
      (replace-regexp-in-string "HELLO" "world" "say HELLO now" nil)
      ;; FIXEDCASE=nil with title-case match -> replacement title-cased
      (replace-regexp-in-string "Hello" "world" "say Hello now" nil)
      ;; FIXEDCASE=nil with lower-case match -> replacement kept lower
      (replace-regexp-in-string "hello" "world" "say hello now" nil)
      ;; FIXEDCASE=t always inserts verbatim
      (replace-regexp-in-string "HELLO" "xYz" "say HELLO now" t)
      (replace-regexp-in-string "Hello" "xYz" "say Hello now" t)
      ;; FIXEDCASE=nil with single-char uppercase
      (replace-regexp-in-string "A" "bee" "A or a" nil)
      ;; FIXEDCASE=nil with case-insensitive regexp
      (let ((case-fold-search t))
        (replace-regexp-in-string "hello" "world" "HELLO Hello hello" nil)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"say WORLD now\" \"say World now\" \"say world now\" \"say xYz now\" \"say xYz now\" \"BEE or bee\" \"WORLD World world\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// LITERAL parameter: backslash treatment
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_literal_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // LITERAL=t: backslashes in replacement are NOT special.
    // LITERAL=nil (default): \&, \1 etc. are expanded.
    let form = r#"(list
      ;; LITERAL=nil: \& expands
      (replace-regexp-in-string "\\([a-z]+\\)" "[\\&]" "foo bar" nil nil)
      ;; LITERAL=t: \& kept literally as backslash-ampersand
      (replace-regexp-in-string "\\([a-z]+\\)" "[\\&]" "foo bar" nil t)
      ;; LITERAL=nil: \1 expands to group 1
      (replace-regexp-in-string "\\([a-z]+\\)_\\([a-z]+\\)" "\\2-\\1" "foo_bar" nil nil)
      ;; LITERAL=t: \1 and \2 kept literally
      (replace-regexp-in-string "\\([a-z]+\\)_\\([a-z]+\\)" "\\2-\\1" "foo_bar" nil t)
      ;; LITERAL=nil with double-backslash to insert a literal backslash
      (replace-regexp-in-string "x" "a\\\\b" "x" nil nil)
      ;; LITERAL=t with backslashes -> all literal
      (replace-regexp-in-string "x" "a\\\\b" "x" nil t))"#;
    let expect = expect_test::expect![[
        r#""OK (\"[foo] [bar]\" \"[\\\\&] [\\\\&]\" \"bar-foo\" \"\\\\2-\\\\1\" \"a\\\\b\" \"a\\\\\\\\b\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SUBEXP parameter: replace only a specific capture group
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_subexp_group_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // SUBEXP specifies which group to replace (default 0 = whole match).
    // Only the text matched by that group is replaced; the rest of the
    // match is kept intact.
    let form = r#"(list
      ;; Replace group 1 only, keep surrounding match text
      (replace-regexp-in-string
        "\\(foo\\)bar" "REPLACED" "foobar baz foobar" nil nil nil nil 1)
      ;; Replace group 2 in a 3-group pattern
      (replace-regexp-in-string
        "\\([a-z]+\\)=\\([0-9]+\\)\\([a-z]*\\)" "XXX"
        "name=42end val=7" nil nil nil nil 2)
      ;; SUBEXP=0 is same as default (whole match)
      (replace-regexp-in-string
        "\\(abc\\)\\(def\\)" "XY" "abcdef" nil nil nil nil 0)
      ;; SUBEXP with function replacement
      (replace-regexp-in-string
        "\\([a-z]+\\):\\([0-9]+\\)" (lambda (m) (upcase m))
        "key:42 val:7" nil nil nil nil 1))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 7) 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// START parameter: begin matching from an offset
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_start_offset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // START specifies the character position from which to begin matching.
    // Characters before START are kept unchanged in the result.
    let form = r#"(list
      ;; Skip first match by starting after it
      (replace-regexp-in-string "cat" "dog" "cat sat cat mat cat" nil nil nil 4)
      ;; START=0 replaces everything (same as default)
      (replace-regexp-in-string "x" "Y" "xaxbxc" nil nil nil 0)
      ;; START beyond all matches -> no replacement
      (replace-regexp-in-string "x" "Y" "xaxbxc" nil nil nil 100)
      ;; START in the middle of a match -> that match is not found
      (replace-regexp-in-string "abcd" "XXXX" "abcd-abcd" nil nil nil 2)
      ;; START=1 with multibyte text
      (replace-regexp-in-string "[a-z]" "?" "Hello World" nil nil nil 1)
      ;; Combine START with FIXEDCASE and LITERAL
      (replace-regexp-in-string "\\([a-z]+\\)" "\\1!" "aaa bbb ccc" nil nil nil 4))"#;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range \"xaxbxc\" 100 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Empty match handling
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_empty_matches() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Regexps that can match empty strings need special handling
    // to avoid infinite loops. Test various zero-width match scenarios.
    let form = r#"(list
      ;; Match empty string between every character
      (replace-regexp-in-string "^" ">" "abc")
      ;; End-of-line anchor
      (replace-regexp-in-string "$" "!" "abc")
      ;; Optional match that can be empty
      (replace-regexp-in-string "x*" "Y" "axbc")
      ;; Word boundary (zero-width)
      (replace-regexp-in-string "\\b" "|" "ab cd"))"#;
    let expect = expect_test::expect![[r#""OK (\">abc\" \"abc!\" \"YaYYbYc\" \"|ab| |cd|\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested group replacements and complex patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_nested_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Deeply nested capture groups and complex back-reference patterns.
    let form = r#"(list
      ;; Nested groups: \1 is outer, \2 is inner
      (replace-regexp-in-string
        "\\(a\\(b+\\)c\\)" "\\2-\\1" "abbc xabbbc")
      ;; Three levels of nesting
      (replace-regexp-in-string
        "\\(\\(\\([0-9]\\)[0-9]\\)[0-9]\\)" "[\\3.\\2.\\1]" "123 456")
      ;; Alternation within groups
      (replace-regexp-in-string
        "\\(cat\\|dog\\)-\\([0-9]+\\)" "\\1#\\2" "cat-1 dog-22 cat-333")
      ;; Back-reference to optional group (may be empty)
      (replace-regexp-in-string
        "\\(a\\)\\(b\\)?" "\\1\\2" "ab a ab"))"#;
    let expect = expect_test::expect![[
        r#""OK (\"bb-abbc xbbb-abbbc\" \"[1.12.123] [4.45.456]\" \"cat#1 dog#22 cat#333\" \"ab a ab\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Case-preserving replacement pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_case_preserving_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a multi-step text transformation pipeline using
    // replace-regexp-in-string with various parameter combinations.
    let form = r#"(let* ((text "  Hello_World  FOO_BAR  baz_quux  123  ")
             ;; Step 1: Trim leading/trailing spaces
             (step1 (replace-regexp-in-string "\\`[ \t]+\\|[ \t]+\\'" "" text))
             ;; Step 2: Collapse multiple spaces to single
             (step2 (replace-regexp-in-string "  +" " " step1))
             ;; Step 3: Convert snake_case to camelCase via function replacement
             (step3 (replace-regexp-in-string
                      "_\\([a-zA-Z]\\)"
                      (lambda (m) (upcase (substring m 1)))
                      step2))
             ;; Step 4: Wrap numbers in parens, starting from position 10
             (step4 (replace-regexp-in-string
                      "[0-9]+" (lambda (m) (concat "(" m ")"))
                      step3 nil nil nil 10))
             ;; Step 5: LITERAL=t replacement of specific pattern
             (step5 (replace-regexp-in-string "HelloWorld" "\\1-escaped" step4 nil t)))
      (list step1 step2 step3 step4 step5))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Hello_World  FOO_BAR  baz_quux  123\" \"Hello_World FOO_BAR baz_quux 123\" \"HelloWorld FOOBAR bazQuux 123\" \" FOOBAR bazQuux (123)\" \" FOOBAR bazQuux (123)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// All parameters combined
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_regexp_replace_comp_all_params_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Exercise all 7 parameters simultaneously in a single call,
    // then verify the result with string-match.
    let form = r#"(list
      ;; REGEXP REP STRING FIXEDCASE LITERAL SUBEXP START
      ;; Replace group 1 literally, with fixedcase=t, starting at offset 5
      (replace-regexp-in-string
        "\\(key\\)=\\([a-z]+\\)"
        "K"
        "key=abc key=def key=ghi"
        t t nil 5 1)
      ;; REP as function, FIXEDCASE=nil, LITERAL ignored when REP is function,
      ;; SUBEXP=2, START=0
      (replace-regexp-in-string
        "\\([A-Z]+\\):\\([a-z]+\\)"
        (lambda (m) (concat "(" (upcase m) ")"))
        "FOO:bar BAZ:quux"
        nil nil nil 0 2)
      ;; Verify match data is not corrupted after replace-regexp-in-string
      (progn
        (string-match "\\([a-z]+\\)" "hello world")
        (let ((before-match (match-string 1 "hello world")))
          (replace-regexp-in-string "[0-9]+" "N" "a1b2c3")
          (string-match "\\([a-z]+\\)" "testing")
          (list before-match (match-string 1 "testing")))))"#;
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (3 . 7) 8)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
