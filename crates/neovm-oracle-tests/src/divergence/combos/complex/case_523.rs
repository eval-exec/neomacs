/// Batch 523: sequence operations - copy-sequence on all types, length implicit.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx523_copy_sequence_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((v [1 2 3]) (c (copy-sequence [1 2 3])))
  (aset c 0 99)
  (list (aref v 0) (aref c 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx523_copy_sequence_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((l '(1 2 3)) (c (copy-sequence '(1 2 3))))
  (setcar c 99)
  (list (car l) (car c)))
"##,
        expect,
    );
}

#[test]
fn div_cx523_copy_sequence_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" \"hello\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s "hello") (c (copy-sequence "hello")))
  (list s c (equal s c)))
"##,
        expect,
    );
}

#[test]
fn div_cx523_length_implicit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 5 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (length [1 2 3]) (length '(a b c d)) (length "hello") (length (make-bool-vector 10 t)))
"##,
        expect,
    );
}

#[test]
fn div_cx523_elt_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 30 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (elt '(10 20 30) 0) (elt '(10 20 30) 2) (elt '(10 20 30) -1))
"##,
        expect,
    );
}

#[test]
fn div_cx523_elt_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range [10 20 30] -1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (elt [10 20 30] 0) (elt [10 20 30] 2) (elt [10 20 30] -1))
"##,
        expect,
    );
}

#[test]
fn div_cx523_elt_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range \"abc\" -1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (elt "abc" 0) (elt "abc" 2) (elt "abc" -1))
"##,
        expect,
    );
}

#[test]
fn div_cx523_reverse_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"olleh\" \"a\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (reverse "hello") (reverse "a") (reverse ""))
"##,
        expect,
    );
}

#[test]
fn div_cx523_reverse_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((3 2 1) (a) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (reverse '(1 2 3)) (reverse '(a)) (reverse '()))
"##,
        expect,
    );
}

#[test]
fn div_cx523_reverse_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([3 2 1] [1] [])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (reverse [1 2 3]) (reverse [1]) (reverse []))
"##,
        expect,
    );
}

#[test]
fn div_cx523_nreverse_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((l (list 1 2 3))) (nreverse l) l)
"##,
        expect,
    );
}

#[test]
fn div_cx523_sort_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 3 4 5 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(sort '(3 1 4 1 5 9) #'<)
"##,
        expect,
    );
}

#[test]
fn div_cx523_member_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((3 4) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (member 3 '(1 2 3 4)) (member 5 '(1 2 3)))
"##,
        expect,
    );
}

#[test]
fn div_cx523_delete_dup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3 4) (a b))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (delete-dups '(1 2 1 3 2 4)) (delete-dups '(a b a)))
"##,
        expect,
    );
}

#[test]
fn div_cx523_assoc_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((b . 2) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (assq 'b '((a . 1) (b . 2) (c . 3))) (assq 'd '((a . 1))))
"##,
        expect,
    );
}
