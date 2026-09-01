use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo91_config_org_startup_folded() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:A-invis nil) (:B-invis org-fold-outline))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (let ((org-startup-folded t)) (insert "* A\n** B\n** C\n")
  (org-set-startup-visibility) (let ((r '())) (goto-char (point-min))
   (push (list :A-invis (get-char-property (point) 'invisible)) r)
   (search-forward "** B") (push (list :B-invis (get-char-property (point) 'invisible)) r)
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo91_config_org_todo_keywords_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (let ((org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE" "CANCELED"))))
  (insert "* TODO Task\n") (goto-char (point-min))
  (let ((r '())) (push :init (org-get-todo-state) r) (org-todo) (push (org-get-todo-state) r)
   (org-todo 'right) (push (org-get-todo-state) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo91_config_org_export_exclude_select_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-select-tags '("pub")) (org-export-exclude-tags '("draft"))
       (org-export-show-temporary-export-buffer nil))
  (insert "* A :pub:\n* B :draft:\n* C :pub:draft:\n* D\n")
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t)))
   (push (list :ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo91_config_org_highest_lowest_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:highest 65 :lowest 67 :default 66)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :highest (when (boundp 'org-highest-priority) org-highest-priority)
 :lowest (when (boundp 'org-lowest-priority) org-lowest-priority)
 :default (when (boundp 'org-default-priority) org-default-priority)))"##,
        expect,
    );
}
#[test]
fn combo91_config_org_cycle_max_level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:C-invis org-fold-outline))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (let ((org-startup-folded t) (org-cycle-max-level 2))
  (insert "* A\n** B\n*** C\n**** D\n")
  (org-set-startup-visibility) (let ((r '())) (goto-char (point-min))
   (search-forward "*** C") (push (list :C-invis (get-char-property (point) 'invisible)) r)
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo91_config_org_clock_into_drawer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:drawers (\"CLOCKLOGBOOK\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock)
 (let ((org-clock-persist nil) (org-clock-into-drawer "CLOCKLOGBOOK"))
  (insert "* Task\n") (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
  (let ((r '())) (push (list :drawers (mapcar (lambda (d) (org-element-property :drawer-name d))
   (org-element-map (org-element-parse-buffer) 'drawer #'identity))) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo91_config_org_pretty_entities_include_sub() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:pretty-fbound t :include-sub-fbound t :use-sub-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :pretty-fbound (boundp 'org-pretty-entities)
 :include-sub-fbound (boundp 'org-pretty-entities-include-sub-superscripts)
 :use-sub-fbound (boundp 'org-use-sub-superscripts)))"##,
        expect,
    );
}
#[test]
fn combo91_config_org_image_actual_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:image-fbound t :image-default t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :image-fbound (boundp 'org-image-actual-width)
 :image-default (when (boundp 'org-image-actual-width) org-image-actual-width)))"##,
        expect,
    );
}
#[test]
fn combo91_config_org_babel_default_inline_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:inline-fbound t :header-fbound t :results-fbound (:results . \"replace\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-core) (list
 :inline-fbound (boundp 'org-babel-default-inline-header-args)
 :header-fbound (boundp 'org-babel-default-header-args)
 :results-fbound (when (boundp 'org-babel-default-header-args)
  (assq :results org-babel-default-header-args))))"##,
        expect,
    );
}
#[test]
fn combo91_config_org_refile_allow_creating_parent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:allow-fbound t :use-outline-fbound t :use-cache-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-refile) (list
 :allow-fbound (boundp 'org-refile-allow-creating-parent-nodes)
 :use-outline-fbound (boundp 'org-refile-use-outline-path)
 :use-cache-fbound (boundp 'org-refile-use-cache)))"##,
        expect,
    );
}
