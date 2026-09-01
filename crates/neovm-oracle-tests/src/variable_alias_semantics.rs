//! Oracle parity tests for variable alias semantics.
//!
//! GNU implements `defvaralias` and `internal-delete-indirect-variable` in
//! `src/eval.c`, while value access follows `SYMBOL_VARALIAS` in `src/data.c`.
//! These tests focus on alias value migration, chain following, cycle
//! rejection, let-bound rejection, definition provenance, and alias deletion.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_defvaralias_records_alias_as_variable_definition_origin() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((file
       (make-temp-file
        "neomacs-oracle-defvaralias-origin-" nil ".el"
        "(defvar neomacs--oracle-alias-origin-base nil)\n\
(defvaralias 'neomacs--oracle-alias-origin-name \
'neomacs--oracle-alias-origin-base)\n")))
  (unwind-protect
      (progn
        (load file nil nil t)
        (let ((origin
               (symbol-file 'neomacs--oracle-alias-origin-name 'defvar)))
          (list
           (stringp origin)
           (equal origin file)
           (and
            (memq 'neomacs--oracle-alias-origin-name
                  (cdr (assoc file load-history)))
            t))))
    (condition-case nil
        (internal-delete-indirect-variable
         'neomacs--oracle-alias-origin-name)
      (error nil))
    (when (boundp 'neomacs--oracle-alias-origin-base)
      (makunbound 'neomacs--oracle-alias-origin-base))
    (delete-file file)))
"#;

    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_defvaralias_migrates_existing_alias_value_to_void_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (condition-case nil
          (internal-delete-indirect-variable 'neomacs--oracle-alias-name)
        (error nil))
      (makunbound 'neomacs--oracle-alias-base)
      (makunbound 'neomacs--oracle-alias-name)
      (set 'neomacs--oracle-alias-name 'preexisting)
      (defvaralias 'neomacs--oracle-alias-name
        'neomacs--oracle-alias-base
        "Alias doc.")
      (list
       (eq (indirect-variable 'neomacs--oracle-alias-name)
           'neomacs--oracle-alias-base)
       (boundp 'neomacs--oracle-alias-base)
       (symbol-value 'neomacs--oracle-alias-base)
       (symbol-value 'neomacs--oracle-alias-name)
       (set 'neomacs--oracle-alias-base 'set-through-base)
       (symbol-value 'neomacs--oracle-alias-name)
       (set 'neomacs--oracle-alias-name 'set-through-alias)
       (symbol-value 'neomacs--oracle-alias-base)
       (documentation-property 'neomacs--oracle-alias-name
                               'variable-documentation t)))
  (condition-case nil
      (internal-delete-indirect-variable 'neomacs--oracle-alias-name)
    (error nil))
  (dolist (sym '(neomacs--oracle-alias-base
                 neomacs--oracle-alias-name))
    (when (boundp sym)
      (makunbound sym))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t preexisting preexisting set-through-base set-through-base set-through-alias set-through-alias \"Alias doc.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_indirect_variable_follows_alias_chain_and_rejects_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (set 'neomacs--oracle-alias-chain-c 'leaf)
      (defvaralias 'neomacs--oracle-alias-chain-b
        'neomacs--oracle-alias-chain-c)
      (defvaralias 'neomacs--oracle-alias-chain-a
        'neomacs--oracle-alias-chain-b)
      (list
       (eq (indirect-variable 'neomacs--oracle-alias-chain-a)
           'neomacs--oracle-alias-chain-c)
       (symbol-value 'neomacs--oracle-alias-chain-a)
       (set 'neomacs--oracle-alias-chain-a 'via-a)
       (symbol-value 'neomacs--oracle-alias-chain-c)
       (condition-case err
           (defvaralias 'neomacs--oracle-alias-chain-c
             'neomacs--oracle-alias-chain-a)
         (t (list (car err) (cdr err))))
       (indirect-variable 42)
       (indirect-variable "not-a-symbol")))
  (dolist (sym '(neomacs--oracle-alias-chain-a
                 neomacs--oracle-alias-chain-b))
    (condition-case nil
        (internal-delete-indirect-variable sym)
      (error nil)))
  (dolist (sym '(neomacs--oracle-alias-chain-a
                 neomacs--oracle-alias-chain-b
                 neomacs--oracle-alias-chain-c))
    (when (boundp sym)
      (makunbound sym))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t leaf via-a via-a (cyclic-variable-indirection (neomacs--oracle-alias-chain-a)) 42 \"not-a-symbol\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_defvaralias_rejects_constants_and_let_bound_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (list
     (condition-case err
         (defvaralias nil 'neomacs--oracle-alias-base)
       (t (list (car err) (cdr err))))
     (condition-case err
         (defvaralias t 'neomacs--oracle-alias-base)
       (t (list (car err) (cdr err))))
     (condition-case err
         (defvaralias :neomacs-oracle-alias-keyword
           'neomacs--oracle-alias-base)
       (t (list (car err) (cdr err))))
     (let ((neomacs--oracle-let-bound-alias 'let-value))
       (condition-case err
           (defvaralias 'neomacs--oracle-let-bound-alias
             'neomacs--oracle-alias-base)
         (t (list (car err) (cdr err))))))
  (dolist (sym '(neomacs--oracle-alias-base
                 neomacs--oracle-let-bound-alias))
    (condition-case nil
        (internal-delete-indirect-variable sym)
      (error nil))
    (when (boundp sym)
      (makunbound sym))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((error (\"Cannot make a constant an alias: nil\")) (error (\"Cannot make a constant an alias: t\")) (error (\"Cannot make a constant an alias: :neomacs-oracle-alias-keyword\")) neomacs--oracle-alias-base)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_internal_delete_indirect_variable_restores_plain_void_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(unwind-protect
    (progn
      (dolist (sym '(neomacs--oracle-delete-alias-name
                     neomacs--oracle-delete-alias-base))
        (condition-case nil
            (internal-delete-indirect-variable sym)
          (error nil))
        (when (boundp sym)
          (makunbound sym)))
      (set 'neomacs--oracle-delete-alias-base 'base-value)
      (defvaralias 'neomacs--oracle-delete-alias-name
        'neomacs--oracle-delete-alias-base
        "Deleted alias doc.")
      (let ((before (list
                     (eq (indirect-variable 'neomacs--oracle-delete-alias-name)
                         'neomacs--oracle-delete-alias-base)
                     (symbol-value 'neomacs--oracle-delete-alias-name)
                     (documentation-property 'neomacs--oracle-delete-alias-name
                                             'variable-documentation t))))
        (list
         before
         (eq (internal-delete-indirect-variable
              'neomacs--oracle-delete-alias-name)
             'neomacs--oracle-delete-alias-name)
         (eq (indirect-variable 'neomacs--oracle-delete-alias-name)
             'neomacs--oracle-delete-alias-name)
         (boundp 'neomacs--oracle-delete-alias-name)
         (condition-case err
             (symbol-value 'neomacs--oracle-delete-alias-name)
           (t (list (car err) (cdr err))))
         (symbol-value 'neomacs--oracle-delete-alias-base)
         (documentation-property 'neomacs--oracle-delete-alias-name
                                 'variable-documentation t))))
  (dolist (sym '(neomacs--oracle-delete-alias-name
                 neomacs--oracle-delete-alias-base))
    (condition-case nil
        (internal-delete-indirect-variable sym)
      (error nil))
    (when (boundp sym)
      (makunbound sym))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((t base-value \"Deleted alias doc.\") t t nil (void-variable (neomacs--oracle-delete-alias-name)) base-value nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
