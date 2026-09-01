//! Strict combo oracle probes, batch 211: subr-x utilities. string-join/split/
//! trim family, string-empty-p/string-blank-p, hash-table-keys/values,
//! if-let/when-let, and thread-first/thread-last.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_subrx_string_join_split_trim() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'subr-x)
(list (string-join '("a" "b" "c") "-")
      (string-join '("only") "-")
      (string-join '() "-")
      (string-split "a-b-c-d" "-")
      (string-split "a-b-c-d" "-" t)
      (string-split "  a b c  ")
      (string-trim "  hello world  ")
      (string-trim-left "...hello" "[.]+")
      (string-trim-right "world..." "[.]+")
      (string-empty-p "")
      (string-empty-p " ")
      (string-blank-p "   ")
      (string-blank-p "x"))
"##;
    let expect = expect_test::expect![[
        r#""OK (\"a-b-c\" \"only\" \"\" (\"a\" \"b\" \"c\" \"d\") (\"a\" \"b\" \"c\" \"d\") (\"a\" \"b\" \"c\") \"hello world\" \"hello\" \"world\" t nil 0 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_subrx_if_let_when_let_thread() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'subr-x)
(list (if-let ((x 5)) (* x 2) 'else)
      (if-let ((x nil)) 'then 'else)
      (if-let* ((a 1) (b 2)) (+ a b) 'no)
      (when-let ((x 3)) (* x 10))
      (when-let ((x nil)) 'should-not)
      (thread-first 5 (* 2) (+ 3))
      (thread-last 5 (* 2) (+ 3))
      (thread-first 1 (+ 10) (* 2) (- 5)))
"##;
    let expect = expect_test::expect![[r#""OK (10 else 3 30 nil 13 13 17)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_subrx_hash_table_keys_values_emptiness() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'subr-x)
(let ((h (make-hash-table :test 'equal)))
  (puthash 'a 1 h)
  (puthash 'b 2 h)
  (puthash 'c 3 h)
  (list (length (hash-table-keys h))
        (length (hash-table-values h))
        (sort (hash-table-keys h)
              (lambda (a b) (string< (symbol-name a) (symbol-name b))))
        (member 2 (hash-table-values h))
        (hash-table-empty-p (make-hash-table))
        (hash-table-empty-p h)))
"##;
    let expect = expect_test::expect![[r#""OK (3 3 (a b c) (2 1) t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
