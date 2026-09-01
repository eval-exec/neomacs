//! Oracle parity tests for type predicates: `arrayp`, `vectorp`,
//! `char-table-p`, `bool-vector-p`, `keywordp`.
//!
//! GNU src/data.c: type predicates distinguish Lisp object types.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_arrayp_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(arrayp [1 2 3])"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_arrayp_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(arrayp "hello")"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_arrayp_list_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(arrayp '(a b c))"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_vectorp_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(vectorp [1 2])"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_vectorp_string_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(vectorp "hello")"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_char_table_p_on_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(char-table-p (make-char-table 'syntax-table))"#,
        expect,
    );
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_char_table_p_on_vector_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(char-table-p [1 2 3])"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_keywordp_on_keyword() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(keywordp :test)"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_keywordp_on_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(keywordp 'test)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_keywordp_initial_obarray_strict_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/data.c:Fkeywordp requires a symbol whose print name starts with
    // ':' and whose symbol object is interned in the initial obarray.
    let form = r#"
(let ((local-obarray (make-vector 17 0)))
  (list
   (keywordp :)
   (keywordp :neomacs-oracle-keyword)
   (keywordp (intern ":neomacs-oracle-keyword"))
   (keywordp (make-symbol ":neomacs-oracle-keyword"))
   (keywordp (intern ":neomacs-oracle-keyword" local-obarray))
   (keywordp (intern-soft ":neomacs-oracle-keyword" local-obarray))
   (keywordp (intern-soft ":neomacs-oracle-keyword"))
   (keywordp ':)
   (keywordp "::double")
   (condition-case err
       (keywordp)
     (error (cons (car err) (cdr err))))
   (condition-case err
       (keywordp :x :y)
     (error (cons (car err) (cdr err))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil nil nil t t nil (wrong-number-of-arguments keywordp 0) (wrong-number-of-arguments keywordp 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_sequencep_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(sequencep '(a b))"#, expect);
    assert_ok_eq("t", &o, &n);
}

#[test]
fn oracle_sequencep_integer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(sequencep 42)"#, expect);
    assert_ok_eq("nil", &o, &n);
}

#[test]
fn oracle_data_c_sequence_array_vector_predicate_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/data.c defines these as primitive tag predicates.  The matrix
    // locks down the exact object-family split, especially bool-vectors and
    // char-tables, which are easy to accidentally classify like normal vectors.
    let form = r#"
(let* ((ct (make-char-table 'syntax-table nil))
       (bv (bool-vector t nil t))
       (rec (record 'neovm-oracle-record 'a 'b))
       (values (list nil '(a b) '(a . b) [] [a b] "abc" bv ct rec 42 'sym)))
  (list
   (mapcar (lambda (v)
             (list (sequencep v)
                   (arrayp v)
                   (vectorp v)
                   (vector-or-char-table-p v)
                   (char-table-p v)
                   (bool-vector-p v)
                   (recordp v)))
           values)
   (condition-case err
       (bool-vector-p)
     (error (cons (car err) (cdr err))))
   (condition-case err
       (sequencep nil nil)
     (error (cons (car err) (cdr err))))))
"#;
    let expect = expect_test::expect![[
        r#""OK (((t nil nil nil nil nil nil) (t nil nil nil nil nil nil) (t nil nil nil nil nil nil) (t t t t nil nil nil) (t t t t nil nil nil) (t t nil nil nil nil nil) (t t nil nil nil t nil) (t t nil t t nil nil) (nil nil nil nil nil nil t) (nil nil nil nil nil nil nil) (nil nil nil nil nil nil nil)) (wrong-number-of-arguments bool-vector-p 0) (wrong-number-of-arguments sequencep 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
