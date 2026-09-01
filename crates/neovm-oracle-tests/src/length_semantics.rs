//! Oracle parity tests for GNU length-related edge semantics.
//!
//! GNU implements `length`, `safe-length`, `length<`, `length>`, `length=`,
//! and `proper-list-p` in `src/fns.c`.  The comparison functions use a bounded
//! fast path for list inputs, which makes circular and dotted inputs observable.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_length_dotted_and_type_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (length '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (length '(a . b))
   (error (list (car err) (cdr err))))
 (condition-case err
     (length 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (length< '(a b c) 'x)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (listp c)) (wrong-type-argument (listp b)) (wrong-type-argument (sequencep 42)) (wrong-type-argument (fixnump x)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_length_comparisons_on_dotted_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (length< '(a b . c) 1)
 (length< '(a b . c) 2)
 (length< '(a b . c) 3)
 (length> '(a b . c) 0)
 (length> '(a b . c) 1)
 (length> '(a b . c) 2)
 (length= '(a b . c) -1)
 (length= '(a b . c) 0)
 (length= '(a b . c) 1)
 (length= '(a b . c) 2))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil t t t nil nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_length_comparisons_on_circular_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((x (list 'a 'b 'c))
       (_ (setcdr (last x) x)))
  (list
   (length< x 0)
   (length< x 1)
   (length< x 3)
   (length> x 0)
   (length> x 2)
   (length= x -1)
   (length= x 0)
   (length= x 2)
   (proper-list-p x)
   (integerp (safe-length x))))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil nil t t nil nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_length_circular_list_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((x (list 'a 'b 'c))
       (_ (setcdr (last x) x)))
  (list
   (condition-case err
       (length x)
     (error (let ((arg (cadr err)))
              (list (car err)
                    (length (cdr err))
                    (eq arg (cddr x))
                    (listp arg)
                    (car arg)))))
   (condition-case err
       (length< x 65535)
     (error (let ((arg (cadr err)))
              (list (car err)
                    (length (cdr err))
                    (eq arg (cddr x))
                    (listp arg)
                    (car arg)))))))
"#;

    let expect =
        expect_test::expect![[r#""OK ((circular-list 1 t t c) (circular-list 1 t t c))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_safe_length_and_proper_list_exact_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((self (list 'x))
       (_ (setcdr self self))
       (lasso (list 'a 'b 'c))
       (_ (setcdr (cddr lasso) (cdr lasso))))
  (list
   (safe-length nil)
   (safe-length 42)
   (safe-length '(a . b))
   (safe-length '(a b . c))
   (safe-length self)
   (safe-length lasso)
   (proper-list-p nil)
   (proper-list-p '(a b c))
   (proper-list-p '(a b . c))
   (proper-list-p 42)
   (proper-list-p self)
   (proper-list-p lasso)))
"#;

    let expect = expect_test::expect![[r#""OK (0 0 1 2 1 4 0 3 nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_length_on_non_list_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((bv (make-bool-vector 5 t)))
  (list
   (length "a中b")
   (string-bytes "a中b")
   (length [a b c])
   (length bv)
   (length (make-char-table 'test nil))
   (condition-case err
       (string-bytes 42)
     (error (list (car err) (cdr err))))))
"#;

    let expect =
        expect_test::expect![[r#""OK (3 5 3 5 4194304 (wrong-type-argument (stringp 42)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
