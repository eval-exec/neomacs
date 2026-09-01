use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_babel_src_info_expand_execute_results_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"emacs-lisp\" \"(+ x y)\" (:var x . 5) value \"calc\" \"(let ((x '5)\\n      (y '7))\\n(+ x y)\\n)\" 12 \"#+NAME: calc\\n#+begin_src emacs-lisp :var x=5 y=7 :results value replace\\n(+ x y)\\n#+end_src\\n\\n#+RESULTS: calc\\n: 12\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: calc\n")
    (insert "#+begin_src emacs-lisp :var x=5 y=7 :results value replace\n")
    (insert "(+ x y)\n")
    (insert "#+end_src\n")
    (goto-char (point-min))
    (search-forward "begin_src")
    (let ((org-confirm-babel-evaluate nil))
      (let ((info (org-babel-get-src-block-info))
            (expanded (org-babel-expand-src-block))
            (result (org-babel-execute-src-block)))
        (list (nth 0 info)
              (nth 1 info)
              (assq :var (nth 2 info))
              (cdr (assq :result-type (nth 2 info)))
              (nth 4 info)
              expanded
              result
              (buffer-substring-no-properties (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_ref_parse_split_resolve_table_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((x) (\"a=1\" \"b=two\" \"c=\\\"three,four\\\"\") ((\"a\" \"b\") (1 2) (3 4)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-ref)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: data\n")
    (insert "| a | b |\n| 1 | 2 |\n| 3 | 4 |\n")
    (goto-char (point-min))
    (list (org-babel-ref-parse "x=data[1,2]")
          (org-babel-ref-split-args "a=1, b=two, c=\"three,four\"")
          (org-babel-ref-resolve "data"))))"##,
        expect,
    );
}

#[test]
fn org_export_environment_and_string_html_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Export Env\") nil (\"One\") (\"1\") t \"<div id=\\\"outline-container-org-id\\\" class=\\\"outline-2\\\">\\n<h2 id=\\\"org-id\\\"><span class=\\\"section-number-2\\\">1.</span> H</h2>\\n<div class=\\\"outline-text-2\\\" id=\\\"text-1\\\">\\n<p>\\nText</p>\\n</div>\\n</div>\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'ox-html)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Export Env\n")
    (insert "#+OPTIONS: toc:nil num:nil\n")
    (insert "* One\n")
    (insert "Paragraph [fn:1].\n")
    (insert "#+CAPTION: Table Cap\n")
    (insert "| A | B |\n| 1 | 2 |\n")
    (insert "[fn:1] Foot.\n")
    (let* ((info (org-export-get-environment 'html nil nil))
           (tree (org-element-parse-buffer))
           (heads
            (org-element-map tree 'headline
              (lambda (headline)
                (org-element-property :raw-value headline))))
           (foots
            (org-element-map tree 'footnote-definition
              (lambda (footnote)
                (org-element-property :label footnote))))
           (html (org-export-string-as
                  "* H\nText" 'html t '(:with-toc nil))))
      (list (mapcar #'substring-no-properties (plist-get info :title))
            (plist-get info :with-toc)
            heads
            foots
            (not (null (string-match-p "<h2" html)))
            (replace-regexp-in-string
             "org[[:alnum:]]+"
             "org-id"
             html)))))"##,
        expect,
    );
}

