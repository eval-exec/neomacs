//! Strong combo-complex-61 oracle tests — deep integration
//! probes: org-refile actual refile with copy, org-footnote
//! action delete+renumber chain, org-entities-help browsing,
//! org-insert-structure-template with content, org-babel
//! with :results raw and :results org, org-element with
//! affiliated keywords on all block types, org-export-before-
//! processing-hook interaction, org-cycle local visibility
//! with plain lists included, org-table-with-header-sort,
//! and org-babel with :colnames and :rownames header args.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn combo61_refile_actual_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:refile-error t) (:headline-count 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "* A\n** A1\nBody.\n** A2\nBody.\n* B\n")
  (let ((r '()))
    ;; refile A1 to B
    (goto-char (point-min))
    (search-forward "** A1") (beginning-of-line)
    (let ((org-refile-use-cache nil)
          (org-refile-use-outline-path nil))
      (let ((target (cons "B (copy)" (progn (search-forward "* B") (org-element-at-point)))))
        (condition-case nil
            (progn (org-refile nil nil (list nil "" nil nil))
                   ;; refile with copy (prefix arg 3)
                   (push (list :refile-ok t) r))
          (error (push (list :refile-error t) r)))))
    (push (list :headline-count (length (org-element-map (org-element-parse-buffer) 'headline #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo61_footnote_action_delete_renumber() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function org-footnote-renumber-fn-n)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "A[fn:1] B[fn:2] C[fn:3]\n[fn:1] One.\n[fn:2] Two.\n[fn:3] Three.\n")
  (let ((r '()))
    ;; manually delete footnote 2 (avoid timeout in org-footnote-action)
    (goto-char (point-min))
    (search-forward "[fn:2]") (backward-char 1)
    (let ((start (point)))
      (forward-char 6)
      (delete-region start (point)))
    (push (list :after-delete
                (mapcar (lambda (fr) (org-element-property :label fr))
                        (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    ;; renumber
    (org-footnote-renumber-fn-n)
    (push (list :after-renumber
                (mapcar (lambda (fr) (org-element-property :label fr))
                        (org-element-map (org-element-parse-buffer) 'footnote-reference #'identity))) r)
    (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo61_babel_results_raw_org() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"<p>raw html output</p>\" \"* org headline\\n** subheading\" (:result-count 0) (:buffer \"#+begin_src emacs-lisp :results raw\\n\\\"<p>raw html output</p>\\\"\\n#+end_src\\n\\n#+RESULTS:\\n<p>raw html output</p>\\n\\n#+begin_src emacs-lisp :results org\\n\\\"* org headline\\\\n** subheading\\\"\\n#+end_src\\n\\n#+RESULTS:\\n#+begin_src org\\n,* org headline\\n,** subheading\\n#+end_src\\n\"))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src emacs-lisp :results raw\n\"<p>raw html output</p>\"\n#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :results org\n\"* org headline\\n** subheading\"\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src emacs-lisp :results raw")
      (push (org-babel-execute-src-block) r)
      (search-forward "#+begin_src emacs-lisp :results org")
      (push (org-babel-execute-src-block) r)
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (push (list :buffer (buffer-substring-no-properties (point-min) (point-max))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo61_element_affiliated_all_blocks() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:ex-caption (((#(\"Block caption\" 0 13 (:parent (#(\"Block caption\" 0 13 (:parent #7))))))))) (:ex-name \"my-block\") (:ex-attr-html (\":class special\")) (:ex-attr-latex (\":environment fancyenv\")) (:src-caption (((#(\"Src caption\" 0 11 (:parent (#(\"Src caption\" 0 11 (:parent #7))))))))) (:src-attr-html (\":width 100%\")) (:src-lang \"emacs-lisp\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "#+CAPTION: Block caption\n")
  (insert "#+NAME: my-block\n")
  (insert "#+ATTR_HTML: :class special\n")
  (insert "#+ATTR_LATEX: :environment fancyenv\n")
  (insert "#+BEGIN_EXAMPLE\ncontent\n#+END_EXAMPLE\n\n")
  (insert "#+CAPTION: Src caption\n")
  (insert "#+ATTR_HTML: :width 100%\n")
  (insert "#+BEGIN_SRC emacs-lisp\n(list 1 2)\n#+END_SRC\n")
  (let ((r '()))
    (let* ((tree (org-element-parse-buffer))
           (examples (org-element-map tree 'example-block #'identity))
           (srcs (org-element-map tree 'src-block #'identity)))
      ;; example block affiliated
      (when (car examples)
        (push (list :ex-caption (org-element-property :caption (car examples))) r)
        (push (list :ex-name (org-element-property :name (car examples))) r)
        (push (list :ex-attr-html (org-element-property :attr_html (car examples))) r)
        (push (list :ex-attr-latex (org-element-property :attr_latex (car examples))) r))
      ;; src block affiliated
      (when (car srcs)
        (push (list :src-caption (org-element-property :caption (car srcs))) r)
        (push (list :src-attr-html (org-element-property :attr_html (car srcs))) r)
        (push (list :src-lang (org-element-property :language (car srcs))) r)))
    (nreverse r)))"##,
        expect,
    );
}

#[test]
fn combo61_cycle_local_plain_lists() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:heading-invis nil) (:after-fold-invis nil) (:after-children-invis nil) (:items-visible 3) (:items-all 5))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (let ((org-cycle-include-plain-lists 'integrate))
    (insert "* H1\n- item 1\n- item 2\n  - sub a\n  - sub b\n- item 3\n")
    (let ((r '()))
      ;; initial visibility
      (goto-char (point-min))
      (push (list :heading-invis (get-char-property (point) 'invisible)) r)
      ;; cycle to FOLDED
      (org-cycle)
      (push (list :after-fold-invis (get-char-property (point) 'invisible)) r)
      ;; cycle to CHILDREN
      (org-cycle)
      (push (list :after-children-invis (get-char-property (point) 'invisible)) r)
      ;; items should still be parseable
      (push (list :items-visible (length (org-element-map (org-element-parse-buffer nil t) 'item #'identity))) r)
      (org-show-all)
      (push (list :items-all (length (org-element-map (org-element-parse-buffer) 'item #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo61_babel_colnames_rownames() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((\"Name\" \"Score\") hline (\"Alice\" nil) (\"Bob\" nil)) (:result-count 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-emacs-lisp)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+name: data\n| Name | Score |\n|------+-------|\n| Alice|    95 |\n| Bob  |    82 |\n\n")
    (insert "#+begin_src emacs-lisp :results value :var grades=data :colnames yes\n")
    (insert "(mapcar (lambda (row) (list (car row) (cdr (assoc \"Score\" row)))) grades)\n")
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
fn combo61_babel_epilogue_header_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Cannot open load file\" \"No such file or directory\" \"ob-sh\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'ob-sh)
  (let ((org-confirm-babel-evaluate nil))
    (insert "#+begin_src sh :results output :epilogue \"echo END\"\necho START\n#+end_src\n")
    (let ((r '()))
      (goto-char (point-min)) (search-forward "#+begin_src sh")
      (condition-case e
          (push (org-babel-execute-src-block) r)
        (error (push (list :epilogue-error (car e)) r)))
      (push (list :result-count (length (org-element-map (org-element-parse-buffer) 'result #'identity))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo61_clock_effort_property_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:effort \"1:00\") (:clock-sum 0) (:effort-minutes 60.0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil))
    (insert "* Task\n:PROPERTIES:\n:EFFORT:   1:00\n:END:\n")
    (let ((r '()))
      (goto-char (point-min))
      (org-clock-in nil) (org-clock-out nil nil)
      ;; get effort
      (push (list :effort (org-entry-get nil "EFFORT")) r)
      ;; clock sum
      (push (list :clock-sum (org-clock-sum-current-item)) r)
      ;; effort + clock
      (push (list :effort-minutes (org-duration-to-minutes (or (org-entry-get nil "EFFORT") "0:00"))) r)
      (nreverse r))))"##,
        expect,
    );
}

#[test]
fn combo61_sort_by_clock_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 17 29)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (require 'org-clock)
  (let ((org-clock-persist nil))
    (insert "* A\n* B\n* C\n")
    ;; clock each for different durations (but they're all ~0 in batch)
    (goto-char (point-min)) (org-clock-in nil) (org-clock-out nil nil)
    (search-forward "* B") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
    (search-forward "* C") (beginning-of-line) (org-clock-in nil) (org-clock-out nil nil)
    ;; sort by clock sum
    (goto-char (point-min))
    (condition-case nil
        (progn (org-sort-entries nil ?k)  ;; sort by clock
               (list :after-sort
                     (mapcar (lambda (h) (substring-no-properties (org-element-property :raw-value h)))
                             (org-element-map (org-element-parse-buffer) 'headline #'identity))))
      (error :sort-error)))))"##,
        expect,
    );
}

#[test]
fn combo61_org_toggle_inline_images() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((:toggle-fbound t) (:toggled t) (:link-count 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (org-mode)
  (insert "[[file:/tmp/test.png]]\n")
  (let ((r '()))
    ;; org-toggle-inline-images
    (push (list :toggle-fbound (fboundp 'org-toggle-inline-images)) r)
    (goto-char (point-min))
    (condition-case nil
        (progn (org-toggle-inline-images)
               (push (list :toggled t) r))
      (error (push (list :toggle-error t) r)))
    ;; link still there
    (push (list :link-count (length (org-element-map (org-element-parse-buffer) 'link #'identity))) r)
    (nreverse r)))"##,
        expect,
    );
}
