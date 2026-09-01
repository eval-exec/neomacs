use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo97_org_element_lineage_root_to_leaf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ctx-type bold :lineage (paragraph section headline headline org-data) :lineage-depth 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\nText *bold* here.\n") (goto-char (point-min)) (search-forward "*bold*") (backward-char 2)
 (let* ((ctx (org-element-context)) (lineage (org-element-lineage ctx))) (list :ctx-type (org-element-type ctx)
  :lineage (mapcar #'org-element-type lineage) :lineage-depth (length lineage))))"##,
        expect,
    );
}
#[test]
fn combo97_org_babel_tangle_header_arg_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:tangle-file t :tangle-body nil :jump t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-tangle) (list
 :tangle-file (fboundp 'org-babel-tangle-file) :tangle-body (fboundp 'org-babel-tangle-body)
 :jump (fboundp 'org-babel-tangle-jump-to-org)))"##,
        expect,
    );
}
#[test]
fn combo97_org_export_block_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* H\n#+BEGIN_EXPORT latex\n\\textbf{B}\n#+END_EXPORT\n")
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t))) (push (list :ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo97_org_agenda_skip_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:skip-scheduled-delay-fbound t :skip-timestamp-fbound t :skip-deadline-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :skip-scheduled-delay-fbound (boundp 'org-agenda-skip-scheduled-delay-if-deadline)
 :skip-timestamp-fbound (boundp 'org-agenda-skip-timestamp-if-deadline-is-shown)
 :skip-deadline-fbound (boundp 'org-agenda-skip-deadline-prewarning-if-scheduled)))"##,
        expect,
    );
}
#[test]
fn combo97_org_table_rotate_mark_counter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (:to-lisp ((\"!\" \"a\" \"b\") hline (\"\" \"1\" \"2\") (\"#\" \"3\" \"4\")))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| ! | a | b |\n|---+---+---|\n|   | 1 | 2 |\n| # | 3 | 4 |\n") (goto-char (point-min))
 (list :to-lisp (org-table-to-lisp)))"##,
        expect,
    );
}
#[test]
fn combo97_org_clock_specific_time_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 7 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock)
 (let ((org-clock-persist nil)) (insert "* A\n* B\n") (goto-char (point-min))
  (org-clock-in nil) (org-clock-out nil nil) (search-forward "* B") (beginning-of-line)
  (org-clock-in nil) (org-clock-out nil nil) (goto-char (point-min))
  (insert "#+BEGIN: clocktable :maxlevel 2 :scope file :tstart \"<2024-01-01>\" :tend \"<2024-12-31>\" :block 2024\n#+END:\n")
  (goto-char (point-min)) (search-forward "#+BEGIN:") (beginning-of-line) (org-dblock-update)
  (list :ok (> (length (buffer-string)) 0)))))"##,
        expect,
    );
}
#[test]
fn combo97_org_babel_session_async_sync() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:async-fbound nil :session-fbound t :switch-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :async-fbound (boundp 'org-babel-async) :session-fbound (fboundp 'org-babel-initiate-session)
 :switch-fbound (fboundp 'org-babel-switch-to-session)))"##,
        expect,
    );
}
#[test]
fn combo97_org_footnote_fill_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:after \"A long footnote\\nreference[fn:1] that should\\nfill across lines.\\n[fn:1] A long footnote definition that should also fill nicely.\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (setq fill-column 30)
 (insert "A long footnote reference[fn:1] that should fill across lines.\n[fn:1] A long footnote definition that should also fill nicely.\n")
 (goto-char (point-min)) (org-fill-paragraph) (list :after (buffer-string)))"##,
        expect,
    );
}
#[test]
fn combo97_org_element_map_callback_with_info_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\n* C\n") (let* ((t (org-element-parse-buffer))
  (result (org-element-map t 'headline (lambda (h) (list (org-element-property :level h)
   (substring-no-properties (org-element-property :raw-value h)))) nil nil 'no-recursion)))
  (list :top-level result)))"##,
        expect,
    );
}
#[test]
fn combo97_org_mark_ring_push_and_goto() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:back-at-B nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\n* C\n") (goto-char (point-min)) (search-forward "** B") (beginning-of-line)
 (condition-case nil (org-mark-ring-push) (error nil))
 (goto-char (point-max)) (condition-case nil (org-mark-ring-goto) (error nil))
 (list :back-at-B (looking-at-p "\\*\\* B")))"##,
        expect,
    );
}
