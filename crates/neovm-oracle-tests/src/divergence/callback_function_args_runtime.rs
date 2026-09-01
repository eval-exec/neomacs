//! Callback / function-argument parity: define-hash-table-test (custom test +
//! hash fns), assoc/member/seq-uniq with predicate, add-hook + run-hooks /
//! run-hook-with-args(-until-success), apply-partially, funcall-interactively,
//! delete-dups / cl-*-duplicates with :test, after-change-functions,
//! sort/cl-sort :key, mapcan/cl-mapcar.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn add_hook_run() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (b a c)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((neo-test-hook-xyz nil) (log nil))
  (add-hook 'neo-test-hook-xyz (lambda () (push 'a log)))
  (add-hook 'neo-test-hook-xyz (lambda () (push 'b log)))
  (add-hook 'neo-test-hook-xyz (lambda () (push 'c log)) t)
  (run-hooks 'neo-test-hook-xyz)
  (nreverse log))"##,
        expect,
    );
}

#[test]
fn apply_partially_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (15 11 (6 7 8))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((add5 (apply-partially #'+ 5)))
  (list (funcall add5 10) (funcall add5 1 2 3) (mapcar add5 '(1 2 3))))"##,
        expect,
    );
}

#[test]
fn assoc_member_testfn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-member)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (assoc "B" '(("a" . 1) ("b" . 2)) (lambda (x y) (string-equal-ignore-case x y)))
        (cl-member 3 '(1 2 3 4) :test #'=)
        (member-ignore-case "FOO" '("foo" "bar"))
        (seq-uniq '("a" "A" "b") #'string-equal-ignore-case))"##,
        expect,
    );
}

#[test]
fn combine_change_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((log nil))
    (add-hook 'after-change-functions (lambda (b e l) (push (list b e l) log)) nil t)
    (insert "abc")
    (delete-region 1 2)
    (> (length log) 0)))"##,
        expect,
    );
}

#[test]
fn define_hash_table_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (two nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (progn
  (define-hash-table-test 'neo-ci-test
    (lambda (a b) (eq (length a) (length b)))
    (lambda (k) (length k)))
  (let ((h (make-hash-table :test 'neo-ci-test)))
    (puthash "ab" 'two h)
    (list (gethash "xy" h) (gethash "abc" h) (hash-table-count h)))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn delete_dups_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-delete-duplicates)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (delete-dups (list 1 2 1 3 2 1))
        (cl-delete-duplicates (list "a" "A" "b") :test #'string-equal-ignore-case)
        (cl-remove-duplicates (list 1 2 3 2 1) :test #'=))"##,
        expect,
    );
}

#[test]
fn funcall_interactively() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 300)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (defun neo-cmd-xyz (x) (interactive "p") (* x 100))
  (list (commandp 'neo-cmd-xyz) (funcall-interactively 'neo-cmd-xyz 3)))"##,
        expect,
    );
}

#[test]
fn mapcan_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-mapcar)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (mapcan (lambda (x) (list x (* x x))) '(1 2 3))
        (mapcar #'1+ '(1 2 3))
        (cl-mapcar #'+ '(1 2 3) '(10 20 30)))"##,
        expect,
    );
}

#[test]
fn run_hook_with_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((10 15) (3 10 15))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((neo-h2-xyz nil) (acc nil))
  (add-hook 'neo-h2-xyz (lambda (x) (push (* x 2) acc)))
  (add-hook 'neo-h2-xyz (lambda (x) (push (* x 3) acc)))
  (run-hook-with-args 'neo-h2-xyz 5)
  (list (sort acc #'<)
        (run-hook-with-args-until-success 'neo-h2-xyz 1)))"##,
        expect,
    );
}

#[test]
fn sort_with_key_fn() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (sort (list "ccc" "a" "bb") :key #'length)
        (sort (vector 3 1 2) #'<)
        (cl-sort (list -3 1 -2) #'< :key #'abs))"##,
        expect,
    );
}
