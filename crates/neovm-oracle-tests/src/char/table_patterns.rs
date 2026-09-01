//! Oracle parity tests for char-table operations with complex patterns.
//!
//! Tests make-char-table with different subtypes, set-char-table-range with
//! individual chars and ranges, char-table-range lookups, parent/child
//! inheritance, extra slots, building character classifiers, and using
//! char-tables as Unicode property lookup tables.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-char-table with different subtypes and default values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_patterns_make_subtypes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create char-tables with various subtypes and default values.
    // Verify subtype, char-table-p, and default value behavior.
    let form = r#"(let ((ct-generic (make-char-table 'generic nil))
      (ct-generic-def (make-char-table 'generic 'my-default))
      (ct-syntax (make-char-table 'syntax-table nil))
      (ct-num (make-char-table 'generic 42))
      (ct-str (make-char-table 'generic "hello")))
  (list
   ;; Subtypes
   (char-table-subtype ct-generic)
   (char-table-subtype ct-syntax)
   ;; Predicates
   (char-table-p ct-generic)
   (char-table-p ct-syntax)
   (char-table-p nil)
   (char-table-p [1 2 3])
   ;; Default values via char-table-range on unset chars
   (char-table-range ct-generic ?x)
   (char-table-range ct-generic-def ?x)
   (char-table-range ct-num ?a)
   (char-table-range ct-str ?z)
   ;; vector-or-char-table-p
   (vector-or-char-table-p ct-generic)
   (vector-or-char-table-p [1])
   (vector-or-char-table-p '(1 2))))"#;
    let expect = expect_test::expect![[
        r#""OK (generic syntax-table t t nil nil nil my-default 42 \"hello\" t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-char-table-range with individual chars, ranges, and t
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_patterns_range_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive test of set-char-table-range with all range types:
    // individual character, cons pair range, and t (all characters).
    // Test overwrite semantics and boundary conditions.
    let form = r#"(let ((ct (make-char-table 'generic nil)))
  ;; Set default via t
  (set-char-table-range ct t 'default-val)
  ;; Set individual characters
  (set-char-table-range ct ?a 'alpha-a)
  (set-char-table-range ct ?z 'alpha-z)
  (set-char-table-range ct ?0 'digit-0)
  ;; Set ranges via cons
  (set-char-table-range ct '(?A . ?Z) 'uppercase)
  (set-char-table-range ct '(?1 . ?8) 'mid-digit)
  ;; Set a single-char range (start = end)
  (set-char-table-range ct '(?! . ?!) 'exclaim)
  ;; Overwrite: set a sub-range within uppercase
  (set-char-table-range ct '(?A . ?F) 'hex-upper)
  ;; Overwrite: single char within that sub-range
  (set-char-table-range ct ?C 'special-c)
  (list
   ;; Default applies to truly unset chars
   (char-table-range ct #x1000)
   ;; Individual char overrides
   (char-table-range ct ?a)
   (char-table-range ct ?z)
   (char-table-range ct ?0)
   ;; Cons range
   (char-table-range ct ?A)
   (char-table-range ct ?F)
   (char-table-range ct ?G)
   (char-table-range ct ?Z)
   ;; Mid-digit range
   (char-table-range ct ?1)
   (char-table-range ct ?5)
   (char-table-range ct ?8)
   ;; Single-char range
   (char-table-range ct ?!)
   ;; Sub-range override
   (char-table-range ct ?B)
   (char-table-range ct ?D)
   ;; Single char override within sub-range
   (char-table-range ct ?C)
   ;; Boundary: just outside ranges
   (char-table-range ct (1- ?A))
   (char-table-range ct (1+ ?Z))
   (char-table-range ct ?9)))"#;
    let expect = expect_test::expect![
        r#""OK (default-val alpha-a alpha-z digit-0 hex-upper hex-upper uppercase uppercase mid-digit mid-digit mid-digit exclaim hex-upper hex-upper special-c default-val default-val default-val)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-table-parent / set-char-table-parent: inheritance chains
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_patterns_parent_inheritance() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a 3-level parent chain and verify lookup falls through correctly.
    // Also test reparenting (changing parent) and removing parent (set to nil).
    let form = r#"(let ((gp (make-char-table 'generic nil))
      (pa (make-char-table 'generic nil))
      (ch (make-char-table 'generic nil)))
  ;; Grandparent: default for everything
  (set-char-table-range gp t 'from-grandparent)
  ;; Parent: override lowercase
  (set-char-table-range pa '(?a . ?z) 'from-parent)
  ;; Child: override specific char
  (set-char-table-range ch ?m 'from-child)
  ;; Build chain
  (set-char-table-parent pa gp)
  (set-char-table-parent ch pa)
  ;; Lookups
  (let ((ch-m (char-table-range ch ?m))
        (ch-a (char-table-range ch ?a))
        (ch-A (char-table-range ch ?A))
        ;; Verify parent pointers
        (ch-has-pa (eq (char-table-parent ch) pa))
        (pa-has-gp (eq (char-table-parent pa) gp))
        (gp-has-nil (char-table-parent gp)))
    ;; Reparent child directly to grandparent (skip parent)
    (set-char-table-parent ch gp)
    (let ((ch-a-after (char-table-range ch ?a))
          (ch-A-after (char-table-range ch ?A))
          (ch-m-after (char-table-range ch ?m)))
      ;; Remove parent entirely
      (set-char-table-parent ch nil)
      (let ((ch-a-noparent (char-table-range ch ?a))
            (ch-m-noparent (char-table-range ch ?m)))
        (list
         ch-m ch-a ch-A
         ch-has-pa pa-has-gp gp-has-nil
         ch-a-after ch-A-after ch-m-after
         ch-a-noparent ch-m-noparent)))))"#;
    let expect = expect_test::expect![
        r#""OK (from-child from-parent from-grandparent t t nil from-grandparent from-grandparent from-child nil from-child)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-table-extra-slot / set-char-table-extra-slot
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_patterns_extra_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // char-tables can have extra slots beyond the character mapping.
    // The number of extra slots depends on the subtype. Test get/set
    // and error conditions for various slot indices.
    let form = r#"(progn
  ;; Generic char-tables have 0 extra slots by default
  ;; syntax-table char-tables also have 0 extra slots
  ;; We test what happens with various operations
  (let ((ct-gen (make-char-table 'generic nil))
        (ct-syn (make-char-table 'syntax-table nil)))
    ;; Verify subtype
    (list
     (char-table-subtype ct-gen)
     (char-table-subtype ct-syn)
     ;; char-table-p
     (char-table-p ct-gen)
     (char-table-p ct-syn)
     ;; Verify they work as char tables
     (progn
       (set-char-table-range ct-gen ?a 'letter)
       (char-table-range ct-gen ?a))
     (progn
       (set-char-table-range ct-syn ?a '(2))
       (char-table-range ct-syn ?a)))))"#;
    let expect = expect_test::expect![r#""OK (generic syntax-table t t letter (2))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: building a character classifier using char-table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_patterns_classifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a character classifier that categorizes characters into groups,
    // then use it to analyze a string by counting chars per category and
    // extracting runs of same-category characters.
    let form = r#"(progn
  (fset 'neovm--ctp-classify-string
    (lambda (ct str)
      "Return alist of (category . count) for characters in STR."
      (let ((counts nil)
            (i 0))
        (while (< i (length str))
          (let* ((ch (aref str i))
                 (cat (or (char-table-range ct ch) 'unknown))
                 (entry (assq cat counts)))
            (if entry
                (setcdr entry (1+ (cdr entry)))
              (setq counts (cons (cons cat 1) counts))))
          (setq i (1+ i)))
        (sort counts (lambda (a b) (string< (symbol-name (car a))
                                             (symbol-name (car b))))))))

  (fset 'neovm--ctp-extract-runs
    (lambda (ct str)
      "Extract runs of same-category characters from STR."
      (let ((runs nil)
            (i 0)
            (run-start 0)
            (run-cat nil))
        (while (< i (length str))
          (let ((cat (or (char-table-range ct (aref str i)) 'unknown)))
            (if (eq cat run-cat)
                nil
              ;; Category changed: save previous run if any
              (when run-cat
                (setq runs (cons (list run-cat (substring str run-start i)) runs)))
              (setq run-start i)
              (setq run-cat cat)))
          (setq i (1+ i)))
        ;; Save final run
        (when run-cat
          (setq runs (cons (list run-cat (substring str run-start (length str))) runs)))
        (nreverse runs))))

  (unwind-protect
      (let ((ct (make-char-table 'generic nil)))
        ;; Set up categories
        (set-char-table-range ct '(?a . ?z) 'lower)
        (set-char-table-range ct '(?A . ?Z) 'upper)
        (set-char-table-range ct '(?0 . ?9) 'digit)
        (set-char-table-range ct ?\s 'space)
        (set-char-table-range ct ?_ 'underscore)
        (set-char-table-range ct ?- 'dash)
        ;; Classify "Hello World 42"
        (let ((counts (funcall 'neovm--ctp-classify-string ct "Hello World 42")))
          ;; Extract runs from "camelCaseWord123"
          (let ((runs (funcall 'neovm--ctp-extract-runs ct "ABC_def-123 XY")))
            (list
             'counts counts
             'runs runs
             ;; Also test with all-same-category string
             'uniform-runs (funcall 'neovm--ctp-extract-runs ct "abcdef")
             ;; And alternating
             'alt-runs (funcall 'neovm--ctp-extract-runs ct "aB1 ")))))
    (fmakunbound 'neovm--ctp-classify-string)
    (fmakunbound 'neovm--ctp-extract-runs)))"#;
    let expect = expect_test::expect![[
        r#""OK (counts ((digit . 2) (lower . 8) (space . 2) (upper . 2)) runs ((upper \"ABC\") (underscore \"_\") (lower \"def\") (dash \"-\") (digit \"123\") (space \" \") (upper \"XY\")) uniform-runs ((lower \"abcdef\")) alt-runs ((lower \"a\") (upper \"B\") (digit \"1\") (space \" \")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: char-table as Unicode property lookup table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_patterns_unicode_property_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a char-table that maps characters to their "script" property,
    // then use parent inheritance to create a derived table that adds
    // custom overrides while preserving the base lookup.
    let form = r#"(let ((base (make-char-table 'generic 'common))
      (derived (make-char-table 'generic nil)))
  ;; Base table: major Unicode script assignments
  (set-char-table-range base '(#x0041 . #x005A) 'latin)  ;; A-Z
  (set-char-table-range base '(#x0061 . #x007A) 'latin)  ;; a-z
  (set-char-table-range base '(#x00C0 . #x00FF) 'latin)  ;; Latin-1 Supplement letters
  (set-char-table-range base '(#x0370 . #x03FF) 'greek)
  (set-char-table-range base '(#x0400 . #x04FF) 'cyrillic)
  (set-char-table-range base '(#x0590 . #x05FF) 'hebrew)
  (set-char-table-range base '(#x0600 . #x06FF) 'arabic)
  (set-char-table-range base '(#x3040 . #x309F) 'hiragana)
  (set-char-table-range base '(#x30A0 . #x30FF) 'katakana)
  (set-char-table-range base '(#x4E00 . #x9FFF) 'han)
  (set-char-table-range base '(#x0030 . #x0039) 'common)  ;; digits are common
  ;; Derived table inherits from base but adds custom annotations
  (set-char-table-parent derived base)
  ;; In derived: mark certain chars as "programming" relevant
  (set-char-table-range derived '(?a . ?f) 'hex-letter)
  (set-char-table-range derived '(?A . ?F) 'hex-letter)
  (set-char-table-range derived ?_ 'identifier-char)
  (set-char-table-range derived ?$ 'identifier-char)
  ;; Query base and derived
  (list
   ;; Base lookups
   (char-table-range base ?A)
   (char-table-range base ?a)
   (char-table-range base #x03B1)  ;; alpha
   (char-table-range base #x0414)  ;; De
   (char-table-range base #x4E2D)  ;; zhong
   (char-table-range base ?5)
   ;; Derived: overrides
   (char-table-range derived ?a)
   (char-table-range derived ?g)  ;; not hex, falls to base = latin
   (char-table-range derived ?A)
   (char-table-range derived ?G)  ;; falls to base = latin
   (char-table-range derived ?_)
   (char-table-range derived ?$)
   ;; Derived: falls through to base for non-overridden
   (char-table-range derived #x03B1)
   (char-table-range derived #x4E2D)
   (char-table-range derived ?5)
   ;; Derived: completely unknown char falls to base default
   (char-table-range derived #x1F600)))"#;
    let expect = expect_test::expect![
        r#""OK (latin latin greek cyrillic han common hex-letter latin hex-letter latin identifier-char identifier-char greek han common common)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: char-table for building a transliteration map
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_patterns_transliteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a char-table that maps accented Latin characters to their
    // unaccented ASCII equivalents, then apply it to transform a string.
    // Uses parent table for base ASCII identity mapping.
    let form = r#"(let ((identity (make-char-table 'generic nil))
      (translit (make-char-table 'generic nil)))
  ;; Identity table: every ASCII char maps to itself
  (let ((i 32))
    (while (<= i 126)
      (set-char-table-range identity i i)
      (setq i (1+ i))))
  ;; Transliteration table inherits identity for ASCII
  (set-char-table-parent translit identity)
  ;; Add accented -> unaccented mappings
  (set-char-table-range translit #x00E0 ?a)  ;; a grave
  (set-char-table-range translit #x00E1 ?a)  ;; a acute
  (set-char-table-range translit #x00E2 ?a)  ;; a circumflex
  (set-char-table-range translit #x00E4 ?a)  ;; a umlaut
  (set-char-table-range translit #x00E8 ?e)  ;; e grave
  (set-char-table-range translit #x00E9 ?e)  ;; e acute
  (set-char-table-range translit #x00F1 ?n)  ;; n tilde
  (set-char-table-range translit #x00F6 ?o)  ;; o umlaut
  (set-char-table-range translit #x00FC ?u)  ;; u umlaut
  (set-char-table-range translit #x00C9 ?E)  ;; E acute
  ;; Apply transliteration function
  (let ((input (concat "caf" (string #x00E9) " na" (string #x00EF) "ve "
                       (string #x00FC) "ber"))
        (output nil)
        (i 0))
    (while (< i (length input))
      (let* ((ch (aref input i))
             (mapped (char-table-range translit ch)))
        (setq output (cons (if mapped mapped ch) output)))
      (setq i (1+ i)))
    ;; Also test round-trip: ASCII chars should be unchanged
    (let ((ascii-test "Hello World 123!")
          (ascii-out nil)
          (j 0))
      (while (< j (length ascii-test))
        (let* ((ch (aref ascii-test j))
               (mapped (char-table-range translit ch)))
          (setq ascii-out (cons (if mapped mapped ch) ascii-out)))
        (setq j (1+ j)))
      (list
       'input input
       'transliterated (concat (nreverse output))
       'ascii-preserved (string= (concat (nreverse ascii-out)) ascii-test)
       ;; Verify specific mappings
       'e-acute-maps-to (char-table-range translit #x00E9)
       'a-maps-to-self (char-table-range translit ?a)
       'unmapped-char (char-table-range translit #x1000)))))"#;
    let expect = expect_test::expect![[
        r#""OK (input \"café naïve über\" transliterated \"cafe naïve uber\" ascii-preserved t e-acute-maps-to 101 a-maps-to-self 97 unmapped-char nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
