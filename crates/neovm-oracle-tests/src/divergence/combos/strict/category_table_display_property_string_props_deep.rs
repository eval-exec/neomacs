//! Strict combo oracle probes, batch 113: category table deep operations,
//! display property variants (space/align-to/margin), string vs buffer
//! text-property interaction, and bool-vector on large vectors.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s7_category_table_deep_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((ct (make-category-table)))
  (define-category ?p "probe category" ct)
  (modify-category-entry ?a ?p ct)
  (modify-category-entry ?b ?p ct)
  (list (category-docstring ?p ct)
        (aref ct ?a)
        (aref ct ?b)
        (aref ct ?c)
        (category-set-mnemonics (aref ct ?a))
        (char-table-p ct)
        (char-table-subtype ct)))
"####,
    );
}

#[test]
fn div_s7_display_property_string_width_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (string-width (propertize "x" 'display '(space :width 5)))
      (string-width (propertize "x" 'display '(space :align-to 10)))
      (string-width (propertize "x" 'display "YYY"))
      (string-width (propertize "x" 'display '(margin left-margin)))
      (string-width (propertize "abc" 'display "XY")))
"####,
    );
}

#[test]
fn div_s7_string_props_vs_buffer_text_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((s1 (propertize "hello" 'face 'bold 'rear-nonsticky t))
      (s2 #("world" 0 3 (face italic) 3 5 (face underline))))
  (list (text-properties-at 0 s1)
        (text-properties-at 4 s1)
        (text-properties-at 0 s2)
        (text-properties-at 3 s2)
        (text-properties-at 4 s2)
        (next-single-property-change 0 'face s1)
        (next-single-property-change 0 'face s2)))
"####,
    );
}

#[test]
fn div_s7_bool_vector_large_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((a (make-bool-vector 100 nil))
      (b (make-bool-vector 100 nil)))
  (dotimes (i 100)
    (when (= (% i 2) 0) (aset a i t))
    (when (= (% i 3) 0) (aset b i t)))
  (let ((inter (bool-vector-intersection a b))
        (union (bool-vector-union a b))
        (compl (bool-vector-complement a)))
    (list (bool-vector-count-set-bits inter)
          (bool-vector-count-set-bits union)
          (bool-vector-count-set-bits compl)
          (bool-vector-subsetp inter union))))
"####,
    );
}

#[test]
fn div_s7_coding_system_plist_all_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((pl (coding-system-plist 'utf-8)))
  (list (plist-get pl :name)
        (plist-get pl :mime-charset)
        (plist-get pl :coding-type)
        (plist-get pl :ascii-compatible-p)
        (plist-get pl :eol-type)
        (plist-get pl :charset-list)))
"####,
    );
}
