use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_pcomplete_file_options_startup_tags_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"STA\" (#(\"STARTUP: \" 0 1 (completion--unquoted \"STARTUP: \" face completions-common-part) 1 3 (face completions-common-part)))) (\"hid\" (#(\"hideblocks\" 0 1 (completion--unquoted \"hideblocks\" face completions-common-part) 1 3 (face completions-common-part)) #(\"hidedrawers\" 0 1 (completion--unquoted \"hidedrawers\" face completions-common-part) 1 3 (face completions-common-part)) #(\"hidestars\" 0 1 (completion--unquoted \"hidestars\" face completions-common-part) 1 3 (face completions-common-part)))) (\"\" (#(\"work(w) home(h) { a(a) b(b) }\" 0 1 (completion--unquoted \"work(w) home(h) { a(a) b(b) }\")))) (\"\" (#(\"export ship\" 0 1 (completion--unquoted \"export ship\")))) (\"\" (#(\"noexport draft\" 0 1 (completion--unquoted \"noexport draft\")))) (\"\" (#(\"Ada Lovelace\" 0 1 (completion--unquoted \"Ada Lovelace\")))))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-pcomplete)
  (require 'ox)
  (with-temp-buffer
    (let ((org-tag-alist '(("work" . ?w) ("home" . ?h) (:startgroup)
                           ("a" . ?a) ("b" . ?b) (:endgroup)))
          (org-export-select-tags '("export" "ship"))
          (org-export-exclude-tags '("noexport" "draft"))
          (user-full-name "Ada Lovelace")
          (user-mail-address "ada@example.invalid"))
      (org-mode)
      (org-set-regexps-and-options)
      (cl-labels
          ((complete-at
            (needle)
            (goto-char (point-min))
            (search-forward needle)
            (let* ((capf (run-hook-with-args-until-success
                          'completion-at-point-functions))
                   (beg (nth 0 capf))
                   (end (nth 1 capf))
                   (table (nth 2 capf))
                   (stub (buffer-substring-no-properties beg end)))
              (list stub
                    (sort (all-completions stub table) #'string<)))))
        (insert "#+STA\n")
        (insert "#+STARTUP: hid\n")
        (insert "#+TAGS: \n")
        (insert "#+SELECT_TAGS: \n")
        (insert "#+EXCLUDE_TAGS: \n")
        (insert "#+AUTHOR: \n")
        (list (complete-at "#+STA")
              (complete-at "hid")
              (complete-at "#+TAGS: ")
              (complete-at "#+SELECT_TAGS: ")
              (complete-at "#+EXCLUDE_TAGS: ")
              (complete-at "#+AUTHOR: "))))))"##,
        expect,
    );
}

#[test]
fn org_pcomplete_heading_todo_tag_property_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-pcomplete)
  (with-temp-buffer
    (let ((org-todo-keywords '((sequence "TODO" "NEXT" "WAIT" "|" "DONE")))
          (org-tag-alist '(("work" . ?w) ("urgent" . ?u) ("blocked" . ?b))))
      (org-mode)
      (insert "#+PROPERTY: Effort_ALL 0:15 0:30 1:00\n")
      (insert "* TODO Alpha :work:\n")
      (insert ":PROPERTIES:\n:Owner: Ada\n:END:\n")
      (insert "** NEXT Beta :urgent:\n")
      (insert "* TO\n")
      (insert "* Plain :ur\n")
      (insert ":PROPERTIES:\n:Eff\n:END:\n")
      (org-set-regexps-and-options)
      (cl-labels
          ((complete-at
            (needle)
            (goto-char (point-min))
            (search-forward needle)
            (let* ((capf (run-hook-with-args-until-success
                          'completion-at-point-functions))
                   (beg (nth 0 capf))
                   (end (nth 1 capf))
                   (table (nth 2 capf))
                   (stub (buffer-substring-no-properties beg end)))
              (list stub
                    (sort (all-completions stub table) #'string<)
                    (org-thing-at-point)
                    (org-command-at-point)))))
        (list (complete-at "* TO")
              (complete-at ":ur")
              (complete-at ":Eff")))))"##,
        expect,
    );
}

#[test]
fn org_pcomplete_link_drawer_src_block_option_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"is\" (#(\"issue:\" 0 1 (completion--unquoted \"issue:\" face completions-common-part) 1 2 (face completions-common-part))) (\"link\") \"link\") (\"PRO\" (#(\"PROPERTIES:\" 0 1 (completion--unquoted \"PROPERTIES:\" face completions-common-part) 1 3 (face completions-common-part))) (\"drawer\") \"drawer\") (\":res\" (#(\":results\" 0 1 (completion--unquoted \":results\" face completions-common-part) 1 4 (face completions-common-part))) (\"block-option\" . \"src\") \"block-option/src\") (\":sc\" (#(\":scope\" 0 1 (completion--unquoted \":scope\" face completions-common-part) 1 3 (face completions-common-part))) (\"block-option\" . \"clocktable\") \"block-option/clocktable\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-pcomplete)
  (require 'ob-core)
  (with-temp-buffer
    (let ((org-link-abbrev-alist-local '(("issue" . "https://example/%s")))
          (org-link-abbrev-alist '(("doc" . "file:%s")))
          (org-babel-load-languages '((emacs-lisp . t) (shell . t))))
      (org-mode)
      (insert "[[is\n")
      (insert "* H\n:LOG\n:END:\n:PRO\n")
      (insert "#+begin_src emacs-lisp :res\n(+ 1 2)\n#+end_src\n")
      (insert "#+BEGIN: clocktable :sc\n#+END:\n")
      (cl-labels
          ((complete-at
            (needle)
            (goto-char (point-min))
            (search-forward needle)
            (let* ((capf (run-hook-with-args-until-success
                          'completion-at-point-functions))
                   (beg (nth 0 capf))
                   (end (nth 1 capf))
                   (table (nth 2 capf))
                   (stub (buffer-substring-no-properties beg end)))
              (list stub
                    (sort (all-completions stub table) #'string<)
                    (org-thing-at-point)
                    (org-command-at-point)))))
        (list (complete-at "[[is")
              (complete-at ":PRO")
              (complete-at ":res")
              (complete-at ":sc"))))))"##,
        expect,
    );
}

#[test]
fn org_pcomplete_entities_searchhead_options_matrix_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function complete-at)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-pcomplete)
  (require 'ox)
  (with-temp-buffer
    (let ((org-file-tags '("filetag" "shared"))
          (org-export-default-language "en")
          (org-priority-highest ?A)
          (org-priority-lowest ?D)
          (org-priority-default ?B)
          (org-babel-load-languages
           '((emacs-lisp . t) (shell . t) (python . nil)))
          (user-full-name "Ada Lovelace")
          (user-mail-address "ada@example.invalid"))
      (org-mode)
      (insert "* Alpha Heading\n")
      (insert "** Beta Child\n")
      (insert "[[*Al\n")
      (insert "\\alp\n")
      (insert "#+DATE: \n")
      (insert "#+EMAIL: \n")
      (insert "#+LANGUAGE: \n")
      (insert "#+PRIORITIES: \n")
      (insert "#+FILETAGS: \n")
      (insert "#+OPTIONS: to\n")
      (cl-labels
          ((complete-at
            (needle)
            (goto-char (point-min))
            (search-forward needle)
            (let* ((thing (org-thing-at-point))
                   (command (org-command-at-point))
                   (args (org-parse-arguments))
                   (capf (run-hook-with-args-until-success
                          'completion-at-point-functions))
                   (beg (nth 0 capf))
                   (end (nth 1 capf))
                   (table (nth 2 capf))
                   (stub (buffer-substring-no-properties beg end)))
              (list needle
                    thing
                    command
                    stub
                    (sort (all-completions stub table) #'string<)
                    args)))))
        (list (complete-at "[[*Al")
              (complete-at "\\alp")
              (complete-at "#+DATE: ")
              (complete-at "#+EMAIL: ")
              (complete-at "#+LANGUAGE: ")
              (complete-at "#+PRIORITIES: ")
              (complete-at "#+FILETAGS: ")
              (complete-at "to"))))))"##,
        expect,
    );
}

#[test]
fn org_pcomplete_keyword_tag_drawer_property_omission_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function complete-at)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-pcomplete)
  (require 'ox)
  (with-temp-buffer
    (let ((buffer-file-name "/tmp/oracle-title.org")
          (org-tag-alist '(("work" . ?w) ("urgent" . ?u)
                           ("home" . ?h) ("blocked" . ?b)))
          (org-file-tags '("filetag" "shared"))
          (org-html-infojs-opts-table
           '((path . "Path") (view . "View") (toc . "Toc")))
          (sample-bound-variable 42))
      (org-mode)
      (insert "#+PROPERTY: Owner_ALL Ada Bea Cy\n")
      (insert "#+TITLE: \n")
      (insert "#+BIND: sample-bound\n")
      (insert "#+INFOJS_OPT: pa\n")
      (insert "* TODO Alpha :work:ur\n")
      (insert ":PROPERTIES:\n")
      (insert ":Owner: Ada\n")
      (insert ":Eff\n")
      (insert ":END:\n")
      (insert ":LOGBOOK:\n")
      (insert ":END:\n")
      (insert ":LO\n")
      (org-set-regexps-and-options)
      (cl-labels
          ((complete-at
            (needle)
            (goto-char (point-min))
            (search-forward needle)
            (let* ((thing (org-thing-at-point))
                   (command (org-command-at-point))
                   (args (org-parse-arguments))
                   (capf (run-hook-with-args-until-success
                          'completion-at-point-functions))
                   (beg (nth 0 capf))
                   (end (nth 1 capf))
                   (table (nth 2 capf))
                   (stub (buffer-substring-no-properties beg end)))
              (list needle
                    thing
                    command
                    stub
                    (sort (all-completions stub table) #'string<)
                    args)))))
        (list (complete-at "#+TITLE: ")
              (complete-at "sample-bound")
              (complete-at "pa")
              (complete-at ":work:ur")
              (complete-at ":Eff")
              (complete-at ":LO"))))))"##,
        expect,
    );
}

#[test]
fn org_pcomplete_repeated_options_babel_searchhead_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"ispell-lookup-words: No plain word-list found at systemdefault locations.  Customize ‘ispell-alternate-dictionary’ to set yours.\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-pcomplete)
  (require 'ob-core)
  (require 'ob-shell)
  (require 'ox)
  (with-temp-buffer
    (let ((buffer-file-name "/tmp/org-pcomplete-report.org")
          (org-file-tags '("filetag" "shared"))
          (org-link-abbrev-alist-local
           '(("bug" . "https://bugs.example/%s")
             ("doc" . "file:docs/%s")))
          (org-link-abbrev-alist
           '(("global" . "https://global.example/%s")
             ("doc" . "file:global-docs/%s")))
          (org-export-registered-backends nil)
          (org-html-infojs-opts-table
           '((path . "Path") (view . "View") (toc . "Toc")
             (ltoc . "Local toc")))
          (org-tag-alist
           '((:startgroup)
             ("work" . ?w) ("home" . ?h)
             (:endgroup)
             ("urgent" . ?u) ("blocked" . ?b)))
          (org-todo-keywords
           '((sequence "TODO(t)" "NEXT(n)" "WAIT(w@)" "|"
                       "DONE(d!)" "CANCELED(c@)")))
          (org-babel-load-languages
           '((emacs-lisp . t) (shell . t) (python . nil)))
          (org-src-lang-modes '(("shell" . sh) ("emacs-lisp" . emacs-lisp))))
      (org-mode)
      (org-set-regexps-and-options)
      (insert "#+TITLE: \n")
      (insert "#+STARTUP: hidestars fold\n")
      (insert "#+OPTIONS: toc:nil author:nil to\n")
      (insert "#+INFOJS_OPT: path:foo view:co\n")
      (insert "#+TAGS: \n")
      (insert "#+FILETAGS: \n")
      (insert "#+BEGIN_SRC shell :res :exports\n")
      (insert "echo hi\n")
      (insert "#+END_SRC\n")
      (insert "#+begin_src emacs-lisp :lex :session\n")
      (insert "(+ 1 2)\n")
      (insert "#+end_src\n")
      (insert "#+BEGIN: clocktable :scope file :max\n")
      (insert "#+END:\n")
      (insert "* TODO Alpha Heading :work:ur\n")
      (insert ":PROPERTIES:\n")
      (insert ":Effort_ALL: 0:15 0:30 1:00\n")
      (insert ":Owner_ALL: Ada Bea Cy\n")
      (insert ":Owner: Ada\n")
      (insert ":Eff\n")
      (insert ":END:\n")
      (insert "** NEXT Beta Child With  Spaces\n")
      (insert "[[*Alpha He\n")
      (insert "[[bu\n")
      (insert "\\bet\n")
      (cl-labels
          ((complete-at
            (needle)
            (goto-char (point-min))
            (search-forward needle)
            (let* ((thing (org-thing-at-point))
                   (command (org-command-at-point))
                   (args (org-parse-arguments))
                   (capf (run-hook-with-args-until-success
                          'completion-at-point-functions))
                   (beg (nth 0 capf))
                   (end (nth 1 capf))
                   (table (nth 2 capf))
                   (stub (and beg end
                              (buffer-substring-no-properties beg end))))
              (list needle
                    thing
                    command
                    args
                    stub
                    (and stub
                         (sort (all-completions stub table) #'string<))
                    (and beg (- beg (point-min)))
                    (and end (- end (point-min)))))))
        (list (complete-at "#+TITLE: ")
              (complete-at "fold")
              (complete-at "to")
              (complete-at "co")
              (complete-at "#+TAGS: ")
              (complete-at "#+FILETAGS: ")
              (complete-at ":res")
              (complete-at ":exports")
              (complete-at ":lex")
              (complete-at ":session")
              (complete-at ":max")
              (complete-at ":work:ur")
              (complete-at ":Eff")
              (complete-at "[[*Alpha He")
              (complete-at "[[bu")
               (complete-at "\\bet"))))))"##,
        expect,
    );
}

#[test]
fn org_pcomplete_keyword_tag_link_option_at_point_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"#+\" (\"file-option\" . \"TITLE\") \"file-option/title\" ((\"#+TITLE:\" \"Test\") 1 10) \"TITLE: \" nil 9 2) (\"#+T\" (\"file-option\" . \"TITLE\") \"file-option/title\" ((\"#+TITLE:\" \"Test\") 1 10) \"ITLE: \" nil 9 3) (\"#+TAGS: \" (\"file-option\" . \"TAGS\") \"file-option/tags\" ((\"#+TAGS:\" \"work(w)\" \"home(h)\" \"urgent(u)\") 15 23 31 39) nil nil nil nil) (\"#+TODO: \" (\"file-option\" . \"TODO\") \"file-option/todo\" ((\"#+TODO:\" \"TODO\" \"WAIT\" \"|\" \"DONE\" \"CANCELED\") 49 57 62 67 69 74) nil nil nil nil) (\"* TODO \" nil nil ((\"*\" \"TODO\" \"Alpha\" \":work:\") 83 85 90 96) nil nil nil nil) (\":wor\" (\"tag\") \"tag\" ((\"*\" \"TODO\" \"Alpha\" \":work:\") 83 85 90 96) \"\" (#(\"home:\" 0 1 (completion--unquoted \"home:\"))) 99 99) (\"[[\" (\"file-option\" . \"TITLE\") \"file-option/title\" ((\"#+TITLE:\" \"Test\") 1 10) \"#+TITLE: \" nil 9 0) (\"[[b\" (\"file-option\" . \"TITLE\") \"file-option/title\" ((\"#+TITLE:\" \"Test\") 1 10) \"#+TITLE: \" nil 9 0))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'pcomplete)
  (with-temp-buffer
    (org-mode)
    (insert "#+TITLE: Test\n")
    (insert "#+TAGS: work(w) home(h) urgent(u)\n")
    (insert "#+TODO: TODO WAIT | DONE CANCELED\n")
    (insert "* TODO Alpha :work:\n")
    (insert "** Beta :home:\n")
    (let* ((complete-at
            (lambda (needle)
              (goto-char (point-min))
              (search-forward needle nil t)
              (let* ((pos (point))
                     (thing (org-thing-at-point))
                     (command (org-command-at-point))
                     (args (org-parse-arguments))
                     (capf (run-hook-with-args-until-success
                            'completion-at-point-functions))
                     (beg (nth 0 capf))
                     (end (nth 1 capf))
                     (table (nth 2 capf))
                     (stub (and beg end
                                (buffer-substring-no-properties beg end))))
                (list needle
                      thing
                      command
                      args
                      stub
                      (and stub
                           (sort (all-completions stub table) #'string<))
                      (and beg (- beg (point-min)))
                      (and end (- end (point-min))))))))
      (list (funcall complete-at "#+")
            (funcall complete-at "#+T")
            (funcall complete-at "#+TAGS: ")
            (funcall complete-at "#+TODO: ")
            (funcall complete-at "* TODO ")
            (funcall complete-at ":wor")
            (funcall complete-at "[[")
            (funcall complete-at "[[b")))))"##,
        expect,
    );
}
