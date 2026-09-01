//! Deep combo: cl-pcase + destructuring + guards + pattern matching.
//! Tests pattern matching with complex data structures and guards.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_pcase_basic_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-pcase)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (list (cl-pcase 42\n\
         (1 'one) (42 'found) (t 'other))\n\
         (cl-pcase 'hello\n\
         ('world 'no) ('hello 'yes))))",
        expect,
    );
}

#[test]
fn deficiency_pcase_with_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-pcase)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-pcase '(1 2 3)\n\
         ((and (app car x) (app cadr y))\n\
         (list x y))))",
        expect,
    );
}

#[test]
fn deficiency_pcase_with_guard_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-loop for n from 0 to 10\n\
         collect (cl-pcase n\n\
         ((pred cl-evenp) 'even)\n\
         ((pred (> 5)) 'small-odd)\n\
         (_ 'big-odd))))",
        expect,
    );
}

#[test]
fn deficiency_pcase_with_app_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-pcase)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-pcase '(\"hello\" 42 (a b c))\n\
         ((app car s) (app cadr n) (app caddr lst))\n\
         (list s n lst)))",
        expect,
    );
}

#[test]
fn deficiency_pcase_nested_list_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-pcase)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-pcase '((1 2) (3 4) (5 6))\n\
         (`((,a ,b) (,c ,d) (,e ,f))\n\
         (list a b c d e f))))",
        expect,
    );
}

#[test]
fn deficiency_pcase_with_string_match_guard() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-loop for s in '(\"hello\" \"world\" \"test\" \"elisp\")\n\
         collect (cl-pcase s\n\
         ((pred (lambda (x) (string-match \"l\" x))) 'has-l)\n\
         (_ 'no-l))))",
        expect,
    );
}

#[test]
fn deficiency_pcase_on_vector_with_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-pcase)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-pcase [1 2 3]\n\
         ((pred (lambda (v) (= (length v) 3)))\n\
         (append [1 2 3] nil))\n\
         (_ 'other)))",
        expect,
    );
}

#[test]
fn deficiency_pcase_with_or_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-loop for x in '(a b c d e f)\n\
         collect (cl-pcase x\n\
         ((or 'a 'b 'c) 'group-1)\n\
         ((or 'd 'e 'f) 'group-2))))",
        expect,
    );
}

#[test]
fn deficiency_pcase_on_alist_lookup_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (let ((alist '((a . 1) (b . 2) (c . 3))))\n\
         (cl-loop for key in '(a b c d)\n\
         collect (cl-pcase (assq key alist)\n\
         ((and (cons k v) (guard (numberp v)))\n\
         (list k v))\n\
         (_ 'missing)))))",
        expect,
    );
}

#[test]
fn deficiency_pcase_with_and_or_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
         (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)\n\
         collect (cl-pcase x\n\
         ((and (pred cl-evenp) (pred (> 5))) 'big-even)\n\
         ((pred cl-evenp) 'small-even)\n\
         ((pred (> 5)) 'big-odd)\n\
         (_ 'small-odd))))",
        expect,
    );
}
