//! Strict combo oracle probes, batch 201: string construction + format-on-types.
//! concat of strings/list-of-chars/vector-of-chars, string/unibyte-string from
//! char args, mixed unibyte/multibyte concat, and format %s/%S/%d over
//! records/vectors/markers.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_string_construct_concat_list_vector_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (string ?a ?b ?c)
      (apply #'string '(?x ?y ?z))
      (concat "uni" " код" "日本")
      (concat '(?a ?b ?c))
      (concat [?x ?y])
      (unibyte-string 65 66 67)
      (make-string 5 ?*)
      (make-string 3 ?日)
      (concat "a" "b" "c"))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"abc\" \"xyz\" \"uni код日本\" \"abc\" \"xy\" \"ABC\" \"*****\" \"日日日\" \"abc\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_format_s_over_records_vectors_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(with-temp-buffer
  (insert "marker-test")
  (let ((m (set-marker (make-marker) 3)))
    (list (format "%s" (vector 1 2 3))
          (format "%S" (vector 1 2 3))
          (format "%s" (record 'foo 1 2))
          (format "%S" (record 'foo 1 2))
          (format "%s" [a b c])
          (format "%s" "plain")
          (format "%S" "with quotes")
          (format "%d" 42)
          (format "%s" '(a b c))
          (format "%s" (make-bool-vector 3 t)))))
"##;
    let expect = expect_test::expect![[
        r##""OK (\"[1 2 3]\" \"[1 2 3]\" \"#s(foo 1 2)\" \"#s(foo 1 2)\" \"[a b c]\" \"plain\" \"\\\"with quotes\\\"\" \"42\" \"(a b c)\" \"#&3\\\"\u{7}\\\"\")""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_concat_mixed_unibyte_multibyte_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (multibyte-string-p "abc")
      (multibyte-string-p "日本")
      (multibyte-string-p (unibyte-string 200))
      (string-bytes "abc")
      (string-bytes "日本")
      (string-bytes (unibyte-string 200))
      (length "日本")
      (length (unibyte-string 200))
      (concat "abc" (string ?日))
      (multibyte-string-p (concat "abc" (string ?日)))
      (aref (unibyte-string 200) 0)))
"##;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 12 37)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
