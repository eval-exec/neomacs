//! Strict combo oracle probes, batch 118: cl-case/cl-ccase/cl-ecase with
//! error conditions, copy-tree with vectors, secure-hash binary input,
//! prin1 of font-spec/category-set, and numeric edge cases
//! (subnormals, transcendental of pi, integer-as-float formatting).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_t2_cl_case_ccase_ecase_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (cl-case 2 (1 'one) (2 'two) (t 'other))
      (cl-case 99 (1 'one) (2 'two) (t 'other))
      (cl-case 'b (a 1) ((b c) 2) (d 3))
      (condition-case err (cl-ecase 99 (1 'one) (2 'two)) (cl-ecase-failure-type nil) (error (car err)))
      (condition-case err (cl-ccase 99 (1 'one) (2 'two)) (error (car err))))
"####,
    );
}

#[test]
fn div_t2_copy_tree_with_vectors_and_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let* ((original (list [1 2 3] (list (vector 'a 'b) 'c) "str" 42))
       (copied (copy-tree original t)))
  (list (eq (car original) (car copied))
        (equal (car original) (car copied))
        (eq (cadr original) (cadr copied))
        (eq (caadr original) (caadr copied))
        copied))
"####,
    );
}

#[test]
fn div_t2_secure_hash_binary_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((unibyte (string-make-unibyte (string 0 1 255 200 128))))
  (list (secure-hash 'sha256 unibyte)
        (secure-hash 'md5 unibyte)
        (length (secure-hash 'sha256 "plain text"))
        (secure-hash 'sha512 "")))
"####,
    );
}

#[test]
fn div_t2_numeric_transcendental_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (sin float-pi)
      (cos float-pi)
      (tan (/ float-pi 4))
      (log (exp 1.0))
      (log 100.0 10.0)
      (expt 10 100)
      (expt 2 -10)
      (expt 0 0)
      (expt 0.0 0.0)
      (format "%f" 42)
      (format "%f" 0)
      (format "%.0f" -0.5))
"####,
    );
}

#[test]
fn div_t2_prin1_font_spec_category_set_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((fs (font-spec :family "Monospace" :weight 'bold)))
  (list (string-match "font-spec" (format "%S" fs))
        (string-match "Monospace" (format "%S" fs))
        (let ((ct (standard-category-table)))
          (string-match "category-table" (format "%S" ct)))))
"####,
    );
}
