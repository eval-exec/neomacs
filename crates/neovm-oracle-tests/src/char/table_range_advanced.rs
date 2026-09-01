//! Advanced oracle parity tests for `char-table-range` and `set-char-table-range`.
//!
//! Covers: setting/reading single character entries, character range (cons pair)
//! entries, `t` range (all characters), nil range handling, overlapping range
//! semantics, and building a Unicode block classifier using char-table-range.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// set-char-table-range with single character
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_range_adv_single_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set individual characters with distinct values, read them back,
    // verify unset characters return the default. Then overwrite a
    // previously set character and verify the update.
    let form = r#"(let ((ct (make-char-table 'generic 'unset)))
  ;; Set individual characters
  (set-char-table-range ct ?a 'alpha-lower-a)
  (set-char-table-range ct ?z 'alpha-lower-z)
  (set-char-table-range ct ?A 'alpha-upper-a)
  (set-char-table-range ct ?0 'digit-zero)
  (set-char-table-range ct ?9 'digit-nine)
  (set-char-table-range ct ?\s 'space-char)
  (set-char-table-range ct #x4e2d 'cjk-zhong)
  (let ((results
         (list
          ;; Read back set values
          (char-table-range ct ?a)
          (char-table-range ct ?z)
          (char-table-range ct ?A)
          (char-table-range ct ?0)
          (char-table-range ct ?9)
          (char-table-range ct ?\s)
          (char-table-range ct #x4e2d)
          ;; Unset characters return default
          (char-table-range ct ?b)
          (char-table-range ct ?B)
          (char-table-range ct ?1)
          (char-table-range ct ?!))))
    ;; Overwrite ?a
    (set-char-table-range ct ?a 'alpha-lower-a-v2)
    (list results
          (char-table-range ct ?a)
          ;; Neighboring char unaffected
          (char-table-range ct ?b))))"#;
    let expect = expect_test::expect![
        r#""OK ((alpha-lower-a alpha-lower-z alpha-upper-a digit-zero digit-nine space-char cjk-zhong unset unset unset unset) alpha-lower-a-v2 unset)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-char-table-range with character range (cons pair)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_range_adv_cons_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set ranges using cons pairs, verify all characters in range get the value,
    // and characters just outside don't. Test multiple non-overlapping ranges,
    // then test a range that is a single-character range (start = end).
    let form = r#"(let ((ct (make-char-table 'generic nil)))
  ;; Lowercase letters
  (set-char-table-range ct '(?a . ?z) 'lowercase)
  ;; Uppercase letters
  (set-char-table-range ct '(?A . ?Z) 'uppercase)
  ;; Digits
  (set-char-table-range ct '(?0 . ?9) 'digit)
  ;; Single-character range
  (set-char-table-range ct '(?@ . ?@) 'at-sign)
  ;; Verify range boundaries
  (list
   ;; Lowercase range
   (char-table-range ct ?a)
   (char-table-range ct ?m)
   (char-table-range ct ?z)
   ;; Just outside lowercase
   (char-table-range ct (1- ?a))
   (char-table-range ct (1+ ?z))
   ;; Uppercase range
   (char-table-range ct ?A)
   (char-table-range ct ?Z)
   ;; Digit range
   (char-table-range ct ?0)
   (char-table-range ct ?5)
   (char-table-range ct ?9)
   ;; Just outside digits
   (char-table-range ct ?/)
   (char-table-range ct ?:)
   ;; Single-char range
   (char-table-range ct ?@)
   ;; Verify non-overlapping
   (char-table-range ct ?\s)
   (char-table-range ct ?!)))"#;
    let expect = expect_test::expect![
        r#""OK (lowercase lowercase lowercase nil nil uppercase uppercase digit digit digit nil nil at-sign nil nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// char-table-range reading back set values with overwrite semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_range_adv_readback_overwrite() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set a broad range, then override subsets and individual chars.
    // Verify the most recently set value wins at each position.
    let form = r#"(let ((ct (make-char-table 'generic 'base)))
  ;; Set entire ASCII printable range
  (set-char-table-range ct '(32 . 126) 'ascii-printable)
  ;; Override subranges
  (set-char-table-range ct '(?a . ?z) 'lower)
  (set-char-table-range ct '(?A . ?Z) 'upper)
  (set-char-table-range ct '(?0 . ?9) 'digit)
  ;; Override sub-subrange
  (set-char-table-range ct '(?a . ?f) 'hex-lower)
  (set-char-table-range ct '(?A . ?F) 'hex-upper)
  ;; Override individual char within hex range
  (set-char-table-range ct ?a 'the-letter-a)
  (set-char-table-range ct ?F 'the-letter-F)
  (list
   ;; Individual overrides win
   (char-table-range ct ?a)
   (char-table-range ct ?F)
   ;; Hex sub-subrange
   (char-table-range ct ?b)
   (char-table-range ct ?f)
   (char-table-range ct ?A)
   (char-table-range ct ?E)
   ;; Non-hex lower/upper
   (char-table-range ct ?g)
   (char-table-range ct ?z)
   (char-table-range ct ?G)
   (char-table-range ct ?Z)
   ;; Digit range
   (char-table-range ct ?0)
   (char-table-range ct ?9)
   ;; ASCII printable outside letter/digit
   (char-table-range ct ?!)
   (char-table-range ct ?~)
   ;; Outside ASCII printable: base default
   (char-table-range ct 31)
   (char-table-range ct 127)
   ;; Far outside: base default
   (char-table-range ct #x1000)))"#;
    let expect = expect_test::expect![
        r#""OK (the-letter-a the-letter-F hex-lower hex-lower hex-upper hex-upper lower lower upper upper digit digit ascii-printable ascii-printable base base base)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-char-table-range with t (all characters)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_range_adv_t_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Setting range `t` sets the default for all characters. Then specific
    // overrides still take precedence. Setting `t` again changes the default
    // for all non-overridden characters.
    let form = r#"(let ((ct (make-char-table 'generic nil)))
  ;; Initially all nil
  (let ((before-a (char-table-range ct ?a))
        (before-cjk (char-table-range ct #x4e00)))
    ;; Set default via t
    (set-char-table-range ct t 'everything)
    (let ((after-a (char-table-range ct ?a))
          (after-cjk (char-table-range ct #x4e00)))
      ;; Override specific characters
      (set-char-table-range ct ?a 'special-a)
      (set-char-table-range ct '(?0 . ?9) 'digit)
      ;; Check: overrides take precedence
      (let ((override-a (char-table-range ct ?a))
            (override-5 (char-table-range ct ?5))
            ;; Non-overridden chars still get 'everything
            (default-b (char-table-range ct ?b))
            (default-bang (char-table-range ct ?!)))
        ;; Change the default again
        (set-char-table-range ct t 'new-default)
        (let ((new-b (char-table-range ct ?b))
              ;; Overrides still hold
              (still-a (char-table-range ct ?a))
              (still-5 (char-table-range ct ?5)))
          (list
           before-a before-cjk
           after-a after-cjk
           override-a override-5
           default-b default-bang
           new-b still-a still-5))))))"#;
    let expect = expect_test::expect![
        r#""OK (nil nil everything everything special-a digit everything everything new-default new-default new-default)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-char-table-range with nil range
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_range_adv_nil_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // nil as range argument to set-char-table-range should set the default
    // value (same as t in Emacs). Test the behavior and compare with t.
    // Also test char-table-range with nil argument to read the default.
    let form = r#"(let ((ct1 (make-char-table 'generic nil))
      (ct2 (make-char-table 'generic nil)))
  ;; Set default via nil on ct1
  (set-char-table-range ct1 nil 'default-via-nil)
  ;; Set default via t on ct2
  (set-char-table-range ct2 t 'default-via-t)
  ;; Both should behave as setting the default
  (let ((ct1-a (char-table-range ct1 ?a))
        (ct2-a (char-table-range ct2 ?a))
        (ct1-z (char-table-range ct1 ?z))
        (ct2-z (char-table-range ct2 ?z))
        ;; Read default slot via nil
        (ct1-nil (char-table-range ct1 nil))
        (ct2-nil (char-table-range ct2 nil)))
    ;; Override a specific char in ct1
    (set-char-table-range ct1 ?a 'special)
    (let ((ct1-a-after (char-table-range ct1 ?a))
          (ct1-b-after (char-table-range ct1 ?b)))
      (list
       ct1-a ct2-a ct1-z ct2-z
       ct1-nil ct2-nil
       ct1-a-after ct1-b-after))))"#;
    let expect = expect_test::expect![
        r#""OK (default-via-nil default-via-t default-via-nil default-via-t default-via-nil nil special default-via-nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: Unicode block classifier with multi-level overrides
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_range_adv_unicode_block_classifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a char-table that classifies characters into Unicode blocks
    // with multiple levels of granularity: broad blocks, then sub-blocks,
    // then special individual characters. Use it as a classifier function
    // that processes a mixed-script string.
    let form = r#"(progn
  (fset 'neovm--ctrange-classify
    (lambda (ct str)
      "Classify each character in STR using char-table CT, return alist of (class . count)."
      (let ((counts (make-hash-table :test 'eq))
            (i 0))
        (while (< i (length str))
          (let* ((ch (aref str i))
                 (class (char-table-range ct ch)))
            (puthash class (1+ (gethash class counts 0)) counts))
          (setq i (1+ i)))
        ;; Convert to sorted alist
        (let ((result nil))
          (maphash (lambda (k v) (setq result (cons (cons k v) result))) counts)
          (sort result (lambda (a b) (string< (symbol-name (car a))
                                               (symbol-name (car b)))))))))

  (unwind-protect
      (let ((ct (make-char-table 'generic 'other)))
        ;; Level 1: Broad Unicode blocks
        (set-char-table-range ct '(#x0000 . #x007F) 'basic-latin)
        (set-char-table-range ct '(#x0080 . #x00FF) 'latin-supplement)
        (set-char-table-range ct '(#x0100 . #x024F) 'latin-extended)
        (set-char-table-range ct '(#x0370 . #x03FF) 'greek)
        (set-char-table-range ct '(#x0400 . #x04FF) 'cyrillic)
        (set-char-table-range ct '(#x4E00 . #x9FFF) 'cjk)
        (set-char-table-range ct '(#x3040 . #x309F) 'hiragana)
        (set-char-table-range ct '(#x30A0 . #x30FF) 'katakana)
        ;; Level 2: Sub-blocks within basic-latin
        (set-char-table-range ct '(?0 . ?9) 'ascii-digit)
        (set-char-table-range ct '(?A . ?Z) 'ascii-upper)
        (set-char-table-range ct '(?a . ?z) 'ascii-lower)
        (set-char-table-range ct '(#x21 . #x2F) 'ascii-punct)
        ;; Level 3: Special individual characters
        (set-char-table-range ct ?\s 'whitespace)
        (set-char-table-range ct ?\t 'whitespace)
        (set-char-table-range ct ?\n 'whitespace)
        (set-char-table-range ct ?@ 'at-sign)
        ;; Test classification of individual characters
        (let ((individual-results
               (list
                (char-table-range ct ?a)
                (char-table-range ct ?Z)
                (char-table-range ct ?5)
                (char-table-range ct ?\s)
                (char-table-range ct ?@)
                (char-table-range ct ?!)
                (char-table-range ct #x00E9)
                (char-table-range ct #x03B1)
                (char-table-range ct #x0414)
                (char-table-range ct #x4E2D)
                (char-table-range ct #x3042)
                (char-table-range ct #x30A2)
                (char-table-range ct #x1F600))))
          ;; Test classifier function on a mixed string
          (let ((classification (funcall 'neovm--ctrange-classify ct "Hello World 42!")))
            (list
             individual-results
             classification
             ;; Verify specific expected counts from "Hello World 42!"
             ;; H(upper) e(lower) l(lower) l(lower) o(lower) (ws) W(upper)
             ;; o(lower) r(lower) l(lower) d(lower) (ws) 4(digit) 2(digit) !(punct)
             (length classification)))))
    (fmakunbound 'neovm--ctrange-classify)))"#;
    let expect = expect_test::expect![
        r#""OK ((ascii-lower ascii-upper ascii-digit whitespace at-sign ascii-punct latin-supplement greek cyrillic cjk hiragana katakana other) ((ascii-digit . 2) (ascii-lower . 8) (ascii-punct . 1) (ascii-upper . 2) (whitespace . 2)) 5)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Char-table with parent: range queries fall through to parent
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_char_table_range_adv_parent_fallthrough() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build parent with broad ranges, child with specific overrides.
    // Verify char-table-range on child falls through to parent for
    // non-overridden positions. Then set-char-table-range on child
    // for a sub-range and verify partial override behavior.
    let form = r#"(let ((parent (make-char-table 'generic 'p-default))
      (child (make-char-table 'generic nil)))
  ;; Parent has detailed ranges
  (set-char-table-range parent '(?a . ?z) 'p-lower)
  (set-char-table-range parent '(?A . ?Z) 'p-upper)
  (set-char-table-range parent '(?0 . ?9) 'p-digit)
  (set-char-table-range parent ?! 'p-exclaim)
  ;; Set parent relationship
  (set-char-table-parent child parent)
  ;; Child initially empty: everything falls through
  (let ((fall-a (char-table-range child ?a))
        (fall-Z (char-table-range child ?Z))
        (fall-5 (char-table-range child ?5))
        (fall-bang (char-table-range child ?!))
        (fall-space (char-table-range child ?\s)))
    ;; Child overrides a sub-range of lowercase
    (set-char-table-range child '(?a . ?f) 'c-hex)
    ;; And one specific uppercase
    (set-char-table-range child ?X 'c-special-x)
    (let ((over-a (char-table-range child ?a))
          (over-f (char-table-range child ?f))
          ;; g falls through to parent
          (fall-g (char-table-range child ?g))
          (fall-z (char-table-range child ?z))
          ;; X is overridden, Y falls through
          (over-X (char-table-range child ?X))
          (fall-Y (char-table-range child ?Y))
          ;; Digit still falls through
          (fall-7 (char-table-range child ?7))
          ;; Verify parent unchanged
          (p-a (char-table-range parent ?a))
          (p-f (char-table-range parent ?f)))
      (list
       fall-a fall-Z fall-5 fall-bang fall-space
       over-a over-f fall-g fall-z
       over-X fall-Y fall-7
       p-a p-f))))"#;
    let expect = expect_test::expect![
        r#""OK (p-lower p-upper p-digit p-exclaim p-default c-hex c-hex p-lower p-lower c-special-x p-upper p-digit p-lower p-lower)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}
