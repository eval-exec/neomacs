//! Strong combo-complex-49 oracle tests — deep multi-step divergence-prone
//! workflows: property API across mutations, clock in/out with logbook,
//! org-map-entries with mutation and remap, table formula iterate,
//! visibility/fold cycles, element adopt/extract chains, babel
//! session with cross-block vars, dynamic block lifecycle,
//! export environment mutability, and footnote normalize/edit cycles.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

// ═══════════════════════════════════════════════════════════════════════
// Build doc → set properties → mutate structure → remap → verify all
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_property_mutate_remap_verify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (user-error \"State ‘WAIT’ not valid in this file\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* TODO Root\n** DONE A\n** TODO B\n* TODO Other\n")
  (let ((r '()))
    ;; initial map
    (push (list :init (org-map-entries
                       (lambda () (list (org-get-heading t t t t)
                                        (org-get-todo-state))))) r)
    ;; set properties on root
    (goto-char (point-min))
    (org-entry-put nil "PRIORITY" "A")
    (org-entry-put nil "REVIEWER" "alice")
    ;; set property on A
    (forward-line 1)
    (org-entry-put nil "HOURS" "3")
    ;; set property on B
    (forward-line 1)
    (org-entry-put nil "HOURS" "5")
    (org-todo "WAIT")
    ;; re-map with properties
    (push (list :after-mutate
                (org-map-entries
                 (lambda () (list (org-get-heading t t t t)
                                  (org-get-todo-state)
                                  (org-entry-get nil "HOURS")
                                  (org-entry-get nil "PRIORITY")
                                  (org-entry-get nil "REVIEWER"))))) r)
    ;; change B back to TODO
    (goto-char (point-min))
    (search-forward "** WAIT B")
    (beginning-of-line)
    (org-todo "TODO")
    ;; delete the REVIEWER property from root
    (goto-char (point-min))
    (org-entry-delete nil "REVIEWER")
    ;; re-map
    (push (list :final-map
                (org-map-entries
                 (lambda () (list (org-get-heading t t t t)
                                  (org-get-todo-state)
                                  (org-entry-get nil "HOURS")
                                  (org-entry-get nil "PRIORITY")
                                  (org-entry-get nil "REVIEWER"))))) r)
    ;; buffer state
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Clock in → out multiple times → clock-sum → over different headings
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_clock_multi_entry_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:a-in t) (:a-out t) (:b-in t) (:b-out t) (:a-clock-count 3) (:b-clock-count 0) (:a-clock-sum 0) (:b-clock-sum 0) (:logbook-count 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil)
        (org-clock-out-remove-zero-clock-sum t))
    (insert "* Task A\n* Task B\n")
    (let ((r '()))
      ;; clock in → out on A
      (goto-char (point-min))
      (org-clock-in nil)
      (push (list :a-in (org-clocking-p)) r)
      (org-clock-out nil nil)
      (push (list :a-out (not (org-clocking-p))) r)
      ;; clock in → out on B
      (goto-char (point-min))
      (search-forward "* Task B")
      (beginning-of-line)
      (org-clock-in nil)
      (push (list :b-in (org-clocking-p)) r)
      (org-clock-out nil nil)
      (push (list :b-out (not (org-clocking-p))) r)
      ;; clock in again on A, out
      (goto-char (point-min))
      (org-clock-in nil)
      (org-clock-out nil nil)
      ;; now count clock entries
      (push (list :a-clock-count (progn (goto-char (point-min))
                                       (length (org-element-map (org-element-parse-buffer) 'clock #'identity)))) r)
      (push (list :b-clock-count (progn (search-forward "* Task B")
                                       (length (org-element-map (org-element-at-point) 'clock #'identity)))) r)
      ;; clock-sum each
      (goto-char (point-min))
      (push (list :a-clock-sum (org-clock-sum-current-item)) r)
      (search-forward "* Task B")
      (beginning-of-line)
      (push (list :b-clock-sum (org-clock-sum-current-item)) r)
      ;; logbook drawer count
      (push (list :logbook-count (length (org-element-map (org-element-parse-buffer) 'drawer
                                          (lambda (d) (when (equal "LOGBOOK" (org-element-property :drawer-name d)) d))))) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Build doc → fold → parse → unfold → reparse → compare element counts
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_fold_parse_unfold_reparse() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:init-headlines 6) (:init-sections 4) (:overview-vis-headlines 2) (:after-showall-headlines 6) (:after-showall-sections 4) (:after-hide-a-headlines 6) (:after-show-a-headlines 6))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\nBody 1.\n** A2\nBody 2.\n*** A2a\nBody 2a.\n* B\n** B1\nBody B1.\n")
  (let ((r '()))
    ;; initial parse: all visible
    (push (list :init-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (push (list :init-sections (length (org-element-map (org-element-parse-buffer) 'section #'identity))) r)
    ;; overview: fold all to level 1
    (goto-char (point-min))
    (org-overview)
    ;; parse visible only
    (push (list :overview-vis-headlines (length (org-element-map (org-element-parse-buffer nil t) 'headline #'identity))) r)
    ;; show all
    (org-show-all)
    (push (list :after-showall-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (push (list :after-showall-sections (length (org-element-map (org-element-parse-buffer) 'section #'identity))) r)
    ;; fold children of A
    (goto-char (point-min))
    (org-fold-hide-subtree)
    ;; now A's subtree is hidden but still parseable
    (push (list :after-hide-a-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    ;; reveal A
    (goto-char (point-min))
    (org-fold-show-subtree)
    (push (list :after-show-a-headlines (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Multi-table with formula iteration across edits
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_table_iterate_across_edits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"Not at a table\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+name: T1\n| a | b |\n| 5 | 3 |\n| 2 | 7 |\n\n")
  (insert "#+name: T2\n| sum | max | min |\n|     |     |     |\n")
  (insert "#+TBLFM: @2$1=vsum(remote(T1,@2$1..@3$1))::@2$2=vmax(remote(T1,@2$2..@3$2))::@2$3=vmin(remote(T1,@2$2..@3$2))\n")
  (let ((r '()))
    ;; iterate (recalc until stable)
    (goto-char (point-min))
    (search-forward "T2")
    (forward-line) (forward-line)
    (org-table-iterate)
    (org-table-align)
    (push (list :after-iterate (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; modify T1 values and re-iterate
    (goto-char (point-min))
    (search-forward "T1")
    (forward-line) (forward-line)
    (org-table-kill-row)
    (insert " 10 | 20 ")
    (org-table-iterate)
    (push (list :after-mod-iterate (buffer-substring-no-properties (point-min) (point-max))) r)
    ;; get specific computed values from T2
    (goto-char (point-min))
    (search-forward "T2")
    (forward-line) (forward-line)
    (push (list :t2-sum (org-table-get "" "sum")) r)
    (push (list :t2-max (org-table-get "" "max")) r)
    (push (list :t2-min (org-table-get "" "min")) r)
    ;; to-lisp of T2
    (push (list :t2-to-lisp (org-table-to-lisp)) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Element adopt/extract across different parent types
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_adopt_extract_cross_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-element-adopt-element)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* H1\nPara 1.\n* H2\nPara 2.\n* H3\nPara 3.\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (headlines (org-element-map tree 'headline #'identity))
           (h1 (nth 0 headlines))
           (h2 (nth 1 headlines))
           (h3 (nth 2 headlines)))
      ;; extract h2
      (let ((ex (org-element-extract-element h2)))
        (push (list :extracted-type (org-element-type ex)) r)
        (push (list :extracted-value (substring-no-properties (org-element-property :raw-value ex))) r))
      ;; after extract: tree has only 2 top-level headlines
      (push (list :after-extract-top-count
                  (length (org-element-map tree 'headline
                            (lambda (h) (when (eq (org-element-property :parent h) tree) h))))) r)
      ;; adopt h2 as child of h3 (but first get h2 out, it's detached)
      (let ((h2-detached (car (last (org-element-map tree 'headline #'identity
                                     (lambda (h) (when (eq (org-element-type (org-element-property :parent h)) 'org-data) h)))))))
        (when h2-detached
          (push (list :found-detached (substring-no-properties (org-element-property :raw-value h2-detached))) r)))
      ;; org-element-adopt-element: create new child and adopt under h3
      (let ((new-kid (org-element-create 'headline '(:level 3 :raw-value "AdoptedKid" :parent h3))))
        (org-element-adopt-element h3 new-kid)
        (push (list :h3-children (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                                         (org-element-map h3 'headline #'identity))) r))
      ;; interpret the whole tree
      (push (list :interpreted-length (> (length (substring-no-properties (org-element-interpret-data tree))) 0)) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Babel session with cross-block variable sharing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_babel_session_cross_block() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ob-sh\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (require 'ob-sh)
  (let ((org-confirm-babel-evaluate nil))
    (insert "* Cross-block session\n")
    ;; define function
    (insert "#+name: adder\n")
    (insert "#+begin_src emacs-lisp :results value\n(lambda (x) (+ x 10))\n#+end_src\n\n")
    ;; use it
    (insert "#+begin_src emacs-lisp :results value :var fn=adder\n(funcall fn 32)\n#+end_src\n\n")
    ;; sh block that takes elisp result
    (insert "#+name: sh-input\n")
    (insert "#+begin_src emacs-lisp :results output\n(princ \"hello world\")\n#+end_src\n\n")
    (insert "#+begin_src sh :results output :var msg=sh-input\necho \"MSG=$msg\"\n#+end_src\n")
    (let ((r '()))
      ;; execute adder
      (goto-char (point-min))
      (search-forward "#+name: adder")
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; execute user of adder
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; execute sh-input
      (search-forward "sh-input")
      (search-forward "#+begin_src emacs-lisp")
      (push (org-babel-execute-src-block) r)
      ;; execute sh block
      (search-forward "#+begin_src sh")
      (push (org-babel-execute-src-block) r)
      ;; count all result blocks
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-capture template expansion in context
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_capture_template_context() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:template-count 2) (:keys (\"t\" \"n\")) (:types (\"t\" \"n\")) (:descs (\"Todo\" \"Note\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-capture)
  (let ((org-capture-templates
         '(("t" "Todo" entry (file+headline "/tmp/capture-test.org" "Tasks")
            "* TODO %?\n  %i\n  %a")
           ("n" "Note" entry (file+headline "" "Notes")
            "* %^{Title}\n  %T\n  %?"))))
    (list
     ;; count templates
     (list :template-count (length org-capture-templates))
     ;; first template key
     (list :keys (mapcar #'car org-capture-templates))
     ;; first template type
     (list :types (mapcar (lambda (tpl) (nth 0 tpl)) org-capture-templates))
     ;; template descriptions
     (list :descs (mapcar (lambda (tpl) (nth 1 tpl)) org-capture-templates)))))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Export environment: build → read env → mutate → reread → verify diff
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_export_env_mutate_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:title1 (#(\"Alpha\" 0 5 (:parent (#(\"Alpha\" 0 5 (:parent #5))))))) (:author1 (#(\"Bob\" 0 3 (:parent (#(\"Bob\" 0 3 (:parent #5))))))) (:email1 \"bob@test\") (:date1 (#(\"2024-01-01\" 0 10 (:parent (#(\"2024-01-01\" 0 10 (:parent #5))))))) (:options nil) (:export-file nil) (:title2 nil) (:author2 (#(\"Bob\" 0 3 (:parent (#(\"Bob\" 0 3 (:parent #5))))))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ox)
  (insert "#+TITLE: Alpha\n#+AUTHOR: Bob\n#+EMAIL: bob@test\n#+DATE: 2024-01-01\n\n")
  (insert "* H1\n* H2\n")
  (let ((r '()))
    ;; initial env
    (let ((e (org-export-get-environment)))
      (push (list :title1 (plist-get e :title)) r)
      (push (list :author1 (plist-get e :author)) r)
      (push (list :email1 (plist-get e :email)) r)
      (push (list :date1 (plist-get e :date)) r))
    ;; add export-specific keywords
    (goto-char (point-min))
    (search-forward "#+AUTHOR:")
    (end-of-line)
    (insert "\n#+OPTIONS: num:nil toc:t\n#+EXPORT_FILE_NAME: output.pdf")
    (let ((e (org-export-get-environment)))
      (push (list :options (plist-get e :options)) r)
      (push (list :export-file (plist-get e :export-file-name)) r))
    ;; remove TITLE
    (goto-char (point-min))
    (search-forward "#+TITLE:")
    (beginning-of-line)
    (kill-line)
    (let ((e (org-export-get-environment)))
      (push (list :title2 (plist-get e :title)) r)
      (push (list :author2 (plist-get e :author)) r))
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// Footnote: create → delete → renumber → gap handling
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_footnote_create_delete_renumber() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-footnote-renumber-fn-n)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* Footnotes\n")
  (insert "First[fn:aaa] and second[fn:bbb] and third[fn:ccc].\n")
  (insert "[fn:aaa] A.\n[fn:bbb] B.\n[fn:ccc] C.\n")
  (let ((r '()))
    ;; initial labels
    (push (list :init-labels (mapcar (lambda (fr) (org-element-property :label fr))
                                     (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    ;; delete reference to bbb (not the definition, just the reference)
    (goto-char (point-min))
    (search-forward "[fn:bbb]")
    (replace-match "")
    ;; renumber
    (org-footnote-renumber-fn-n)
    (push (list :after-delete-and-renumber
                (mapcar (lambda (fr) (list (org-element-property :label fr)
                                           (substring-no-properties
                                            (buffer-substring-no-properties
                                             (org-element-property :begin fr)
                                             (org-element-property :end fr)))))
                        (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    ;; normalize
    (org-footnote-normalize 'sort)
    (push (list :after-normalize-labels
                (mapcar (lambda (fr) (org-element-property :label fr))
                        (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

// ═══════════════════════════════════════════════════════════════════════
// org-element: secondary-p for elements in titles, captions, and tags
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn combo49_element_secondary_p_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp [org-element-deferred org-element--headline-parse-title (t) t])""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-element)
  (insert "* Headline with *bold* :tag1:tag2:\n")
  (insert "#+CAPTION: Table /captioned/.\n")
  (insert "| a | b |\n|---+---|\n| 1 | 2 |\n")
  (let ((r '()))
    ;; secondary check: bold in headline title
    (goto-char (point-min))
    (search-forward "*bold*")
    (push (list :bold-in-title-secondary (org-element-secondary-p (org-element-context))) r)
    ;; secondary check: italic in caption
    (search-forward "/captioned/")
    (push (list :italic-in-caption-secondary (org-element-secondary-p (org-element-context))) r)
    ;; secondary check: bold NOT in title but in body
    (goto-char (point-min))
    (insert "\n\nParagraph with *bold-body*.\n")
    (search-forward "*bold-body*")
    (push (list :bold-in-body-secondary (org-element-secondary-p (org-element-context))) r)
    ;; secondary property name
    (let* ((tree (org-element-parse-buffer))
           (hl (car (org-element-map tree 'headline #'identity)))
           (title-bold (car (org-element-map hl 'bold #'identity))))
      (when title-bold
        (push (list :title-bold-secondary-property (org-element-secondary-p title-bold)) r)))
    (nreverse r)))"##,
        expect,
    );
}
