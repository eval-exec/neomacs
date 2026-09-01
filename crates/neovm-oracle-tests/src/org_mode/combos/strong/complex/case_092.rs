use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo92_config_org_babel_min_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:min-lines-fbound t :min-lines 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :min-lines-fbound (boundp 'org-babel-min-lines-for-block-output) :min-lines (when (boundp 'org-babel-min-lines-for-block-output) org-babel-min-lines-for-block-output)))"##,
        expect,
    );
}
#[test]
fn combo92_config_org_list_allow_alphabetical() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:allow-alpha-fbound t :alpha-default nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :allow-alpha-fbound (boundp 'org-list-allow-alphabetical) :alpha-default (when (boundp 'org-list-allow-alphabetical) org-list-allow-alphabetical)))"##,
        expect,
    );
}
#[test]
fn combo92_config_org_table_number_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:number-regexp-bound t :number-fraction-bound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :number-regexp-bound (boundp 'org-table-number-regexp) :number-fraction-bound (boundp 'org-table-number-fraction)))"##,
        expect,
    );
}
#[test]
fn combo92_config_org_timestamp_rounding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:round-minutes-fbound t :round-default (0 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :round-minutes-fbound (boundp 'org-time-stamp-rounding-minutes) :round-default (when (boundp 'org-time-stamp-rounding-minutes) org-time-stamp-rounding-minutes)))"##,
        expect,
    );
}
#[test]
fn combo92_config_org_export_backends_available() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:odt-loaded t :beamer-loaded t :total-backends 6 :default beamer)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox) (condition-case nil (require 'ox-odt) (error nil))
 (condition-case nil (require 'ox-beamer) (error nil)) (list :odt-loaded (featurep 'ox-odt) :beamer-loaded (featurep 'ox-beamer)
  :total-backends (length org-export-registered-backends) :default (org-export-backend-name (car org-export-registered-backends))))"##,
        expect,
    );
}
#[test]
fn combo92_config_org_babel_python_session() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (condition-case nil (require 'ob-python) (error nil))
 (let ((org-confirm-babel-evaluate nil)) (condition-case nil (insert "#+begin_src python :results output :session py91\nprint(1+2)\n#+end_src\n")
  (goto-char (point-min)) (search-forward "#+begin_src python") (push (org-babel-execute-src-block) nil) (list :python-ok t))
 (error (list :python-error t)))"##,
        expect,
    );
}
#[test]
fn combo92_config_org_agenda_entry_types_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:entry-types-bound t :default-types (:deadline :scheduled :timestamp :sexp))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :entry-types-bound (boundp 'org-agenda-entry-types) :default-types (when (boundp 'org-agenda-entry-types) org-agenda-entry-types)))"##,
        expect,
    );
}
#[test]
fn combo92_config_org_export_latex_inputenc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:inputenc-fbound t :packages-fbound t :packages 13)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox-latex) (list
 :inputenc-fbound (boundp 'org-latex-inputenc-alist) :packages-fbound (boundp 'org-latex-default-packages-alist)
 :packages (when (boundp 'org-latex-default-packages-alist) (length org-latex-default-packages-alist))))"##,
        expect,
    );
}
#[test]
fn combo92_config_org_entities_user_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:user-fbound t :user-default nil :ascii-fbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-entities) (list
 :user-fbound (boundp 'org-entities-user) :user-default (when (boundp 'org-entities-user) org-entities-user)
 :ascii-fbound (boundp 'org-entities-ascii-explanatory)))"##,
        expect,
    );
}
#[test]
fn combo92_config_org_startup_options_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:startup-folded-bound t :startup-align-bound t :startup-indented-bound t :startup-truncated-bound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :startup-folded-bound (boundp 'org-startup-folded) :startup-align-bound (boundp 'org-startup-align-all-tables)
 :startup-indented-bound (boundp 'org-startup-indented) :startup-truncated-bound (boundp 'org-startup-truncated)))"##,
        expect,
    );
}
