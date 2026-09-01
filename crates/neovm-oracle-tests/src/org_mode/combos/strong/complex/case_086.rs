use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo86_org_unindent_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:col 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* A\n  Body.\n")
 (let ((r '())) (goto-char (point-max)) (push (list :col (current-column)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo86_org_block_bounds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:src-count 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+BEGIN_SRC emacs-lisp\n(+ 1 2)\n#+END_SRC\n")
 (let ((r '())) (goto-char (point-min)) (push (list :src-count (length (org-element-map (org-element-parse-buffer) 'src-block #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo86_org_sparse_tree_regex_date() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:deadline-match (\"A\" \"B\" \"C\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\nDEADLINE: <2024-01-15>\n* B\nSCHEDULED: <2024-06-01>\n* C\n")
 (let ((r '())) (goto-char (point-min)) (condition-case nil (org-match-sparse-tree nil "DEADLINE<=\"<2024-02-01>\"")
  (error nil)) (push (list :deadline-match (org-element-map (org-element-parse-buffer nil t) 'headline
   (lambda (h) (substring-no-properties (org-element-property :raw-value h))))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo86_org_update_all_statistics() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:after \"* [1/2]\\n- [X] one\\n- [ ] two\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* [/]\n- [X] one\n- [ ] two\n")
 (let ((r '())) (goto-char (point-min)) (org-update-statistics-cookies t)
  (push (list :after (buffer-string)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo86_org_babel_noweb_tangle_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:tangle-only-fbound t :noweb-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ob-tangle) (list
 :tangle-only-fbound (fboundp 'org-babel-tangle-publish) :noweb-fbound (fboundp 'org-babel-expand-noweb-references)))"##,
        expect,
    );
}
#[test]
fn combo86_org_entities_ascii_latex_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:ascii \"&alpha;\" :latin1 \"alpha\" :utf8 \"alpha\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-entities) (let ((e (org-entity-get "alpha")))
 (list :ascii (nth 3 e) :latin1 (nth 4 e) :utf8 (nth 5 e))))"##,
        expect,
    );
}
#[test]
fn combo86_org_agenda_span_day_week_month() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:possible-spans (day week month year) :current-span week)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :possible-spans (when (boundp 'org-agenda-span) '(day week month year))
 :current-span (when (boundp 'org-agenda-span) org-agenda-span)))"##,
        expect,
    );
}
#[test]
fn combo86_org_column_view_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:cleanup-fbound t :compute-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-colview) (list
 :cleanup-fbound (fboundp 'org-columns-remove-overlays) :compute-fbound (fboundp 'org-columns-compute)))"##,
        expect,
    );
}
#[test]
fn combo86_org_table_reference_remote_2level() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range [nil 0 1] 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+name: a\n| 10 |\n| 20 |\n\n#+name: b\n| total |\n|       |\n")
 (insert "#+TBLFM: @2$1=vsum(remote(a,@2$1..@3$1))\n")
 (let ((r '())) (goto-char (point-min)) (search-forward "b") (forward-line) (forward-line)
  (org-table-recalculate t) (push (list :result (org-table-get nil nil)) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo86_org_shiftmeta_right_left() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:levels (1 2 1)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (insert "* A\n** B\n*** C\n")
 (let ((r '())) (goto-char (point-min)) (search-forward "*** C") (beginning-of-line)
  (condition-case nil (org-shiftmetaleft) (error nil)) (condition-case nil (org-shiftmetaleft) (error nil))
  (push (list :levels (mapcar (lambda (h) (org-element-property :level h))
   (org-element-map (org-element-parse-buffer) 'headline #'identity))) r) (nreverse r)))"##,
        expect,
    );
}
