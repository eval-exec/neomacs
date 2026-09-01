//! Strict combo oracle probes, batch 289: pp pp-to-string pp-buffer
//! pretty-print combo. Any nil-in-Neomacs/t-in-GNU is a missing-variable bug.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_pp_to_string_atom_list_and_quote_form_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (pp-to-string 'hello)
      (pp-to-string 42)
      (pp-to-string '(a b c d e f g h i j k l m n o p))
      (pp-to-string '(lambda (x y) (let ((z (+ x y))) (list x y z))))
      (pp-to-string (make-hash-table)))
"##;
    let expect = expect_test::expect![[
        r##""OK (\"hello\\n\" \"42\\n\" \"(a b c d e f g h i j k l m n o p)\\n\" \"(lambda (x y) (let ((z (+ x y))) (list x y z)))\\n\" \"#s(hash-table)\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_pp_buffer_inserts_pretty_form_into_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (pp '(alpha beta (gamma delta epsilon)
        zeta eta theta iota kappa lambda mu) (current-buffer))
  (list (buffer-string)
        (= (point) (point-max))
        (> (buffer-size) 0)
        (buffer-live-p (current-buffer))))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"(alpha beta (gamma delta epsilon) zeta eta theta iota kappa lambda mu)\\n\" t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_pp_with_temp_buffer_round_trip_and_fill_column_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((long-data (make-list 40 'item))
      (fill-column 20))
  (with-temp-buffer
    (pp long-data (current-buffer))
    (list (length (buffer-string))
          (> (count-lines (point-min) (point-max)) 1)
          (save-excursion
            (goto-char (point-min))
            (forward-list)
            (bobp))
          (pp-to-string '(one two three)))))
"##;
    let expect = expect_test::expect![[r#""OK (274 t nil \"(one two three)\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
