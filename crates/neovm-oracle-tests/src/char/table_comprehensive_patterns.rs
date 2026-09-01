//! Comprehensive oracle parity tests for char-table operations:
//! `make-char-table`, `char-table-p`, `set-char-table-range`, `char-table-range`,
//! `map-char-table`, `char-table-parent`, `set-char-table-parent`,
//! `char-table-extra-slot`, `set-char-table-extra-slot`, inheritance via parent,
//! range-based mapping, and char-table used as category/syntax tables.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// map-char-table with single-char entries and accumulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_map_single_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((ct (make-char-table 'generic nil))
      (result nil))
  ;; Set individual chars
  (set-char-table-range ct ?a 'vowel)
  (set-char-table-range ct ?e 'vowel)
  (set-char-table-range ct ?i 'vowel)
  (set-char-table-range ct ?o 'vowel)
  (set-char-table-range ct ?u 'vowel)
  (set-char-table-range ct ?b 'consonant)
  (set-char-table-range ct ?c 'consonant)
  (set-char-table-range ct ?d 'consonant)
  ;; map-char-table collects (key . value) pairs
  (map-char-table
    (lambda (key val)
      (setq result (cons (cons key val) result)))
    ct)
  ;; Sort results by key for deterministic output
  (setq result (sort result (lambda (a b) (< (car a) (car b)))))
  (list (length result) result))"#;
    let expect =
        expect_test::expect![r#""ERR (wrong-type-argument number-or-marker-p (118 . 4194303))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// map-char-table with range entries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_map_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((ct (make-char-table 'generic nil))
      (result nil))
  ;; Set a contiguous range
  (set-char-table-range ct '(?A . ?Z) 'upper)
  ;; Set a single override within the range
  (set-char-table-range ct ?M 'middle)
  ;; map-char-table iterates all non-nil entries
  (map-char-table
    (lambda (key val)
      (setq result (cons (list key val) result)))
    ct)
  ;; Check specific chars via char-table-range
  (list
    (char-table-range ct ?A)
    (char-table-range ct ?M)
    (char-table-range ct ?Z)
    (char-table-range ct ?a)
    (length result)))"#;
    let expect = expect_test::expect![r#""OK (upper middle upper nil 3)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Extra slots: set and get on char-table with extra slots
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_extra_slots_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // syntax-table subtype provides 3 extra slots
    let form = r#"(let ((ct (make-char-table 'syntax-table nil)))
  ;; Set extra slots
  (set-char-table-extra-slot ct 0 'slot-zero)
  (set-char-table-extra-slot ct 1 '(complex data 42))
  (set-char-table-extra-slot ct 2 "string-in-slot")
  ;; Read them back
  (list
    (char-table-extra-slot ct 0)
    (char-table-extra-slot ct 1)
    (char-table-extra-slot ct 2)
    ;; Overwrite and re-read
    (progn
      (set-char-table-extra-slot ct 0 'replaced)
      (char-table-extra-slot ct 0))
    ;; Verify others unchanged
    (char-table-extra-slot ct 1)
    (char-table-extra-slot ct 2)))"#;
    let expect = expect_test::expect![
        r#""ERR (args-out-of-range #^[nil nil syntax-table nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] 0)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deep parent chain with 4 levels of inheritance
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_deep_parent_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((root (make-char-table 'generic nil))
      (l1 (make-char-table 'generic nil))
      (l2 (make-char-table 'generic nil))
      (l3 (make-char-table 'generic nil)))
  ;; Root: default for everything
  (set-char-table-range root t 'root-val)
  ;; Level 1: override digits
  (set-char-table-range l1 '(?0 . ?9) 'l1-digit)
  ;; Level 2: override uppercase
  (set-char-table-range l2 '(?A . ?Z) 'l2-upper)
  ;; Level 3: override specific chars
  (set-char-table-range l3 ?x 'l3-x)
  (set-char-table-range l3 ?5 'l3-five)
  ;; Wire up chain: l3 -> l2 -> l1 -> root
  (set-char-table-parent l1 root)
  (set-char-table-parent l2 l1)
  (set-char-table-parent l3 l2)
  (list
    ;; ?x directly in l3
    (char-table-range l3 ?x)
    ;; ?5 directly in l3 (overrides l1-digit)
    (char-table-range l3 ?5)
    ;; ?3 not in l3, not in l2, found in l1 (digit range)
    (char-table-range l3 ?3)
    ;; ?A not in l3, found in l2 (upper range)
    (char-table-range l3 ?A)
    ;; ?! not anywhere except root
    (char-table-range l3 ?!)
    ;; Verify parent chain accessors
    (eq (char-table-parent l3) l2)
    (eq (char-table-parent l2) l1)
    (eq (char-table-parent l1) root)
    (null (char-table-parent root))
    ;; Remove parent from l2, ?3 should now come from root
    (progn
      (set-char-table-parent l2 root)
      (char-table-range l3 ?3))))"#;
    let expect =
        expect_test::expect![r#""OK (l3-x l3-five l1-digit l2-upper root-val t t t t root-val)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parent with nil masking: child sets nil, parent has value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_nil_inheritance_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // In Emacs, nil in the child does NOT mask parent (nil means "not set"),
    // so the parent's value is inherited. Only an explicit non-nil value overrides.
    let form = r#"(let ((parent (make-char-table 'generic nil))
      (child (make-char-table 'generic nil)))
  (set-char-table-range parent ?a 'from-parent)
  (set-char-table-range parent ?b 'parent-b)
  (set-char-table-range parent t 'parent-default)
  (set-char-table-parent child parent)
  ;; Child inherits ?a from parent
  (let ((v1 (char-table-range child ?a)))
    ;; Now set ?a explicitly in child
    (set-char-table-range child ?a 'child-a)
    (let ((v2 (char-table-range child ?a))
          ;; ?b still inherited
          (v3 (char-table-range child ?b))
          ;; Unknown char gets parent default
          (v4 (char-table-range child ?z)))
      (list v1 v2 v3 v4))))"#;
    let expect =
        expect_test::expect![r#""OK (parent-default child-a parent-default parent-default)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// map-char-table with default (t) value
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_map_with_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((ct (make-char-table 'generic 'base))
      (count 0))
  ;; Override a few chars
  (set-char-table-range ct ?x 'special-x)
  (set-char-table-range ct '(?0 . ?9) 'digit)
  ;; map-char-table should iterate at least over the explicitly set entries
  ;; and the default range(s)
  (map-char-table
    (lambda (key val)
      (setq count (1+ count)))
    ct)
  ;; count should be > 0 (exact number is implementation-dependent,
  ;; but we can verify the overrides are accessible)
  (list
    (> count 0)
    (char-table-range ct ?x)
    (char-table-range ct ?5)
    (char-table-range ct ?A)))"#;
    let expect = expect_test::expect![r#""OK (t special-x digit base)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-table-subtype and multiple subtypes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_subtypes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(list
  ;; Various subtypes
  (char-table-subtype (make-char-table 'generic))
  (char-table-subtype (make-char-table 'syntax-table))
  (char-table-subtype (make-char-table nil))
  ;; char-table-p on various types
  (char-table-p (make-char-table 'generic))
  (char-table-p [1 2 3])
  (char-table-p '(1 2 3))
  (char-table-p (make-bool-vector 10 t))
  (char-table-p (make-char-table nil))
  ;; Verify subtype is preserved after modifications
  (let ((ct (make-char-table 'generic)))
    (set-char-table-range ct ?a 42)
    (char-table-subtype ct)))"#;
    let expect = expect_test::expect![r#""OK (generic syntax-table nil t nil nil nil t generic)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Using char-table as a transliteration engine
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_transliteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((ct (make-char-table 'generic nil)))
  ;; Build a Caesar cipher shift-3
  (let ((i 0))
    (while (< i 26)
      ;; Lowercase: shift by 3 with wrap
      (set-char-table-range ct (+ ?a i) (+ ?a (% (+ i 3) 26)))
      ;; Uppercase: shift by 3 with wrap
      (set-char-table-range ct (+ ?A i) (+ ?A (% (+ i 3) 26)))
      (setq i (1+ i))))
  ;; Apply cipher to a string
  (let ((input "Hello World Xyz")
        (output nil)
        (idx 0))
    (while (< idx (length input))
      (let* ((ch (aref input idx))
             (mapped (char-table-range ct ch)))
        (setq output (cons (if mapped mapped ch) output)))
      (setq idx (1+ idx)))
    ;; Build a reverse cipher
    (let ((rev-ct (make-char-table 'generic nil))
          (i 0))
      (while (< i 26)
        (set-char-table-range rev-ct (+ ?a (% (+ i 3) 26)) (+ ?a i))
        (set-char-table-range rev-ct (+ ?A (% (+ i 3) 26)) (+ ?A i))
        (setq i (1+ i)))
      ;; Apply reverse to the ciphered text
      (let ((ciphered (concat (nreverse output)))
            (decrypted nil)
            (idx2 0))
        (while (< idx2 (length ciphered))
          (let* ((ch (aref ciphered idx2))
                 (mapped (char-table-range rev-ct ch)))
            (setq decrypted (cons (if mapped mapped ch) decrypted)))
          (setq idx2 (1+ idx2)))
        (list ciphered
              (concat (nreverse decrypted))
              (string= input (concat (nreverse decrypted))))))))"#;
    let expect = expect_test::expect![[r#""OK (\"Khoor Zruog Abc\" \"Hello World Xyz\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Char-table with Unicode ranges and CJK classification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_unicode_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((ct (make-char-table 'generic 'unknown)))
  ;; ASCII control chars
  (set-char-table-range ct '(0 . 31) 'control)
  ;; ASCII printable
  (set-char-table-range ct '(32 . 126) 'printable)
  ;; Sub-classify printable
  (set-char-table-range ct '(?0 . ?9) 'digit)
  (set-char-table-range ct '(?A . ?Z) 'upper)
  (set-char-table-range ct '(?a . ?z) 'lower)
  (set-char-table-range ct ?\s 'whitespace)
  ;; Latin Extended
  (set-char-table-range ct '(#x00C0 . #x00FF) 'latin-ext)
  ;; Greek
  (set-char-table-range ct '(#x0370 . #x03FF) 'greek)
  ;; CJK Unified Ideographs
  (set-char-table-range ct '(#x4E00 . #x9FFF) 'cjk)
  ;; Emoji range (partial)
  (set-char-table-range ct '(#x1F600 . #x1F64F) 'emoji)
  (list
    (char-table-range ct 0)         ;; control
    (char-table-range ct 10)        ;; control (newline)
    (char-table-range ct ?\s)       ;; whitespace
    (char-table-range ct ?5)        ;; digit
    (char-table-range ct ?G)        ;; upper
    (char-table-range ct ?m)        ;; lower
    (char-table-range ct ?~)        ;; printable
    (char-table-range ct #x00E9)    ;; latin-ext (e with accent)
    (char-table-range ct #x03B1)    ;; greek (alpha)
    (char-table-range ct #x4E2D)    ;; cjk (zhong)
    (char-table-range ct #x1F600)   ;; emoji
    (char-table-range ct #x10000)))"#;
    let expect = expect_test::expect![
        r#""OK (control control whitespace digit upper lower printable latin-ext greek cjk emoji unknown)""#
    ];
    // unknown
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Char-table used to build a simple scoring function
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_scoring_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use a char-table to assign "scrabble-like" scores per letter, then score words
    let form = r#"(progn
  (fset 'neovm--ct-build-scores
    (lambda ()
      (let ((ct (make-char-table 'generic 0)))
        ;; 1 point: a, e, i, o, u, l, n, s, t, r
        (dolist (ch '(?a ?e ?i ?o ?u ?l ?n ?s ?t ?r))
          (set-char-table-range ct ch 1))
        ;; 2 points: d, g
        (dolist (ch '(?d ?g))
          (set-char-table-range ct ch 2))
        ;; 3 points: b, c, m, p
        (dolist (ch '(?b ?c ?m ?p))
          (set-char-table-range ct ch 3))
        ;; 4 points: f, h, v, w, y
        (dolist (ch '(?f ?h ?v ?w ?y))
          (set-char-table-range ct ch 4))
        ;; 5 points: k
        (set-char-table-range ct ?k 5)
        ;; 8 points: j, x
        (dolist (ch '(?j ?x))
          (set-char-table-range ct ch 8))
        ;; 10 points: q, z
        (dolist (ch '(?q ?z))
          (set-char-table-range ct ch 10))
        ct)))

  (fset 'neovm--ct-score-word
    (lambda (ct word)
      (let ((total 0) (i 0))
        (while (< i (length word))
          (let ((ch (aref (downcase word) i)))
            (setq total (+ total (or (char-table-range ct ch) 0))))
          (setq i (1+ i)))
        total)))

  (unwind-protect
      (let ((scores (funcall 'neovm--ct-build-scores)))
        (list
          (funcall 'neovm--ct-score-word scores "hello")
          (funcall 'neovm--ct-score-word scores "quiz")
          (funcall 'neovm--ct-score-word scores "jazz")
          (funcall 'neovm--ct-score-word scores "EXTRA")
          (funcall 'neovm--ct-score-word scores "a")
          (funcall 'neovm--ct-score-word scores "")
          ;; Compare two words
          (let ((s1 (funcall 'neovm--ct-score-word scores "python"))
                (s2 (funcall 'neovm--ct-score-word scores "lisp")))
            (list s1 s2 (> s1 s2)))))
    (fmakunbound 'neovm--ct-build-scores)
    (fmakunbound 'neovm--ct-score-word)))"#;
    let expect = expect_test::expect![r#""OK (8 22 29 12 1 0 (14 6 t))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// map-char-table collecting only entries matching a filter
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_comprehensive_map_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((ct (make-char-table 'generic nil))
      (vowels nil)
      (consonants nil))
  ;; Classify lowercase letters
  (dolist (ch '(?a ?e ?i ?o ?u))
    (set-char-table-range ct ch 'vowel))
  (dolist (ch '(?b ?c ?d ?f ?g ?h ?j ?k ?l ?m ?n ?p ?q ?r ?s ?t ?v ?w ?x ?y ?z))
    (set-char-table-range ct ch 'consonant))
  ;; Use map-char-table to filter into two lists
  (map-char-table
    (lambda (key val)
      (cond
        ((eq val 'vowel)
         (setq vowels (cons key vowels)))
        ((eq val 'consonant)
         (setq consonants (cons key consonants)))))
    ct)
  (list
    (length (setq vowels (sort vowels '<)))
    (length (setq consonants (sort consonants '<)))
    vowels
    ;; Just first 5 consonants for brevity
    (let ((first5 nil) (i 0))
      (while (and (< i 5) (nth i consonants))
        (setq first5 (cons (nth i consonants) first5))
        (setq i (1+ i)))
      (nreverse first5))))"#;
    let expect =
        expect_test::expect![r#""ERR (wrong-type-argument number-or-marker-p (123 . 4194303))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
