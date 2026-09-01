//! Strict combo oracle probes, batch 69: executable-set-magic (shebang
//! insertion), fringe-bitmap introspection, iso-transl (ISO8859 transliteration),
//! and hideshow (hs-minor-mode code folding).
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::assert_oracle_parity_with_load;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_o3_executable_set_magic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"line1\\nline2\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\n")
  (executable-set-magic "sh")
  (goto-char (point-min))
  (let ((valid-magic (and (looking-at "#!.*/sh$") t)))
    (forward-line 1)
    (list valid-magic
          (buffer-substring-no-properties (point) (point-max)))))
"##,
        expect,
    );
}

#[test]
fn div_o3_fringe_bitmap_introspection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function fringe-bitmaps)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (fringe-bitmap-p 'left-triangle)
      (fringe-bitmap-p 'right-arrow)
      (fringe-bitmap-p 'nonexistent-bitmap)
      (> (length (fringe-bitmaps)) 5)
      (memq 'left-triangle (fringe-bitmaps)))
"##,
        expect,
    );
}

#[test]
fn div_o3_iso_transl_decode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function iso-transl-decode)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(list (iso-transl-decode "A`")
      (iso-transl-decode "E'")
      (iso-transl-decode "u:")
      (iso-transl-decode "n~")
      (length iso-transl-esc-map))
"##,
        &["international/iso-transl.el"],
        expect,
    );
}

#[test]
fn div_o3_hideshow_minor_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_with_load_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo ()\n  (let ((x 1))\n    body))\n")
  (hs-minor-mode 1)
  (goto-char (point-min))
  (hs-hide-block)
  (list (hs-already-hidden-p)
        (get-text-property 2 'invisible)
        (hs-show-block)
        (hs-already-hidden-p)))
"##,
        &["progmodes/hideshow.el"],
        expect,
    );
}
