//! Issue #559: native completion reads the same binding as Lisp.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn completion_case_policy_follows_local_binding_and_restoration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((completion-ignore-case nil)
                 (completion-regexp-list nil)
                 (table '("*Messages*")))
              (with-temp-buffer
                (setq-local completion-ignore-case t)
                (list
                 (list (try-completion "*mes" table)
                       (all-completions "*mes" table)
                       (test-completion "*messages*" table)
                       (car (completion--flex-cost-gotoh "MES" "Messages")))
                 (let ((completion-ignore-case nil))
                   (list (try-completion "*mes" table)
                         (all-completions "*mes" table)
                         (test-completion "*messages*" table)
                         (completion--flex-cost-gotoh "MES" "Messages")))
                 (try-completion "*mes" table)
                 (with-temp-buffer (try-completion "*mes" table))
                 (try-completion "*mes" table))))"#,
        expect_test::expect![[
            r#""OK ((\"*Messages*\" (\"*Messages*\") t 0) (nil nil nil nil) \"*Messages*\" nil \"*Messages*\")""#
        ]],
    );
}

#[test]
fn local_case_policy_applies_to_list_alist_hash_and_obarray_tables() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((completion-ignore-case nil)
                 (completion-regexp-list nil)
                 (hash (make-hash-table :test 'equal))
                 (symbols (make-vector 7 0)))
              (puthash "*Messages*" t hash)
              (intern "*Messages*" symbols)
              (with-temp-buffer
                (setq-local completion-ignore-case nil)
                (let ((completion-ignore-case t))
                  (mapcar (lambda (table)
                            (list (try-completion "*mes" table)
                                  (all-completions "*mes" table)
                                  (test-completion "*messages*" table)
                                  (let ((completion-regexp-list '("mes")))
                                    (all-completions "" table))))
                          (list '("*Messages*") '(("*Messages*" . t)) hash symbols)))))"#,
        expect_test::expect![[
            r#""OK ((\"*Messages*\" (\"*Messages*\") t (\"*Messages*\")) (\"*Messages*\" (\"*Messages*\") t (\"*Messages*\")) (\"*Messages*\" (\"*Messages*\") t (\"*Messages*\")) (\"*Messages*\" (\"*Messages*\") t (\"*Messages*\")))""#
        ]],
    );
}

#[test]
fn local_regexp_filter_follows_dynamic_binding_and_restoration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((completion-ignore-case nil)
                 (completion-regexp-list '("Beta"))
                 (table '("Alpha" "Beta")))
              (with-temp-buffer
                (setq-local completion-regexp-list '("Alpha"))
                (list
                 (list (try-completion "" table) (all-completions "" table)
                       (test-completion "Alpha" table) (test-completion "Beta" table))
                 (let ((completion-regexp-list '("Beta")))
                   (all-completions "" table))
                 (all-completions "" table)
                 (with-temp-buffer (all-completions "" table)))))"#,
        expect_test::expect![[
            r#""OK ((\"Alpha\" (\"Alpha\") t nil) (\"Beta\") (\"Alpha\") (\"Beta\"))""#
        ]],
    );
}

#[test]
fn file_completion_shares_the_buffer_local_regexp_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((directory (make-temp-file "issue559-" t))
                 (completion-regexp-list '("Beta")))
              (unwind-protect
                  (progn
                    (write-region "" nil (expand-file-name "Alpha" directory) nil 'silent)
                    (write-region "" nil (expand-file-name "Beta" directory) nil 'silent)
                    (with-temp-buffer
                      (setq-local completion-regexp-list '("Alpha"))
                      (list (file-name-completion "" directory)
                            (file-name-all-completions "" directory))))
                (delete-directory directory t)))"#,
        expect_test::expect![[r#""OK (\"Alpha\" (\"Alpha\"))""#]],
    );
}
