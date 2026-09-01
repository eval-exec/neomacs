//! Strong combo-complex-57 oracle tests — deep divergence-probe
//! workflows: org-sparse-tree with date/tag regex combos, clock
//! with org-clock-report, agenda-style todo filter + map chain,
//! org-todo with custom sequences and logging, babel with :exports
//! both/results/code, org-export with subtree scope + body-only,
//! org-element cache coherence after repeated parse+modify,
//! org-toggle-* display state, org-paste-subtree with level adjust,
//! and org-meta-return context-dependent heading insertion.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo57_sparse_tree_date_tag_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:match-work+urgent (\"A\" \"B\" \"C\")) (:match-work-or-home (\"A\" \"A1\" \"B\" \"C\")) (:date-match (\"A\" \"B\" \"C\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A :work:\nSCHEDULED: <2024-01-15 Mon>\n** A1 :work:\n* B :urgent:\nSCHEDULED: <2024-03-01 Fri>\n* C :home:urgent:\n")
  (let ((r '()))
    ;; match work+urgent
    (org-match-sparse-tree nil "work+urgent")
    (push (list :match-work+urgent (org-element-map (org-element-parse-buffer nil t) 'headline
                                     (lambda (h) (substring-no-properties (org-element-property :raw-value h))))) r)
    (org-remove-occur-highlights)
    ;; match work|home
    (org-match-sparse-tree nil "work|home")
    (push (list :match-work-or-home (org-element-map (org-element-parse-buffer nil t) 'headline
                                      (lambda (h) (substring-no-properties (org-element-property :raw-value h))))) r)
    (org-remove-occur-highlights)
    ;; match with date: SCHEDULED before Feb
    (condition-case nil
        (progn (org-match-sparse-tree nil "SCHEDULED<\"<2024-02-01>\"")
               (push (list :date-match (org-element-map (org-element-parse-buffer nil t) 'headline
                                         (lambda (h) (substring-no-properties (org-element-property :raw-value h))))) r))
      (error (push (list :date-match-error t) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo57_clock_report_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:clocktable-created t) (:tables 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil))
    (insert "* Task A\n** Sub A1\n* Task B\n")
    ;; clock each
    (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
    (search-forward "* Task B") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
    (search-backward "** Sub A1") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
    ;; create clock table
    (goto-char (point-min))
    (insert "#+BEGIN: clocktable :maxlevel 3 :scope file :tstart \"<2024-01-01>\" :tend \"<2024-12-31>\"\n#+END:\n")
    (let ((r '()))
      (goto-char (point-min))
      (search-forward "#+BEGIN: clocktable") (beginning-of-line)
      (org-dblock-update)
      (push (list :clocktable-created (> (length (buffer-string)) 0)) r)
      (push (list :tables (length (org-element-map (org-element-parse-buffer) 'table #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo57_todo_custom_sequence_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init \"TODO\") (:1 #(\"DONE\" 0 4 (org-todo-head \"TODO\"))) (:2 nil) (:3-right #(\"TODO\" 0 4 (org-todo-head \"TODO\"))) (:cycle-states (#(\"DONE\" 0 4 (org-todo-head \"TODO\")) nil #(\"TODO\" 0 4 (org-todo-head \"TODO\")))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (let ((org-todo-keywords '((sequence "TODO" "WAIT" "|" "DONE" "CANCELED")))
        (org-log-done nil))
    (insert "* TODO Task\n")
    (let ((r '()))
      ;; cycle through states
      (goto-char (point-min))
      (push (list :init (org-get-todo-state)) r)
      (org-todo) (push (list :1 (org-get-todo-state)) r)
      (org-todo) (push (list :2 (org-get-todo-state)) r)  ;; DONE
      ;; Now cycle backwards (shift-right)
      (goto-char (point-min))
      (org-todo 'right)
      (push (list :3-right (org-get-todo-state)) r)
      ;; go to WAIT from CANCELED
      (goto-char (point-min))
      (let ((states nil))
        (dotimes (i 3)
          (org-todo)
          (push (org-get-todo-state) states))
        (push (list :cycle-states (nreverse states)) r))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo57_babel_exports_both_results_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (3 \"output text\" (1 2) (:exports-values (\":results value :exports both\" \":results output :exports results\" \":results value :exports code\")) (:result-count 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results value :exports both\n(+ 1 2)\n#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results output :exports results\n(princ \"output text\")\n#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results value :exports code\n(list 1 2)\n#+end_src\n")
    (let ((r '()))
      ;; execute each
      (goto-char (point-min))
      (search-forward "#+begin_src emacs-lisp :results value :exports both")
      (push (org-babel-execute-src-block) r)
      (search-forward "#+begin_src emacs-lisp :results output :exports results")
      (push (org-babel-execute-src-block) r)
      (search-forward "#+begin_src emacs-lisp :results value :exports code")
      (push (org-babel-execute-src-block) r)
      ;; check :exports parameters on src-blocks
      (push (list :exports-values
                  (mapcar (lambda (s) (org-element-property :parameters s))
                          (org-element-map (org-element-parse-buffer) 'src-block #'identity))) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo57_export_subtree_body_only() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox-ascii)
  (require 'ox-html)
  (let ((org-export-show-temporary-export-buffer nil)
        (org-ascii-text-width 72))
    (insert "* A\n** B\nBody B.\n** C\nBody C.\n* D\nBody D.\n")
    (let ((r '()))
      ;; export D subtree only
      (goto-char (point-min))
      (search-forward "* D") (beginning-of-line)
      (let ((ascii-sub (org-export-as 'ascii 'subtree nil t)))
        (push (list :ascii-sub-has-D (and ascii-sub (string-match-p "Body D" ascii-sub))) r)
        (push (list :ascii-sub-no-A (and ascii-sub (not (string-match-p "Body B" ascii-sub)))) r))
      ;; export B body only
      (goto-char (point-min))
      (search-forward "** B") (beginning-of-line)
      (let ((html-sub (org-export-as 'html 'subtree t nil t)))
        (push (list :html-body-only (and html-sub (not (string-match-p "<!DOCTYPE" html-sub)))) r)
        (push (list :html-has-B (and html-sub (string-match-p "Body B" html-sub))) r))
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo57_element_cache_repeated_parse_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:t1-headlines 2) (:t2-headlines 3) (:t3-headlines 4) (:t3-levels (1 1 1 2)) (:at-bob headline) (:at-eob paragraph))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\nBody A.\n* B\nBody B.\n")
  (let ((r '()))
    ;; parse once
    (let ((t1 (org-element-parse-buffer)))
      (push (list :t1-headlines (length (org-element-map t1 'headline #'identity))) r))
    ;; modify
    (goto-char (point-max))
    (insert "\n* C\nBody C.\n")
    ;; parse again
    (let ((t2 (org-element-parse-buffer)))
      (push (list :t2-headlines (length (org-element-map t2 'headline #'identity))) r))
    ;; check t1 still usable
    ;; actually old tree may be stale now, but we parsed fresh
    ;; modify more
    (goto-char (point-max))
    (insert "\n** C1\nSub.\n")
    (let ((t3 (org-element-parse-buffer)))
      (push (list :t3-headlines (length (org-element-map t3 'headline #'identity))) r)
      (push (list :t3-levels (mapcar (lambda (h) (org-element-property :level h))
                                     (org-element-map t3 'headline #'identity))) r))
    ;; at-point consistency
    (goto-char (point-min))
    (push (list :at-bob (org-element-type (org-element-at-point))) r)
    (goto-char (point-max))
    (push (list :at-eob (org-element-type (org-element-at-point))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo57_paste_subtree_level_adjust() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-paste-level1 ((1 \"A\") (2 \"A1\") (1 \"B\") (1 \"A1\"))) (:after-paste-level2 ((1 \"A\") (2 \"B\") (2 \"A1\") (1 \"B\") (1 \"A1\"))) (:buffer \"* A\\n** B\\n** A1\\nBody A1.\\n* B\\n* A1\\nBody A1.\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\nBody A1.\n* B\n")
  (let ((r '()))
    ;; copy A1 subtree
    (goto-char (point-min))
    (search-forward "** A1") (beginning-of-line)
    (org-copy-subtree)
    ;; paste under B as a sibling (level 1)
    (goto-char (point-min))
    (search-forward "* B") (end-of-line)
    (org-paste-subtree 1)  ;; paste at level 1
    (push (list :after-paste-level1
                (mapcar (lambda (h) (list (org-element-property :level h)
                                          (substring-no-properties (org-element-property :raw-value h))))
                        (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; copy B subtree
    (goto-char (point-min))
    (search-forward "* B") (beginning-of-line)
    (org-copy-subtree)
    ;; paste under A as child (level 2)
    (goto-char (point-min))
    (search-forward "* A") (end-of-line)
    (org-paste-subtree 2)
    (push (list :after-paste-level2
                (mapcar (lambda (h) (list (org-element-property :level h)
                                          (substring-no-properties (org-element-property :raw-value h))))
                        (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo57_meta_return_context_dependent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:after-heading (\"New Heading\" \"Heading\")) (:after-item (\"\" \"\" \"\")) (:buffer \"* New Heading\\n* Heading\\n- new item\\n- item1\\n- item2\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Heading\n- item1\n- item2\n")
  (let ((r '()))
    ;; M-RET on heading: should insert heading at same level
    (goto-char (point-min))
    (condition-case nil
        (progn (org-meta-return)
               (insert "New Heading"))
      (error nil))
    (push (list :after-heading (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                       (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; M-RET on list item: should insert new item
    (goto-char (point-min))
    (search-forward "item1") (beginning-of-line)
    (condition-case nil
        (progn (org-meta-return)
               (insert "new item"))
      (error nil))
    (push (list :after-item (mapcar (lambda (i) (substring-no-properties (or (org-element-property :raw-value i) "")))
                                    (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo57_toggle_checkbox_nested_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (search-failed \"[-] Parent\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Task [/]\n")
  (insert "- [-] Parent [/]\n")
  (insert "  - [X] Child 1\n")
  (insert "  - [ ] Child 2\n")
  (insert "  - [ ] Child 3\n")
  (let ((r '()))
    (push (list :init (buffer-string)) r)
    (org-update-statistics-cookies t)
    (push (list :after-stats (buffer-string)) r)
    ;; toggle Child 2
    (goto-char (point-min)) (search-forward "Child 2") (beginning-of-line)
    (org-toggle-checkbox)
    (org-update-statistics-cookies t)
    (push (list :after-child2 (buffer-string)) r)
    ;; toggle Child 3
    (goto-char (point-min)) (search-forward "Child 3") (beginning-of-line)
    (org-toggle-checkbox)
    (org-update-statistics-cookies t)
    (push (list :after-child3 (buffer-string)) r)
    ;; all children checked => Parent should be [X]
    (push (list :parent-checkbox
                (progn (goto-char (point-min))
                       (search-forward "[-] Parent")
                       (org-element-property :checkbox (org-element-at-point)))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo57_property_drawer_duplicate_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:A-value \"2\") (:B-value \"3\") (:A-after-put \"2\") (:all-keys (\"CATEGORY\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* H\n:PROPERTIES:\n:A: 1\n:A: 2\n:B: 3\n:END:\n")
  (let ((r '()))
    ;; what does org-entry-get return for A? (first or last?)
    (goto-char (point-min))
    (push (list :A-value (org-entry-get nil "A")) r)
    (push (list :B-value (org-entry-get nil "B")) r)
    ;; set A again
    (org-entry-put nil "A" "42")
    (push (list :A-after-put (org-entry-get nil "A")) r)
    ;; get all properties
    (push (list :all-keys (sort (mapcar #'car (org-entry-properties nil t)) #'string-lessp)) r)
    (nreverse r)))"##,
        expect,
    );
}
