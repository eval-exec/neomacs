//! Oracle parity for copy-sequence, copy-hash-table, copy-alist independence.
//! GNU src/fns.c.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// --- copy-sequence vector independence ---

#[test]
fn oracle_copy_sequence_vector_independent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 nil)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq v [1 2 3]) (setq cp (copy-sequence v)) (aset v 0 99) (list (aref cp 0) (eq v cp)))"#,
        expect,
    );
    assert_ok_eq("(1 nil)", &o, &n);
}

#[test]
fn oracle_copy_sequence_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\"""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(copy-sequence "hello")"#, expect);
    assert_ok_eq("\"hello\"", &o, &n);
}

#[test]
fn oracle_copy_sequence_string_copies_intervals_shallowly() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Fcopy_sequence copies string intervals with
    // copy_intervals.  GNU src/intervals.c:copy_properties copies the
    // interval plist spine with Fcopy_sequence, but does not deep-copy plist
    // values.
    let form = r#"
(let* ((shared (list 'front))
       (orig (copy-sequence "abcdef")))
  (put-text-property 1 4 'face 'bold orig)
  (put-text-property 2 5 'payload shared orig)
  (let* ((copy (copy-sequence orig))
         (orig-before (object-intervals orig))
         (copy-before (object-intervals copy))
         (same-payload (eq (get-text-property 2 'payload orig)
                           (get-text-property 2 'payload copy))))
    (put-text-property 1 4 'face 'italic copy)
    (setcar shared 'mutated)
    (list
     (equal-including-properties orig copy)
     orig-before
     copy-before
     same-payload
     (list (get-text-property 2 'face orig)
           (get-text-property 2 'face copy))
     (list (get-text-property 2 'payload orig)
           (get-text-property 2 'payload copy)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil ((0 1 nil) (1 2 (face bold)) (2 4 (payload (mutated) face bold)) (4 5 (payload (mutated))) (5 6 nil)) ((0 1 nil) (1 2 (face italic)) (2 4 (payload (mutated) face italic)) (4 5 (payload (mutated))) (5 6 nil)) t (bold italic) ((mutated) (mutated)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_copy_sequence_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (a b c)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(r#"(copy-sequence '(a b c))"#, expect);
    assert_ok_eq("(a b c)", &o, &n);
}

// --- copy-hash-table independence ---

#[test]
fn oracle_copy_hash_table_values_independent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 99 2)""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq ht (make-hash-table :test 'eq)) (puthash 'a 1 ht) (puthash 'b 2 ht) (setq cp (copy-hash-table ht)) (puthash 'a 99 ht) (list (gethash 'a cp) (gethash 'a ht) (hash-table-count cp)))"#,
        expect,
    );
    assert_ok_eq("(1 99 2)", &o, &n);
}

// --- hash-table-size ---

#[test]
fn oracle_hash_table_size_after_inserts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq ht (make-hash-table)) (puthash 'a 1 ht) (puthash 'b 2 ht) (hash-table-count ht))"#,
        expect,
    );
    assert_ok_eq("2", &o, &n);
}

// --- hash-table-test ---

#[test]
fn oracle_hash_table_test_returns_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK eq""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(hash-table-test (make-hash-table :test 'eq))"#,
        expect,
    );
    assert_ok_eq("eq", &o, &n);
}

// --- maphash ---

#[test]
fn oracle_maphash_iterates_all_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq ht (make-hash-table :test 'eq)) (puthash 'a 1 ht) (puthash 'b 2 ht) (setq count 0) (maphash (lambda (k v) (setq count (+ count 1))) ht) count)"#,
        expect,
    );
    assert_ok_eq("2", &o, &n);
}

// --- hash-table-count after clrhash ---

#[test]
fn oracle_hash_table_count_after_clrhash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq ht (make-hash-table :test 'eq)) (puthash 'a 1 ht) (puthash 'b 2 ht) (clrhash ht) (hash-table-count ht))"#,
        expect,
    );
    assert_ok_eq("0", &o, &n);
}

// --- copy-alist nested ---

#[test]
fn oracle_copy_alist_nested_independent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 (99))""#]];
    let (o, n) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn (setq orig '((a . 1) (b . 2))) (setq cp (copy-alist orig)) (setcdr (assq 'a orig) '(99)) (list (cdr (assq 'a cp)) (cdr (assq 'a orig))))"#,
        expect,
    );
    assert_ok_eq("(1 (99))", &o, &n);
}
