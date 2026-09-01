//! Oracle parity tests for GNU `append`/`nconc` edge semantics.
//!
//! GNU implements `append` through `concat_to_list` in `src/fns.c`: every
//! argument except the last is copied into fresh cons cells, while the last
//! argument is used directly as the final tail.  GNU `nconc` mutates preceding
//! list arguments and permits a non-list final argument.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_append_copies_prefix_and_shares_final_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((prefix (list 'a 'b))
       (tail (list 'c 'd))
       (result (append prefix tail)))
  (setcar prefix 'changed-prefix)
  (setcar tail 'changed-tail)
  (list result
        prefix
        tail
        (eq (nthcdr 2 result) tail)
        (eq result prefix)))
"#;

    let expect = expect_test::expect![
        r#""OK ((a b changed-tail d) (changed-prefix b) (changed-tail d) t nil)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_append_sequence_arguments_and_dotted_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((bv (make-bool-vector 4 nil)))
  (aset bv 1 t)
  (aset bv 3 t)
  (list
   (append "a中" [x y] bv '(tail))
   (append nil nil 'final-atom)
   (append "ab" 'tail)
   (condition-case err
       (append '(a b . c) '(tail))
     (error (list (car err) (cdr err))))
   (condition-case err
       (append 42 '(tail))
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![
        r#""OK ((97 20013 x y nil t nil t tail) final-atom (97 98 . tail) (wrong-type-argument (listp c)) (wrong-type-argument (sequencep 42)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_append_rejects_char_table_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:concat_to_list accepts vectors, strings, bool-vectors,
    // closures, nil, and conses.  Char-tables are not accepted here even
    // though `copy-sequence` accepts them.
    let form = r#"
(let ((table (make-char-table 'generic 65)))
  (condition-case err
      (append table nil)
    (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![
        r#""OK (wrong-type-argument (sequencep #^[65 nil generic 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65]))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_append_vconcat_accept_byte_code_but_concat_rejects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:concat_to_list and concat_to_vector accept CLOSUREP
    // arguments, which includes byte-code functions.  concat_to_string does
    // not accept CLOSUREP and signals `wrong-type-argument' with `sequencep'.
    let form = r#"
(let ((bc #[257 "\300\207" [42] 1]))
  (list
   (type-of bc)
   (length bc)
   (append bc nil)
   (vconcat bc)
   (condition-case err
       (concat bc)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (byte-code-function 4 (257 \"��\" [42] 1) [257 \"��\" [42] 1] (wrong-type-argument (sequencep #[257 \"��\" [42] 1])))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_nconc_mutates_prefix_and_shares_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((a (list 'a1 'a2))
       (b (list 'b1 'b2))
       (first a)
       (result (nconc a b)))
  (setcar b 'changed-b)
  (list result
        first
        b
        (eq result first)
        (eq (nthcdr 2 result) b)))
"#;

    let expect = expect_test::expect![
        r#""OK ((a1 a2 changed-b b2) (a1 a2 changed-b b2) (changed-b b2) t t)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_nconc_nil_arguments_and_dotted_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((a (list 'a))
       (b (list 'b))
       (with-middle-nil (nconc a nil b))
       (dotted (nconc (list 'x 'y) 'tail)))
  (list with-middle-nil
        (eq with-middle-nil a)
        dotted
        (nconc nil nil 'final)
        (condition-case err
            (nconc 'not-last (list 'tail))
          (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![
        r#""OK ((a b) t (x y . tail) final (wrong-type-argument (consp not-last)))""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_nconc_overwrites_dotted_nonfinal_tail() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dotted (cons 'head 'old-tail))
       (tail (list 'new-tail))
       (result (nconc dotted tail)))
  (list result
        dotted
        tail
        (eq result dotted)
        (eq (cdr result) tail)))
"#;

    let expect = expect_test::expect![r#""OK ((head new-tail) (head new-tail) (new-tail) t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
