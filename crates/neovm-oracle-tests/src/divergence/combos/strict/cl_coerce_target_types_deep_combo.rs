//! Strict combo oracle probes, batch 364: cl-coerce target types deep.
//! cl-coerce between list/vector/string/character/integer target types.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_coerce_list_vector_string_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-coerce '(1 2 3) 'vector)
      (cl-coerce [a b c] 'list)
      (cl-coerce '(?a ?b ?c) 'string)
      (cl-coerce "abc" 'list)
      (cl-coerce (cl-coerce "test" 'list) 'string)
      (cl-coerce (cl-coerce '(1 2 3) 'vector) 'list))
"##;
    let expect =
        expect_test::expect![[r#""OK ([1 2 3] (a b c) \"abc\" (97 98 99) \"test\" (1 2 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_coerce_single_char_integer_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-coerce 65 'char)
      (cl-coerce ?A 'integer)
      (cl-coerce ?a 'string)
      (cl-coerce "x" 'character)
      (cl-coerce 256 'char))
"##;
    let expect = expect_test::expect![[r#""ERR (error \"Unknown type char\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_coerce_vector_to_string_bool_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-coerce [?h ?i] 'string)
      (cl-coerce [1 2 3] 'list)
      (cl-coerce nil 'vector)
      (cl-coerce [] 'list)
      (cl-coerce "" 'list)
      (length (cl-coerce "hello" 'vector)))
"##;
    let expect = expect_test::expect![[r#""OK (\"hi\" (1 2 3) [] nil nil 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
