//! Strict combo oracle probes, batch 275: data / symbol / obarray CORE variable
//! sweep. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_obarray_symbols_with_pos_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'obarray)
      (boundp 'symbols-with-pos-enabled)
      (boundp 'symbols-with-pos-use)
      (boundp 'read-with-symbol-positions)
      (boundp 'read-symbol-positions-list)
      (boundp 'default-major-mode)
      (boundp 'initial-major-mode)
      (boundp 'default-mode-line-format)
      (boundp 'kill-emacs-hook)
      (boundp 'kill-emacs-query-functions)
      (boundp 'lambda-allocation-checking)
      (boundp 'purecopy-strings))
"##;
    let expect = expect_test::expect![[r#""OK (t t nil nil nil nil t nil t t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_char_table_pure_memory_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'char-table-extra-slot)
      (boundp 'char-coding-system-table)
      (boundp 'char-script-table)
      (boundp 'char-width-table)
      (boundp 'char-direction-table)
      (boundp 'print-gensym)
      (boundp 'print-quoted)
      (boundp 'print-escape-newlines)
      (boundp 'print-escape-control-characters)
      (boundp 'print-escape-nonascii)
      (boundp 'print-circle)
      (boundp 'print-length))
"##;
    let expect = expect_test::expect![[r#""OK (nil nil t t nil t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_misc_data_symbol_var_sweep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (boundp 'most-negative-fixnum)
      (boundp 'most-positive-float)
      (boundp 'most-negative-float)
      (boundp 'least-positive-float)
      (boundp 'least-negative-float)
      (boundp 'least-positive-normalized-float)
      (boundp 'least-negative-normalized-float)
      (boundp 'float-epsilon)
      (boundp 'float-negative-epsilon)
      (boundp 'integer-width)
      (boundp 'binary-as-unsigned))
"##;
    let expect = expect_test::expect![[r#""OK (t nil nil nil nil nil nil nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
