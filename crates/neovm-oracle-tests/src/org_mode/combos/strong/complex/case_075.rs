use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo75_babel_ob_forth_screen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:ob-forth ob-forth :ob-screen ob-screen)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (list
 :ob-forth (condition-case nil (require 'ob-forth) (error (featurep 'ob-forth)))
 :ob-screen (condition-case nil (require 'ob-screen) (error (featurep 'ob-screen)))
 ))"##,
        expect,
    );
}
#[test]
fn combo75_agenda_check_deadline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable r)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-agenda)
 (insert "* TODO A :work:\nDEADLINE: <2024-01-15 Mon>\n* TODO B :urgent:\nDEADLINE: <2024-01-10 Thu>\n")
 (let ((r '()))
  (push (list :deadline-entries (length (org-map-entries (lambda () (org-get-heading t t t t))
    "DEADLINE<>\"\""))) r)
  (push (list :urgent-deadline (length (org-map-entries (lambda () (org-get-heading t t t t))
    "DEADLINE<=\"<2024-01-12>\""))) r))
 (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo75_element_create_radio_target() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:type radio-target) (:value \"my-target\") (:interpreted nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-element)
 (let* ((rt (org-element-create 'radio-target '(:value "my-target")))
        (str (substring-no-properties (org-element-interpret-data rt)))
        (r '()))
  (push (list :type (org-element-type rt)) r)
  (push (list :value (org-element-property :value rt)) r)
  (push (list :interpreted (string-match-p "my-target" str)) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo75_org_publish_sitemap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:sitemap-fbound t :sitemap-sort-folders-bound t :sitemap-date-format-bound nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'ox-publish) (list
 :sitemap-fbound (fboundp 'org-publish-sitemap)
 :sitemap-sort-folders-bound (boundp 'org-publish-sitemap-sort-folders)
 :sitemap-date-format-bound (boundp 'org-publish-sitemap-date-format)
 ))"##,
        expect,
    );
}
#[test]
fn combo75_babel_with_headers_var_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil))
  (insert "#+begin_src emacs-lisp :results value :var x=1 :var y=2 :var x=99\nx\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo75_element_full_interpret_roundtrip_3x() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\nPara *bold* here.\n| 5 |\n")
 (let ((r '()) (s (buffer-string)))
  (dotimes (i 3)
   (let* ((t (org-element-parse-buffer)) (i2 (substring-no-properties (org-element-interpret-data t)))
          (t2 (with-temp-buffer (org-mode) (insert i2) (goto-char (point-min)) (org-element-parse-buffer))))
    (push (list (intern (format ":iter%d-headlines" i)) (length (org-element-map t2 'headline #'identity))) r)))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo75_org_shift_right_left_heading() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"** B\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n** B\nBody B.\n*** C\n")
 (let ((r '())) (goto-char (point-min))
  (search-forward "** B") (beginning-of-line) (org-shiftright)
  (push (list :after-right (mapcar (lambda (h) (list (org-element-property :level h)
    (substring-no-properties (org-element-property :raw-value h))))
    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (goto-char (point-min)) (search-forward "** B") (beginning-of-line) (org-shiftleft)
  (push (list :after-left (mapcar (lambda (h) (list (org-element-property :level h)
    (substring-no-properties (org-element-property :raw-value h))))
    (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo75_org_table_analyze() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:analyze-fbound t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "| a | b |\n| 1 | 2 |\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :analyze-fbound (fboundp 'org-table-analyze)) r)
  (when (fboundp 'org-table-analyze)
    (condition-case nil (let ((result (org-table-analyze)) (push (list :analyzed t) r))) (error nil)))
  (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo75_org_agenda_entry_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:entry-text-fbound nil :get-priority-fbound nil :format-fbound t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'org-agenda) (list
 :entry-text-fbound (fboundp 'org-agenda-entry-text)
 :get-priority-fbound (fboundp 'org-agenda-get-priority)
 :format-fbound (fboundp 'org-agenda-format-item)
 ))"##,
        expect,
    );
}
#[test]
fn combo75_org_export_to_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* X\n")
  (list :to-buffer-fbound (fboundp 'org-export-to-buffer)
   :export-ok (condition-case nil (let ((out (org-export-as 'ascii nil nil t))) (and out (> (length out) 0))) (error nil)))
  )))"##,
        expect,
    );
}
