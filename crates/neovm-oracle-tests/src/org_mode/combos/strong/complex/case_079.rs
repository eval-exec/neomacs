use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo79_persist_write_all_gc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:write-all-fbound t :gc-fbound t :load-all-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-persist)
 (list :write-all-fbound (fboundp 'org-persist-write-all) :gc-fbound (fboundp 'org-persist-gc)
  :load-all-fbound (fboundp 'org-persist-load-all)))"##,
        expect,
    );
}
#[test]
fn combo79_protocol_check_filename() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:check-filename-fbound t :protocol-fbound nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-protocol)
 (list :check-filename-fbound (fboundp 'org-protocol-check-filename-for-protocol)
  :protocol-fbound (fboundp 'org-protocol-do-capture)))"##,
        expect,
    );
}
#[test]
fn combo79_publish_attachment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:attachment-fbound t :org-publish-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox-publish)
 (list :attachment-fbound (fboundp 'org-publish-attachment)
  :org-publish-fbound (fboundp 'org-publish-org-to)))"##,
        expect,
    );
}
#[test]
fn combo79_refile_check_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:check-fbound t :get-location-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-refile)
 (list :check-fbound (fboundp 'org-refile-check-position)
  :get-location-fbound (fboundp 'org-refile-get-location)))"##,
        expect,
    );
}
#[test]
fn combo79_timer_set_mode_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:mode-line-fbound t :item-fbound t :start-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-timer)
 (list :mode-line-fbound (fboundp 'org-timer-set-mode-line)
  :item-fbound (fboundp 'org-timer-item)
  :start-fbound (fboundp 'org-timer-start)))"##,
        expect,
    );
}
#[test]
fn combo79_element_at_point_no_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:A-at headline) (:A-no-ctx headline) (:plain-at paragraph))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-element)
 (insert "* A\n** B\nPlain.\n* C\n")
 (let ((r '())) (goto-char (point-min)) (push (list :A-at (org-element-type (org-element-at-point))) r)
  (push (list :A-no-ctx (when (fboundp 'org-element-at-point-no-context)
   (org-element-type (org-element-at-point-no-context)))) r)
  (search-forward "Plain.") (push (list :plain-at (org-element-type (org-element-at-point))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo79_export_prune_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:prune-fbound t) (:pruned-count 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox)
 (insert "* A\n** B\n** C :noexport:\n* D\n")
 (let ((r '())) (let* ((tree (org-element-parse-buffer)) (info (org-export-get-environment))
   (resolved (when (fboundp 'org-export--prune-tree) (org-export--prune-tree tree info))))
  (push (list :prune-fbound (fboundp 'org-export--prune-tree)) r)
  (push (list :pruned-count (when resolved (length (org-element-map resolved 'headline #'identity)))) r))
 (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo79_org_indent_to_virtual() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:enabled t) (:headlines 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-indent)
 (insert "* H\n** H1\nBody.\n")
 (let ((r '())) (goto-char (point-min)) (condition-case nil
  (when (fboundp 'org-indent-mode) (org-indent-mode 1) (push (list :enabled t) r)) (error nil))
  (push (list :headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo79_babel_results_latex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"$x^2$\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+begin_src emacs-lisp :results latex\n\"$x^2$\"\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo79_org_babel_remove_result() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:remove-result-fbound t :remove-inline-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :remove-result-fbound (fboundp 'org-babel-remove-result)
 :remove-inline-fbound (fboundp 'org-babel-remove-inline-result)))"##,
        expect,
    );
}
