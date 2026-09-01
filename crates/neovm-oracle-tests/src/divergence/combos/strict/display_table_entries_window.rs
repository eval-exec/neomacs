//! Strict combo oracle probes, batch 93: display-table glyph entries (setting
//! and reading actual character→glyph mappings) and window-display-table.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_q7_display_table_glyph_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([65] [66] nil [88 89] 2 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dt (make-display-table)))
  (aset dt ?a [?A])
  (aset dt ?\n [?B])
  (aset dt 128 [?X ?Y])
  (list (aref dt ?a)
        (aref dt ?\n)
        (aref dt ?b)
        (aref dt 128)
        (length (aref dt 128))
        (char-table-p dt)))
"##,
        expect,
    );
}

#[test]
fn div_q7_window_display_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t [65] nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dt (make-display-table)))
  (aset dt ?a [?A])
  (set-window-display-table nil dt)
  (list (eq (window-display-table) dt)
        (aref (window-display-table) ?a)
        (aref (window-display-table) ?b)))
"##,
        expect,
    );
}

#[test]
fn div_q7_standard_display_table_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function standard-display-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((dt (standard-display-table)))
  (list (null dt)
        (if dt (aref dt ?\t) nil)
        (if dt (aref dt ?\n) nil)
        (if dt (char-table-p dt) nil)))
"##,
        expect,
    );
}
