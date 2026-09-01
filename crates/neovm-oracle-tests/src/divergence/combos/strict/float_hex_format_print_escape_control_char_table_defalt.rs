//! Strict combo oracle probes, batch 115: hex float format (%a/%A),
//! print-escape-control-characters, char-table :defalt, decode/encode-time
//! list form, and subr prin1 form.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s9_hex_float_format_a_A() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (condition-case err (format "%a" 1.5) (error (car err)))
      (condition-case err (format "%A" 1.5) (error (car err)))
      (condition-case err (format "%a" 0.0) (error (car err)))
      (condition-case err (format "%a" -1.0) (error (car err)))
      (condition-case err (format "%a" 3.14159) (error (car err)))
      (condition-case err (format "%a" (/ 1.0 0.0)) (error (car err))))
"####,
    );
}

#[test]
fn div_s9_print_escape_control_characters() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((s (string 7 8 9 10 13 27 65)))
  (list (let ((print-escape-control-characters t)) (prin1-to-string s))
        (let ((print-escape-control-characters nil)) (prin1-to-string s))
        (let ((print-escape-newlines t)) (prin1-to-string (string 10 13)))
        (let ((print-escape-control-characters t)
              (print-escape-newlines t))
          (prin1-to-string (string 9 10 7 8 13 27)))))
"####,
    );
}

#[test]
fn div_s9_char_table_defalt_and_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let* ((parent (make-char-table 'syntax-table 'parent-default))
       (child (make-char-table 'syntax-table 'child-default)))
  (set-char-table-parent child parent)
  (aset parent ?a 'parent-a)
  (aset child ?b 'child-b)
  (list (char-table-range parent ?a)
        (char-table-range parent ?b)
        (char-table-range parent ?z)
        (char-table-range child ?a)
        (char-table-range child ?b)
        (char-table-range child ?c)
        (char-table-range child nil)
        (char-table-parent child)))
"####,
    );
}

#[test]
fn div_s9_decode_encode_time_list_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (decode-time '(0 0 0 1 1 2000))
      (decode-time '(30 45 12 15 6 2025) 0)
      (encode-time '(0 0 0 1 1 2000))
      (encode-time '(30 0 12 1 1 1970))
      (encode-time '(59 59 23 31 12 1999) 0))
"####,
    );
}

#[test]
fn div_s9_subr_and_compiled_print_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (subrp (symbol-function 'car))
      (subrp (symbol-function '+))
      (condition-case err (format "%s" (symbol-function 'car))
        (error (cons 'err (car err))))
      (string-match "subr" (format "%s" (symbol-function 'car)))
      (compiled-function-p (symbol-function 'mapcar))
      (string-match "closure\\|lambda\\|compiled" (format "%s" (symbol-function 'mapcar))))
"####,
    );
}
