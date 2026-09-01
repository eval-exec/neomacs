//! Deep combo: obarray + intern + unintern + mapatoms + symbol-function + symbol-value.
//! Tests symbol table operations with dynamic binding and function cells.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_intern_soft_vs_intern_obarray_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"hello\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 13 0)))\n\
         (intern \\\"hello\\\" ob)\n\
         (intern \\\"world\\\" ob)\n\
         (intern \\\"hello\\\" ob)\n\
         (list (intern-soft \\\"hello\\\" ob)\n\
         (intern-soft \\\"world\\\" ob)\n\
         (intern-soft \\\"missing\\\" ob)\n\
         (intern-soft \\\"HELLO\\\" ob))))",
        expect,
    );
}

#[test]
fn deficiency_mapatoms_counts_and_collect_with_custom_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 7 0))\n\
         (collected nil))\n\
         (dotimes (i 10)\n\
         (intern (format \\\"sym-%02d\\\" i) ob))\n\
         (mapatoms (lambda (s) (push (symbol-name s) collected)) ob)\n\
         (let ((sorted (sort collected #'string<)))\n\
         (list (length sorted)\n\
         (nth 0 sorted) (nth 4 sorted) (nth 9 sorted)\n\
         (intern-soft \\\"sym-00\\\" ob)\n\
         (intern-soft \\\"sym-09\\\" ob))))",
        expect,
    );
}

#[test]
fn deficiency_unintern_then_reintern_preserves_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 7 0)))\n\
         (let ((s1 (intern \\\"test-sym\\\" ob)))\n\
         (set s1 42)\n\
         (let ((val1 (symbol-value s1)))\n\
         (unintern \\\"test-sym\\\" ob)\n\
         (let ((s2 (intern \\\"test-sym\\\" ob)))\n\
         (set s2 99)\n\
         (list val1\n\
         (eq s1 s2)\n\
         (symbol-value s2)\n\
         (intern-soft \\\"test-sym\\\" ob))))))",
        expect,
    );
}

#[test]
fn deficiency_intern_with_symbol_function_and_value_cells() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 13 0)))\n\
         (let ((fn-sym (intern \\\"my-test-fn\\\" ob)))\n\
         (fset fn-sym (lambda (x) (* x x)))\n\
         (let ((val-sym (intern \\\"my-test-val\\\" ob)))\n\
         (set val-sym '(1 2 3))\n\
         (list (funcall fn-sym 7)\n\
         (symbol-value val-sym)\n\
         (functionp fn-sym)\n\
         (fboundp fn-sym)\n\
         (boundp val-sym)\n\
         (symbol-name fn-sym)\n\
         (symbol-name val-sym)))))",
        expect,
    );
}

#[test]
fn deficiency_symbol_plist_with_obarray_interned_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"plist-sym\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 7 0)))\n\
         (let ((s1 (intern \\\"plist-sym\\\" ob)))\n\
         (put s1 'color 'red)\n\
         (put s1 'size 'large)\n\
         (put s1 'weight 42)\n\
         (let ((s2 (intern \\\"plist-sym\\\" ob)))\n\
         (list (get s1 'color)\n\
         (get s1 'size)\n\
         (get s1 'weight)\n\
         (get s1 'missing)\n\
         (eq s1 s2)\n\
         (symbol-plist s1)\n\
         (symbol-plist s2))))))",
        expect,
    );
}

#[test]
fn deficiency_mapatoms_with_fboundp_and_boundp_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 11 0)))\n\
         (dolist (name '(\\\"alpha\\\" \\\"beta\\\" \\\"gamma\\\" \\\"delta\\\" \\\"epsilon\\\"))\n\
         (let ((s (intern name ob)))\n\
         (when (member name '(\\\"alpha\\\" \\\"gamma\\\" \\\"epsilon\\\"))\n\
         (fset s (lambda () 'active)))))\n\
         (let ((bound-syms nil)\n\
         (fbound-syms nil))\n\
         (mapatoms (lambda (s)\n\
         (when (fboundp s) (push (symbol-name s) fbound-syms))) ob)\n\
         (list (sort fbound-syms #'string<)\n\
         (length fbound-syms)\n\
         (intern-soft \\\"beta\\\" ob))))",
        expect,
    );
}

#[test]
fn deficiency_obarray_hash_collision_with_similar_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 3 0)))\n\
         (intern \\\"abc\\\" ob)\n\
         (intern \\\"def\\\" ob)\n\
         (intern \\\"ghi\\\" ob)\n\
         (intern \\\"jkl\\\" ob)\n\
         (intern \\\"mno\\\" ob)\n\
         (intern \\\"pqr\\\" ob)\n\
         (let ((count 0))\n\
         (mapatoms (lambda (_) (setq count (1+ count))) ob)\n\
         (list count\n\
         (intern-soft \\\"abc\\\" ob)\n\
         (intern-soft \\\"def\\\" ob)\n\
         (intern-soft \\\"pqr\\\" ob)\n\
         (intern-soft \\\"xyz\\\" ob))))",
        expect,
    );
}

#[test]
fn deficiency_symbol_name_intern_and_substring_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"test-prefix-\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let* ((base \\\"test-prefix-\\\")\n\
         (ob (make-vector 7 0))\n\
         (syms (cl-loop for i from 1 to 5\n\
         for name = (concat base (number-to-string i))\n\
         collect (intern name ob))))\n\
         (let ((names (mapcar #'symbol-name syms)))\n\
         (dolist (s syms)\n\
         (put s 'index (cl-position s syms)))\n\
         (list names\n\
         (mapcar (lambda (s) (get s 'index)) syms)\n\
         (intern-soft \\\"test-prefix-3\\\" ob)\n\
         (intern-soft \\\"test-prefix-6\\\" ob)))))",
        expect,
    );
}

#[test]
fn deficiency_unintern_during_mapatoms_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 7 0)))\n\
         (dotimes (i 5)\n\
         (intern (format \\\"item-%d\\\" i) ob))\n\
         (let ((collected nil)\n\
         (to-delete 'item-2))\n\
         (mapatoms (lambda (s)\n\
         (push (symbol-name s) collected)\n\
         (when (equal (symbol-name s) \\\"item-2\\\")\n\
         (unintern s ob)))\n\
         ob)\n\
         (let ((after-map (sort collected #'string<)))\n\
         (let ((post-count 0))\n\
         (mapatoms (lambda (_) (setq post-count (1+ post-count))) ob)\n\
         (list after-map post-count\n\
         (intern-soft \\\"item-0\\\" ob)\n\
         (intern-soft \\\"item-2\\\" ob))))))",
        expect,
    );
}

#[test]
fn deficiency_intern_global_vs_obarray_namespace_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\\"unique-test-name-xyzzy\\\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ob (make-vector 7 0)))\n\
         (intern \\\"unique-test-name-xyzzy\\\" ob)\n\
         (let ((in-ob (intern-soft \\\"unique-test-name-xyzzy\\\" ob))\n\
         (in-global (intern-soft \\\"unique-test-name-xyzzy\\\")))\n\
         (list (if in-ob 'found-in-ob 'missing-in-ob)\n\
         (if in-global 'found-in-global 'missing-in-global)\n\
         (eq in-ob in-global)))))",
        expect,
    );
}
