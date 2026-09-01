use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_src_edit_switches_indent_writeback_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-src)
  (with-temp-buffer
    (org-mode)
    (insert "* Code\n")
    (insert "#+begin_src emacs-lisp -n -r :results value :exports both\n")
    (insert "  (let ((x 1))\n")
    (insert "    (+ x 2))\n")
    (insert "#+end_src\n")
    (goto-char (point-min))
    (search-forward "(let")
    (let ((before-info (org-babel-get-src-block-info)))
      (org-edit-src-code)
      (let ((edit-mode major-mode)
            (edit-before (buffer-substring-no-properties
                          (point-min) (point-max))))
        (goto-char (point-max))
        (insert ";; tail\n")
        (org-edit-src-exit)
        (goto-char (point-min))
        (search-forward "begin_src")
        (let ((element (org-element-at-point)))
          (list (nth 0 before-info)
                (nth 1 before-info)
                (cdr (assq :exports (nth 2 before-info)))
                edit-mode
                edit-before
                (org-element-property :switches element)
                (org-element-property :parameters element)
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_demarcate_hash_visibility_navigation_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((24 \"emacs-lisp\" \"(let ((x 1))\\n  (+ x 2)\\n  (* x 4))\" \"  (+ x 2)\") ((\"keep-me\" \"split-me\") \"* Code\\n#+name: split-me\\n#+begin_src emacs-lisp :results value replace :cache yes\\n  (let ((x 1))\\n    (+ x 2)\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace :cache yes\\n\\n  (* x 4))\\n#+end_src\\n\\n#+NAME: keep-me\\n#+begin_src emacs-lisp :results output replace\\n(princ \\\"alpha\\\\nbeta\\\")\\n#+end_src\\n#+RESULTS[oldhasholdhash]: keep-me\\n: alpha\\n: beta\\n\") (176 188 \"\\n  (* x 4))\\n\") (119 \"#+begin_src emacs-lisp :results value replace :cache yes\" 119 24 \"#+begin_src emacs-lisp :results value replace :cache yes\") (nil \"d5e81c8a318f3f1f27e56af23b800d1abd05cf48\" nil nil) (((\": alpha\" nil nil) (\": beta\" nil nil) (\"(* x 4)\" nil nil)) 0 ((\": alpha\" nil nil) (\": beta\" nil nil) (\"(* x 4)\" nil nil)) nil) ((headline \"Code\") (src \"split-me\" \"emacs-lisp\" \":results value replace :cache yes\" \"  (let ((x 1))\\n    (+ x 2)\\n\") (src nil \"emacs-lisp\" \":results value replace :cache yes\" \"\\n  (* x 4))\\n\") (src \"keep-me\" \"emacs-lisp\" \":results output replace\" \"(princ \\\"alpha\\\\nbeta\\\")\\n\") (fixed-width \"alpha\\nbeta\")) \"* Code\\n#+name: split-me\\n#+begin_src emacs-lisp :results value replace :cache yes\\n  (let ((x 1))\\n    (+ x 2)\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace :cache yes\\n\\n  (* x 4))\\n#+end_src\\n\\n#+NAME: keep-me\\n#+begin_src emacs-lisp :results output replace\\n(princ \\\"alpha\\\\nbeta\\\")\\n#+end_src\\n#+RESULTS[oldhasholdhash]: keep-me\\n: alpha\\n: beta\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (insert "* Code\n")
    (insert "#+NAME: split-me\n")
    (insert "#+begin_src emacs-lisp :results value replace :cache yes\n")
    (insert "(let ((x 1))\n")
    (insert "  (+ x 2)\n")
    (insert "  (* x 4))\n")
    (insert "#+end_src\n\n")
    (insert "#+NAME: keep-me\n")
    (insert "#+begin_src emacs-lisp :results output replace\n")
    (insert "(princ \"alpha\\nbeta\")\n")
    (insert "#+end_src\n")
    (insert "#+RESULTS[oldhasholdhash]: keep-me\n")
    (insert ": alpha\n: beta\n")
    (let ((offset (lambda (pos) (and pos (- pos (point-min)))))
          split-before split-after mark-summary navigation-summary
          hash-summary visibility-summary parsed)
      (goto-char (point-min))
      (search-forward "(+ x 2)")
      (setq split-before
            (list (funcall offset (org-babel-where-is-src-block-head))
                  (nth 0 (org-babel-get-src-block-info 'no-eval))
                  (nth 1 (org-babel-get-src-block-info 'no-eval))
                  (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position))))
      (org-babel-demarcate-block)
      (setq split-after
            (list (org-babel-src-block-names)
                  (buffer-substring-no-properties
                   (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "(* x 4)")
      (org-babel-mark-block)
      (setq mark-summary
            (list (funcall offset (region-beginning))
                  (funcall offset (region-end))
                  (buffer-substring-no-properties
                   (region-beginning) (region-end))))
      (deactivate-mark)
      (goto-char (point-min))
      (org-babel-next-src-block 2)
      (setq navigation-summary
            (list (funcall offset (point))
                  (buffer-substring-no-properties
                   (line-beginning-position) (line-end-position))
                  (funcall offset (org-babel-where-is-src-block-head))))
      (org-babel-previous-src-block 1)
      (setq navigation-summary
            (append navigation-summary
                    (list (funcall offset (point))
                          (buffer-substring-no-properties
                           (line-beginning-position) (line-end-position)))))
      (goto-char (point-min))
      (search-forward "keep-me")
      (search-forward "begin_src")
      (let* ((info (org-babel-get-src-block-info))
             (hash (org-babel-sha1-hash info))
             (result-pos (org-babel-where-is-src-block-result nil info)))
        (goto-char result-pos)
        (setq hash-summary
              (list (org-babel-current-result-hash info)
                    hash
                    (org-babel-hash-at-point (point))))
        (org-babel-hide-hash)
        (setq hash-summary
              (append hash-summary
                      (list
                       (mapcar (lambda (ov)
                                 (list (funcall offset (overlay-start ov))
                                       (funcall offset (overlay-end ov))
                                       (overlay-get ov 'babel-hash)
                                       (overlay-get ov 'invisible)))
                               (overlays-in (line-beginning-position)
                                            (line-end-position)))))))
      (org-babel-result-hide-all)
      (setq visibility-summary
            (list
             (mapcar
              (lambda (needle)
                (let ((pos (save-excursion
                             (goto-char (point-min))
                             (search-forward needle)
                             (point))))
                  (list needle
                        (invisible-p pos)
                        (get-text-property pos 'invisible))))
              '(": alpha" ": beta" "(* x 4)"))
             (length org-babel-hide-result-overlays)))
      (org-babel-show-result-all)
      (setq visibility-summary
            (append visibility-summary
                    (list
                     (mapcar
                      (lambda (needle)
                        (let ((pos (save-excursion
                                     (goto-char (point-min))
                                     (search-forward needle)
                                     (point))))
                          (list needle
                                (invisible-p pos)
                                (get-text-property pos 'invisible))))
                      '(": alpha" ": beta" "(* x 4)"))
                     org-babel-hide-result-overlays)))
      (setq parsed
            (org-element-map (org-element-parse-buffer)
                '(headline src-block keyword fixed-width)
              (lambda (el)
                (pcase (org-element-type el)
                  ('headline
                   (list 'headline
                         (org-element-property :raw-value el)))
                  ('src-block
                   (list 'src
                         (org-element-property :name el)
                         (org-element-property :language el)
                         (org-element-property :parameters el)
                         (org-element-property :value el)))
                  ('keyword
                   (list 'keyword
                         (org-element-property :key el)
                         (org-element-property :value el)))
                  ('fixed-width
                   (list 'fixed-width
                         (org-element-property :value el)))))))
      (list split-before
            split-after
            mark-summary
            navigation-summary
            hash-summary
            visibility-summary
            parsed
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_babel_noweb_expand_export_processing_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"yes\" \"5967780cccbd23934e48e31153d6b4602782970b\" t \"(defun helper (x) (+ x 10))\\n(helper 5)\\n\" \"#+begin_src emacs-lisp :noweb yes :exports both\\n(defun helper (x) (+ x 10))\\n(helper 5)\\n#+end_src\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-exp)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (insert "#+PROPERTY: header-args:emacs-lisp :results value replace\n")
    (insert "#+NAME: helper\n")
    (insert "#+begin_src emacs-lisp\n")
    (insert "(defun helper (x) (+ x 10))\n")
    (insert "#+end_src\n\n")
    (insert "#+begin_src emacs-lisp :noweb yes :exports both\n")
    (insert "<<helper>>\n")
    (insert "(helper 5)\n")
    (insert "#+end_src\n")
    (goto-char (point-min))
    (search-forward ":noweb")
    (let* ((info (org-babel-get-src-block-info))
           (expanded (org-babel-expand-src-block))
           (hash (org-babel-sha1-hash info :export))
           (exported-code (let ((org-babel-exp-reference-buffer (current-buffer)))
                            (org-babel-exp-code info 'block))))
      (list (cdr (assq :noweb (nth 2 info)))
            hash
            (not (null (string-match-p "(defun helper" expanded)))
            expanded
            exported-code))))"##,
        expect,
    );
}

#[test]
fn org_babel_inline_and_block_result_replace_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: calc\n")
    (insert "#+begin_src emacs-lisp :results value replace drawer\n")
    (insert "(list 1 2 3)\n")
    (insert "#+end_src\n\n")
    (insert "Inline src_emacs-lisp[:results raw replace]{(+ 4 5)} end.\n")
    (let ((org-confirm-babel-evaluate nil))
      (org-babel-execute-buffer))
    (let ((after-first
           (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "(list 1 2 3)")
      (replace-match "(list 3 2 1)" t t)
      (goto-char (point-min))
      (search-forward "(+ 4 5)")
      (replace-match "(* 2 7)" t t)
      (org-babel-execute-buffer)
      (list after-first
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
    );
}

#[test]
fn org_src_preserve_indentation_save_abort_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-src)
  (with-temp-buffer
    (let ((org-src-preserve-indentation t)
          (org-edit-src-content-indentation 4)
          (org-src-window-setup 'current-window))
      (org-mode)
      (insert "* Code\n")
      (insert "#+begin_src emacs-lisp\n")
      (insert "    (message \"one\")\n")
      (insert "      (message \"two\")\n")
      (insert "#+end_src\n")
      (goto-char (point-min))
      (search-forward "one")
      (let (edit-before edit-after-save)
        (org-edit-src-code)
        (setq edit-before (buffer-substring-no-properties
                           (point-min) (point-max)))
        (goto-char (point-max))
        (insert "  (message \"saved\")\n")
        (org-edit-src-save)
        (setq edit-after-save
              (with-current-buffer (marker-buffer org-src--beg-marker)
                (buffer-substring-no-properties (point-min) (point-max))))
        (insert "  (message \"aborted\")\n")
        (org-edit-src-abort)
        (list edit-before
              edit-after-save
              (buffer-substring-no-properties (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_edit_special_example_and_src_modes_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-src)
  (with-temp-buffer
    (let ((org-src-window-setup 'current-window)
          (org-edit-fixed-width-region-mode 'fundamental-mode))
      (org-mode)
      (insert "* Mixed\n")
      (insert "#+begin_example\n")
      (insert "example line\n")
      (insert "#+end_example\n\n")
      (insert ": fixed\n: width\n\n")
      (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
      (let (example-mode example-text fixed-mode fixed-text src-mode)
        (goto-char (point-min))
        (search-forward "example line")
        (org-edit-special)
        (setq example-mode major-mode
              example-text (buffer-substring-no-properties
                            (point-min) (point-max)))
        (goto-char (point-max))
        (insert "example added\n")
        (org-edit-src-exit)
        (goto-char (point-min))
        (search-forward "fixed")
        (org-edit-special)
        (setq fixed-mode major-mode
              fixed-text (buffer-substring-no-properties
                          (point-min) (point-max)))
        (goto-char (point-max))
        (insert "third\n")
        (org-edit-src-exit)
        (goto-char (point-min))
        (search-forward "(+ 1 2)")
        (org-edit-special)
        (setq src-mode major-mode)
        (org-edit-src-abort)
        (list example-mode
              example-text
              fixed-mode
              fixed-text
              src-mode
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_babel_update_body_remove_result_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: calc\n")
    (insert "#+begin_src emacs-lisp :results value replace\n")
    (insert "(+ 1 2)\n")
    (insert "#+end_src\n")
    (insert "#+RESULTS: calc\n: 3\n\n")
    (insert "Inline src_emacs-lisp[:results raw replace]{(+ 2 3)} {{{results(=5=)}}}.\n")
    (goto-char (point-min))
    (search-forward "(+ 1 2)")
    (let ((info-before (org-babel-get-src-block-info)))
      (org-babel-update-block-body "(let ((x 4))\n  (+ x 6))")
      (let ((after-update
             (buffer-substring-no-properties (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-remove-result)
        (goto-char (point-min))
        (search-forward "src_emacs-lisp")
        (org-babel-remove-inline-result)
        (list (nth 1 info-before)
              after-update
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_babel_named_navigation_results_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (with-temp-buffer
    (org-mode)
    (insert "* Code\n")
    (insert "#+NAME: alpha\n")
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n")
    (insert "#+RESULTS: alpha\n: 3\n\n")
    (insert "** More\n")
    (insert "#+NAME: beta\n")
    (insert "#+begin_src emacs-lisp\n(+ 3 4)\n#+end_src\n")
    (insert "#+RESULTS: beta\n: 7\n")
    (let ((offset
           (lambda (pos) (and pos (- pos (point-min)))))
          alpha-head alpha-result beta-head beta-result current-head)
      (setq alpha-head (funcall offset (org-babel-find-named-block "alpha"))
            beta-head (funcall offset (org-babel-find-named-block "beta"))
            alpha-result (funcall offset
                                  (org-babel-find-named-result "alpha"))
            beta-result (funcall offset
                                 (org-babel-find-named-result "beta")))
      (goto-char (point-min))
      (search-forward "(+ 3 4)")
      (setq current-head (funcall offset (org-babel-where-is-src-block-head)))
      (org-babel-goto-named-result "alpha")
      (let ((after-result (list (funcall offset (point))
                                (buffer-substring-no-properties
                                 (line-beginning-position)
                                 (line-end-position)))))
        (org-babel-goto-named-src-block "beta")
        (list (org-babel-src-block-names)
              (org-babel-result-names)
              alpha-head
              alpha-result
              beta-head
              beta-result
              current-head
              after-result
              (funcall offset (point))
              (buffer-substring-no-properties
               (line-beginning-position)
               (line-end-position))))))"##,
        expect,
    );
}

#[test]
fn org_babel_subtree_execute_hooks_results_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (let ((org-confirm-babel-evaluate
           (lambda (lang body)
             (push (list 'confirm lang
                         (replace-regexp-in-string "[ \t\n]+" " " body))
                   events)
             nil))
          (org-babel-after-execute-hook
           (list (lambda ()
                   (push (list 'after
                               (org-babel-where-is-src-block-head)
                               (save-excursion
                                 (org-babel-goto-src-block-head)
                                 (org-element-property
                                  :name (org-element-at-point))))
                         events))))
          events)
      (org-mode)
      (insert "#+PROPERTY: header-args:emacs-lisp :results value replace drawer\n")
      (insert "* Run\n")
      (insert ":PROPERTIES:\n:header-args:emacs-lisp: :var base=10\n:END:\n")
      (insert "#+NAME: first\n")
      (insert "#+begin_src emacs-lisp\n(+ base 1)\n#+end_src\n\n")
      (insert "** Child\n")
      (insert "#+NAME: second\n")
      (insert "#+begin_src emacs-lisp :var base=20\n(+ base 2)\n#+end_src\n")
      (insert "* Skip\n")
      (insert "#+NAME: outside\n")
      (insert "#+begin_src emacs-lisp\n(+ 100 3)\n#+end_src\n")
      (goto-char (point-min))
      (search-forward "* Run")
      (beginning-of-line)
      (org-babel-execute-subtree)
      (let ((after-subtree
             (buffer-substring-no-properties (point-min) (point-max)))
            (subtree-events (nreverse events))
            (first-result (save-excursion
                            (org-babel-goto-named-result "first")
                            (buffer-substring-no-properties
                             (line-beginning-position)
                             (line-end-position))))
            (second-result (save-excursion
                             (org-babel-goto-named-result "second")
                             (buffer-substring-no-properties
                              (line-beginning-position)
                              (line-end-position))))
            (outside-result (org-babel-find-named-result "outside")))
        (setq events nil)
        (goto-char (point-min))
        (search-forward "(+ 100 3)")
        (org-babel-execute-src-block)
        (let ((outside-after-one
               (save-excursion
                 (org-babel-goto-named-result "outside")
                 (buffer-substring-no-properties
                  (line-beginning-position)
                  (line-end-position)))))
          (goto-char (point-min))
          (search-forward "(+ base 1)")
          (replace-match "(* base 3)" t t)
          (org-babel-execute-buffer)
          (list subtree-events
                first-result
                second-result
                outside-result
                outside-after-one
                (nreverse events)
                after-subtree
                (buffer-substring-no-properties
                 (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_tangle_collect_single_block_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"a.el\") (\"a.el\") ((\"a.el\" ((\"emacs-lisp\" \"helper\" nil \"no\" \"(defun helper () 10)\" nil) (\"emacs-lisp\" \"Second:1\" nil \"yes\" \"(defun helper () 10)\\n(+ (helper) 5)\" nil))) (\"b.el\" ((\"emacs-lisp\" \"Other:1\" \"no\" \"no\" \"(message \\\"other\\\")\" nil)))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-tangle)
  (let* ((root (make-temp-file "org-tangle-collect" t))
         (org-file (expand-file-name "main.org" root))
         (out-a (expand-file-name "a.el" root))
         (out-b (expand-file-name "b.el" root)))
    (unwind-protect
        (with-current-buffer (find-file-noselect org-file)
          (erase-buffer)
          (org-mode)
          (insert "#+PROPERTY: header-args:emacs-lisp :comments both\n")
          (insert "* First\nText for comments.\n")
          (insert "#+NAME: helper\n")
          (insert "#+begin_src emacs-lisp :tangle \"" out-a "\"\n")
          (insert "(defun helper () 10)\n")
          (insert "#+end_src\n\n")
          (insert "* Second\n")
          (insert "#+begin_src emacs-lisp :noweb yes :tangle \"" out-a "\"\n")
          (insert "<<helper>>\n(+ (helper) 5)\n")
          (insert "#+end_src\n\n")
          (insert "* Other\n")
          (insert "#+begin_src emacs-lisp :tangle \"" out-b "\" :comments no\n")
          (insert "(message \"other\")\n")
          (insert "#+end_src\n")
          (save-buffer)
          (goto-char (point-min))
          (search-forward ":noweb")
          (let* ((single (org-babel-tangle-single-block 1 t))
                 (collected (org-babel-tangle-collect-blocks "emacs-lisp"))
                 (limited (org-babel-tangle-collect-blocks
                           "emacs-lisp" out-a))
                 (summary
                  (mapcar
                   (lambda (entry)
                     (list (file-name-nondirectory (car entry))
                           (mapcar
                            (lambda (block)
                              (let ((spec (cdr block)))
                                (list (car block)
                                      (nth 3 spec)
                                      (cdr (assq :comments (nth 4 spec)))
                                      (cdr (assq :noweb (nth 4 spec)))
                                      (nth 5 spec)
                                      (nth 6 spec))))
                            (cdr entry))))
                   collected)))
            (list (mapcar (lambda (entry)
                            (file-name-nondirectory (car entry)))
                          single)
                  (mapcar (lambda (entry)
                            (file-name-nondirectory (car entry)))
                          limited)
                  summary)))
      (when (get-file-buffer org-file)
        (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_babel_header_merge_insert_result_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((:results . \"output drawer\") (:var . \"local=3\") (:exports . \"code\")) ((:cache . \"yes\") (:colname-names) (:exports . \"code\") (:hlines . \"no\") (:lexical . \"no\") (:noweb . \"no\") (:result-params \"replace\" \"value\") (:result-type . value) (:results . \"replace value\") (:rowname-names) (:session . \"none\") (:tangle . \"no\") (:var base . 5) (:var extra . 7)) ((:results . \"drawer output replace\") (:exports . \"code\") (:var base . 5) (:var extra . 7) (:var . \"local=3\") (:session . \"none\") (:noweb . \"no\") (:hlines . \"no\") (:tangle . \"no\") (:lexical . \"no\") (:cache . \"yes\") (:result-type . value) (:result-params \"replace\" \"value\") (:rowname-names) (:colname-names)) ((:result-params \"drawer\" \"output\" \"replace\" \"value\") (:exports . \"code\") (:cache . \"yes\") (:var base . 5)) 250 \"#+PROPERTY: header-args:emacs-lisp :results value replace drawer :exports both\\n* Run\\n:PROPERTIES:\\n:header-args:emacs-lisp: :var base=5 :cache yes\\n:END:\\n#+NAME: calc\\n#+begin_src emacs-lisp :var extra=7 :results value replace\\n(+ base extra)\\n#+end_src\\n\\n#+RESULTS[28f671f4c141b24f436060c0f34ea4a7fb63a3ac]: calc\\n:results:\\nline one\\nline two\\n:end:\\n\" \"#+PROPERTY: header-args:emacs-lisp :results value replace drawer :exports both\\n* Run\\n:PROPERTIES:\\n:header-args:emacs-lisp: :var base=5 :cache yes\\n:END:\\n#+NAME: calc\\n#+begin_src emacs-lisp :var extra=7 :results value replace\\n(+ base extra)\\n#+end_src\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (insert "#+PROPERTY: header-args:emacs-lisp :results value replace drawer :exports both\n")
    (insert "* Run\n")
    (insert ":PROPERTIES:\n:header-args:emacs-lisp: :var base=5 :cache yes\n:END:\n")
    (insert "#+NAME: calc\n")
    (insert "#+begin_src emacs-lisp :var extra=7 :results value replace\n")
    (insert "(+ base extra)\n")
    (insert "#+end_src\n")
    (goto-char (point-min))
    (search-forward "begin_src")
    (let* ((parsed (org-babel-parse-header-arguments
                    ":results output drawer :var local=3 :exports code"))
           (info (org-babel-get-src-block-info))
           (merged (org-babel-merge-params (nth 2 info) parsed))
           (processed (org-babel-process-params merged))
           (hash (org-babel-sha1-hash info))
           result-pos after-insert)
      (org-babel-insert-result
       "line one\nline two"
       '("output" "drawer" "replace")
       info
       hash
       "emacs-lisp"
       "0.01")
      (setq result-pos
            (org-babel-where-is-src-block-result nil info hash))
      (setq after-insert
            (buffer-substring-no-properties (point-min) (point-max)))
      (goto-char (point-min))
      (search-forward "begin_src")
      (org-babel-remove-result info)
      (list parsed
            (nth 2 info)
            merged
            (list (assq :result-params processed)
                  (assq :exports processed)
                  (assq :cache processed)
                  (assq :var processed))
            (and result-pos (- result-pos (point-min)))
            after-insert
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_babel_result_read_hide_replace_remove_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (96 252 ((1 2) (3 4)) nil ((\"| 1 | 2 |\" nil nil) (\"line one\" nil nil) (\"line two\" nil nil)) ((\"| 1 | 2 |\" nil nil) (\"line one\" nil nil) (\"line two\" nil nil)) \"#+NAME: table-calc\\n#+begin_src emacs-lisp :results value table replace\\n'((1 2) (3 4))\\n#+end_src\\n#+RESULTS: table-calc\\n| 5 | 6 |\\n| 7 | 8 |\\n\\n#+NAME: drawer-calc\\n#+begin_src emacs-lisp :results output drawer replace\\n(princ \\\"line one\\\\nline two\\\")\\n#+end_src\\n#+RESULTS: drawer-calc\\n:results:\\nline one\\nline two\\n:end:\\n\" \"#+NAME: table-calc\\n#+begin_src emacs-lisp :results value table replace\\n'((1 2) (3 4))\\n#+end_src\\n#+RESULTS: table-calc\\n| 5 | 6 |\\n| 7 | 8 |\\n\\n#+NAME: drawer-calc\\n#+begin_src emacs-lisp :results output drawer replace\\n(princ \\\"line one\\\\nline two\\\")\\n#+end_src\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: table-calc\n")
    (insert "#+begin_src emacs-lisp :results value table replace\n")
    (insert "'((1 2) (3 4))\n")
    (insert "#+end_src\n")
    (insert "#+RESULTS: table-calc\n")
    (insert "| 1 | 2 |\n| 3 | 4 |\n\n")
    (insert "#+NAME: drawer-calc\n")
    (insert "#+begin_src emacs-lisp :results output drawer replace\n")
    (insert "(princ \"line one\\nline two\")\n")
    (insert "#+end_src\n")
    (insert "#+RESULTS: drawer-calc\n")
    (insert ":results:\nline one\nline two\n:end:\n")
    (let ((offset (lambda (pos) (and pos (- pos (point-min)))))
          table-pos drawer-pos table-read drawer-read hidden shown
          after-replace after-remove)
      (setq table-pos (org-babel-find-named-result "table-calc")
            drawer-pos (org-babel-find-named-result "drawer-calc"))
      (goto-char table-pos)
      (setq table-read (org-babel-read-result))
      (goto-char drawer-pos)
      (setq drawer-read (org-babel-read-result))
      (org-babel-result-hide-all)
      (setq hidden
            (mapcar
             (lambda (needle)
               (let ((pos (save-excursion
                            (goto-char (point-min))
                            (search-forward needle)
                            (point))))
                 (list needle
                       (invisible-p pos)
                       (get-text-property pos 'invisible))))
             '("| 1 | 2 |" "line one" "line two")))
      (org-babel-show-result-all)
      (setq shown
            (mapcar
             (lambda (needle)
               (let ((pos (save-excursion
                            (goto-char (point-min))
                            (search-forward needle)
                            (point))))
                 (list needle
                       (invisible-p pos)
                       (get-text-property pos 'invisible))))
             '("| 1 | 2 |" "line one" "line two")))
      (goto-char (point-min))
      (search-forward "table-calc")
      (search-forward "begin_src")
      (let ((info (org-babel-get-src-block-info)))
        (org-babel-insert-result
         '((5 6) (7 8))
         '("replace" "table")
         info nil "emacs-lisp")
        (setq after-replace
              (buffer-substring-no-properties (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "drawer-calc")
      (search-forward "begin_src")
      (org-babel-remove-result)
      (setq after-remove
            (buffer-substring-no-properties (point-min) (point-max)))
      (list (funcall offset table-pos)
            (funcall offset drawer-pos)
            table-read
            drawer-read
            hidden
            shown
            after-replace
            after-remove))))"##,
        expect,
    );
}

#[test]
fn org_babel_tangle_write_noweb_comments_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"nested.el\" \"out.el\") t t \";; Library\\n;; Comment text.\\n;; #+NAME: lib\\n\\n;; [[file:main.org::lib][lib]]\\n(defun lib (x) (+ x 1))\\n;; lib ends here\\n\\n;; [[file:main.org::*Caller][Caller:1]]\\n(defun lib (x) (+ x 1))\\n(lib 4)\\n;; Caller:1 ends here\\n\" \"(message \\\"nested\\\")\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-tangle)
  (require 'ob-emacs-lisp)
  (let* ((root (make-temp-file "org-tangle-write" t))
         (org-file (expand-file-name "main.org" root))
         (out (expand-file-name "out.el" root))
         (nested (expand-file-name "sub/nested.el" root))
         (org-confirm-babel-evaluate nil))
    (unwind-protect
        (with-current-buffer (find-file-noselect org-file)
          (erase-buffer)
          (org-mode)
          (insert "#+PROPERTY: header-args:emacs-lisp :mkdirp yes\n")
          (insert "* Library\nComment text.\n")
          (insert "#+NAME: lib\n")
          (insert "#+begin_src emacs-lisp :tangle \"" out "\" :comments both\n")
          (insert "(defun lib (x) (+ x 1))\n")
          (insert "#+end_src\n\n")
          (insert "* Caller\n")
          (insert "#+begin_src emacs-lisp :noweb yes :tangle \"" out "\" :comments link\n")
          (insert "<<lib>>\n(lib 4)\n")
          (insert "#+end_src\n\n")
          (insert "* Nested\n")
          (insert "#+begin_src emacs-lisp :tangle \"" nested "\"\n")
          (insert "(message \"nested\")\n")
          (insert "#+end_src\n")
          (make-directory (file-name-directory nested) t)
          (save-buffer)
          (let ((files (mapcar #'file-name-nondirectory
                               (org-babel-tangle nil nil "emacs-lisp"))))
            (list (sort files #'string<)
                  (file-exists-p out)
                  (file-exists-p nested)
                  (with-temp-buffer
                    (insert-file-contents out)
                    (buffer-substring-no-properties
                     (point-min) (point-max)))
                  (with-temp-buffer
                    (insert-file-contents nested)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))
      (when (get-file-buffer org-file)
        (kill-buffer (get-file-buffer org-file)))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_src_fontify_coderef_escape_inline_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-src)
  (with-temp-buffer
    (let ((org-src-fontify-natively t)
          (org-src-block-faces '(("emacs-lisp" (:background "gray10"))))
          (org-coderef-label-format "(ref:%s)")
          (org-edit-src-content-indentation 2))
      (org-mode)
      (insert "* Code\n")
      (insert "#+begin_src emacs-lisp -n -r :label-fmt \"<%s>\"\n")
      (insert "(defun demo (x)                         <def>\n")
      (insert "  ;; comment line\n")
      (insert "  (let ((y (+ x 1)))                 <let>\n")
      (insert "    y))\n")
      (insert "#+end_src\n")
      (insert "Inline src_emacs-lisp[:results raw]{(+ 1 2)} and src_text{plain}.\n")
      (insert "#+begin_example -n -r\n")
      (insert ",* escaped headline                  (ref:ex)\n")
      (insert ",#+escaped keyword\n")
      (insert "#+end_example\n")
      (font-lock-ensure (point-min) (point-max))
      (let* ((tree (org-element-parse-buffer))
             (src (car (org-element-map tree 'src-block #'identity)))
             (example (car (org-element-map tree 'example-block #'identity)))
             (inline (org-element-map tree 'inline-src-block
                       (lambda (e)
                         (list (org-element-property :language e)
                               (org-element-property :value e)
                               (org-element-property :parameters e)))))
             (fmt-src (org-src-coderef-format src))
             (fmt-ex (org-src-coderef-format example))
             (regexp-src (org-src-coderef-regexp fmt-src))
             (regexp-src-let (org-src-coderef-regexp fmt-src "let"))
             (regexp-ex (org-src-coderef-regexp fmt-ex))
             (probes
              (mapcar
               (lambda (needle)
                 (save-excursion
                   (goto-char (point-min))
                   (search-forward needle)
                   (list needle
                         (get-text-property (match-beginning 0) 'face)
                         (get-text-property (match-beginning 0)
                                            'font-lock-face)
                         (get-text-property (match-beginning 0)
                                            'font-lock-fontified)
                         (get-text-property (match-beginning 0) 'display))))
               '("defun" "comment line" "(+ 1 2)" "src_text" "escaped headline")))
             escaped-string unescaped-string region-after-unescape)
        (setq escaped-string
              (org-escape-code-in-string "* H\n#+K: v\n,,* already\nbody"))
        (setq unescaped-string (org-unescape-code-in-string escaped-string))
        (goto-char (point-min))
        (search-forward ",* escaped headline")
        (beginning-of-line)
        (let ((beg (point)))
          (search-forward ",#+escaped keyword")
          (end-of-line)
          (org-unescape-code-in-region beg (point))
          (setq region-after-unescape
                (buffer-substring-no-properties beg (point))))
        (list (list fmt-src fmt-ex regexp-src regexp-src-let regexp-ex)
              (mapcar (lambda (line)
                        (list line
                              (string-match regexp-src line)
                              (and (string-match regexp-src line)
                                   (match-string 3 line))
                              (string-match regexp-src-let line)
                              (and (string-match regexp-ex line)
                                   (match-string 3 line))))
                      '("(defun demo (x)                         <def>"
                        "  (let ((y (+ x 1)))                 <let>"
                        ",* escaped headline                  (ref:ex)"))
              inline
              probes
              escaped-string
              unescaped-string
              region-after-unescape
              (buffer-substring-no-properties
               (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_src_edit_buffer_coordinates_multi_block_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-src)
  (with-temp-buffer
    (let ((org-src-window-setup 'current-window)
          (org-src-preserve-indentation nil)
          (org-edit-src-content-indentation 2))
      (org-mode)
      (insert "* Source edit\n")
      (insert "#+NAME: first\n")
      (insert "#+begin_src emacs-lisp -n -r :results value\n")
      (insert "  (let ((x 1))\n")
      (insert "    (+ x 2))\n")
      (insert "#+end_src\n\n")
      (insert "#+NAME: second\n")
      (insert "#+begin_src emacs-lisp :results output replace\n")
      (insert "  (princ \"a\")\n")
      (insert "  (princ \"b\")\n")
      (insert "#+end_src\n")
      (let (first-summary after-first-save second-summary after-abort)
        (goto-char (point-min))
        (search-forward "(+ x 2)")
        (let* ((src (org-element-at-point))
               (area (org-src--contents-area src))
               (beg (copy-marker (nth 0 area)))
               (end (copy-marker (nth 1 area)))
               (coord (org-src--coordinates (point) beg end)))
          (org-edit-src-code)
          (setq first-summary
                (list (buffer-name)
                      major-mode
                      (org-src-edit-buffer-p)
                      (eq (org-src-source-buffer)
                          (marker-buffer org-src--beg-marker))
                      (org-src-source-type)
                      (buffer-substring-no-properties
                       (point-min) (point-max))
                      coord
                      (with-current-buffer (org-src-source-buffer)
                        (list (buffer-name (org-src--edit-buffer beg end))
                              (org-src--coordinates
                               (marker-position beg) beg end)
                              (org-src--coordinates
                               (marker-position end) beg end)))))
          (goto-char (point-min))
          (search-forward "(+ x 2)")
          (replace-match "(* x 3)" t t)
          (goto-char (point-max))
          (insert ";; saved tail\n")
          (org-edit-src-save)
          (setq after-first-save
                (with-current-buffer (marker-buffer org-src--beg-marker)
                  (buffer-substring-no-properties
                   (point-min) (point-max))))
          (org-edit-src-exit))
        (goto-char (point-min))
        (search-forward "(princ \"b\")")
        (let* ((src (org-element-at-point))
               (area (org-src--contents-area src))
               (beg (copy-marker (nth 0 area)))
               (end (copy-marker (nth 1 area))))
          (org-edit-special)
          (setq second-summary
                (list (buffer-name)
                      major-mode
                      (org-src-edit-buffer-p)
                      (org-src-source-type)
                      (buffer-substring-no-properties
                       (point-min) (point-max))
                      (with-current-buffer (org-src-source-buffer)
                        (buffer-name (org-src--edit-buffer beg end)))))
          (goto-char (point-min))
          (search-forward "(princ \"a\")")
          (replace-match "(princ \"aborted\")" t t)
          (org-edit-src-abort)
          (setq after-abort
                (buffer-substring-no-properties
                 (point-min) (point-max))))
        (list first-summary
              after-first-save
              second-summary
              after-abort
              (mapcar (lambda (block)
                        (list (org-element-property :name block)
                              (org-element-property :switches block)
                              (org-element-property :parameters block)
                          (org-element-property :value block)))
                      (org-element-map
                          (org-element-parse-buffer)
                          'src-block #'identity))))))"##,
        expect,
    );
}

#[test]
fn org_babel_hash_hide_mutate_reexecute_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (let ((org-confirm-babel-evaluate nil))
      (org-mode)
      (insert "#+PROPERTY: header-args:emacs-lisp :results value replace :cache yes\n")
      (insert "#+NAME: cached\n")
      (insert "#+begin_src emacs-lisp :var x=4\n")
      (insert "(list \"value\" x (* x x))\n")
      (insert "#+end_src\n\n")
      (insert "#+NAME: output\n")
      (insert "#+begin_src emacs-lisp :results output drawer replace\n")
      (insert "(princ \"alpha\\nbeta\")\n")
      (insert "#+end_src\n")
      (goto-char (point-min))
      (search-forward "cached")
      (search-forward "begin_src")
      (let* ((info-before (org-babel-get-src-block-info))
             (hash-before (org-babel-sha1-hash info-before))
             (result-before (org-babel-execute-src-block nil info-before))
             (pos-before (org-babel-where-is-src-block-result
                          nil info-before hash-before))
             (current-hash-before (org-babel-current-result-hash info-before))
             (read-before (save-excursion
                            (goto-char pos-before)
                            (forward-line 1)
                            (org-babel-read-result))))
        (goto-char (point-min))
        (search-forward "output")
        (search-forward "begin_src")
        (let* ((output-info (org-babel-get-src-block-info))
               (output-result (org-babel-execute-src-block nil output-info))
               (output-pos (org-babel-where-is-src-block-result nil output-info))
               (output-read (save-excursion
                              (goto-char output-pos)
                              (forward-line 1)
                              (org-babel-read-result))))
          (org-babel-result-hide-all)
          (let ((hidden
                 (mapcar
                  (lambda (needle)
                    (save-excursion
                      (goto-char (point-min))
                      (search-forward needle)
                      (list needle
                            (invisible-p (point))
                            (get-text-property (point) 'invisible))))
                  '("value" "alpha" "beta"))))
            (goto-char (point-min))
            (search-forward "(* x x)")
            (replace-match "(* x x x)" t t)
            (goto-char (point-min))
            (search-forward "cached")
            (search-forward "begin_src")
            (let* ((info-after-edit (org-babel-get-src-block-info))
                   (hash-after-edit
                    (org-babel-sha1-hash info-after-edit))
                   (current-hash-after-edit
                    (org-babel-current-result-hash info-after-edit))
                   (result-after
                    (org-babel-execute-src-block nil info-after-edit))
                   (pos-after
                    (org-babel-where-is-src-block-result
                     nil info-after-edit hash-after-edit))
                   (read-after
                    (save-excursion
                      (goto-char pos-after)
                      (forward-line 1)
                      (org-babel-read-result))))
              (org-babel-show-result-all)
              (let ((shown
                     (mapcar
                      (lambda (needle)
                        (save-excursion
                          (goto-char (point-min))
                          (search-forward needle)
                          (list needle
                                (invisible-p (point))
                                (get-text-property (point) 'invisible))))
                      '("value" "alpha" "beta"))))
                (list (nth 0 info-before)
                      (nth 1 info-before)
                      (cdr (assq :cache (nth 2 info-before)))
                      hash-before
                      result-before
                      (and pos-before (- pos-before (point-min)))
                      current-hash-before
                      read-before
                      output-result
                      (and output-pos (- output-pos (point-min)))
                      output-read
                      hidden
                      hash-after-edit
                      current-hash-after-edit
                      result-after
                      (and pos-after (- pos-after (point-min)))
                      read-after
                      shown
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_insert_remove_file_example_result_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (let* ((root (make-temp-file "org-babel-result" t))
         (result-file (expand-file-name "out data.txt" root)))
    (unwind-protect
        (with-temp-buffer
          (setq default-directory root)
          (org-mode)
          (insert "* Results\n")
          (insert "#+NAME: output-block\n")
          (insert "#+begin_src emacs-lisp :results output replace\n")
          (insert "(princ \"old\")\n")
          (insert "#+end_src\n\n")
          (insert "#+NAME: drawer-block\n")
          (insert "#+begin_src emacs-lisp :results value drawer replace\n")
          (insert "(list 1 2)\n")
          (insert "#+end_src\n\n")
          (insert "#+NAME: file-block\n")
          (insert "#+begin_src emacs-lisp :results file link replace :file \"out data.txt\"\n")
          (insert "result-file\n")
          (insert "#+end_src\n\n")
          (let ((snapshot
                 (lambda (label)
                   (list label
                         (org-element-map (org-element-parse-buffer)
                             '(src-block example-block fixed-width drawer
                               keyword link)
                           (lambda (el)
                             (list (org-element-type el)
                                   (org-element-property :name el)
                                   (org-element-property :key el)
                                   (org-element-property :value el)
                                   (org-element-property :type el)
                                   (org-element-property :path el)
                                   (org-element-property :begin el)
                                   (org-element-property :end el))))
                         (buffer-substring-no-properties
                          (point-min) (point-max)))))
                states output-read drawer-read file-link remove-keep
                remove-full example-lower example-upper file-result)
            (push (funcall snapshot 'initial) states)
            (goto-char (point-min))
            (search-forward "output-block")
            (search-forward "begin_src")
            (let ((info (org-babel-get-src-block-info)))
              (org-babel-insert-result "alpha\nbeta"
                                       '("output" "replace")
                                       info nil "emacs-lisp")
              (setq output-read
                    (save-excursion
                      (goto-char (org-babel-where-is-src-block-result
                                  nil info))
                      (forward-line 1)
                      (org-babel-read-result))))
            (push (funcall snapshot 'after-output) states)
            (goto-char (point-min))
            (search-forward "drawer-block")
            (search-forward "begin_src")
            (let ((info (org-babel-get-src-block-info)))
              (org-babel-insert-result '((1 2) (3 4))
                                       '("value" "drawer" "replace")
                                       info nil "emacs-lisp")
              (setq drawer-read
                    (save-excursion
                      (goto-char (org-babel-where-is-src-block-result
                                  nil info))
                      (forward-line 2)
                      (org-babel-read-result))))
            (push (funcall snapshot 'after-drawer) states)
            (goto-char (point-min))
            (search-forward "file-block")
            (search-forward "begin_src")
            (let ((info (org-babel-get-src-block-info)))
              (with-temp-file result-file
                (insert "file body\n"))
              (setq file-result
                    (org-babel-result-to-file result-file "File Desc"))
              (org-babel-insert-result result-file
                                       '("file" "link" "replace")
                                       info nil "emacs-lisp")
              (setq file-link
                    (org-element-map (org-element-parse-buffer) 'link
                      (lambda (link)
                        (list (org-element-property :type link)
                              (org-element-property :path link)
                              (and (org-element-contents-begin link)
                                   (buffer-substring-no-properties
                                    (org-element-contents-begin link)
                                    (org-element-contents-end link))))))))
            (push (funcall snapshot 'after-file) states)
            (goto-char (point-min))
            (search-forward "drawer-block")
            (search-forward "begin_src")
            (org-babel-remove-result nil t)
            (setq remove-keep
                  (buffer-substring-no-properties (point-min) (point-max)))
            (goto-char (point-min))
            (search-forward "output-block")
            (search-forward "begin_src")
            (org-babel-remove-result)
            (setq remove-full
                  (buffer-substring-no-properties (point-min) (point-max)))
            (with-temp-buffer
              (insert "one\n")
              (org-babel-examplify-region (point-min) (point-max)
                                          '("replace") nil)
              (setq example-lower (buffer-string)))
            (let ((org-babel-uppercase-example-markers t))
              (with-temp-buffer
                (insert "one\ntwo\nthree\n")
                (org-babel-examplify-region (point-min) (point-max)
                                            '("replace") nil)
                (setq example-upper (buffer-string))))
            (list (nreverse states)
                  output-read
                  drawer-read
                  file-result
                  file-link
                  remove-keep
                  remove-full
                  example-lower
                  example-upper
                  (replace-regexp-in-string
                   (regexp-quote root)
                   "<root>"
                   (buffer-substring-no-properties
                    (point-min) (point-max))))))
      (when (file-directory-p root) (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_babel_tangle_noweb_header_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"<root>/out.el\") (\"out.el\") \"(defvar *initialized* nil)\\n\\n(defun helper-a () 1)\\n(defun helper-b () 2)\\n\\n(defvar *initialized* nil)\\n(defun helper-a () 1)\\n(defun helper-b () 2)\\n(defun main () (+ (helper-a) (helper-b)))\\n\" ((\"setup\" \"emacs-lisp\" nil \"(defvar *initialized* nil)\\n\") (\"helpers\" \"emacs-lisp\" nil \"(defun helper-a () 1)\\n(defun helper-b () 2)\\n\") (nil \"emacs-lisp\" \":noweb yes\" \"<<setup>>\\n<<helpers>>\\n(defun main () (+ (helper-a) (helper-b)))\\n\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-tangle)
  (require 'ob-emacs-lisp)
  (let* ((root (make-temp-file "org-tangle-deep" t))
         (src (expand-file-name "project.org" root))
         (out (expand-file-name "out.el" root))
         (org-confirm-babel-evaluate nil))
    (unwind-protect
        (progn
          (with-temp-file src
            (insert "#+PROPERTY: header-args:emacs-lisp :tangle " out " :noweb yes\n\n")
            (insert "#+NAME: setup\n")
            (insert "#+begin_src emacs-lisp\n")
            (insert "(defvar *initialized* nil)\n")
            (insert "#+end_src\n\n")
            (insert "#+NAME: helpers\n")
            (insert "#+begin_src emacs-lisp\n")
            (insert "(defun helper-a () 1)\n")
            (insert "(defun helper-b () 2)\n")
            (insert "#+end_src\n\n")
            (insert "#+begin_src emacs-lisp :noweb yes\n")
            (insert "<<setup>>\n")
            (insert "<<helpers>>\n")
            (insert "(defun main () (+ (helper-a) (helper-b)))\n")
            (insert "#+end_src\n"))
          (with-current-buffer (find-file-noselect src)
            (org-mode)
            ;; Tangle
            (let ((tangle-result (org-babel-tangle)))
              ;; Read tangled file
              (let ((tangled-content
                     (when (file-exists-p out)
                       (with-temp-buffer
                         (insert-file-contents out)
                         (buffer-string))))
                    ;; Parse buffer
                    (src-blocks
                     (org-element-map (org-element-parse-buffer) 'src-block
                       (lambda (sb)
                         (list (org-element-property :name sb)
                               (org-element-property :language sb)
                               (org-element-property :parameters sb)
                               (org-element-property :value sb))))))
                (kill-buffer)
                (list (mapcar (lambda (f)
                                (replace-regexp-in-string
                                 (regexp-quote root) "<root>" f))
                              tangle-result)
                      (mapcar #'file-name-nondirectory tangle-result)
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (or tangled-content "no-file"))
                       src-blocks)))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_babel_tangle_header_args_noweb_comments_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 57 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-tangle)
  (require 'ob-emacs-lisp)
  (let* ((root (make-temp-file "org-tangle-hdr" t))
         (src (expand-file-name "project.org" root))
         (out-a (expand-file-name "a.el" root))
         (out-b (expand-file-name "b.el" root))
         (org-confirm-babel-evaluate nil))
    (unwind-protect
        (progn
          (with-temp-file src
            (insert "#+PROPERTY: header-args:emacs-lisp :comments both\n\n")
            (insert "#+NAME: shared\n")
            (insert "#+begin_src emacs-lisp\n")
            (insert "(defconst shared-val 42)\n")
            (insert "#+end_src\n\n")
            (insert "#+begin_src emacs-lisp :tangle " out-a " :noweb yes\n")
            (insert ";;; a.el --- A file\n")
            (insert "<<shared>>\n")
            (insert "(defun func-a () shared-val)\n")
            (insert ";;; a.el ends here\n")
            (insert "#+end_src\n\n")
            (insert "#+begin_src emacs-lisp :tangle " out-b "\n")
            (insert ";;; b.el --- B file\n")
            (insert "(defun func-b () 99)\n")
            (insert ";;; b.el ends here\n")
            (insert "#+end_src\n"))
          (with-current-buffer (find-file-noselect src)
            (org-mode)
            (let ((tangle-result (org-babel-tangle)))
              (let ((content-a
                     (when (file-exists-p out-a)
                       (with-temp-buffer
                         (insert-file-contents out-a)
                         (buffer-string))))
                    (content-b
                     (when (file-exists-p out-b)
                       (with-temp-buffer
                         (insert-file-contents out-b)
                         (buffer-string)))))
                (kill-buffer)
                (list (mapcar #'file-name-nondirectory tangle-result)
                      (sort (mapcar #'file-name-nondirectory tangle-result)
                            #'string<)
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (or content-a "no-a"))
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (or content-b "no-b"))
                      ;; Check noweb expansion happened
                      (and content-a
                           (string-match-p "shared-val" content-a))
                       (and content-a
                            (not (string-match-p "<<shared>>" content-a)))))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_src_edit_exit_writeback_preserve_structure_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable edit-mode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-src)
  (with-temp-buffer
    (org-mode)
    (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n\n")
    (insert "#+begin_src python\nprint('hello')\n#+end_src\n\n")
    (insert "Between blocks.\n\n")
    (insert "#+begin_src emacs-lisp\n(message \"test\")\n#+end_src\n")
    (goto-char (point-min))
    (search-forward "(+ 1 2)")
    (org-edit-src-code)
    (let ((edit-mode major-mode)
          (edit-buf (buffer-substring-no-properties
                     (point-min) (point-max))))
      (erase-buffer)
      (insert "(+ 10 20)\n(+ 30 40)\n")
      (org-edit-src-exit))
    (let ((after-first-edit (buffer-substring-no-properties
                             (point-min) (point-max))))
      (goto-char (point-min))
      (search-forward "print")
      (org-edit-src-code)
      (erase-buffer)
      (insert "print('modified')\nprint('extra')\n")
      (org-edit-src-exit)
      (let ((after-second-edit (buffer-substring-no-properties
                                (point-min) (point-max))))
        (let ((blocks
               (org-element-map (org-element-parse-buffer) 'src-block
                 (lambda (sb)
                   (list (org-element-property :language sb)
                         (org-element-property :value sb))))))
          (list edit-mode edit-buf after-first-edit after-second-edit blocks)))))))"##,
        expect,
    );
}

#[test]
fn org_src_block_edit_tangle_multi_lang_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (org-mode ((\"emacs-lisp\" \"  (+ 10 20)\\n  (+ 30 40)\\n\" nil) (\"emacs-lisp\" \"(* 3 4)\\n\" nil)) \"#+PROPERTY: header-args :tangle (concat (file-name-directory (buffer-file-name)) \\\"out.el\\\")\\n\\n#+begin_src emacs-lisp\\n  (+ 10 20)\\n  (+ 30 40)\\n#+end_src\\n\\n#+begin_src emacs-lisp\\n(* 3 4)\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-tangle)
  (let* ((root (make-temp-file "org-tangle-" t))
         (tangle-dir root))
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          (insert "#+PROPERTY: header-args :tangle (concat (file-name-directory (buffer-file-name)) \"out.el\")\n\n")
          (insert "#+begin_src emacs-lisp\n(+ 1 2)\n#+end_src\n\n")
          (insert "#+begin_src emacs-lisp\n(* 3 4)\n#+end_src\n\n")
          ;; Edit first block
          (goto-char (point-min))
          (search-forward "(+ 1 2)")
          (org-edit-src-code)
          (erase-buffer)
          (insert "(+ 10 20)\n(+ 30 40)\n")
          (org-edit-src-exit)
          ;; Parse blocks
          (let ((blocks
                 (org-element-map (org-element-parse-buffer) 'src-block
                   (lambda (sb)
                     (list (org-element-property :language sb)
                           (org-element-property :value sb)
                           (org-element-property :parameters sb)))))
                (edit-mode major-mode))
            (list edit-mode blocks
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))
      (delete-directory root t))))"##,
        expect,
    );
}
