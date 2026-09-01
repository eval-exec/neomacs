//! Oracle parity tests for GNU derived major-mode semantics.
//!
//! These tests cover `define-derived-mode`, parent relationships, aliases,
//! extra parents, generated metadata, hooks, local maps, syntax tables, abbrev
//! tables, and current-buffer `derived-mode-p` behavior.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_define_derived_mode_metadata_and_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'derived)
  (let ((events nil))
    (define-derived-mode neomacs-oracle-base-mode fundamental-mode "OracleBase"
      "Oracle base doc."
      (push 'base-body events))
    (define-derived-mode neomacs-oracle-child-mode neomacs-oracle-base-mode "OracleChild"
      "Oracle child doc."
      :group 'neomacs-oracle-group
      :after-hook (push 'child-after events)
      (push 'child-body events))
    (add-hook 'neomacs-oracle-base-mode-hook
              (lambda () (push 'base-hook events)))
    (add-hook 'neomacs-oracle-child-mode-hook
              (lambda () (push 'child-hook events)))
    (with-temp-buffer
      (neomacs-oracle-child-mode)
      (list major-mode
            mode-name
            events
            (get 'neomacs-oracle-child-mode 'custom-mode-group)
            (get 'neomacs-oracle-child-mode 'derived-mode-parent)
            (provided-mode-derived-p 'neomacs-oracle-child-mode 'neomacs-oracle-base-mode)
            (derived-mode-p 'neomacs-oracle-base-mode)
            (string-match-p "neomacs-oracle-child-mode-hook"
                            (documentation 'neomacs-oracle-child-mode))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (neomacs-oracle-child-mode \"OracleChild\" (child-after child-hook base-hook child-body base-body) neomacs-oracle-group neomacs-oracle-base-mode neomacs-oracle-base-mode neomacs-oracle-base-mode 128)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_derived_mode_maps_syntax_and_abbrev_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'derived)
  (let ((syntax (make-syntax-table))
        (abbrev (make-abbrev-table)))
    (modify-syntax-entry ?_ "w" syntax)
    (define-derived-mode neomacs-oracle-table-mode fundamental-mode "OracleTable"
      "Mode with explicit tables."
      :syntax-table syntax
      :abbrev-table abbrev
      (define-key neomacs-oracle-table-mode-map (kbd "C-c o") 'ignore))
    (with-temp-buffer
      (neomacs-oracle-table-mode)
      (list
       (eq (syntax-table) syntax)
       (eq local-abbrev-table abbrev)
       (lookup-key (current-local-map) (kbd "C-c o"))
       (symbol-value 'neomacs-oracle-table-mode-map)
       (get 'neomacs-oracle-table-mode-map 'variable-documentation)
       (get 'neomacs-oracle-table-mode-syntax-table 'variable-documentation)
       (get 'neomacs-oracle-table-mode-abbrev-table 'variable-documentation)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (t t ignore (keymap (3 keymap (111 . ignore))) \"Keymap for `neomacs-oracle-table-mode'.\" nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_derived_mode_all_parents_aliases_and_extra_parents() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'derived)
  (define-derived-mode neomacs-oracle-root-mode fundamental-mode "OracleRoot")
  (define-derived-mode neomacs-oracle-mid-mode neomacs-oracle-root-mode "OracleMid")
  (define-derived-mode neomacs-oracle-leaf-mode neomacs-oracle-mid-mode "OracleLeaf")
  (defalias 'neomacs-oracle-alias-mode 'neomacs-oracle-leaf-mode)
  (derived-mode-add-parents 'neomacs-oracle-leaf-mode '(text-mode prog-mode))
  (list
   (derived-mode-all-parents 'neomacs-oracle-leaf-mode)
   (provided-mode-derived-p 'neomacs-oracle-leaf-mode '(fundamental-mode neomacs-oracle-root-mode))
   (provided-mode-derived-p 'neomacs-oracle-leaf-mode 'text-mode)
   (provided-mode-derived-p 'neomacs-oracle-leaf-mode 'prog-mode)
   (provided-mode-derived-p 'neomacs-oracle-alias-mode 'neomacs-oracle-root-mode)
   (get 'neomacs-oracle-root-mode 'derived-mode--followers)
   (get 'text-mode 'derived-mode--followers)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((neomacs-oracle-leaf-mode neomacs-oracle-mid-mode neomacs-oracle-root-mode text-mode prog-mode) neomacs-oracle-root-mode text-mode prog-mode neomacs-oracle-root-mode (neomacs-oracle-mid-mode) (neomacs-oracle-leaf-mode))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_derived_mode_no_parent_and_noninteractive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (require 'derived)
  (define-derived-mode neomacs-oracle-plain-mode nil "OraclePlain"
    :interactive nil
    (setq-local neomacs-oracle-local 17))
  (let ((interactive-form (interactive-form 'neomacs-oracle-plain-mode)))
    (with-temp-buffer
      (neomacs-oracle-plain-mode)
      (list
       major-mode
       mode-name
       interactive-form
       neomacs-oracle-local
       (local-variable-p 'neomacs-oracle-local)
       (provided-mode-derived-p 'neomacs-oracle-plain-mode nil)
       (derived-mode-p nil)
       (string-match-p "Uses keymap" (documentation 'neomacs-oracle-plain-mode))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (neomacs-oracle-plain-mode \"OraclePlain\" nil 17 t nil nil 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
