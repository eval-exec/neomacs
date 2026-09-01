use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo101_org_reveal_full_tree() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:overview-vis nil) (:reveal-A1-vis nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** A1\nBody A1.\n** A2\nBody A2.\n* B\n** B1\nBody B1.\n")
 (let ((r '())) (goto-char (point-min)) (org-overview)
  (push (list :overview-vis (get-char-property (point) 'invisible)) r)
  (search-forward "** A1") (beginning-of-line) (condition-case nil (org-reveal) (error nil))
  (push (list :reveal-A1-vis (get-char-property (point) 'invisible)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo101_org_table_auto_fill_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:aligned t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (setq fill-column 10) (insert "| a very long cell |\n")
 (goto-char (point-min)) (condition-case nil (org-table-align) (error nil))
 (list :aligned (org-at-table-p)))"##,
        expect,
    );
}
#[test]
fn combo101_org_babel_multi_lang_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ob-sh\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp) (require 'ob-sh)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+name: el\n#+begin_src emacs-lisp :results output\n(princ \"data\")\n#+end_src\n\n")
  (insert "#+begin_src sh :results output :var x=el\necho \"$x processed\"\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp :results output")
   (push (org-babel-execute-src-block) r) (search-forward "#+begin_src sh")
   (condition-case e (push (org-babel-execute-src-block) r) (error (push (list :err (car e)) r)))
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo101_org_export_ignore_headings_completely() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:has-B 11) (:no-A t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil) (org-export-exclude-tags '("ignore")))
  (insert "* A :ignore:\nBody A.\n* B\nBody B.\n")
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t)))
   (push (list :has-B (and out (string-match-p "Body B" out))) r)
   (push (list :no-A (and out (not (string-match-p "Body A" out)))) r))
  (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo101_org_update_todo_dependencies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:ok :c3-ok)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (let ((org-enforce-todo-dependencies t))
  (insert "* TODO P\n** TODO C1\n** DONE C2\n** TODO C3\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "** TODO C1") (beginning-of-line)
   (condition-case nil (progn (org-todo "DONE") (push :ok r)) (error (push :blocked-by-parent r)))
   (goto-char (point-min)) (org-todo "DONE")
   (goto-char (point-min)) (search-forward "** TODO C3") (beginning-of-line)
   (condition-case nil (progn (org-todo "DONE") (push :c3-ok r)) (error (push :c3-blocked r)))
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo101_org_entry_clock_total() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:total 0 :clock-count 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock)
 (let ((org-clock-persist nil)) (insert "* Task\n** Sub\n")
  (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
  (search-forward "** Sub") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
  (goto-char (point-min))
  (list :total (org-clock-sum-current-item) :clock-count (length (org-element-map (org-element-parse-buffer) 'clock #'identity)))))"##,
        expect,
    );
}
#[test]
fn combo101_org_element_contents_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\n:PROPERTIES:\n:A: 1\n:END:\nBody.\n")
 (let* ((t (org-element-parse-buffer)) (h (car (org-element-map t 'headline #'identity)))
  (contents (org-element-contents h)) (r '()))
  (push (list :contents-count (length contents)) r)
  (push (list :section-type (when (car contents) (org-element-type (car contents)))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo101_org_babel_src_block_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:lang \"emacs-lisp\" :body \"(+ 1 2)\" :params ((:colname-names) (:rowname-names) (:result-params \"value\" \"replace\") (:result-type . value) (:results . \"value replace\") (:exports . \"code\") (:lexical . \"no\") (:tangle . \"no\") (:hlines . \"no\") (:noweb . \"no\") (:cache . \"no\") (:session . \"none\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-core)
 (insert "#+begin_src emacs-lisp :results value\n(+ 1 2)\n#+end_src\n")
 (goto-char (point-min)) (search-forward "#+begin_src")
 (let ((info (org-babel-get-src-block-info)))
  (list :lang (car info) :body (nth 1 info) :params (nth 2 info))))"##,
        expect,
    );
}
#[test]
fn combo101_org_time_stamp_active_range_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-timestamp-parser)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org)
 (let ((ts (org-timestamp-from-string "<2024-01-15 Mon>--<2024-01-20 Sat>")))
  (list :type (org-element-property :type ts)
   :start-day (org-element-property :day-start ts)
   :end-day (org-element-property :day-end ts)
   :format-start (org-timestamp-format ts "%Y-%m-%d")
   :duration-days (- (org-element-property :day-end ts) (org-element-property :day-start ts)))))"##,
        expect,
    );
}
#[test]
fn combo101_org_fill_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:filled \"aaaa bbbb cccc dddd\\neeee ffff gggg hhhh\\niiii jjjj kkkk llll\\nmmmm\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (setq fill-column 20)
 (insert "aaaa bbbb cccc dddd eeee ffff gggg\nhhhh iiii jjjj kkkk llll mmmm\n")
 (goto-char (point-min)) (condition-case nil (fill-paragraph) (error nil))
 (list :filled (buffer-string)))"##,
        expect,
    );
}
