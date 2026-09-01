//! Strict combo oracle probes, batch 300: display-table / glyphless /
//! standard-display behavioral. make-display-table glyph vectors,
//! glyphless-char-display, and standard-display-ascii.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_display_table_glyph_vector_aset_aref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((dt (make-display-table)))
  (aset dt ?a [?A ?B])
  (aset dt ?\t [?\\ ?t])
  (list (aref dt ?a)
        (aref dt ?b)
        (aref dt ?\t)
        (length dt)
        (aref dt 0)))
"##;
    let expect = expect_test::expect![[r#""OK ([65 66] nil [92 116] 4194304 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_glyphless_char_display_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((saved glyphless-char-display))
  (unwind-protect
      (progn
        (setq glyphless-char-display nil)
        (list (glyphless-char-p ?a glyphless-char-display)
              (let ((g (make-glyph-code ?A 'bold)))
                (and (consp g) (integerp (car g))))))
    (setq glyphless-char-display saved)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function glyphless-char-p)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_standard_display_ascii_current_display_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((saved standard-display-table))
  (unwind-protect
      (progn
        (setq standard-display-table (make-display-table))
        (standard-display-ascii ?\t "^I")
        (list (aref standard-display-table ?\t)
              (eq standard-display-table (or current-display-table standard-display-table))
              (vectorp standard-display-table)))
    (setq standard-display-table saved)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable current-display-table)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
