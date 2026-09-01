//! Strong combo-complex-63 oracle tests — babel multi-language
//! (python/R), agenda buffer creation (non-interactive),
//! export filter hooks, org-babel-lob call lines, org-depend
//! blocking, custom export transcoders, org-export-dispatch
//! backend lookup, and org-babel with :post header.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo63_babel_python_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ob-python-loaded t) \"hello from python\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (condition-case nil
      (require 'ob-python)
    (error nil))
  (let ((org-confirm-babel-evaluate nil)
        (r '()))
    (push (list :ob-python-loaded (featurep 'ob-python)) r)
    (condition-case nil
        (progn
          (insert "#+begin_src python :results output\nprint('hello from python')\n#+end_src\n")
          (goto-char (point-min)) (search-forward "#+begin_src python")
          (push (org-babel-execute-src-block) r))
      (error (push (list :python-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo63_babel_r_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:ob-R-loaded t) (:ob-R-fbound t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (condition-case nil
      (require 'ob-R)
    (error nil))
  (let ((org-confirm-babel-evaluate nil)
        (r '()))
    (push (list :ob-R-loaded (featurep 'ob-R)) r)
    (push (list :ob-R-fbound (fboundp 'org-babel-execute:R)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo63_agenda_todo_list_noninteractive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:todo-list-fbound t) (:agenda-fbound t) (:todo-count 3) (:agenda-error t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-agenda)
  (insert "* TODO A :work:\n** TODO B :work:\n* DONE C :home:\n* TODO D :urgent:\n")
  (let ((r '()))
    ;; org-todo-list
    (condition-case nil
        (progn
          (push (list :todo-list-fbound (fboundp 'org-todo-list)) r)
          (push (list :agenda-fbound (fboundp 'org-agenda)) r)
          ;; get todo entries via map (agenda-like)
          (push (list :todo-count (length (org-map-entries
                                           (lambda () (org-get-heading t t t t))
                                           "TODO=\"TODO\""))) r)
          ;; get tags view via map
          (push (list :work-tags (org-map-entries
                                  (lambda () (list (org-get-heading t t t t)
                                                   (org-get-tags)))
                                  "work")))
                r)
      (error (push (list :agenda-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo63_export_filter_hooks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:export-ok t) (:has-prefix 19))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-export-filter-headline-functions
         '((lambda (contents backend info)
             (replace-regexp-in-string "H" "HEADING" contents))))
        (org-export-filter-paragraph-functions
         '((lambda (contents backend info)
             (concat "PREFIX: " contents)))))
    (insert "* H1\nParagraph.\n")
    (let ((r '()))
      (condition-case nil
          (let ((out (org-export-as 'ascii nil nil t)))
            (push (list :export-ok (> (length out) 0)) r)
            (push (list :has-prefix (and out (string-match-p "PREFIX" out))) r))
        (error (push (list :export-error t) r)))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo63_babel_lob_call_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (require 'ob-lob)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+name: square\n")
    (insert "#+begin_src emacs-lisp :results value :var x=0\n(* x x)\n#+end_src\n\n")
    (insert "#+call: square(x=7)\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp") (org-babel-execute-src-block)
      (goto-char (point-min))
      (search-forward "#+call: square")
      (condition-case e
          (push (org-babel-lob-execute-maybe) r)
        (error (push (list :lob-error (car e)) r)))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo63_depend_blocking_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"org-depend\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-depend)
  (insert "* TODO Parent\n** TODO Child\n")
  (let ((r '()))
    (push (list :depend-fbound (fboundp 'org-depend-block-todo)) r)
    ;; check org-depend triggers
    (push (list :trigger-fbound (fboundp 'org-depend-trigger-todo)) r)
    ;; todo chain checking
    (goto-char (point-min))
    (search-forward "** TODO Child") (beginning-of-line)
    (condition-case nil
        (let ((blocked (when (fboundp 'org-depend-block-todo)
                         (org-depend-block-todo (org-get-todo-state)))))
          (push (list :blocked-by-depend blocked) r))
      (error (push (list :depend-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo63_export_custom_transcoder_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:backend-error t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox)
  (let ((r '()))
    ;; create backend with custom transcoders
    (condition-case nil
        (let* ((test-b (org-export-create-backend
                        :parent 'ascii
                        :name 'custom-test
                        :transcoders
                        '((bold . (lambda (bold contents info) (concat "**" contents "**")))
                          (italic . (lambda (italic contents info) (concat "//" contents "//"))))))
               (exported (org-export-string-as "*bold* /italic/." 'custom-test t)))
          (push (list :backend-created t) r)
          (push (list :export-ok (> (length exported) 0)) r)
          (push (list :has-bold-marker (and (stringp exported) (string-match-p "\\*\\*" exported))) r))
      (error (push (list :backend-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo63_babel_post_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable val)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+name: doubler\n")
    (insert "#+begin_src emacs-lisp :results value :var n=0\n(* n 2)\n#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results value :var val=doubler(n=21) :post (* val 2)\n")
    (insert "val\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+name: doubler")
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo63_agenda_tags_view_noninteractive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:tags-view-fbound t) (:urgent-headings (\"B\" \"D\")) (:work+urgent (\"B\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-agenda)
  (insert "* TODO A :work:\n** DONE B :work:urgent:\n* TODO C :home:\n* DONE D :urgent:\n")
  (let ((r '()))
    ;; org-tags-view
    (push (list :tags-view-fbound (fboundp 'org-tags-view)) r)
    ;; get tag-based map entries
    (push (list :urgent-headings (org-map-entries
                                  (lambda () (org-get-heading t t t t))
                                  "urgent")) r)
    (push (list :work+urgent (org-map-entries
                              (lambda () (org-get-heading t t t t))
                              "work+urgent")) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo63_export_icalendar_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:export-ok nil) (:has-vevent nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-icalendar)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-icalendar-combined-agenda-file "/dev/null"))
    (insert "* Event\nSCHEDULED: <2024-07-01 Mon>\n")
    (let ((r '()))
      (condition-case e
          (let ((out (org-export-as 'icalendar nil nil t)))
            (push (list :export-ok (and out (> (length out) 0))) r)
            (push (list :has-vevent (and out (string-match-p "VEVENT" out))) r))
        (error (push (list :ical-error (car e)) r)))
      (nreverse r))))"##,
        expect,
    );
}
