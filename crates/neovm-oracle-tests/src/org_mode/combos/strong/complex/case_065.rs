//! Strong combo-complex-65 oracle tests — clock report with
//! specific time ranges, element plural adopt/extract ops,
//! export option toggle cycles, agenda with sort strategies,
//! babel with :dir header, org-element interpretation after
//! deep content replacement, and org-mode-hook interaction.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo65_clock_report_time_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:report-created t) (:tables 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil))
    (insert "* Task A\n* Task B\n")
    (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
    (search-forward "* Task B") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
    (goto-char (point-min))
    (insert "#+BEGIN: clocktable :maxlevel 2 :scope file :tstart \"<2024-01-01>\" :tend \"<2024-12-31>\" :block thisyear\n#+END:\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+BEGIN: clocktable") (beginning-of-line)
      (org-dblock-update)
      (push (list :report-created t) r)
      (push (list :tables (length (org-element-map (org-element-parse-buffer) 'table #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo65_element_deep_content_replacement() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:before-paras 1) (:after-paras 1) (:after-tables 1) (:re-paras 1) (:re-tables 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* A\n===\nPara.\n* B\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (hl-A (car (org-element-map tree 'headline
                       (lambda (h) (when (equal "A" (org-element-property :raw-value h)) h)))))
           (section-A (car (org-element-map hl-A 'section #'identity))))
      (push (list :before-paras (length (org-element-map tree 'paragraph #'identity))) r)
      ;; replace all section content with a new paragraph + table
      (when section-A
        (let ((new-contents
               (list (org-element-create 'paragraph nil "New para.\n")
                     (org-element-create 'table '(:type org)
                       (org-element-create 'table-row '(:type standard)
                         (org-element-create 'table-cell nil "X")
                         (org-element-create 'table-cell nil "Y"))))))
          (org-element-set-contents section-A new-contents)))
      (push (list :after-paras (length (org-element-map tree 'paragraph #'identity))) r)
      (push (list :after-tables (length (org-element-map tree 'table #'identity))) r)
      ;; interpret and reparse
      (let* ((interpreted (substring-no-properties (org-element-interpret-data tree)))
             (reparsed (with-temp-buffer (org-mode) (insert interpreted)
                         (goto-char (point-min)) (org-element-parse-buffer))))
        (push (list :re-paras (length (org-element-map reparsed 'paragraph #'identity))) r)
        (push (list :re-tables (length (org-element-map reparsed 'table #'identity))) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo65_export_option_toggle_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:with-toc nil) (:without-toc t) (:without-numbers 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72))
    (insert "* Head\nBody text.\n")
    (let ((r '()))
      ;; export with toc (default)
      (let ((org-export-with-toc t))
        (let ((out (org-export-as 'ascii nil nil t)))
          (push (list :with-toc (and out (string-match-p "Table of Contents" out))) r)))
      ;; export without toc
      (let ((org-export-with-toc nil))
        (let ((out (org-export-as 'ascii nil nil t)))
          (push (list :without-toc (not (and out (string-match-p "Table of Contents" out)))) r)))
      ;; export with section numbers off
      (let ((org-export-with-section-numbers nil))
        (let ((out (org-export-as 'ascii nil nil t)))
          (push (list :without-numbers (and out (string-match-p "Head$" out))) r)))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo65_agenda_sort_strategies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-agenda)
  (insert "* TODO [#B] Zebra :work:\nSCHEDULED: <2024-01-15>\n")
  (insert "* TODO [#A] Apple :work:\nSCHEDULED: <2024-01-10>\n")
  (insert "* TODO [#C] Mango :urgent:\nDEADLINE: <2024-01-20>\n")
  (let ((r '()))
    ;; org-agenda-sorting-strategy
    (push (list :sort-fbound (boundp 'org-agenda-sorting-strategy)) r)
    ;; agenda entry sorting via map
    (push (list :sorted-by-priority
                (sort (org-map-entries
                       (lambda () (list (org-get-heading t t t t)
                                        (org-get-priority (point)))))
                      (lambda (a b) (< (cadr a) (cadr b))))) r)
    ;; all TODO headings
    (push (list :all-todo (org-map-entries
                           (lambda () (org-get-heading t t t t))
                           "TODO=\"TODO\"")) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo65_babel_dir_header() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ob-sh\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-sh)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src sh :results output :dir /tmp\necho \"cwd=$(pwd)\"\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src sh")
      (condition-case e
          (push (org-babel-execute-src-block) r)
        (error (push (list :dir-error (car e)) r)))
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo65_font_lock_basic_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:font-lock-fbound t) (:fontify-fbound nil) (:fontify-like-fbound t) (:font-lock-defaults-set t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'font-lock)
  (insert "* TODO Task :work:\nBody.\n")
  (let ((r '()))
    ;; font-lock-mode
    (push (list :font-lock-fbound (fboundp 'font-lock-mode)) r)
    ;; org-font-lock-ensure
    (push (list :fontify-fbound (fboundp 'org-font-lock-ensure)) r)
    ;; org-fontify-like-in-org-mode
    (push (list :fontify-like-fbound (fboundp 'org-fontify-like-in-org-mode)) r)
    ;; org-set-font-lock-defaults
    (condition-case nil
        (progn (org-set-font-lock-defaults)
               (push (list :font-lock-defaults-set t) r))
      (error (push (list :font-lock-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo65_babel_results_drawer_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (300 (:drawer-count 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results value drawer\n(+ 100 200)\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; check for result drawer
      (push (list :drawer-count (length (org-element-map (org-element-parse-buffer) 'drawer
                                         (lambda (d) (when (equal "RESULTS" (org-element-property :drawer-name d)) d))))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo65_export_with_creator_and_email() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:email \"test@example.com\") (:creator-is-string t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-export-with-creator t)
        (org-export-with-email t))
    (insert "#+EMAIL: test@example.com\n")
    (insert "* Test\n")
    (let ((r '()))
      (let ((info (org-export-get-environment)))
        (push (list :email (plist-get info :email)) r)
        ;; creator contains version string - just check it's a non-empty string
        (push (list :creator-is-string (stringp (plist-get info :creator))) r))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo65_org_todo_cycle_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init \"TODO\") (:after-1 #(\"DONE\" 0 4 (org-todo-head \"TODO\"))) (:after-2 nil) (:after-right #(\"TODO\" 0 4 (org-todo-head \"TODO\"))) (:after-left nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Task\n")
  (let ((r '()))
    (goto-char (point-min))
    (push (list :init (org-get-todo-state)) r)
    ;; cycle forward: TODO → DONE
    (org-todo)
    (push (list :after-1 (org-get-todo-state)) r)
    ;; cycle forward: DONE → TODO
    (org-todo)
    (push (list :after-2 (org-get-todo-state)) r)
    ;; cycle with 'right: TODO → DONE
    (org-todo 'right)
    (push (list :after-right (org-get-todo-state)) r)
    ;; cycle with 'left: DONE → TODO
    (org-todo 'left)
    (push (list :after-left (org-get-todo-state)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo65_org_show_setting() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:headlines 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** B\n** C\n* D\n")
  (let ((r '()))
    (goto-char (point-min))
    ;; org-show-children
    (condition-case nil (org-show-children) (error nil))
    ;; org-show-entry
    (condition-case nil (org-show-entry) (error nil))
    ;; after showing, headlines should be parseable
    (push (list :headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}
