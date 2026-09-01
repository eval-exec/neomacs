//! Divergence tests: subr + seq-let + map-let + cl-lib deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_cl_flet_labels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-labels ((test-fact-xxx (n)
               (if (<= n 1) 1
                 (* n (test-fact-xxx (1- n))))))
    (list (test-fact-xxx 1)
          (= (test-fact-xxx 1) 1)
          (test-fact-xxx 5)
          (= (test-fact-xxx 5) 120)
          (test-fact-xxx 10)
          (= (test-fact-xxx 10) 3628800)))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_flet_temporary_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-flet)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-fo-xxx (x) (* x 2))
  (list (test-fo-xxx 5)
        (= (test-fo-xxx 5) 10)
        (cl-flet ((test-fo-xxx (x) (* x 10)))
          (test-fo-xxx 5))
        (= (cl-flet ((test-fo-xxx (x) (* x 10)))
             (test-fo-xxx 5))
           50)
        (test-fo-xxx 5)
        (= (test-fo-xxx 5) 10))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_macrolet_expansion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-macrolet)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-macrolet ((test-swap-xxx (a b)
                  (list 'let (list (list a b) (list b a))
                        (list 'list a b))))
    (let ((x 1) (y 2))
      (test-swap-xxx x y)))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_symbol_macrolet() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-symbol-macrolet)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-symbol-macrolet ((test-pi-xxx 31415))
    (list test-pi-xxx
          (= test-pi-xxx 31415)
          (+ test-pi-xxx 1)
          (= (+ test-pi-xxx 1) 31416)
          (* test-pi-xxx 2)
          (= (* test-pi-xxx 2) 62830)))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_the_and_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-the)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-the integer (+ 1 2))
        (= (cl-the integer (+ 1 2)) 3)
        (cl-the string "hello")
        (string= (cl-the string "hello") "hello")
        (cl-the cons '(1 2))
        (equal (cl-the cons '(1 2)) '(1 2))
        (cl-the number 3.14)
        (= (cl-the number 3.14) 3.14))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_eval_when() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function eval-when)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defvar test-ew-xxx nil)
  (eval-when-compile (setq test-ew-xxx 'compile-time))
  (eval-when (eval load) (setq test-ew-xxx 'run-time))
  (list test-ew-xxx
        (eq test-ew-xxx 'run-time)
        (eval-when (eval) (+ 1 2))
        (= (eval-when (eval) (+ 1 2)) 3)
        (eval-when (compile) 'skipped))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_locally_declarations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-locally)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (cl-locally
    (list (+ 1 2)
          (= (+ 1 2) 3)
          (concat "a" "b")
          (string= (concat "a" "b") "ab")
          (* 10 10)
          (= (* 10 10) 100)
          (list 1 2 3)
          (equal (list 1 2 3) '(1 2 3))))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_assoc_rassoc_extended() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 13 35)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((alist '(("a" . 1) ("b" . 2) ("c" . 3)
                 (a . 4) (b . 5))))
    (list (assoc "a" alist)
          (equal (assoc "a" alist) '("a" . 1))
          (assoc 'a alist)
          (equal (assoc 'a alist) '(a . 4))
          (rassoc 2 alist)
          (equal (rassoc 2 alist) '("b" . 2))
          (rassoc 5 alist)
          (equal (rassoc 5 alist) '(b . 5))
          (length alist)
          (= (length alist) 5)))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_subseq_substitute() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-subseq)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (cl-subseq '(1 2 3 4 5) 1 4)
        (equal (cl-subseq '(1 2 3 4 5) 1 4) '(2 3 4))
        (cl-subseq '(a b c d e) 2)
        (equal (cl-subseq '(a b c d e) 2) '(c d e))
        (cl-substitute 99 3 '(1 2 3 4 3 5))
        (equal (cl-substitute 99 3 '(1 2 3 4 3 5)) '(1 2 99 4 99 5))
        (cl-substitute 99 3 '(1 2 3 4 3 5) :count 1)
        (equal (cl-substitute 99 3 '(1 2 3 4 3 5) :count 1) '(1 2 99 4 3 5))
        (cl-remove 3 '(1 2 3 4 3 5))
        (equal (cl-remove 3 '(1 2 3 4 3 5)) '(1 2 4 5)))) #"#,
        expect,
    );
}

#[test]
fn divergence_cl_merge_sort_stable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function copy-list)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((a (copy-list '(1 3 5 7)))
        (b (copy-list '(2 4 6 8))))
    (list (cl-merge 'list a b '<)
          (equal (cl-merge 'list a b '<) '(1 2 3 4 5 6 7 8))
          (cl-sort (copy-list '(3 1 4 1 5 9 2 6)) '<)
          (equal (cl-sort (copy-list '(3 1 4 1 5 9 2 6)) '<)
                 '(1 1 2 3 4 5 6 9))
          (cl-stable-sort (copy-list '((a . 1) (b . 1) (c . 2)))
                          (lambda (x y) (< (cdr x) (cdr y))))
          (= (length (cl-stable-sort (copy-list '((a . 1) (b . 1) (c . 2)))
                                     (lambda (x y) (< (cdr x) (cdr y)))))
             3)))) #"#,
        expect,
    );
}
