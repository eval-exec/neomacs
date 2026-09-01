//! Deep combo: cl-labels + closures + mutation + recursion + buffer side effects.
//! Tests local function definitions with closures capturing mutable state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_cl_labels_mutual_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-labels ((even? (n) (if (= n 0) t (odd? (1- n))))\n\
         (odd? (n) (if (= n 0) nil (even? (1- n)))))\n\
         (list (even? 0) (even? 4) (odd? 3) (odd? 0))))",
        expect,
    );
}

#[test]
fn deficiency_cl_labels_closure_over_mutable_var() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((counter 0))\n\
         (cl-labels ((inc () (cl-incf counter))\n\
         (get-count () counter))\n\
         (inc) (inc) (inc)\n\
         (list (get-count) counter))))",
        expect,
    );
}

#[test]
fn deficiency_cl_labels_with_buffer_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"clb\")))\n\
         (with-current-buffer buf\n\
         (cl-labels ((insert-tagged (tag content)\n\
         (let ((start (point)))\n\
         (insert content)\n\
         (put-text-property start (point) 'tag tag)))\n\
         (get-tagged (tag)\n\
         (cl-loop for i from 1 to (buffer-size)\n\
         when (eq (get-text-property i 'tag) tag)\n\
         collect (cons i (buffer-substring i (1+ i))))))\n\
         (insert-tagged 'alpha \"ABC\")\n\
         (insert-tagged 'beta \"DEF\")\n\
         (insert-tagged 'alpha \"GHI\")\n\
         (list (get-tagged 'alpha)\n\
         (get-tagged 'beta)\n\
         (buffer-string))))\n\
         (kill-buffer buf)))",
        expect,
    );
}

#[test]
fn deficiency_cl_labels_higher_order_factory() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-labels ((make-adder (n) (lambda (x) (+ x n)))\n\
         (make-multiplier (n) (lambda (x) (* x n))))\n\
         (let ((add5 (make-adder 5))\n\
         (mul3 (make-multiplier 3)))\n\
         (list (funcall add5 10)\n\
         (funcall mul3 7)\n\
         (funcall (make-adder 100) 1)))))",
        expect,
    );
}

#[test]
fn deficiency_cl_flet_shadowing_builtins() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-flet)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-flet ((+ (a b) (- a b)))\n\
         (list (+ 10 3)\n\
         (+ 100 50))))",
        expect,
    );
}

#[test]
fn deficiency_cl_labels_nested_with_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((result nil))\n\
         (cl-labels ((walk (lst depth)\n\
         (cond\n\
         ((null lst) nil)\n\
         ((listp (car lst))\n\
         (walk (car lst) (1+ depth))\n\
         (walk (cdr lst) depth))\n\
         (t\n\
         (push (list (car lst) depth) result)\n\
         (walk (cdr lst) depth)))))\n\
         (walk '(a (b (c d)) e (f)) 0)\n\
         (nreverse result)))",
        expect,
    );
}

#[test]
fn deficiency_cl_labels_tail_accumulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-labels ((rev-append (lst acc)\n\
         (if (null lst) acc\n\
         (rev-append (cdr lst) (cons (car lst) acc)))))\n\
         (rev-append '(1 2 3 4 5) nil)))",
        expect,
    );
}

#[test]
fn deficiency_cl_labels_with_hash_table_builder() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((ht (make-hash-table :test 'equal)))\n\
         (cl-labels ((add-pair (k v) (puthash k v ht))\n\
         (add-list (pairs)\n\
         (dolist (p pairs)\n\
         (add-pair (car p) (cdr p))))\n\
         (count-matches (pred)\n\
         (cl-loop for v being the hash-values of ht\n\
         when (funcall pred v) count t)))\n\
         (add-list '((\"a\" . 1) (\"b\" . 2) (\"c\" . 3) (\"d\" . 4)))\n\
         (list (hash-table-count ht)\n\
         (count-matches #'cl-evenp)\n\
         (count-matches #'cl-oddp)\n\
         (gethash \"b\" ht)))))",
        expect,
    );
}

#[test]
fn deficiency_cl_function_lambda_closure_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-function)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((x 10))\n\
         (let ((f1 (cl-function (lambda (y) (+ x y))))\n\
         (f2 (lambda (y) (+ x y))))\n\
         (list (funcall f1 5)\n\
         (funcall f2 5)\n\
         (functionp f1)\n\
         (functionp f2)))))",
        expect,
    );
}

#[test]
fn deficiency_cl_labels_with_marker_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-labels)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((buf (generate-new-buffer \"mlt\")))\n\
         (with-current-buffer buf\n\
         (let ((m1 (copy-marker 1))\n\
         (m2 (copy-marker 1)))\n\
         (cl-labels ((insert-at (m text)\n\
         (goto-char m)\n\
         (insert text)\n\
         (put-text-property (- (point) (length text)) (point)\n\
         'inserted t)))\n\
         (insert \"INITIAL\")\n\
         (insert-at m1 \"PRE-\")\n\
         (insert-at m2 \"POST-\")\n\
         (list (buffer-string)\n\
         (marker-position m1)\n\
         (marker-position m2)\n\
         (get-text-property 1 'inserted)\n\
         (get-text-property 5 'inserted)))))\n\
         (kill-buffer buf)))",
        expect,
    );
}
