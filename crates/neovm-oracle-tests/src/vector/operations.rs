//! Oracle parity tests for vector operations: `make-vector`, `vconcat`,
//! `vectorp`, `arrayp`, `elt`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use proptest::prelude::*;

use crate::common::{ORACLE_PROP_CASES, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// make-vector
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_make_vector_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [0 0 0 0 0]""#]];
    crate::common::assert_oracle_parity_expect("(make-vector 5 0)", expect);
    let expect = expect_test::expect![[r#""OK [nil nil nil]""#]];
    crate::common::assert_oracle_parity_expect("(make-vector 3 nil)", expect);
    let expect = expect_test::expect![[r#""OK []""#]];
    crate::common::assert_oracle_parity_expect("(make-vector 0 42)", expect);
    let expect = expect_test::expect![[r#""OK [hello hello hello hello]""#]];
    crate::common::assert_oracle_parity_expect("(make-vector 4 'hello)", expect);
}

#[test]
fn oracle_prop_make_vector_with_string_init() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [\"test\" \"test\" \"test\"]""#]];
    crate::common::assert_oracle_parity_expect(r#"(make-vector 3 "test")"#, expect);
}

#[test]
fn oracle_prop_make_vector_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // All elements share the same initial value
    let form = "(let ((v (make-vector 5 0)))
                  (aset v 0 10)
                  (aset v 2 20)
                  (aset v 4 30)
                  (list (aref v 0) (aref v 1) (aref v 2)
                        (aref v 3) (aref v 4)))";
    let expect = expect_test::expect![[r#""OK (10 0 20 0 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vconcat
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vconcat_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [1 2 3 4 5 6]""#]];
    crate::common::assert_oracle_parity_expect("(vconcat [1 2 3] [4 5 6])", expect);
    let expect = expect_test::expect![[r#""OK [1 2 3]""#]];
    crate::common::assert_oracle_parity_expect("(vconcat [1] [2] [3])", expect);
    let expect = expect_test::expect![[r#""OK [1 2]""#]];
    crate::common::assert_oracle_parity_expect("(vconcat [] [1 2])", expect);
    let expect = expect_test::expect![[r#""OK [1 2]""#]];
    crate::common::assert_oracle_parity_expect("(vconcat [1 2] [])", expect);
    let expect = expect_test::expect![[r#""OK []""#]];
    crate::common::assert_oracle_parity_expect("(vconcat)", expect);
}

#[test]
fn oracle_prop_vconcat_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [1 2 3]""#]];
    // vconcat can accept lists
    crate::common::assert_oracle_parity_expect("(vconcat '(1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK [a b c d]""#]];
    crate::common::assert_oracle_parity_expect("(vconcat '(a b) '(c d))", expect);
    let expect = expect_test::expect![[r#""OK [1 2 3 4]""#]];
    crate::common::assert_oracle_parity_expect("(vconcat [1 2] '(3 4))", expect);
}

#[test]
fn oracle_prop_vconcat_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [97 98 99]""#]];
    // vconcat converts strings to vectors of char codes
    crate::common::assert_oracle_parity_expect(r#"(vconcat "abc")"#, expect);
    let expect = expect_test::expect![[r#""OK [104 105 33]""#]];
    crate::common::assert_oracle_parity_expect(r#"(vconcat "hi" [33])"#, expect);
}

#[test]
fn oracle_prop_vconcat_multiple_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [1 2 3 4 65 66]""#]];
    crate::common::assert_oracle_parity_expect(r#"(vconcat [1 2] '(3 4) "AB")"#, expect);
}

#[test]
fn oracle_vconcat_rejects_char_table_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:concat_to_vector accepts vectors, strings, bool-vectors,
    // closures, nil, and conses.  Char-tables fail the sequence predicate
    // before any vectorlike storage is exposed.
    let form = r#"
(let ((table (make-char-table 'generic 65)))
  (condition-case err
      (vconcat table)
    (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (wrong-type-argument (sequencep #^[65 nil generic 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65 65]))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// vectorp / arrayp
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vectorp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(vectorp [1 2 3])", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(vectorp [])", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(vectorp '(1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(vectorp nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(vectorp 42)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(r#"(vectorp "hello")"#, expect);
}

#[test]
fn oracle_prop_arrayp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect("(arrayp [1 2 3])", expect);
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(r#"(arrayp "hello")"#, expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(arrayp '(1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(arrayp nil)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(arrayp 42)", expect);
}

// ---------------------------------------------------------------------------
// elt (works on sequences: lists, vectors, strings)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_elt_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 10""#]];
    crate::common::assert_oracle_parity_expect("(elt [10 20 30 40] 0)", expect);
    let expect = expect_test::expect![[r#""OK 30""#]];
    crate::common::assert_oracle_parity_expect("(elt [10 20 30 40] 2)", expect);
    let expect = expect_test::expect![[r#""OK 40""#]];
    crate::common::assert_oracle_parity_expect("(elt [10 20 30 40] 3)", expect);
}

#[test]
fn oracle_prop_elt_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK a""#]];
    crate::common::assert_oracle_parity_expect("(elt '(a b c d) 0)", expect);
    let expect = expect_test::expect![[r#""OK c""#]];
    crate::common::assert_oracle_parity_expect("(elt '(a b c d) 2)", expect);
    let expect = expect_test::expect![[r#""OK d""#]];
    crate::common::assert_oracle_parity_expect("(elt '(a b c d) 3)", expect);
}

#[test]
fn oracle_prop_elt_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 104""#]];
    crate::common::assert_oracle_parity_expect(r#"(elt "hello" 0)"#, expect);
    let expect = expect_test::expect![[r#""OK 111""#]];
    crate::common::assert_oracle_parity_expect(r#"(elt "hello" 4)"#, expect);
}

#[test]
fn oracle_prop_elt_complex_sequence_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // elt dispatches based on sequence type
    let form = r####"(let ((vec [a b c])
                        (lst '(x y z))
                        (str "ABC"))
                    (list (elt vec 1) (elt lst 1) (elt str 1)))"####;
    let expect = expect_test::expect![[r#""OK (b y 66)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete (destructive removal)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_from_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 4 5)""#]];
    crate::common::assert_oracle_parity_expect("(delete 3 (list 1 2 3 4 3 5))", expect);
    let expect = expect_test::expect![[r#""OK (a c d)""#]];
    crate::common::assert_oracle_parity_expect("(delete 'b (list 'a 'b 'c 'b 'd))", expect);
    let expect = expect_test::expect![[r#""OK (1 2 3)""#]];
    crate::common::assert_oracle_parity_expect("(delete 99 (list 1 2 3))", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(delete 1 (list 1))", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(delete 1 nil)", expect);
}

#[test]
fn oracle_prop_delete_string_from_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // delete uses equal comparison
    let form = r####"(delete "hello" (list "hello" "world" "hello" "foo"))"####;
    let expect = expect_test::expect![[r#""OK (\"world\" \"foo\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_delete_from_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK [1 2 4 5]""#]];
    // delete on vectors returns a new vector
    crate::common::assert_oracle_parity_expect("(delete 3 [1 2 3 4 3 5])", expect);
    let expect = expect_test::expect![[r#""OK [1 2 3]""#]];
    crate::common::assert_oracle_parity_expect("(delete 99 [1 2 3])", expect);
}

// ---------------------------------------------------------------------------
// number-sequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_number_sequence_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4 5)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 1 5)", expect);
    let expect = expect_test::expect![[r#""OK (0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0 0)", expect);
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 5 1)", expect);
}

#[test]
fn oracle_prop_number_sequence_with_step() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 2 4 6 8 10)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0 10 2)", expect);
    let expect = expect_test::expect![[r#""OK (0 3 6 9)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0 10 3)", expect);
    let expect = expect_test::expect![[r#""OK (10 8 6 4 2 0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 10 0 -2)", expect);
    let expect = expect_test::expect![[r#""OK (5 2 -1 -4)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 5 -5 -3)", expect);
}

#[test]
fn oracle_prop_number_sequence_float() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0.0 0.25 0.5 0.75 1.0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 0.0 1.0 0.25)", expect);
    let expect = expect_test::expect![[r#""OK (1.0 1.5 2.0)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 1.0 2.0 0.5)", expect);
}

#[test]
fn oracle_prop_number_sequence_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (42)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 42 42)", expect);
    let expect = expect_test::expect![[r#""OK (42)""#]];
    crate::common::assert_oracle_parity_expect("(number-sequence 42 42 5)", expect);
}

// ---------------------------------------------------------------------------
// Complex: vector as lookup table
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_vector_as_lookup_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a frequency table using a vector
    let form = r####"(let ((data '(3 1 4 1 5 9 2 6 5 3 5))
                        (freq (make-vector 10 0)))
                    (dolist (n data)
                      (aset freq n (1+ (aref freq n))))
                    (let ((result nil))
                      (dotimes (i 10)
                        (when (> (aref freq i) 0)
                          (setq result
                                (cons (cons i (aref freq i)) result))))
                      (nreverse result)))"####;
    let expect =
        expect_test::expect![[r#""OK ((1 . 2) (2 . 1) (3 . 2) (4 . 1) (5 . 3) (6 . 1) (9 . 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_vector_matrix_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // 2x2 matrix multiply using vectors
    let form = "(let ((a [1 2 3 4])
                      (b [5 6 7 8]))
                  ;; Matrix multiply: [a00*b00+a01*b10, a00*b01+a01*b11,
                  ;;                   a10*b00+a11*b10, a10*b01+a11*b11]
                  (let ((r (make-vector 4 0)))
                    (aset r 0 (+ (* (aref a 0) (aref b 0))
                                 (* (aref a 1) (aref b 2))))
                    (aset r 1 (+ (* (aref a 0) (aref b 1))
                                 (* (aref a 1) (aref b 3))))
                    (aset r 2 (+ (* (aref a 2) (aref b 0))
                                 (* (aref a 3) (aref b 2))))
                    (aset r 3 (+ (* (aref a 2) (aref b 1))
                                 (* (aref a 3) (aref b 3))))
                    r))";
    let expect = expect_test::expect![[r#""OK [19 22 43 50]""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_vconcat_flatten_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Flatten a list of vectors into one
    let form = "(let ((chunks '([1 2] [3 4 5] [6])))
                  (let ((result []))
                    (dolist (chunk chunks)
                      (setq result (vconcat result chunk)))
                    result))";
    let expect = expect_test::expect![[r#""OK [1 2 3 4 5 6]""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// proptest: number-sequence length
// ---------------------------------------------------------------------------

proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(ORACLE_PROP_CASES))]

    #[test]
    fn oracle_prop_number_sequence_length(
        from in 0i64..20,
        count in 1usize..10,
    ) {
        return_if_neovm_enable_oracle_proptest_not_set!(Ok(()));

        let to = from + count as i64 - 1;
        let form = format!(
            "(length (number-sequence {} {}))",
            from, to
        );
        let (oracle, neovm) = eval_oracle_and_neovm(&form);
        let expected = format!("OK {}", count);
        prop_assert_eq!(neovm.as_str(), expected.as_str());
        prop_assert_eq!(oracle.as_str(), expected.as_str());
    }
}
