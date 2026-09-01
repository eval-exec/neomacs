use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};
#[test]
fn combo89_combined_props_clock_archive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:effort \"1:00\") (:clock-count 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock) (require 'org-archive)
 (let ((org-clock-persist nil)) (insert "* TODO Task\n") (goto-char (point-min))
  (org-set-property "EFFORT" "1:00") (org-clock-in nil) (org-clock-out nil nil)
  (let ((r '())) (push (list :effort (org-entry-get nil "EFFORT")) r)
   (push (list :clock-count (length (org-element-map (org-element-parse-buffer) 'clock #'identity))) r)
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo89_combined_babel_props_recalc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (30)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp)
 (let ((org-confirm-babel-evaluate nil)) (insert "* Calc\n:PROPERTIES:\n:X: 10\n:Y: 20\n:END:\n")
  (insert "#+begin_src emacs-lisp :results value :var x=(string-to-number (org-entry-get nil \"X\")) :var y=(string-to-number (org-entry-get nil \"Y\"))\n(+ x y)\n#+end_src\n")
  (let ((r '())) (goto-char (point-min)) (search-forward "#+begin_src")
   (push (org-babel-execute-src-block) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo89_combined_timestamp_export_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* Events\n<2024-03-15 Fri>\n- item\n| a |\n| 1 |\n")
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t)))
   (push (list :ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo89_combined_footnote_export_cite() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ox-ascii)
 (let ((org-export-show-temporary-export-buffer nil)) (insert "* Doc[fn:1]\n[fn:1] Note.\n")
  (let ((r '())) (let ((out (org-export-as 'ascii nil nil t)))
   (push (list :ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo89_combined_dblock_macro_entity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (user-error \"Before first headline at position 1 in buffer  *temp*\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock)
 (let ((org-clock-persist nil)) (insert "#+MACRO: title NeoMACS\n* {{{title}}}\n")
  (insert "\\alpha \\beta\n") (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
  (goto-char (point-min)) (insert "#+BEGIN: clocktable :maxlevel 2 :scope file\n#+END:\n")
  (goto-char (point-min)) (search-forward "#+BEGIN:") (beginning-of-line) (org-dblock-update)
  (let ((r '())) (push (list :ok (> (length (buffer-string)) 0)) r) (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo89_combined_src_block_list_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* Mixed\n- item\n| a | b |\n| 1 | 2 |\n#+begin_src emacs-lisp\n1\n#+end_src\n")
 (let ((r '())) (let* ((t (org-element-parse-buffer))) (push (list :items (length (org-element-map t 'item #'identity))) r)
  (push (list :tables (length (org-element-map t 'table #'identity))) r)
  (push (list :src (length (org-element-map t 'src-block #'identity))) r)) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo89_combined_link_timestamp_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (setting-constant t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* H\n:PROPERTIES:\n:URL: https://x.com\n:END:\n<2024-01-01 Mon>\n[[https://x.com][Link]]\n")
 (let ((r '())) (goto-char (point-min))
  (push (list :prop (org-entry-get nil "URL")) r) (push (list :ts (org-entry-get nil "SCHEDULED")) r)
  (let* ((t (org-element-parse-buffer)) (links (org-element-map t 'link #'identity)))
   (push (list :link-type (mapcar (lambda (l) (org-element-property :type l)) links)) r)) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo89_combined_checkbox_update_clock() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:cookie \"* Task [1/3]\\n:LOGBOOK:\\nCLOCK: [2026-06-15 Mon 12:00]--[2026-06-15 Mon 12:00] =>  0:00\\n:END:\\n- [X] a\\n- [ ] b\\n- [ ] c\\n\") (:clock 1))""#
    ]];
    crate::common::assert_oracle_parity_frozen_time_expect(
        r##"(with-temp-buffer (org-mode) (require 'org-clock)
 (let ((org-clock-persist nil)) (insert "* Task [/]\n- [X] a\n- [ ] b\n- [ ] c\n")
  (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil) (org-update-statistics-cookies t)
  (let ((r '())) (push (list :cookie (buffer-string)) r)
   (push (list :clock (length (org-element-map (org-element-parse-buffer) 'clock #'identity))) r)
   (nreverse r))))"##,
        expect,
    );
}
#[test]
fn combo89_combined_columnview_sort_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Nothing to sort\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode)
 (insert "* A\n:PROPERTIES:\n:WEIGHT: 3\n:END:\n* B\n:PROPERTIES:\n:WEIGHT: 1\n:END:\n* C\n:PROPERTIES:\n:WEIGHT: 2\n:END:\n")
 (let ((r '())) (goto-char (point-min)) (org-sort-entries nil ?r ?p "WEIGHT" nil #'string<)
  (push (list :sorted (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
   (org-element-map (org-element-parse-buffer) 'headline #'identity))) r) (nreverse r)))"##,
        expect,
    );
}
#[test]
fn combo89_combined_babel_noweb_export() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 (:export-ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer (org-mode) (require 'ob-emacs-lisp) (require 'ox-ascii)
 (let ((org-confirm-babel-evaluate nil) (org-export-show-temporary-export-buffer nil))
  (insert "#+name: code\n#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n\n")
  (insert "#+begin_src emacs-lisp :results value :noweb yes :exports both\n<<code>>\n#+end_src\n")
  (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")(search-forward "#+begin_src emacs-lisp")
  (let ((r '())) (push (org-babel-execute-src-block) r)
   (let ((out (org-export-as 'ascii nil nil t))) (push (list :export-ok (> (length out) 0)) r)) (nreverse r))))"##,
        expect,
    );
}
