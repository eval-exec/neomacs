//! Oracle parity tests for GNU function-cell semantics.
//!
//! GNU implements `fboundp`, `fmakunbound`, `symbol-function`, `fset`, and
//! `indirect-function` in `src/data.c`.  These tests focus on function-cell
//! return values, nil/t protection, and cyclic function indirection checks.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_function_cell_fset_and_fmakunbound_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((fn (lambda (x) (list 'called x))))
  (unwind-protect
      (list
       (fboundp 'neomacs--oracle-fcell-basic)
       (symbol-function 'neomacs--oracle-fcell-basic)
       (eq (fset 'neomacs--oracle-fcell-basic fn) fn)
       (fboundp 'neomacs--oracle-fcell-basic)
       (eq (symbol-function 'neomacs--oracle-fcell-basic) fn)
       (neomacs--oracle-fcell-basic 7)
       (fmakunbound 'neomacs--oracle-fcell-basic)
       (fboundp 'neomacs--oracle-fcell-basic)
       (symbol-function 'neomacs--oracle-fcell-basic)
       (condition-case err
           (neomacs--oracle-fcell-basic 8)
         (error (list (car err) (cdr err)))))
    (when (fboundp 'neomacs--oracle-fcell-basic)
      (fmakunbound 'neomacs--oracle-fcell-basic))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil t t t (called 7) neomacs--oracle-fcell-basic nil nil (void-function (neomacs--oracle-fcell-basic)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_fmakunbound_protects_nil_and_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (fmakunbound nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (fmakunbound t)
   (error (list (car err) (cdr err))))
 (condition-case err
     (fset nil (lambda () 'bad))
   (error (list (car err) (cdr err))))
 (condition-case err
     (fset nil nil)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((setting-constant (nil)) (setting-constant (t)) (setting-constant (nil)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_fset_rejects_cyclic_function_indirection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (fset 'neomacs--oracle-fcell-a 'neomacs--oracle-fcell-b)
      (fset 'neomacs--oracle-fcell-b 'neomacs--oracle-fcell-c)
      (list
       (condition-case err
           (fset 'neomacs--oracle-fcell-c 'neomacs--oracle-fcell-a)
         (error (list (car err) (cdr err))))
       (fboundp 'neomacs--oracle-fcell-c)
       (symbol-function 'neomacs--oracle-fcell-c)))
  (dolist (sym '(neomacs--oracle-fcell-a
                 neomacs--oracle-fcell-b
                 neomacs--oracle-fcell-c))
    (when (fboundp sym)
      (fmakunbound sym))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((cyclic-function-indirection (neomacs--oracle-fcell-c)) nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_indirect_function_follows_symbol_chain_and_stops_at_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((fn (lambda () 'final)))
  (unwind-protect
      (progn
        (fset 'neomacs--oracle-fcell-root 'neomacs--oracle-fcell-mid)
        (fset 'neomacs--oracle-fcell-mid 'neomacs--oracle-fcell-leaf)
        (fset 'neomacs--oracle-fcell-leaf fn)
        (list
         (eq (indirect-function 'neomacs--oracle-fcell-root) fn)
         (eq (indirect-function 'neomacs--oracle-fcell-mid) fn)
         (eq (indirect-function fn) fn)
         (fmakunbound 'neomacs--oracle-fcell-leaf)
         (indirect-function 'neomacs--oracle-fcell-root)
         (indirect-function 'neomacs--oracle-fcell-root t)))
    (dolist (sym '(neomacs--oracle-fcell-root
                   neomacs--oracle-fcell-mid
                   neomacs--oracle-fcell-leaf))
      (when (fboundp sym)
        (fmakunbound sym)))))
"#;

    let expect = expect_test::expect![[r#""OK (t t t neomacs--oracle-fcell-leaf nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
