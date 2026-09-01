//! Oracle parity tests for symbol value-cell edge semantics.
//!
//! GNU implements `boundp`, `makunbound`, `symbol-value`, `set`,
//! `default-boundp`, `default-value`, and `set-default` in `src/data.c`.
//! These tests focus on constant write protection and void/default value
//! behavior around the value cell.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_symbol_value_void_and_default_void_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((sym (make-symbol "neomacs--oracle-void-symbol")))
  (list
   (boundp sym)
   (default-boundp sym)
   (condition-case err
       (symbol-value sym)
     (error (list (car err) (cdr err))))
   (condition-case err
       (default-value sym)
     (error (list (car err) (cdr err))))
   (set sym 'now-bound)
   (boundp sym)
   (default-boundp sym)
   (symbol-value sym)
   (default-value sym)
   (eq (makunbound sym) sym)
   (boundp sym)
   (default-boundp sym)
   (condition-case err
       (symbol-value sym)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil nil (void-variable (neomacs--oracle-void-symbol)) (void-variable (neomacs--oracle-void-symbol)) now-bound t t now-bound now-bound t nil nil (void-variable (neomacs--oracle-void-symbol)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_set_and_set_default_protect_nil_and_t() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (set nil nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (set nil 'changed)
   (error (list (car err) (cdr err))))
 (condition-case err
     (set t t)
   (error (list (car err) (cdr err))))
 (condition-case err
     (set t nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (set-default nil nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (set-default t t)
   (error (list (car err) (cdr err))))
 (condition-case err
     (makunbound nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (makunbound t)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((setting-constant (nil)) (setting-constant (nil)) (setting-constant (t)) (setting-constant (t)) (setting-constant (nil)) (setting-constant (t)) (setting-constant (nil)) (setting-constant (t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_keyword_value_cell_can_only_be_set_to_self() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((kw :neomacs-oracle-keyword-edge))
  (list
   (keywordp kw)
   (boundp kw)
   (default-boundp kw)
   (symbol-value kw)
   (default-value kw)
   ;; GNU's `set_internal' has special constant-write handling for keywords.
   (condition-case err
       (set kw kw)
     (t (list (car err) (cdr err))))
   (condition-case err
       (set-default kw kw)
     (t (list (car err) (cdr err))))
   (condition-case err
       (symbol-value kw)
     (t (list (car err) (cdr err))))
   (condition-case err
       (default-value kw)
     (t (list (car err) (cdr err))))
   (condition-case err
       (set kw 'changed)
     (t (list (car err) (cdr err))))
   (condition-case err
       (set-default kw 'changed)
     (t (list (car err) (cdr err))))
   (condition-case err
       (makunbound kw)
     (t (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t t :neomacs-oracle-keyword-edge :neomacs-oracle-keyword-edge :neomacs-oracle-keyword-edge :neomacs-oracle-keyword-edge :neomacs-oracle-keyword-edge :neomacs-oracle-keyword-edge (setting-constant (:neomacs-oracle-keyword-edge)) (setting-constant (:neomacs-oracle-keyword-edge)) (setting-constant (:neomacs-oracle-keyword-edge)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_set_default_uses_default_cell_not_current_let_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((sym (make-symbol "neomacs--oracle-default-cell")))
  (set sym 'global-1)
  (list
   (symbol-value sym)
   (default-value sym)
   (let ((sym 'lexical-shadow))
     sym)
   (let ((old (symbol-value sym)))
     (let ((neomacs--oracle-dynamic-holder sym))
       (let ((neomacs--oracle-dynamic-holder 'dynamic-value))
         (set-default sym 'global-2)
         (list old
               neomacs--oracle-dynamic-holder
               (symbol-value sym)
               (default-value sym)))))
   (set-default sym 'global-3)
   (symbol-value sym)
   (default-value sym)))
"#;

    let expect = expect_test::expect![[
        r#""OK (global-1 global-1 lexical-shadow (global-1 dynamic-value global-2 global-2) global-3 global-3 global-3)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_default_toplevel_value_ignores_active_let_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (setq neomacs--oracle-top-default 'global)
  (list
   (default-value 'neomacs--oracle-top-default)
   (default-toplevel-value 'neomacs--oracle-top-default)
   (let ((neomacs--oracle-top-default 'let-value))
     (list
      neomacs--oracle-top-default
      (default-value 'neomacs--oracle-top-default)
      (default-toplevel-value 'neomacs--oracle-top-default)
      (set-default 'neomacs--oracle-top-default 'default-set)
      neomacs--oracle-top-default
      (default-value 'neomacs--oracle-top-default)
      (default-toplevel-value 'neomacs--oracle-top-default)
      (set-default-toplevel-value 'neomacs--oracle-top-default 'top-set)
      neomacs--oracle-top-default
      (default-value 'neomacs--oracle-top-default)
      (default-toplevel-value 'neomacs--oracle-top-default)))
   neomacs--oracle-top-default
   (default-value 'neomacs--oracle-top-default)
   (default-toplevel-value 'neomacs--oracle-top-default)))
"#;

    let expect = expect_test::expect![[
        r#""OK (global global (let-value global global default-set let-value default-set default-set nil let-value top-set top-set) top-set top-set top-set)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_default_toplevel_value_errors_and_constant_protection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((sym (make-symbol "neomacs--oracle-top-void")))
  (list
   (condition-case err
       (default-toplevel-value sym)
     (error (list (car err) (cdr err))))
   (set-default-toplevel-value sym 'now-bound)
   (default-toplevel-value sym)
   (default-value sym)
   (condition-case err
       (set-default-toplevel-value nil nil)
     (error (list (car err) (cdr err))))
   (condition-case err
       (set-default-toplevel-value t t)
     (error (list (car err) (cdr err))))
   (condition-case err
       (set-default-toplevel-value :neomacs-oracle-top-key
                                   :neomacs-oracle-top-key)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((void-variable (neomacs--oracle-top-void)) nil now-bound now-bound (setting-constant (nil)) (setting-constant (t)) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_variable_binding_locus_default_let_local_and_alias_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((buf (generate-new-buffer " *neomacs-oracle-locus*")))
  (unwind-protect
      (progn
        (defvar neomacs--oracle-locus-base 'default)
        (defvaralias 'neomacs--oracle-locus-alias
          'neomacs--oracle-locus-base)
        (list
         ;; Global/default and dynamic let bindings are not buffer-local.
         (variable-binding-locus 'neomacs--oracle-locus-base)
         (let ((neomacs--oracle-locus-base 'dynamic))
           (list neomacs--oracle-locus-base
                 (variable-binding-locus 'neomacs--oracle-locus-base)
                 (variable-binding-locus 'neomacs--oracle-locus-alias)))
         ;; Buffer-local active binding returns the current buffer, and alias
         ;; lookup follows GNU's SYMBOL_VARALIAS redirect path.
         (with-current-buffer buf
           (make-local-variable 'neomacs--oracle-locus-base)
           (setq neomacs--oracle-locus-base 'local)
           (list (eq (variable-binding-locus 'neomacs--oracle-locus-base)
                     (current-buffer))
                 (eq (variable-binding-locus 'neomacs--oracle-locus-alias)
                     (current-buffer))
                 neomacs--oracle-locus-alias))
         ;; Void symbols still have a default/global locus of nil.
         (variable-binding-locus 'neomacs--oracle-locus-unbound)
         (condition-case err
             (variable-binding-locus 42)
           (error (list (car err) (cdr err))))))
    (condition-case nil
        (internal-delete-indirect-variable 'neomacs--oracle-locus-alias)
      (error nil))
    (when (boundp 'neomacs--oracle-locus-base)
      (makunbound 'neomacs--oracle-locus-base))
    (when (buffer-live-p buf)
      (kill-buffer buf))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil (dynamic nil nil) (t t local) nil (wrong-type-argument (symbolp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
