//! Strong combo-complex-55 oracle tests — deep untested paths:
//! org-capture template expansion, org-refile-get-targets complex,
//! org-pcomplete completions, org-timer operations, org-element-create
//! for all gross types, org-export-as with backends in sequence,
//! subtree boundary precision, and indirect buffer creation.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo55_capture_template_expand() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:template-count 2) (:keys (\"t\" \"n\")) (:descs (\"Todo\" \"Note\")) (:types (entry item)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-capture)
  (let* ((templates '(("t" "Todo" entry (file+headline "" "Tasks")
                       "* TODO %?\n  %i\n  %a")
                      ("n" "Note" item (file+olp "" "Notes" "Sub")
                       "- [ ] %^{Title}\n  %T\n  %?")))
         (r '()))
    (push (list :template-count (length templates)) r)
    ;; each template key
    (push (list :keys (mapcar #'car templates)) r)
    ;; each template description
    (push (list :descs (mapcar (lambda (tpl) (nth 1 tpl)) templates)) r)
    ;; each template type
    (push (list :types (mapcar (lambda (tpl) (nth 2 tpl)) templates)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo55_refile_targets_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments (0 . 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+FILETAGS: :project:\n")
  (insert "* Tasks :work:\n** TODO A\n** DONE B\n* Notes :home:\n** NOTE C\n** NOTE D\n")
  (let ((r '()))
    (let ((targets (org-refile-get-targets)))
      (push (list :target-count (length targets)) r)
      (push (list :target-names (mapcar #'car targets)) r)
      (push (list :target-levels
                  (mapcar (lambda (tgt) (when (cdr tgt) (org-element-property :level (cdr tgt)))) targets)) r))
    ;; refile-get-targets with specific file
    (let ((targets2 (org-refile-get-targets nil nil)))
      (push (list :buffer-target-count (length targets2)) r))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo55_pcomplete_completions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:completions-A 0) (:org-pcomplete-loaded t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'pcomplete)
  (insert "* Headings\n** Agenda\n** Apple\n** Banana\n** Blog\n** Buffer\n")
  (let ((r '()))
    ;; pcomplete on headline prefix "A"
    (goto-char (point-min))
    (search-forward "** Agenda") (beginning-of-line)
    (goto-char (point-min))
    (condition-case nil
        (progn
          (push (list :completions-A (length (pcomplete-completions))) r))
      (error (push (list :pcomplete-error t) r)))
    ;; check org-pcomplete is available
    (push (list :org-pcomplete-loaded (fboundp 'pcomplete-completions)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo55_timer_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:started t) (:paused t) (:stopped-seconds nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-timer)
  (let ((r '()))
    ;; org-timer-start
    (condition-case nil
        (let ((start (org-timer-start)))
          (push (list :started t) r))
      (error (push (list :start-error t) r)))
    ;; org-timer-pause-or-continue
    (condition-case nil
        (progn (org-timer-pause-or-continue)
               (push (list :paused t) r))
      (error (push (list :pause-error t) r)))
    ;; org-timer-stop
    (condition-case nil
        (let ((seconds (org-timer-stop)))
          (push (list :stopped-seconds (numberp seconds)) r))
      (error (push (list :stop-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo55_element_create_all_gross_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument char-or-string-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (let* ((center-block (org-element-create 'center-block nil "Centered content."))
         (quote-block (org-element-create 'quote-block nil "Quoted."))
         (special-block (org-element-create 'special-block '(:type "abstract") "Abstract text."))
         (example-block (org-element-create 'example-block nil "Example\nwith\n newlines"))
         (verse-block (org-element-create 'verse-block nil "Verse\n  indented line"))
         (plain-list (org-element-create 'plain-list '(:type unordered)
                       (org-element-create 'item nil "Item A")
                       (org-element-create 'item nil "Item B")))
         (interpreted (substring-no-properties (org-element-interpret-data
                         (org-element-create 'org-data nil
                           (org-element-create 'section nil
                             center-block quote-block special-block
                             example-block verse-block plain-list)))))
         (r '()))
    (push (list :center-type (org-element-type center-block)) r)
    (push (list :quote-type (org-element-type quote-block)) r)
    (push (list :special-type (org-element-type special-block)) r)
    (push (list :example-type (org-element-type example-block)) r)
    (push (list :verse-type (org-element-type verse-block)) r)
    (push (list :list-type (org-element-type plain-list)) r)
    ;; reparse interpreted
    (let ((reparsed (with-temp-buffer (org-mode)
                      (insert interpreted)
                      (goto-char (point-min))
                      (org-element-parse-buffer))))
      (push (list :re-center (length (org-element-map reparsed 'center-block #'identity))) r)
      (push (list :re-quote (length (org-element-map reparsed 'quote-block #'identity))) r)
      (push (list :re-example (length (org-element-map reparsed 'example-block #'identity))) r)
      (push (list :re-list (length (org-element-map reparsed 'plain-list #'identity))) r))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo55_export_dispatcher_backends() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:registered-backends (odt latex icalendar html ascii)) (:has-ascii nil) (:has-html nil) (:has-latex nil) (:has-md nil) (:has-texinfo nil) (:has-man nil) (:has-icalendar nil) (:has-beamer nil) (:total-backends 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox)
  (let ((r '()))
    ;; check backend registry
    (push (list :registered-backends
                (mapcar #'org-export-backend-name
                        org-export-registered-backends)) r)
    ;; find specific backends
    (push (list :has-ascii (and (assq 'ascii org-export-registered-backends) t)) r)
    (push (list :has-html (and (assq 'html org-export-registered-backends) t)) r)
    (push (list :has-latex (and (assq 'latex org-export-registered-backends) t)) r)
    (push (list :has-md (and (assq 'md org-export-registered-backends) t)) r)
    (push (list :has-texinfo (and (assq 'texinfo org-export-registered-backends) t)) r)
    (push (list :has-man (and (assq 'man org-export-registered-backends) t)) r)
    (push (list :has-icalendar (and (assq 'icalendar org-export-registered-backends) t)) r)
    (push (list :has-beamer (and (assq 'beamer org-export-registered-backends) t)) r)
    ;; total backend count
    (push (list :total-backends (length org-export-registered-backends)) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo55_subtree_boundary_precision() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-beginning-of-subtree)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\nBody A.\n** A1\nBody A1.\n* B\nBody B.\n")
  (let ((r '()))
    ;; begin and end of subtree for various headings
    (goto-char (point-min))
    (let ((beg-A (progn (org-beginning-of-subtree) (point)))
          (end-A (progn (org-end-of-subtree) (point))))
      (push (list :A-beg-end (list beg-A end-A)) r))
    ;; on A1
    (goto-char (point-min))
    (search-forward "** A1") (beginning-of-line)
    (let ((beg-A1 (progn (org-beginning-of-subtree) (point)))
          (end-A1 (progn (org-end-of-subtree) (point))))
      (push (list :A1-beg-end (list beg-A1 end-A1)) r))
    ;; on B
    (goto-char (point-min))
    (search-forward "* B") (beginning-of-line)
    (let ((beg-B (progn (org-beginning-of-subtree) (point)))
          (end-B (progn (org-end-of-subtree) (point))))
      (push (list :B-beg-end (list beg-B end-B)) r))
    ;; A's subtree should encompass A1
    (goto-char (point-min))
    (let ((a-end (save-excursion (org-end-of-subtree) (point)))
          (a1-end (save-excursion (search-forward "** A1")
                                  (beginning-of-line)
                                  (org-end-of-subtree)
                                  (point))))
      (push (list :a-contains-a1 (> a-end a1-end)) r))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo55_babel_noweb_cross_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (double 42 (:result-count 0))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "* Noweb\n")
    (insert "#+name: helper\n")
    (insert "#+begin_src emacs-lisp :results value\n(defun double (x) (* x 2))\n#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results value :noweb yes\n<<helper>>\n(double 21)\n#+end_src\n")
    (let ((r '()))
      ;; execute helper
      (goto-char (point-min))
      (search-forward "#+name: helper")
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; execute noweb-using block
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; result count
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo55_insert_todo_subheading_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-todo-sub ((1 \"Projects\" nil) (2 \"Subproject A\" \"TODO\"))) (:after-sub ((1 \"Projects\" nil) (2 \"Subproject A\" \"TODO\") (3 \"Subproject B\" nil))) (:total-headlines 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Projects\n")
  (let ((r '()))
    ;; insert TODO subheading
    (goto-char (point-max))
    (condition-case nil
        (progn
          (org-insert-todo-subheading nil)
          (insert "Subproject A")
          (push (list :after-todo-sub
                      (mapcar (lambda (h) (list (org-element-property :level h)
                                                (substring-no-properties (org-element-property :raw-value h))
                                                (org-element-property :todo-keyword h)))
                              (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
          ;; insert subheading
          (org-insert-subheading nil)
          (insert "Subproject B")
          (push (list :after-sub
                      (mapcar (lambda (h) (list (org-element-property :level h)
                                                (substring-no-properties (org-element-property :raw-value h))
                                                (org-element-property :todo-keyword h)))
                              (org-element-map (org-element-parse-buffer) 'headline #'identity))) r))
      (error (push (list :insert-error t) r)))
    ;; headline count
    (push (list :total-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo55_tree_to_indirect_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:indirect-created nil) (:indirect-name nil) (:orig-headlines 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Main\nBody main.\n** Child\nBody child.\n* Other\nBody other.\n")
  (let ((r '()))
    ;; org-tree-to-indirect-buffer should create an indirect buffer
    (goto-char (point-min))
    (condition-case nil
        (let ((indirect (org-tree-to-indirect-buffer)))
          (push (list :indirect-created (bufferp indirect)) r)
          (push (list :indirect-name (when (bufferp indirect) (buffer-name indirect))) r)
          ;; kill the indirect buffer
          (when (bufferp indirect)
            (kill-buffer indirect)))
      (error (push (list :indirect-error t) r)))
    ;; original buffer should be unaffected
    (push (list :orig-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}
