//! Strict combo oracle probes, batch 150: the reader. char-literal escapes
//! (?\C-a ?\M-a ?\x41 ?é ?\^H ?\; ?\( ), #(vector) and #s(record) read
//! forms, #'function and backquote read forms incl circular #1=(a . #1#), and
//! read-from-string trailing/leading-junk + error conditions.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_read_char_literal_escape_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (read "?\\C-a")
      (read "?\\M-a")
      (read "?\\^H")
      (read "?\\^I")
      (read "?\\x41")
      (read "?\\u00e9")
      (read "?\\C-\\M-a")
      (read "?\\e")
      (read "?\\t")
      (read "?\\n")
      (read "?\\;")
      (read "?\\(")
      (read "?\\\\")
      (read "?A"))
"##;
    let expect =
        expect_test::expect![[r#""OK (1 134217825 8 9 65 233 134217729 27 9 10 59 40 92 65)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_read_vector_record_function_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((safe (lambda (s) (condition-case err (read s) (error (cons 'err (cadr err)))))))
  (list (funcall safe "#(1 2 3)")
        (funcall safe "#(a (b c) [d e])")
        (read "#s(foo 1 2)")
        (read "#'symbol")
        (read "#'lambda")
        (read "`(a ,b ,@c)")
        (read "(a . (b . (c . nil)))")
        (read "[1 [2 [3]]]")))
"##;
    let expect = expect_test::expect![[
        r##""OK ((err . \"#\") (err . \"#\") #s(foo 1 2) #'symbol #'lambda `(a ,b ,@c) (a b c) [1 [2 [3]]])""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_read_from_string_matrix_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(list (read-from-string "42 trailing")
      (read-from-string "(a b c) junk")
      (read-from-string "  \n\t  leading-ws")
      (multiple-value-bind (val pos) (read-from-string "#(1 2) tail")
        (list val pos))
      (condition-case err (read-from-string "") (end-of-file 'caught))
      (condition-case err (read-from-string "(") (end-of-file 'caught-paren))
      (condition-case err (read-from-string ")") (invalid-read-syntax 'caught-rparen))
      (condition-case err (read-from-string "#_unknown") (invalid-read-syntax 'caught-hash))
      (let ((s "#(a b c) rest"))
        (multiple-value-bind (val pos) (read-from-string s)
          (list val pos (substring s pos)))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
