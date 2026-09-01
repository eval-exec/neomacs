//! Deep combo: plist + hash-table + symbol-plist + get/put + cl-loop + mapc.
//! Tests property list infrastructure across multiple collection types.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_plist_put_get_with_nonexistent_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (red large nil nil (color red size large) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((pl '(color red size large)))\n\
         (list (plist-get pl 'color)\n\
         (plist-get pl 'size)\n\
         (plist-get pl 'weight)\n\
         (plist-get pl 'missing)\n\
         (plist-member pl 'color)\n\
         (plist-member pl 'missing))))",
        expect,
    );
}

#[test]
fn deficiency_plist_put_returns_new_list_not_mutated() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((a 99 b 2 c 3) (a 99 b 2 c 3) (a 99 b 2 c 3) 99 3 99)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((pl '(a 1 b 2)))\n\
         (let ((pl2 (plist-put pl 'c 3))\n\
         (pl3 (plist-put pl 'a 99)))\n\
         (list pl pl2 pl3\n\
         (plist-get pl 'a)\n\
         (plist-get pl2 'c)\n\
         (plist-get pl3 'a)))))",
        expect,
    );
}

#[test]
fn deficiency_hash_table_with_eql_test_and_float_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (one-half two-half three-int nil 3 nil 2 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ht (make-hash-table :test 'eql)))\n\
         (puthash 1.5 'one-half ht)\n\
         (puthash 2.5 'two-half ht)\n\
         (puthash 3 'three-int ht)\n\
         (list (gethash 1.5 ht)\n\
         (gethash 2.5 ht)\n\
         (gethash 3 ht)\n\
         (gethash 3.0 ht)\n\
         (hash-table-count ht)\n\
         (remhash 1.5 ht)\n\
         (hash-table-count ht)\n\
         (gethash 1.5 ht))))",
        expect,
    );
}

#[test]
fn deficiency_symbol_plist_cross_symbol_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (container (a b c) element sym1 (a b c) (type container items (a b c)) (type element parent sym1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((s1 (make-symbol \"sym1\"))\n\
         (s2 (make-symbol \"sym2\")))\n\
         (put s1 'type 'container)\n\
         (put s1 'items '(a b c))\n\
         (put s2 'type 'element)\n\
         (put s2 'parent s1)\n\
         (list (get s1 'type)\n\
         (get s1 'items)\n\
         (get s2 'type)\n\
         (get s2 'parent)\n\
         (get (get s2 'parent) 'items)\n\
         (symbol-plist s1)\n\
         (symbol-plist s2))))",
        expect,
    );
}

#[test]
fn deficiency_hash_table_iteration_with_maphash_and_collect() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((\"key-0\" . 0) (\"key-1\" . 1) (\"key-2\" . 4) (\"key-3\" . 9) (\"key-4\" . 16)) 5 4 16)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ht (make-hash-table :test 'equal)))\n\
         (dotimes (i 5)\n\
         (puthash (format \"key-%d\" i) (* i i) ht))\n\
         (let ((pairs nil))\n\
         (maphash (lambda (k v) (push (cons k v) pairs)) ht)\n\
         (let ((sorted (sort pairs (lambda (a b) (string< (car a) (car b))))))\n\
         (list sorted\n\
         (hash-table-count ht)\n\
         (gethash \"key-2\" ht)\n\
         (gethash \"key-4\" ht))))))",
        expect,
    );
}

#[test]
fn deficiency_nested_hash_table_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 20 nil 1 2 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((outer (make-hash-table :test 'equal))\n\
         (inner (make-hash-table :test 'equal)))\n\
         (puthash 'x 10 inner)\n\
         (puthash 'y 20 inner)\n\
         (puthash 'grid inner outer)\n\
         (let ((retrieved (gethash 'grid outer)))\n\
         (list (gethash 'x retrieved)\n\
         (gethash 'y retrieved)\n\
         (gethash 'z retrieved)\n\
         (hash-table-count outer)\n\
         (hash-table-count retrieved)\n\
         (eq retrieved inner)))))",
        expect,
    );
}

#[test]
fn deficiency_plist_to_hash_table_conversion_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\\\"Alice\\\" 30 \\\"NYC\\\" 95 \\\"Alice\\\" 30)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((pl '(name \\\"Alice\\\" age 30 city \\\"NYC\\\" score 95)))\n\
         (let ((ht (make-hash-table :test 'eq)))\n\
         (while pl\n\
         (puthash (car pl) (cadr pl) ht)\n\
         (setq pl (cddr pl)))\n\
         (let ((pl2 nil))\n\
         (maphash (lambda (k v) (setq pl2 (plist-put pl2 k v))) ht)\n\
         (list (gethash 'name ht)\n\
         (gethash 'age ht)\n\
         (gethash 'city ht)\n\
         (gethash 'score ht)\n\
         (plist-get pl2 'name)\n\
         (plist-get pl2 'age))))))",
        expect,
    );
}

#[test]
fn deficiency_symbol_plist_with_lots_of_keys_stress() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((sym (make-symbol \"stress\")))\n\
         (dotimes (i 20)\n\
         (put sym (intern (format \"prop-%d\" i)) i))\n\
         (let ((all-vals (cl-loop for i from 0 to 19\n\
         collect (get sym (intern (format \"prop-%d\" i))))))\n\
         (put sym 'prop-5 'overwritten)\n\
         (list all-vals\n\
         (get sym 'prop-5)\n\
         (get sym 'prop-19)\n\
         (get sym 'prop-20)\n\
         (length (symbol-plist sym))))))",
        expect,
    );
}

#[test]
fn deficiency_hash_table_clear_and_refill() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (10 0 5 100 103 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ht (make-hash-table :test 'eql)))\n\
         (dotimes (i 10) (puthash i (* i 10) ht))\n\
         (let ((count1 (hash-table-count ht)))\n\
         (clrhash ht)\n\
         (let ((count2 (hash-table-count ht)))\n\
         (dotimes (i 5) (puthash i (+ i 100) ht))\n\
         (list count1 count2\n\
         (hash-table-count ht)\n\
         (gethash 0 ht)\n\
         (gethash 3 ht)\n\
         (gethash 5 ht)\n\
         (gethash 9 ht))))))",
        expect,
    );
}

#[test]
fn deficiency_plist_member_vs_plist_get_for_nil_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 nil 3 nil nil (b nil c 3 d nil) (d nil) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((pl '(a 1 b nil c 3 d nil)))\n\
         (list (plist-get pl 'a)\n\
         (plist-get pl 'b)\n\
         (plist-get pl 'c)\n\
         (plist-get pl 'd)\n\
         (plist-get pl 'e)\n\
         (plist-member pl 'b)\n\
         (plist-member pl 'd)\n\
         (plist-member pl 'e))))",
        expect,
    );
}
