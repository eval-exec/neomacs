//! Strong combo-complex-60 oracle tests — complex cross-system
//! workflows: babel with :var referencing named tables, agenda
//! custom command construction, capture template multi-target,
//! org-babel-tangle file writing, org-persist basic operations,
//! org-mobile push/pull, org-plot table extraction, org-babel
//! ob-sh integration, org-export with #+INCLUDE, and org-cycle
//! with STARTUP visibility settings.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo60_babel_var_table_reference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((30 70) (:result-count 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+name: mytable\n| 10 | 20 |\n| 30 | 40 |\n\n")
    (insert "#+begin_src emacs-lisp :results value :var data=mytable\n")
    (insert "(mapcar (lambda (row) (apply #'+ row)) data)\n")
    (insert "#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo60_agenda_custom_command_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:command-count 2 :keys (\"w\" \"h\") :names (\"Work\" \"Home\") :types (tags-todo tags) :matches (\"work\" \"home\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-agenda)
  (let ((org-agenda-custom-commands
         '(("w" "Work" tags-todo "work"
            ((org-agenda-overriding-header "Work tasks")))
           ("h" "Home" tags "home"
            ((org-agenda-overriding-header "Home items"))))))
    (list
     :command-count (length org-agenda-custom-commands)
     :keys (mapcar #'car org-agenda-custom-commands)
     :names (mapcar #'cadr org-agenda-custom-commands)
     :types (mapcar #'caddr org-agenda-custom-commands)
     :matches (mapcar #'cadddr org-agenda-custom-commands))))"##,
        expect,
    );
}

#[test]
fn combo60_capture_template_multi_target() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:count 4) (:keys (\"t\" \"j\" \"p\" \"w\")) (:descs (\"Todo\" \"Journal\" \"Protocol\" \"Web\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-capture)
  (let* ((templates
          '(("t" "Todo" entry (file+headline "/tmp/test.org" "Tasks")
             "* TODO %?\n  %T")
            ("j" "Journal" entry (file+datetree "/tmp/journal.org")
             "* %?\n  %U")
            ("p" "Protocol" entry (file+headline "" "Inbox")
             "* %:annotation\n  %i")
            ("w" "Web" plain (file "/tmp/web.org")
             "- %x")))
         (r '()))
    (push (list :count (length templates)) r)
    (push (list :keys (mapcar #'car templates)) r)
    (push (list :descs (mapcar #'cadr templates)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo60_babel_tangle_file_write() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-tangle)
  (let ((tmpfile (make-temp-file "org-tangle-" nil ".el")))
    (insert (format "#+begin_src emacs-lisp :tangle %s\n(message \"hello\")\n#+end_src\n" tmpfile))
    (let ((r '()))
      (goto-char (point-min))
      (condition-case e
          (progn (org-babel-tangle)
                 (push (list :tangled t) r)
                 (push (list :file-exists (file-exists-p tmpfile)) r))
        (error (push (list :tangle-error (car e)) r)))
      (condition-case nil
          (delete-file tmpfile)
        (error nil))
      (nreverse r))))"##,
    );
}

#[test]
fn combo60_persist_basic_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:register-fbound t) (:read-fbound t) (:write-fbound t) (:gc-fbound t) (:registered nil :written nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org-persist)
  (list
   ;; org-persist-register exists
   (list :register-fbound (fboundp 'org-persist-register))
   (list :read-fbound (fboundp 'org-persist-read))
   (list :write-fbound (fboundp 'org-persist-write))
   (list :gc-fbound (fboundp 'org-persist-gc))
   ;; try registering a simple value
   (condition-case e
       (let* ((container (org-persist-register (list :key "test-key") nil :expiry 1))
              (written (org-persist-write container)))
         (list :registered (not (null container))
               :written (not (null written))))
     (error (list :persist-error (car e))))
   ))"##,
        expect,
    );
}

#[test]
fn combo60_plot_table_extract() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:plot-fbound t) (:table-to-lisp ((\"X\" \"Y\") hline (\"1\" \"2\") (\"3\" \"4\") (\"5\" \"6\"))) (:row-count 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-plot)
  (insert "| X | Y |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |\n| 5 | 6 |\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; org-plot/gnuplot should be available
    (push (list :plot-fbound (fboundp 'org-plot/gnuplot)) r)
    ;; org-plot may have table data extraction functions
    (push (list :table-to-lisp (org-table-to-lisp)) r)
    ;; number of data rows
    (push (list :row-count (length (org-element-map (org-element-parse-buffer) 'table-row #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo60_babel_ob_sh_integration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ob-sh\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-sh)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src sh :results output\necho \"hello from shell\"\necho \"line2\"\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+begin_src sh")
      (push (org-babel-execute-src-block) r)
      ;; check result
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo60_export_with_include_directive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:export-ok t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((inc-file (make-temp-file "org-include-" nil ".org")))
    (with-temp-file inc-file
      (insert "* Included\nIncluded content.\n"))
    (insert (format "#+INCLUDE: \"%s\"\n" inc-file))
    (insert "* Main\nMain content.\n")
    (let ((r '()))
      (condition-case e
          (progn (goto-char (point-min))
                 (push (list :export-ok (> (length (org-export-as 'ascii nil nil t)) 0)) r))
        (error (push (list :export-error (car e)) r)))
      (condition-case nil (delete-file inc-file) (error nil))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo60_startup_visibility_settings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:A-invisible nil) (:A1-invisible org-fold-outline) (:after-show-A-invisible nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (setq org-startup-folded 'overview)
  (insert "* A\n** A1\nBody A1.\n** A2\nBody A2.\n* B\nBody B.\n")
  (let ((r '()))
    ;; set startup visibility
    (org-set-startup-visibility)
    ;; check invisible properties
    (goto-char (point-min))
    (push (list :A-invisible (get-char-property (point) 'invisible)) r)
    ;; move to A1 - should be invisible
    (search-forward "** A1")
    (push (list :A1-invisible (get-char-property (point) 'invisible)) r)
    ;; show all
    (org-show-all)
    (goto-char (point-min))
    (push (list :after-show-A-invisible (get-char-property (point) 'invisible)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo60_babel_var_complex_nested_reference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (14 (:result-count 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    ;; source table
    (insert "#+name: src-data\n| 1 | 2 |\n| 3 | 4 |\n\n")
    ;; processing block
    (insert "#+name: processor\n")
    (insert "#+begin_src emacs-lisp :results value :var input=src-data\n")
    (insert "(mapcar (lambda (row) (* (car row) (cadr row))) input)\n")
    (insert "#+end_src\n\n")
    ;; consumer uses processor output
    (insert "#+begin_src emacs-lisp :results value :var nums=processor\n")
    (insert "(apply #'+ nums)\n")
    (insert "#+end_src\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+name: processor")
      (search-forward "#+begin_src emacs-lisp") (org-babel-execute-src-block)
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}
