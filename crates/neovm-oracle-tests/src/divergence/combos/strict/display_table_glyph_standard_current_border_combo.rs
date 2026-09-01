//! Strict combo oracle probes, batch 176: display tables. make-display-table
//! (256-vector), per-glyph vector entries, standard-display-table vs current-
//! display-table binding, glyph-table length, and standard-display + glyphless
//! char display settings.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_display_table_make_glyph_aset_aref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((dt (make-display-table)))
  (aset dt ?a [?A ?B])
  (aset dt ?\n [(glyph 8629)])
  (list (vectorp dt)
        (length dt)
        (aref dt ?a)
        (aref dt ?b)
        (aref dt 0)
        (aref dt ?z)
        (eq (aref dt ?c) nil)))
"##;
    let expect = expect_test::expect![[r#""OK (nil 4194304 [65 66] nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_standard_current_display_table_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((saved standard-display-table))
  (unwind-protect
      (let ((dt (make-display-table)))
        (aset dt ?x [?X])
        (setq standard-display-table dt)
        (list (eq standard-display-table dt)
              (eq (aref standard-display-table ?x) [?X])
              (vectorp (or current-display-table standard-display-table))
              (aref standard-display-table ?x)))
    (setq standard-display-table saved)))
"##;
    let expect = expect_test::expect![[r#""ERR (void-variable current-display-table)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_glyphless_char_display_standard_display_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((dt (make-display-table)))
  (aset dt ?\t [?\\ ?t])
  (list (aref dt ?\t)
        (length dt)
        (aref dt ?\t)
        (condition-case err
            (standard-display-ascii ?\t "^I")
          (error 'err-caught))
        (vectorp dt)
        (length (make-glyph-code ?X 'bold))))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 4194392)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
