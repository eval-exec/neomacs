use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo84_ob_css_makefile_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ob-css ob-css :ob-makefile ob-makefile :ob-latex ob-latex :ob-lisp ob-lisp)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (list
 :ob-css (condition-case nil (require 'ob-css) (error (featurep 'ob-css)))
 :ob-makefile (condition-case nil (require 'ob-makefile) (error (featurep 'ob-makefile)))
 :ob-latex (condition-case nil (require 'ob-latex) (error (featurep 'ob-latex)))
 :ob-lisp (condition-case nil (require 'ob-lisp) (error (featurep 'ob-lisp)))))"##,
        expect,
    );
}
#[test]
fn combo84_org_element_clock_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:type clock :status closed :duration \"1:00\" :value \"[2024-01-01 Mon 10:00]--[2024-01-01 Mon 11:00]\")""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(progn (require 'org-element)
 (let ((clock (org-element-create 'clock '(:status closed
   :value "[2024-01-01 Mon 10:00]--[2024-01-01 Mon 11:00]" :duration "1:00"))))
  (list :type (org-element-type clock) :status (org-element-property :status clock)
   :duration (org-element-property :duration clock) :value (org-element-property :value clock))))"##,
        expect,
    );
}
#[test]
fn combo84_org_log_note_clock_out() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:logbook-has-content t) (:clock-count 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock)
 (let ((org-clock-persist nil) (org-log-note-clock-out t)) (insert "* Task\n")
  (let ((r '())) (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
   (push (list :logbook-has-content (> (length (buffer-string)) 0)) r)
   (push (list :clock-count (length (org-element-map (org-element-parse-buffer) 'clock #'identity))) r)
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo84_org_entities_restricted_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:restricted-count nil :user-count 0 :ascii-explanatory nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-entities) (list
 :restricted-count (when (boundp 'org-entities-restricted) (length org-entities-restricted))
 :user-count (when (boundp 'org-entities-user) (length org-entities-user))
 :ascii-explanatory (when (boundp 'org-entities-ascii-explanatory) org-entities-ascii-explanatory)))"##,
        expect,
    );
}
#[test]
fn combo84_org_sparse_tree_prop_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:match (\"A\" \"B\" \"C\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n:PROPERTIES:\n:COLOR: red\n:END:\n* B\n:PROPERTIES:\n:COLOR: blue\n:END:\n* C\n")
 (let ((r '())) (goto-char (point-min))
  (condition-case nil (org-match-sparse-tree nil "COLOR={red}") (error nil))
  (push (list :match (org-element-map (org-element-parse-buffer nil t) 'headline
   (lambda (h) (substring-no-properties (org-element-property :raw-value h))))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo84_org_export_ignore_subtrees() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil))
  (insert "* A\n** B :ignore:\nBody B.\n** C\nBody C.\n")
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t)))
   (push (list :has-C (and out (string-match-p "Body C" out))) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo84_org_table_edit_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:current-val \"1\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| a | b |\n| 1 | 2 |\n") (let ((r '())) (goto-char (point-min))
  (forward-line 1) (forward-char 2) (push (list :current-val (org-table-get nil nil)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo84_org_agenda_file_to_front() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:file-to-front-fbound t :file-to-end-fbound nil :remove-file-fbound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :file-to-front-fbound (fboundp 'org-agenda-file-to-front) :file-to-end-fbound (fboundp 'org-agenda-file-to-end)
 :remove-file-fbound (fboundp 'org-agenda-remove-file)))"##,
        expect,
    );
}
#[test]
fn combo84_org_babel_detangle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:detangle-fbound t :tangle-fbound t :tangle-file-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-tangle) (list
 :detangle-fbound (fboundp 'org-babel-detangle) :tangle-fbound (fboundp 'org-babel-tangle)
 :tangle-file-fbound (fboundp 'org-babel-tangle-file)))"##,
        expect,
    );
}
#[test]
fn combo84_org_edit_src_abort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:edit-fbound t :exit-fbound t :abort-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-src) (list
 :edit-fbound (fboundp 'org-edit-src-code) :exit-fbound (fboundp 'org-edit-src-exit)
 :abort-fbound (fboundp 'org-edit-src-abort)))"##,
        expect,
    );
}
