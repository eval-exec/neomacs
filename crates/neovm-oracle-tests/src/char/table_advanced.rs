//! Advanced oracle parity tests for char-table primitives:
//! subtypes with extra slots, set-char-table-range with various range types,
//! parent inheritance chains, extra-slot get/set, predicates, and complex
//! char-table usage patterns (Unicode category classifier, case conversion).

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-char-table with subtype, set-char-table-range for single/range/default
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_advanced_range_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test single char, cons range, and t (default) for set-char-table-range
    let form = r#"(let ((ct (make-char-table 'generic nil)))
  ;; Set default via t
  (set-char-table-range ct t 'default)
  ;; Override a single char
  (set-char-table-range ct ?x 'letter-x)
  ;; Override a range
  (set-char-table-range ct '(?0 . ?9) 'digit)
  (list
   ;; Default applies to unset chars
   (char-table-range ct ?A)
   ;; Single char override
   (char-table-range ct ?x)
   ;; Range overrides
   (char-table-range ct ?0)
   (char-table-range ct ?5)
   (char-table-range ct ?9)
   ;; Char just outside the range gets default
   (char-table-range ct ?/)
   (char-table-range ct ?:)))"#;
    let expect =
        expect_test::expect![r#""OK (default letter-x digit digit digit default default)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-table-parent / set-char-table-parent inheritance chain
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_advanced_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three-level inheritance: grandparent -> parent -> child
    let form = r#"(let ((gp (make-char-table 'generic nil))
      (p  (make-char-table 'generic nil))
      (c  (make-char-table 'generic nil)))
  ;; Grandparent has defaults for everything
  (set-char-table-range gp t 'from-gp)
  ;; Parent overrides digits
  (set-char-table-range p '(?0 . ?9) 'from-parent)
  ;; Child overrides ?5 specifically
  (set-char-table-range c ?5 'from-child)
  ;; Build chain
  (set-char-table-parent p gp)
  (set-char-table-parent c p)
  (list
   ;; ?5 found in child directly
   (char-table-range c ?5)
   ;; ?3 not in child, found in parent (digit range)
   (char-table-range c ?3)
   ;; ?A not in child or parent, found in grandparent
   (char-table-range c ?A)
   ;; Verify parent accessors
   (eq (char-table-parent c) p)
   (eq (char-table-parent p) gp)
   (char-table-parent gp)))"#;
    let expect = expect_test::expect![r#""OK (from-child from-parent from-gp t t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-table-p and char-table-subtype predicates
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_advanced_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  (char-table-p (make-char-table 'generic))
  (char-table-p (make-char-table 'syntax-table))
  (char-table-p [1 2 3])
  (char-table-p "hello")
  (char-table-p nil)
  (char-table-subtype (make-char-table 'generic))
  (char-table-subtype (make-char-table 'syntax-table))
  ;; vector-or-char-table-p
  (vector-or-char-table-p (make-char-table 'generic))
  (vector-or-char-table-p [1 2 3])
  (vector-or-char-table-p '(1 2 3)))"#;
    let expect = expect_test::expect![r#""OK (t t nil nil nil generic syntax-table t t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-char-table-range overwriting and querying ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_advanced_range_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Overlapping ranges: later set-char-table-range overrides earlier
    let form = r#"(let ((ct (make-char-table 'generic nil)))
  ;; First set a broad range
  (set-char-table-range ct '(?a . ?z) 'lower)
  ;; Then override a sub-range
  (set-char-table-range ct '(?m . ?p) 'mid)
  ;; Then override a single char within that
  (set-char-table-range ct ?n 'specific)
  (list
   (char-table-range ct ?a)
   (char-table-range ct ?l)
   (char-table-range ct ?m)
   (char-table-range ct ?n)
   (char-table-range ct ?o)
   (char-table-range ct ?z)
   ;; Outside the range
   (char-table-range ct ?A)))"#;
    let expect = expect_test::expect![r#""OK (lower lower mid specific mid lower nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parent inheritance with overrides at child level
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_advanced_parent_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Child overrides parent's default; nil in child does NOT mask parent
    let form = r#"(let ((parent (make-char-table 'generic nil))
      (child (make-char-table 'generic nil)))
  (set-char-table-range parent t 'parent-default)
  (set-char-table-range parent ?a 'parent-a)
  (set-char-table-parent child parent)
  ;; Child has no entries yet, should inherit
  (let ((before-a (char-table-range child ?a))
        (before-b (char-table-range child ?b)))
    ;; Now override ?a in child
    (set-char-table-range child ?a 'child-a)
    (let ((after-a (char-table-range child ?a))
          ;; ?b still inherited
          (after-b (char-table-range child ?b)))
      (list before-a before-b after-a after-b))))"#;
    let expect = expect_test::expect![r#""OK (parent-a parent-default child-a parent-default)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: char-table as Unicode block classifier
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_advanced_unicode_classifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a char-table that classifies characters into Unicode blocks
    let form = r#"(let ((ct (make-char-table 'generic 'other)))
  ;; Basic Latin (U+0000-U+007F)
  (set-char-table-range ct '(0 . 127) 'basic-latin)
  ;; Latin-1 Supplement (U+0080-U+00FF)
  (set-char-table-range ct '(128 . 255) 'latin-1-supplement)
  ;; Override ASCII sub-ranges
  (set-char-table-range ct '(?0 . ?9) 'ascii-digit)
  (set-char-table-range ct '(?A . ?Z) 'ascii-upper)
  (set-char-table-range ct '(?a . ?z) 'ascii-lower)
  ;; Space and punctuation
  (set-char-table-range ct ?\s 'space)
  ;; CJK range (small sample)
  (set-char-table-range ct '(#x4e00 . #x9fff) 'cjk-unified)
  ;; Classify various characters
  (list
   (char-table-range ct ?A)
   (char-table-range ct ?z)
   (char-table-range ct ?5)
   (char-table-range ct ?\s)
   (char-table-range ct ?!)
   (char-table-range ct 200)
   (char-table-range ct #x4e2d)
   ;; A char outside all defined ranges
   (char-table-range ct #x1f600)))"#;
    let expect = expect_test::expect![
        r#""OK (ascii-upper ascii-lower ascii-digit space basic-latin latin-1-supplement cjk-unified other)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: char-table for custom case mapping
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_advanced_case_mapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a ROT13 mapping char-table, then use it to transform a string
    let form = r#"(let ((rot13 (make-char-table 'generic nil)))
  ;; Map a-m -> n-z, n-z -> a-m
  (let ((i 0))
    (while (< i 13)
      (set-char-table-range rot13 (+ ?a i) (+ ?n i))
      (set-char-table-range rot13 (+ ?n i) (+ ?a i))
      (set-char-table-range rot13 (+ ?A i) (+ ?N i))
      (set-char-table-range rot13 (+ ?N i) (+ ?A i))
      (setq i (1+ i))))
  ;; Apply the mapping to transform a string
  (let ((input "Hello World")
        (output nil)
        (idx 0))
    (while (< idx (length input))
      (let* ((ch (aref input idx))
             (mapped (char-table-range rot13 ch)))
        (setq output (cons (if mapped mapped ch) output)))
      (setq idx (1+ idx)))
    ;; Apply twice should return original
    (let ((first-pass (concat (nreverse output)))
          (second-output nil)
          (idx2 0))
      (setq output (nreverse output))
      (while (< idx2 (length first-pass))
        (let* ((ch (aref first-pass idx2))
               (mapped (char-table-range rot13 ch)))
          (setq second-output (cons (if mapped mapped ch) second-output)))
        (setq idx2 (1+ idx2)))
      (list first-pass
            (concat (nreverse second-output))
            (string= input (concat (nreverse second-output)))))))"#;
    let expect = expect_test::expect![[r#""OK (\"Uryyb Jbeyq\" \"Hello World\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: char-table for frequency counting over a string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_advanced_frequency_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a char-table to count character frequencies, then extract top chars
    let form = r#"(let ((freq (make-char-table 'generic 0))
      (input "abracadabra"))
  ;; Count frequencies
  (let ((i 0))
    (while (< i (length input))
      (let ((ch (aref input i)))
        (set-char-table-range freq ch
          (1+ (char-table-range freq ch))))
      (setq i (1+ i))))
  ;; Extract counts for specific chars
  (list
   (char-table-range freq ?a)
   (char-table-range freq ?b)
   (char-table-range freq ?r)
   (char-table-range freq ?c)
   (char-table-range freq ?d)
   ;; Char not in the string
   (char-table-range freq ?z)
   ;; Verify total
   (+ (char-table-range freq ?a)
      (char-table-range freq ?b)
      (char-table-range freq ?r)
      (char-table-range freq ?c)
      (char-table-range freq ?d))))"#;
    let expect = expect_test::expect![r#""OK (5 2 2 1 1 0 11)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
