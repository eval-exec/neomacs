//! Strict combo oracle probes, batch 361: cl-parse-integer string→int parsing.
//! cl-parse-integer with :radix, :junk-allowed, :start, and various bases.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_parse_integer_radix_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-parse-integer "42")
      (cl-parse-integer "ff" :radix 16)
      (cl-parse-integer "777" :radix 8)
      (cl-parse-integer "1010" :radix 2)
      (cl-parse-integer "-100")
      (cl-parse-integer "+50"))
"##;
    let expect = expect_test::expect![[r#""OK (42 255 511 10 -100 50)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_parse_integer_junk_start_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-parse-integer "42abc" :junk-allowed t)
      (condition-case err (cl-parse-integer "42abc") (error 'caught))
      (cl-parse-integer "value: 42 end" :start 8 :junk-allowed t)
      (cl-parse-integer "100" :start 1)
      (cl-parse-integer "" :junk-allowed t)
      (cl-parse-integer "  42  " :junk-allowed t))
"##;
    let expect = expect_test::expect![[r#""OK (42 caught 2 0 nil 42)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_parse_integer_edge_leading_zeros() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-parse-integer "007")
      (cl-parse-integer "000")
      (cl-parse-integer "0" :radix 16)
      (cl-parse-integer "deadbeef" :radix 16)
      (cl-parse-integer "z" :radix 36))
"##;
    let expect = expect_test::expect![[r#""OK (7 0 0 3735928559 35)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
