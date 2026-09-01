use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo94_org_ctags_subject() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:loaded t :tag-fbound t :create-fbound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (condition-case nil (require 'org-ctags) (error nil))
 (list :loaded (featurep 'org-ctags) :tag-fbound (fboundp 'org-ctags-find-tag)
  :create-fbound (fboundp 'org-ctags-create-tags)))"##,
        expect,
    );
}
#[test]
fn combo94_org_plot_gnuplot_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:gnuplot-fbound t :options-fbound t :presets-bound t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-plot) (list
 :gnuplot-fbound (fboundp 'org-plot/gnuplot) :options-fbound (fboundp 'org-plot/gnuplot-script)
 :presets-bound (boundp 'org-plot/preset-plot-types)))"##,
        expect,
    );
}
#[test]
fn combo94_org_agenda_filter_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:regexp-fbound t :tag-preset-fbound t :cat-preset-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :regexp-fbound (fboundp 'org-agenda-filter-by-regexp) :tag-preset-fbound (boundp 'org-agenda-tag-filter-preset)
 :cat-preset-fbound (boundp 'org-agenda-category-filter-preset)))"##,
        expect,
    );
}
#[test]
fn combo94_org_element_parse_with_granularity_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:hl-bolds 0) (:el-bold 0) (:ob-bold 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n*B* /I/ text.\n| a |\n| 1 |\n")
 (let ((r '())) (let* ((t-headline (org-element-parse-buffer 'headline))
  (t-greater (org-element-parse-buffer 'greater-element))
  (t-element (org-element-parse-buffer 'element))
  (t-object (org-element-parse-buffer 'object)))
  (push (list :hl-bolds (length (org-element-map t-headline 'bold #'identity))) r)
  (push (list :el-bold (length (org-element-map t-element 'bold #'identity))) r)
  (push (list :ob-bold (length (org-element-map t-object 'bold #'identity))) r)) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo94_org_link_type_protocol_handlers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable org-link-types)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ol) (list
 :link-types (sort (mapcar #'car org-link-types) #'string-lessp)
 :link-params (mapcar #'car org-link-parameters)))"##,
        expect,
    );
}
#[test]
fn combo94_org_babel_row_col_indexing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp) (require 'ob-ref)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+name: data\n| a | b |\n| 1 | 2 |\n| 3 | 4 |\n\n")
  (insert "#+begin_src emacs-lisp :results value :var d=data[1,1]\nd\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo94_org_timestamp_special_dates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org) (list
 :feb28 (let ((ts (org-timestamp-from-string "<2024-02-28 Wed>")))
  (list (org-element-property :year-start ts) (org-element-property :month-start ts) (org-element-property :day-start ts)))
 :leap-feb29 (let ((ts (org-timestamp-from-string "<2024-02-29 Thu>")))
  (list (org-element-property :year-start ts) (org-element-property :month-start ts) (org-element-property :day-start ts)))
 :dec31 (let ((ts (org-timestamp-from-string "<2024-12-31 Tue>")))
  (list (org-element-property :year-start ts) (org-element-property :month-start ts) (org-element-property :day-start ts))))))"##,
        expect,
    );
}
#[test]
fn combo94_org_block_switch_headers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "#+begin_src emacs-lisp -n -r -l \"(+ 1 2)\"\n(+ 1 2)\n#+end_src\n")
 (let ((r '())) (let* ((t (org-element-parse-buffer)) (s (car (org-element-map t 'src-block #'identity))))
  (when s (push (list :lang (org-element-property :language s)) r)
   (push (list :switches (org-element-property :switches s)) r)
   (push (list :parameters (org-element-property :parameters s)) r)
   (push (list :number-lines (org-element-property :number-lines s)) r))) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo94_org_export_body_only_all_backends_compact() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* B\nBody.\n")
  (let ((r '())) (dolist (b '(ascii html latex)) (condition-case nil
   (let ((out (org-export-as b nil nil t t))) (push (list b (> (length out) 0)) r)) (error nil)))
  (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo94_org_entry_properties_special_and_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (2 . 2) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* TODO Task\n:PROPERTIES:\n:A: 10\n:END:\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :special (org-entry-properties nil 'special)) r)
  (push (list :standard (org-entry-properties nil 'standard)) r)
  (push (list :all (sort (org-entry-properties nil t) (lambda (a b) (string-lessp (car a) (car b)))))) r)
  (nreverse r)))"##,
        expect,
    );
}
