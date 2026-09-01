//! Combo-strict-19 — probes for untested surfaces:
//! org-version, org-key bindings, org-faces extraction,
//! org-element-update-syntax, org-export-smart-quotes,
//! org-capture-templates-contexts, org-agenda-write,
//! org-clock-resolve, org-persist-load-all, org-babel
//! with ob-ditaa/ob-plantuml/ob-dot availability.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn strict_org_version() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:version-fbound t :version-string t :org-version-var t :org-git-version nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   :version-fbound (fboundp 'org-version)
   :version-string (when (fboundp 'org-version)
                     (stringp (org-version)))
   :org-version-var (when (boundp 'org-version) (stringp org-version))
   :org-git-version (when (boundp 'org-git-version) (stringp org-git-version))
   ))"##,
        expect,
    );
}

#[test]
fn strict_org_key_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:mode-map-bound t :mode-map-length 38 :struct-mode-map-bound nil :struct++-mode-map-bound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (list
   :mode-map-bound (boundp 'org-mode-map)
   :mode-map-length (when (boundp 'org-mode-map) (length (keymap-canonicalize org-mode-map)))
   :struct-mode-map-bound (boundp 'orgstruct-mode-map)
   :struct++-mode-map-bound (boundp 'orgstruct++-mode-map)
   ))"##,
        expect,
    );
}

#[test]
fn strict_org_faces_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:level-1-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] :level-2-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] :todo-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] :done-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] :headline-done-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] :date-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] :link-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] :block-begin-line-face [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-faces)
  (list
   :level-1-face (facep 'org-level-1)
   :level-2-face (facep 'org-level-2)
   :todo-face (facep 'org-todo)
   :done-face (facep 'org-done)
   :headline-done-face (facep 'org-headline-done)
   :date-face (facep 'org-date)
   :link-face (facep 'org-link)
   :block-begin-line-face (facep 'org-block-begin-line)
   ))"##,
        expect,
    );
}

#[test]
fn strict_element_update_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:update-syntax-fbound nil :parse-buffer-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-element)
  (list
   :update-syntax-fbound (fboundp 'org-element--update-syntax)
   :parse-buffer-fbound (fboundp 'org-element-parse-buffer)
   ))"##,
        expect,
    );
}

#[test]
fn strict_export_smart_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:smart-quotes-fbound t :smart-quotes-val nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox)
  (list
   :smart-quotes-fbound (boundp 'org-export-with-smart-quotes)
   :smart-quotes-val (when (boundp 'org-export-with-smart-quotes)
                       org-export-with-smart-quotes)
   ))"##,
        expect,
    );
}

#[test]
fn strict_capture_templates_contexts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:templates-contexts-fbound t :templates-bound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-capture)
  (list
   :templates-contexts-fbound (boundp 'org-capture-templates-contexts)
   :templates-bound (boundp 'org-capture-templates)
   ))"##,
        expect,
    );
}

#[test]
fn strict_agenda_write() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:write-fbound t :filter-preset-fbound t :category-filter-fbound t :tag-filter-fbound t :effort-filter-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-agenda)
  (list
   :write-fbound (fboundp 'org-agenda-write)
   :filter-preset-fbound (boundp 'org-agenda-filter-preset)
   :category-filter-fbound (boundp 'org-agenda-category-filter)
   :tag-filter-fbound (boundp 'org-agenda-tag-filter)
   :effort-filter-fbound (boundp 'org-agenda-effort-filter)
   ))"##,
        expect,
    );
}

#[test]
fn strict_clock_resolve() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:resolve-fbound t :resolve-clocks-fbound t :idle-time-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-clock)
  (list
   :resolve-fbound (fboundp 'org-clock-resolve)
   :resolve-clocks-fbound (fboundp 'org-resolve-clocks)
   :idle-time-fbound (fboundp 'org-user-idle-seconds)
   ))"##,
        expect,
    );
}

#[test]
fn strict_persist_load_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:load-all-fbound t :register-fbound t :read-fbound t :write-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-persist)
  (list
   :load-all-fbound (fboundp 'org-persist-load-all)
   :register-fbound (fboundp 'org-persist-register)
   :read-fbound (fboundp 'org-persist-read)
   :write-fbound (fboundp 'org-persist-write)
   ))"##,
        expect,
    );
}

#[test]
fn strict_babel_diagram_backends() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ob-ditaa ob-ditaa :ob-plantuml ob-plantuml :ob-dot ob-dot :ob-gnuplot ob-gnuplot)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (list
   :ob-ditaa (condition-case nil (require 'ob-ditaa) (error (featurep 'ob-ditaa)))
   :ob-plantuml (condition-case nil (require 'ob-plantuml) (error (featurep 'ob-plantuml)))
   :ob-dot (condition-case nil (require 'ob-dot) (error (featurep 'ob-dot)))
   :ob-gnuplot (condition-case nil (require 'ob-gnuplot) (error (featurep 'ob-gnuplot)))
   ))"##,
        expect,
    );
}