#[test]
fn org_babel_header_result_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((:results . \"output drawer replace\") (:exports . \"results\") (:var . \"x=1\") (:var . \"x=2\") (:cache . \"yes\")) ((:results . \"replace table output drawer\") (:exports . \"both\") (:var . \"x=2\") (:var . \"label=\\\"new\\\"\") (:cache . \"yes\")) \"emacs-lisp\" \"code\" \"replace table value\" (:var x . 3) \"(let ((x '3)\\n      (label '\\\"row\\\"))\\n(list (list \\\"label\\\" \\\"n\\\" \\\"square\\\") 'hline (list label x (* x x)) (list \\\"next\\\" (+ x 1) (* (+ x 1) (+ x 1))))\\n)\" ((\"label\" \"n\" \"square\") hline (\"row\" 3 9) (\"next\" 4 16)) ((\"label\" \"n\" \"square\") hline (\"row\" 3 9) (\"next\" 4 16)) \"emacs-lisp\" \"replace list value\" (\"alpha\" \"beta\" \"n=4\") (\"alpha\" \"beta\" \"n=4\") (8 19) \"#+PROPERTY: header-args:emacs-lisp :exports both :results value replace\\n#+HEADER: :var x=3 :var label=\\\"row\\\"\\n#+NAME: table-block\\n#+begin_src emacs-lisp :results value table replace\\n(list (list \\\"label\\\" \\\"n\\\" \\\"square\\\") 'hline (list label x (* x x)) (list \\\"next\\\" (+ x 1) (* (+ x 1) (+ x 1))))\\n#+end_src\\n\\n#+RESULTS: table-block\\n| label | n | square |\\n|-------+---+--------|\\n| row   | 3 |      9 |\\n| next  | 4 |     16 |\\n\\n#+NAME: list-block\\n#+begin_src emacs-lisp :results value list replace\\n(list \\\"alpha\\\" \\\"beta\\\" (format \\\"n=%s\\\" 4))\\n#+end_src\\n\\n#+RESULTS: list-block\\n- alpha\\n- beta\\n- n=4\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (insert "#+PROPERTY: header-args:emacs-lisp :exports both :results value replace\n")
    (insert "#+HEADER: :var x=3 :var label=\"row\"\n")
    (insert "#+NAME: table-block\n")
    (insert "#+begin_src emacs-lisp :results value table replace\n")
    (insert "(list (list \"label\" \"n\" \"square\") 'hline (list label x (* x x)) (list \"next\" (+ x 1) (* (+ x 1) (+ x 1))))\n")
    (insert "#+end_src\n\n")
    (insert "#+NAME: list-block\n")
    (insert "#+begin_src emacs-lisp :results value list replace\n")
    (insert "(list \"alpha\" \"beta\" (format \"n=%s\" 4))\n")
    (insert "#+end_src\n")
    (let* ((org-confirm-babel-evaluate nil)
           (parsed (org-babel-parse-header-arguments
                    ":results output drawer replace :exports results :var x=1 :var x=2 :cache yes"))
           (merged (org-babel-merge-params
                    '((:results . "value replace") (:exports . "code")
                      (:var . "x=1") (:var . "label=\"old\""))
                    parsed
                    '((:results . "table replace") (:exports . "both")
                      (:var . "label=\"new\""))))
           table-info table-expanded table-result table-pos table-read
           list-info list-result list-pos list-read)
      (goto-char (point-min))
      (search-forward "table-block")
      (search-forward "begin_src")
      (setq table-info (org-babel-get-src-block-info))
      (setq table-expanded (org-babel-expand-src-block))
      (setq table-result (org-babel-execute-src-block nil table-info))
      (setq table-pos (org-babel-where-is-src-block-result nil table-info))
      (goto-char table-pos)
      (forward-line 1)
      (setq table-read (org-babel-read-result))
      (goto-char (point-min))
      (search-forward "list-block")
      (search-forward "begin_src")
      (setq list-info (org-babel-get-src-block-info))
      (setq list-result (org-babel-execute-src-block nil list-info))
      (setq list-pos (org-babel-where-is-src-block-result nil list-info))
      (goto-char list-pos)
      (forward-line 1)
      (setq list-read (org-babel-read-result))
      (list parsed
            merged
            (nth 0 table-info)
            (cdr (assq :exports (nth 2 table-info)))
            (cdr (assq :results (nth 2 table-info)))
            (assq :var (nth 2 table-info))
            table-expanded
            table-result
            table-read
            (nth 0 list-info)
            (cdr (assq :results (nth 2 list-info)))
            list-result
            list-read
            (list (line-number-at-pos table-pos)
                  (line-number-at-pos list-pos))
            (buffer-substring-no-properties
             (point-min) (point-max))))))"##,
        expect,
    );
}

#[test]
fn org_babel_tangle_noweb_comments_collect_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp \"main\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-tangle)
  (require 'ob-emacs-lisp)
  (let* ((dir (make-temp-file "org-babel-tangle" t))
         (org-file (expand-file-name "input.org" dir))
         (out-file (expand-file-name "out/generated.el" dir))
         (org-confirm-babel-evaluate nil)
         (org-babel-tangle-use-relative-file-links t)
         (org-babel-tangle-comment-format-beg
          ";; [[file:%link][%source-name:%line]]")
         (org-babel-tangle-comment-format-end ";; %source-name ends here")
         tangled collect before-clean after-clean out-text)
    (unwind-protect
        (progn
          (with-temp-file org-file
            (insert "#+TITLE: Tangle Combo\n")
            (insert "#+PROPERTY: header-args:emacs-lisp :comments link :mkdirp yes\n")
            (insert "* Library\n")
            (insert "#+NAME: helper\n")
            (insert "#+begin_src emacs-lisp :tangle no\n")
            (insert "(defun org-oracle-helper (x)\n  (+ x 10))\n")
            (insert "#+end_src\n\n")
            (insert "** Main\n")
            (insert "#+NAME: main\n")
            (insert "#+begin_src emacs-lisp :tangle out/generated.el :noweb yes :comments both\n")
            (insert "<<helper>>\n")
            (insert "(defun org-oracle-main ()\n  (org-oracle-helper 5))\n")
            (insert "#+end_src\n\n")
            (insert "** Extra\n")
            (insert "#+begin_src emacs-lisp :tangle out/generated.el :comments link\n")
            (insert "(defconst org-oracle-constant 'ok)\n")
            (insert "#+end_src\n"))
          (with-current-buffer (find-file-noselect org-file)
            (org-mode)
            (setq collect
                  (mapcar
                   (lambda (entry)
                     (let ((file (car entry))
                           (blocks (cdr entry)))
                       (list (file-name-nondirectory file)
                             (length blocks)
                             (mapcar
                              (lambda (block)
                                (list (nth 0 block)
                                      (cdr (assq :noweb (nth 4 block)))
                                      (cdr (assq :comments (nth 4 block)))
                                      (substring-no-properties
                                       (nth 1 block))))
                              blocks))))
                   (org-babel-tangle-collect-blocks "emacs-lisp")))
            (setq tangled (mapcar #'file-name-nondirectory
                                  (org-babel-tangle nil nil "emacs-lisp")))
            (setq out-text
                  (with-temp-buffer
                    (insert-file-contents out-file)
                    (buffer-substring-no-properties
                     (point-min) (point-max))))
            (setq before-clean
                  (list (file-exists-p out-file)
                        (file-exists-p (concat out-file "~"))))
            (org-babel-tangle-clean)
            (setq after-clean
                  (list (file-exists-p out-file)
                        (file-exists-p (concat out-file "~"))))
            (list collect
                  tangled
                  (mapcar (lambda (needle)
                            (not (null (string-match-p needle out-text))))
                          '("org-oracle-helper"
                            "org-oracle-main"
                            "org-oracle-constant"
                            "input.org"
                            "helper ends here"))
                  (replace-regexp-in-string
                   (regexp-quote dir)
                   "<tmp>"
                   out-text)
                  before-clean
                  after-clean
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))
      (when (get-file-buffer org-file) (kill-buffer (get-file-buffer org-file)))
      (when (file-exists-p out-file) (delete-file out-file))
      (when (file-exists-p (concat out-file "~")) (delete-file (concat out-file "~")))
      (when (file-directory-p dir) (delete-directory dir t)))))"##,
        expect,
    );
}

