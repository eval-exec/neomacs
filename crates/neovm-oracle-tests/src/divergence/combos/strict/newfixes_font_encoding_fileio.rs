//! Strict combo oracle probes, batch 111: areas from recent remote fixes —
//! font-spec height derivation, encode_lisp_string ASCII round-trip for
//! unmodeled codepage codings, and file-name-directory edge cases.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_s5_font_spec_height_derivation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(let ((fs1 (font-spec :family "Monospace" :size 14))
      (fs2 (font-spec :family "Monospace" :height 140))
      (fs3 (font-spec :family "Monospace")))
  (list (font-get fs1 :size)
        (font-get fs1 :height)
        (font-get fs2 :size)
        (font-get fs2 :height)
        (font-get fs3 :size)
        (font-get fs3 :height)))
"####,
    );
}

#[test]
fn div_s5_encode_lisp_string_ascii_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (encode-coding-string "ABC" 'windows-1252)
      (length (encode-coding-string "ABC" 'windows-1252))
      (encode-coding-string "ABC" 'iso-8859-15)
      (decode-coding-string (encode-coding-string "ABC" 'windows-1255) 'windows-1255)
      (encode-coding-string "café" 'windows-1252)
      (length (encode-coding-string "café" 'windows-1252)))
"####,
    );
}

#[test]
fn div_s5_file_name_directory_edge_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (file-name-directory "/a/b/c")
      (file-name-directory "c")
      (file-name-directory "/")
      (file-name-directory "")
      (file-name-directory "./relative/file")
      (file-name-directory "../up/file")
      (file-name-directory "~/home/file")
      (file-name-nondirectory "/a/b/c.txt")
      (file-name-nondirectory "/")
      (file-name-nondirectory "")
      (file-name-sans-versions "file.txt~")
      (file-name-sans-versions "file.txt.~3~"))
"####,
    );
}

#[test]
fn div_s5_loadup_smoke_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    assert_oracle_parity(
        r####"
(list (fboundp 'car)
      (fboundp 'mapcar)
      (fboundp 'let)
      (fboundp 'condition-case)
      (boundp 'load-path)
      (boundp 'obarray)
      (> (length load-path) 3)
      (fboundp 'define-derived-mode)
      (fboundp 'cl-defgeneric)
      (fboundp 'byte-compile))
"####,
    );
}
