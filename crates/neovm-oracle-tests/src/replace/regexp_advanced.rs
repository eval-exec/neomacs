//! Advanced oracle parity tests for `replace-regexp-in-string` with all
//! parameter combinations: FIXEDCASE, LITERAL, SUBEXP, START, backreferences
//! (\1, \2), function-as-replacement (lambda), complex regexp groups,
//! chaining multiple replacements, and interaction between parameters.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Backreferences \1, \2 with multiple capture groups
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_adv_backreferences_multiple_groups() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test \1 and \2 backreferences with multiple capture groups,
    // swapping group order, repeating groups, and nested groups.
    let form = r####"(list
  ;; Swap two groups: "first-last" -> "last, first"
  (replace-regexp-in-string
    "\\([a-zA-Z]+\\)-\\([a-zA-Z]+\\)"
    "\\2, \\1"
    "alice-smith bob-jones charlie-brown")
  ;; Repeat group 1 twice
  (replace-regexp-in-string
    "\\([0-9]+\\)"
    "\\1+\\1"
    "val=5 x=12 y=300")
  ;; Nested groups: outer (\1) and inner (\2)
  (replace-regexp-in-string
    "\\(\\([a-z]+\\)[0-9]+\\)"
    "[whole=\\1,word=\\2]"
    "abc123 def456 ghi789")
  ;; Three groups with rearrangement
  (replace-regexp-in-string
    "\\([0-9]+\\)\\.\\([0-9]+\\)\\.\\([0-9]+\\)"
    "\\3-\\2-\\1"
    "2024.01.15 and 2023.12.25")
  ;; \& for whole match combined with groups
  (replace-regexp-in-string
    "\\([A-Z]\\)\\([a-z]+\\)"
    "(\\&=\\1.\\2)"
    "Hello World Foo"))"####;
    let expect = expect_test::expect![[
        r#""OK (\"smith, alice jones, bob brown, charlie\" \"val=5+5 x=12+12 y=300+300\" \"[whole=abc123,word=abc] [whole=def456,word=def] [whole=ghi789,word=ghi]\" \"15-01-2024 and 25-12-2023\" \"(Hello=H.Ello) (World=W.Orld) (Foo=F.Oo)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// FIXEDCASE + LITERAL combined with backreferences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_adv_fixedcase_literal_combinations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Systematic testing of all FIXEDCASE x LITERAL combinations
    let form = r####"(let ((text "Hello World HELLO world"))
  (list
    ;; FIXEDCASE=nil LITERAL=nil: case-adapt + backrefs active
    (replace-regexp-in-string "\\([a-zA-Z]+\\)" "(\\1)" text nil nil)
    ;; FIXEDCASE=t LITERAL=nil: exact case + backrefs active
    (replace-regexp-in-string "\\([a-zA-Z]+\\)" "(\\1)" text t nil)
    ;; FIXEDCASE=nil LITERAL=t: case-adapt + backrefs literal
    (replace-regexp-in-string "\\([a-zA-Z]+\\)" "(\\1)" text nil t)
    ;; FIXEDCASE=t LITERAL=t: exact case + backrefs literal
    (replace-regexp-in-string "\\([a-zA-Z]+\\)" "(\\1)" text t t)
    ;; FIXEDCASE=nil with upper->lower transformation
    (replace-regexp-in-string "HELLO" "goodbye" text nil nil)
    ;; FIXEDCASE=t with same
    (replace-regexp-in-string "HELLO" "goodbye" text t nil)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"(Hello) (World) (HELLO) (world)\" \"(Hello) (World) (HELLO) (world)\" \"(\\\\1) (\\\\1) (\\\\1) (\\\\1)\" \"(\\\\1) (\\\\1) (\\\\1) (\\\\1)\" \"Goodbye World GOODBYE world\" \"goodbye World goodbye world\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// SUBEXP argument: replace only a specific capture group
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_adv_subexp_group_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // SUBEXP parameter (5th arg) specifies which group to replace,
    // leaving the rest of the match intact.
    let form = r####"(list
  ;; Replace only group 1 (the key), keep the value
  (replace-regexp-in-string
    "\\([a-z]+\\)=\\([0-9]+\\)" "KEY" "foo=10 bar=20 baz=30" nil nil nil 1)
  ;; Replace only group 2 (the value), keep the key
  (replace-regexp-in-string
    "\\([a-z]+\\)=\\([0-9]+\\)" "VAL" "foo=10 bar=20 baz=30" nil nil nil 2)
  ;; SUBEXP=0 is same as no SUBEXP (replace whole match)
  (replace-regexp-in-string
    "\\([a-z]+\\)=\\([0-9]+\\)" "REPLACED" "foo=10 bar=20" nil nil nil 0)
  ;; SUBEXP with FIXEDCASE=t and LITERAL=t
  (replace-regexp-in-string
    "\\(Name\\): \\([A-Z][a-z]+\\)" "Title"
    "Name: Alice, Name: Bob" t t nil 1)
  ;; Nested groups: replace inner group only
  (replace-regexp-in-string
    "\\(prefix-\\([0-9]+\\)-suffix\\)" "XXX"
    "prefix-42-suffix prefix-99-suffix" nil nil nil 2))"####;
    let expect = expect_test::expect![[
        r#""OK (\"KEY KEY KEY\" \"VAL VAL VAL\" \"REPLACED REPLACED\" \"ame: Alice, Title\" \"efix-42-suffix XXX\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// START arg combined with other parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_adv_start_with_other_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // START parameter interacts with FIXEDCASE, LITERAL, and function replacement
    let form = r####"(list
  ;; START=10 skips initial matches, FIXEDCASE=nil
  (replace-regexp-in-string "cat" "dog"
    "Cat sat on cat mat with cat" nil nil 10)
  ;; START=10, FIXEDCASE=t
  (replace-regexp-in-string "cat" "dog"
    "Cat sat on cat mat with cat" t nil 10)
  ;; START with LITERAL=t and backrefs in replacement string
  (replace-regexp-in-string "\\([0-9]+\\)" "\\1"
    "12 34 56 78 90" nil t 6)
  ;; START with function replacement
  (replace-regexp-in-string "[0-9]+"
    (lambda (m) (number-to-string (* 10 (string-to-number m))))
    "1 2 3 4 5" nil nil 4)
  ;; START=0 (explicit) same as default
  (replace-regexp-in-string "x" "Y" "axbxcxdx" nil nil 0)
  ;; START past all matches
  (replace-regexp-in-string "x" "Y" "axbxcx" nil nil 6)
  ;; START in middle of potential match
  (replace-regexp-in-string "abc" "XYZ" "abc-abc-abc" nil nil 2))"####;
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 10 0 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Lambda replacement with match-data access
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_adv_lambda_with_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When REP is a function, match-data is set before the function is called,
    // so the function can use match-string to access groups.
    let form = r####"(list
  ;; Lambda that reverses the matched string
  (replace-regexp-in-string "[a-z]+"
    (lambda (m) (apply #'string (nreverse (string-to-list m))))
    "hello world foo")
  ;; Lambda that uses match-string for group access
  (replace-regexp-in-string "\\([a-z]+\\)@\\([a-z]+\\)"
    (lambda (m)
      (concat (upcase (match-string 2 m)) ":" (match-string 1 m)))
    "user@host admin@server")
  ;; Lambda returning variable-length strings
  (replace-regexp-in-string "[0-9]+"
    (lambda (m)
      (make-string (string-to-number m) ?*))
    "a3b5c1d0e2")
  ;; Lambda with conditional logic
  (replace-regexp-in-string "[0-9]+"
    (lambda (m)
      (let ((n (string-to-number m)))
        (cond ((> n 50) "BIG")
              ((> n 10) "MED")
              (t "SMALL"))))
    "val=5 x=42 y=100 z=15")
  ;; Lambda counting replacements via side-effect on closed-over variable
  (let ((count 0))
    (replace-regexp-in-string "[aeiou]"
      (lambda (m) (setq count (1+ count)) (number-to-string count))
      "abecidofu")))"####;
    let expect = expect_test::expect![[
        r#""OK (\"olleh dlrow oof\" \"HOST:user SERVER:admin\" \"a***b*****c*de**\" \"val=SMALL x=MED y=BIG z=MED\" \"1b2c3d4f5\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: chaining multiple replacements as a transformation pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_adv_chained_pipeline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multi-step transformation: CSV-like input -> structured output
    let form = r####"(let* ((input "  John,Doe,30;  Jane,Smith,25; Bob,Brown,40  ")
       ;; Step 1: trim leading/trailing whitespace
       (s1 (replace-regexp-in-string "\\`[ \t\n]+" "" input))
       (s2 (replace-regexp-in-string "[ \t\n]+\\'" "" s1))
       ;; Step 2: normalize semicolon-separated records (remove spaces around ;)
       (s3 (replace-regexp-in-string "[ \t]*;[ \t]*" ";" s2))
       ;; Step 3: wrap each record in parens
       (s4 (replace-regexp-in-string "\\([^;]+\\)" "(\\1)" s3))
       ;; Step 4: convert commas to spaces within
       (s5 (replace-regexp-in-string "," " " s4)))
  (list s1 s2 s3 s4 s5))"####;
    let expect = expect_test::expect![[
        r#""OK (\"John,Doe,30;  Jane,Smith,25; Bob,Brown,40  \" \"John,Doe,30;  Jane,Smith,25; Bob,Brown,40\" \"John,Doe,30;Jane,Smith,25;Bob,Brown,40\" \"(John,Doe,30);(Jane,Smith,25);(Bob,Brown,40)\" \"(John Doe 30);(Jane Smith 25);(Bob Brown 40)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex regexp patterns: alternation, quantifiers, character classes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_adv_complex_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Character class with quantifier
  (replace-regexp-in-string "[[:digit:]]+" "#" "abc123def456ghi")
  ;; Alternation in groups
  (replace-regexp-in-string "\\(cat\\|dog\\|bird\\)" "animal"
    "I have a cat and a dog and a bird")
  ;; Greedy vs explicit minimal simulation: match balanced pairs
  (replace-regexp-in-string "\\[\\([^]]*\\)\\]" "{\\1}"
    "text [hello] more [world] end")
  ;; Anchored replacement: only at word boundaries (using \b equivalent)
  (replace-regexp-in-string "\\bthe\\b" "THE"
    "the other theme is the best" nil nil)
  ;; Replace multiple whitespace types (space, tab, newline)
  (replace-regexp-in-string "[ \t\n]+" " "
    "hello   world\t\ttab\n\nnewline")
  ;; Optional group: match with or without prefix
  (replace-regexp-in-string "\\(un\\)?happy" "MOOD"
    "I am happy but unhappy about it"))"####;
    let expect = expect_test::expect![[
        r#""OK (\"abc#def#ghi\" \"I have a animal and a animal and a animal\" \"text {hello} more {world} end\" \"THE other theme is THE best\" \"hello world tab newline\" \"I am MOOD but MOOD about it\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases and boundary conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_replace_regexp_adv_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r####"(list
  ;; Empty match pattern (matches everywhere)
  (replace-regexp-in-string "" "-" "abc")
  ;; Replacement that produces a longer string than original
  (replace-regexp-in-string "x" "XXXXX" "xox")
  ;; Pattern that matches the whole string
  (replace-regexp-in-string ".*" "REPLACED" "hello" nil nil)
  ;; Dot-star with groups
  (replace-regexp-in-string "^\\(.*\\)$" "[\\1]" "single line")
  ;; Consecutive matches with zero-width potential
  (replace-regexp-in-string "[0-9]*" "N" "a1b23c")
  ;; Replace in empty string
  (replace-regexp-in-string "." "x" "")
  ;; Backslash in replacement with LITERAL=t vs nil
  (replace-regexp-in-string "x" "a\\\\b" "x-x" nil nil)
  (replace-regexp-in-string "x" "a\\\\b" "x-x" nil t))"####;
    let expect = expect_test::expect![[
        r#""OK (\"-a-b-c\" \"XXXXXoXXXXX\" \"REPLACED\" \"[single line]\" \"NaNNbNNc\" \"\" \"a\\\\b-a\\\\b\" \"a\\\\\\\\b-a\\\\\\\\b\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