#[test]
fn org_babel_export_inline_result_html_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument consp nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-emacs-lisp)
  (require 'ox-html)
  (let ((org-confirm-babel-evaluate nil))
    (with-temp-buffer
      (org-mode)
      (insert "#+TITLE: Babel Export\n\n")
      (insert "* Section\n")
      (insert "#+begin_src emacs-lisp :results value replace\n(+ 3 4)\n#+end_src\n\n")
      (insert "Call: call_adder[:results raw](a=2,b=3)\n\n")
      (insert "#+NAME: adder\n")
      (insert "#+begin_src emacs-lisp :var a=1 b=2 :results value replace\n")
      (insert "(list :sum (+ a b) :product (* a b))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "(+ 3 4)")
      (org-babel-execute-src-block)
      (goto-char (point-min))
      (search-forward "adder")
      (org-babel-execute-src-block)
      (let ((after-exec (buffer-substring-no-properties
                         (point-min) (point-max)))
            ;; Parse results
            (results
             (org-element-map (org-element-parse-buffer) 'fixed-width
               (lambda (el)
                 (org-element-property :value el))))
            ;; Export
            (html (org-export-as 'html nil nil t nil))
            (has-7 (string-match-p "7" html))
            (has-sum (string-match-p "sum" html)))
        (list after-exec
              results
              has-7
              has-sum
              (replace-regexp-in-string
               "org[[:alnum:]-]\\{8,\\}" "orgHASH"
               (replace-regexp-in-string
                "sec:org[[:alnum:]-]+" "sec:org-id" html))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_tangle_edit_retangle_multi_block_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 42 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-tangle)
  (require 'ob-emacs-lisp)
  (let* ((root (make-temp-file "org-tangle-" t))
         (tangle-file (expand-file-name "out.el" root))
         (file (expand-file-name "src.org" root)))
    (unwind-protect
        (progn
          (with-temp-file file
            (insert "#+PROPERTY: header-args :tangle " tangle-file "\n\n")
            (insert "#+NAME: setup\n")
            (insert "#+begin_src emacs-lisp\n")
            (insert "(setq my-var 42)\n")
            (insert "#+end_src\n\n")
            (insert "#+NAME: compute\n")
            (insert "#+begin_src emacs-lisp\n")
            (insert "(+ my-var 8)\n")
            (insert "#+end_src\n"))
          ;; Tangle
          (with-current-buffer (find-file-noselect file)
            (org-mode)
            (org-babel-tangle)
            (let ((tangled1 (when (file-exists-p tangle-file)
                              (with-temp-file tangle-file
                                (insert-file-contents tangle-file)
                                (buffer-string)))))
              ;; Edit: change compute block
              (goto-char (point-min))
              (search-forward "(+ my-var 8)")
              (replace-match "(* my-var 10)")
              ;; Re-tangle
              (org-babel-tangle)
              (let ((tangled2 (when (file-exists-p tangle-file)
                                (with-temp-file tangle-file
                                  (insert-file-contents tangle-file)
                                  (buffer-string)))))
                (list tangled1 tangled2
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))
      (delete-directory root t))))"##,
        expect,
    );
}
