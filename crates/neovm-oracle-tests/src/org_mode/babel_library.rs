use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn org_babel_lob_ingest_call_execute_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (2 (\"add-pair\" \"decorate\") \"emacs-lisp\" \"(list :sum (+ x y) :product (* x y))\" \"replace drawer value\" \"emacs-lisp\" \"raw replace value\" \"#+CALL: add-pair(x=6,y=7) :results value drawer replace\\n\\n#+RESULTS:\\n:results:\\n| :sum | 13 | :product | 42 |\\n:end:\\n\\nPrefix call_decorate[:results raw](label=\\\"nums\\\", values='(3 4)) nums=(3 4) suffix.\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-lob)
  (require 'ob-emacs-lisp)
  (let* ((root (make-temp-file "org-lob" t))
         (lib (expand-file-name "library.org" root))
         (org-babel-library-of-babel nil))
    (unwind-protect
        (progn
          (with-temp-file lib
            (insert "#+PROPERTY: header-args:emacs-lisp :results value replace\n")
            (insert "#+NAME: add-pair\n")
            (insert "#+begin_src emacs-lisp :var x=1 y=2\n")
            (insert "(list :sum (+ x y) :product (* x y))\n")
            (insert "#+end_src\n\n")
            (insert "#+NAME: decorate\n")
            (insert "#+begin_src emacs-lisp :var label=\"n\" values='(1 2)\n")
            (insert "(format \"%s=%S\" label values)\n")
            (insert "#+end_src\n"))
          (let ((ingested (org-babel-lob-ingest lib)))
            (with-temp-buffer
              (org-mode)
              (insert "#+CALL: add-pair(x=6,y=7) :results value drawer replace\n\n")
              (insert "Prefix call_decorate[:results raw](label=\"nums\", values='(3 4)) suffix.\n")
              (let ((org-confirm-babel-evaluate nil)
                    (org-babel-default-lob-header-args '((:exports . "results"))))
                (goto-char (point-min))
                (let ((call-info (org-babel-lob-get-info)))
                  (org-babel-lob-execute-maybe)
                  (goto-char (point-min))
                  (search-forward "call_decorate")
                  (let ((inline-info (org-babel-lob-get-info)))
                    (org-babel-lob-execute-maybe)
                    (list ingested
                          (sort (mapcar (lambda (cell)
                                          (symbol-name (car cell)))
                                        org-babel-library-of-babel)
                                #'string<)
                          (nth 0 call-info)
                          (nth 1 call-info)
                          (cdr (assq :results (nth 2 call-info)))
                          (nth 0 inline-info)
                          (cdr (assq :results (nth 2 inline-info)))
                          (buffer-substring-no-properties
                           (point-min) (point-max)))))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_babel_ref_remote_table_headline_index_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Reference ‘remote-head’ not found in this buffer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-ref)
  (require 'ob-emacs-lisp)
  (require 'org-id)
  (let* ((root (make-temp-file "org-ref" t))
         (remote (expand-file-name "remote.org" root))
         (org-id-locations-file (expand-file-name ".org-id-locations" root))
         (org-id-track-globally t))
    (unwind-protect
        (progn
          (with-temp-file remote
            (insert "#+NAME: matrix\n")
            (insert "| row | a | b |\n")
            (insert "|-----+---+---|\n")
            (insert "| r1  | 1 | 2 |\n")
            (insert "| r2  | 3 | 4 |\n\n")
            (insert "* Remote headline\n")
            (insert ":PROPERTIES:\n:CUSTOM_ID: remote-head\n:END:\n")
            (insert "First body line.\nSecond body line.\n"))
          (with-temp-buffer
            (org-mode)
            (insert "#+NAME: local\n")
            (insert "| name | score |\n| ann | 5 |\n| bob | 8 |\n\n")
            (insert "#+begin_src emacs-lisp :var cell=local[2,1] :var row=local[1,*] :results value\n")
            (insert "(list cell row)\n")
            (insert "#+end_src\n")
            (let ((org-confirm-babel-evaluate nil))
              (goto-char (point-min))
              (search-forward "begin_src")
              (let ((info (org-babel-get-src-block-info)))
                (list
                 (assq :var (nth 2 info))
                 (org-babel-ref-resolve "local[1,*]")
                 (org-babel-ref-resolve "local[,1]")
                 (org-babel-ref-resolve
                  (concat remote ":matrix[2,1]"))
                 (org-babel-ref-resolve
                  (concat remote ":matrix[1:2,1:2]"))
                 (substring-no-properties
                  (org-babel-ref-resolve "remote-head")))))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_babel_sbe_table_formula_literal_header_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#+NAME: combine\\n#+begin_src emacs-lisp :var label=\\\"\\\" :var n=0 :results value\\n(format \\\"%s:%s\\\" label (* n n))\\n#+end_src\\n\\n| label | n | result  |\\n|-------+---+---------|\\n| alpha | 3 | alpha:9 |\\n| beta  | 4 | beta:16 |\\n#+TBLFM: $3='(org-sbe \\\"combine\\\" (label $$1) (n $2))\\n\" \"line1...\" \"single\" \"#+NAME: combine\\n#+begin_src emacs-lisp :var label=\\\"\\\" :var n=0 :results value\\n(format \\\"%s:%s\\\" label (* n n))\\n#+end_src\\n\\n| label | n | result  |\\n|-------+---+---------|\\n| alpha | 3 | alpha:9 |\\n| beta  | 5 | beta:25 |\\n#+TBLFM: $3='(org-sbe \\\"combine\\\" (label $$1) (n $2))\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'org-table)
  (require 'ob-core)
  (require 'ob-table)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (insert "#+NAME: combine\n")
    (insert "#+begin_src emacs-lisp :var label=\"\" :var n=0 :results value\n")
    (insert "(format \"%s:%s\" label (* n n))\n")
    (insert "#+end_src\n\n")
    (insert "| label | n | result |\n")
    (insert "|-------+---+--------|\n")
    (insert "| alpha | 3 |        |\n")
    (insert "| beta  | 4 |        |\n")
    (insert "#+TBLFM: $3='(org-sbe \"combine\" (label $$1) (n $2))\n")
    (let ((org-confirm-babel-evaluate nil))
      (goto-char (point-min))
      (search-forward "alpha")
      (org-table-recalculate-buffer-tables)
      (let ((after-first (buffer-substring-no-properties
                          (point-min) (point-max))))
        (goto-char (point-min))
        (search-forward "beta")
        (org-table-next-field)
        (delete-char 1)
        (insert "5")
        (org-table-recalculate-buffer-tables)
        (list after-first
              (org-babel-table-truncate-at-newline "line1\nline2")
              (org-babel-table-truncate-at-newline "single")
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_local_call_table_cache_inline_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-lob)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (let ((org-confirm-babel-evaluate nil)
          (org-babel-default-lob-header-args
           '((:exports . "results") (:results . "replace"))))
      (org-mode)
      (insert "#+PROPERTY: header-args:emacs-lisp :results value replace\n")
      (insert "#+NAME: nums\n")
      (insert "| item | n |\n")
      (insert "|------+---|\n")
      (insert "| a    | 2 |\n")
      (insert "| b    | 5 |\n\n")
      (insert "#+NAME: shape\n")
      (insert "#+begin_src emacs-lisp :var rows=nums factor=1 :cache yes\n")
      (insert "(mapcar (lambda (row)\n")
      (insert "          (let ((label (car row))\n")
      (insert "                (n (string-to-number (cadr row))))\n")
      (insert "            (list label n (* factor n))))\n")
      (insert "        rows)\n")
      (insert "#+end_src\n\n")
      (insert "#+CALL: shape(rows=nums[2:3,*], factor=10) :results value table replace :cache yes\n\n")
      (insert "Inline call_shape[:results raw replace](rows=nums[2:2,*], factor=3) end.\n")
      (let (call-info call-noeval call-pos call-read inline-info inline-read
            after-first after-second no-info)
        (goto-char (point-min))
        (search-forward "#+CALL")
        (setq call-info (org-babel-lob-get-info)
              call-noeval (org-babel-lob-get-info nil t))
        (org-babel-execute-maybe)
        (setq call-pos (org-babel-where-is-src-block-result nil call-info))
        (goto-char call-pos)
        (forward-line 1)
        (setq call-read (org-babel-read-result))
        (setq after-first
              (buffer-substring-no-properties (point-min) (point-max)))
        (goto-char (point-min))
        (search-forward "call_shape")
        (setq inline-info (org-babel-lob-get-info))
        (org-babel-execute-maybe)
        (setq inline-read
              (save-excursion
                (goto-char (point-min))
                (search-forward "Inline")
                (buffer-substring-no-properties
                 (line-beginning-position) (line-end-position))))
        (goto-char (point-min))
        (search-forward "| b")
        (search-forward "5")
        (replace-match "7" t t)
        (goto-char (point-min))
        (search-forward "#+CALL")
        (org-babel-execute-maybe)
        (setq after-second
              (buffer-substring-no-properties (point-min) (point-max)))
        (goto-char (point-max))
        (setq no-info (org-babel-lob-get-info))
        (list (nth 0 call-info)
              (nth 1 call-info)
              (assq :var (nth 2 call-info))
              (assq :cache (nth 2 call-info))
              (assq :results (nth 2 call-info))
              (assq :exports (nth 2 call-info))
              (nth 4 call-info)
              (nth 5 call-info)
              (assq :var (nth 2 call-noeval))
              call-read
              after-first
              (nth 0 inline-info)
              (assq :var (nth 2 inline-info))
              (assq :results (nth 2 inline-info))
              inline-read
              after-second
              no-info)))))"##,
        expect,
    );
}

#[test]
fn org_babel_lob_noweb_export_result_replacement_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function helper-lines)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-lob)
  (require 'ob-emacs-lisp)
  (require 'ox-html)
  (require 'ox-ascii)
  (let* ((root (make-temp-file "org-lob-export" t))
         (lib (expand-file-name "lib.org" root))
         (org-babel-library-of-babel nil)
         (org-confirm-babel-evaluate nil)
         (org-export-use-babel t)
         (org-export-with-broken-links t))
    (unwind-protect
        (progn
          (with-temp-file lib
            (insert "#+PROPERTY: header-args:emacs-lisp :results value replace\n")
            (insert "#+NAME: helper-lines\n")
            (insert "#+begin_src emacs-lisp :var prefix=\"x\" :var rows='((\"a\" 1))\n")
            (insert "(mapcar (lambda (row)\n")
            (insert "          (format \"%s:%s=%s\" prefix (car row) (cadr row)))\n")
            (insert "        rows)\n")
            (insert "#+end_src\n\n")
            (insert "#+NAME: wrap-lines\n")
            (insert "#+begin_src emacs-lisp :var title=\"T\" :var rows='((\"a\" 1)) :noweb yes\n")
            (insert "(cons title (helper-lines prefix=title rows=rows))\n")
            (insert "#+end_src\n"))
          (org-babel-lob-ingest lib)
          (with-temp-buffer
            (org-mode)
            (insert "#+TITLE: LOB Export\n")
            (insert "#+NAME: data\n")
            (insert "| key | n |\n")
            (insert "|-----+---|\n")
            (insert "| a   | 2 |\n")
            (insert "| b   | 3 |\n\n")
            (insert "* Calls\n")
            (insert "#+CALL: wrap-lines(title=\"Run\", rows=data[2:3,*]) :results value list replace :exports both\n\n")
            (insert "Inline call_helper-lines[:results raw replace](prefix=\"I\", rows=data[2:2,*]) done.\n")
            (let (call-info inline-info after-call after-inline html ascii ast)
              (goto-char (point-min))
              (search-forward "#+CALL")
              (setq call-info (org-babel-lob-get-info))
              (org-babel-execute-maybe)
              (setq after-call
                    (buffer-substring-no-properties
                     (point-min) (point-max)))
              (goto-char (point-min))
              (search-forward "call_helper-lines")
              (setq inline-info (org-babel-lob-get-info))
              (org-babel-execute-maybe)
              (setq after-inline
                    (buffer-substring-no-properties
                     (point-min) (point-max)))
              (setq ast
                    (org-element-map (org-element-parse-buffer)
                        '(babel-call inline-babel-call plain-list item table)
                      (lambda (el)
                        (list (org-element-type el)
                              (org-element-property :call el)
                              (org-element-property :arguments el)
                              (org-element-property :begin el)
                              (org-element-property :end el)))))
              (setq html
                    (replace-regexp-in-string
                     "org[[:alnum:]]+"
                     "org-id"
                     (org-export-as 'html nil nil t '(:with-toc nil))))
              (setq ascii
                    (let ((org-ascii-charset 'utf-8))
                      (org-export-as 'ascii nil nil t
                                     '(:with-toc nil))))
              (list (sort (mapcar (lambda (cell)
                                    (symbol-name (car cell)))
                                  org-babel-library-of-babel)
                          #'string<)
                    (nth 0 call-info)
                    (assq :var (nth 2 call-info))
                    (assq :results (nth 2 call-info))
                    (nth 0 inline-info)
                    (assq :var (nth 2 inline-info))
                    (assq :results (nth 2 inline-info))
                    after-call
                    after-inline
                    ast
                    (mapcar (lambda (needle)
                              (not (null (string-match-p needle html))))
                            '("LOB Export" "Run" "Run:a=2" "Run:b=3"
                              "I:a=2"))
                    (mapcar (lambda (needle)
                              (not (null (string-match-p needle ascii))))
                            '("LOB Export" "Run:a=2" "Run:b=3" "I:a=2"))
                    html
                    ascii))))
      (when (file-directory-p root) (delete-directory root t)))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_buffer_call_inline_remove_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-lob)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (let ((org-confirm-babel-evaluate nil)
          (org-babel-default-lob-header-args
           '((:exports . "results") (:results . "replace"))))
      (org-mode)
      (insert "#+PROPERTY: header-args:emacs-lisp :results value replace\n")
      (insert "#+NAME: nums\n")
      (insert "| key | n |\n")
      (insert "|-----+---|\n")
      (insert "| a   | 2 |\n")
      (insert "| b   | 4 |\n\n")
      (insert "#+NAME: rows->table\n")
      (insert "#+begin_src emacs-lisp :var rows=nums[2:3,*] :var factor=1 :results value table replace\n")
      (insert "(mapcar (lambda (row)\n")
      (insert "          (list (car row)\n")
      (insert "                (string-to-number (cadr row))\n")
      (insert "                (* factor (string-to-number (cadr row)))))\n")
      (insert "        rows)\n")
      (insert "#+end_src\n\n")
      (insert "#+NAME: scalar\n")
      (insert "#+begin_src emacs-lisp :var label=\"x\" :var n=0 :results value replace\n")
      (insert "(format \"%s:%s\" label (* n n))\n")
      (insert "#+end_src\n\n")
      (insert "* Calls\n")
      (insert "#+CALL: rows->table(rows=nums[2:3,*], factor=3) :results value table replace\n\n")
      (insert "Inline call_scalar[:results value replace](label=\"sq\", n=5) and ")
      (insert "call_scalar[:results value replace](label=\"cube\", n=3) done.\n")
      (let (mapped before after-first call-table inline-line removed-line
            after-edit after-remove executable-types parsed-results)
        (org-babel-map-call-lines nil
          (let ((ctx (org-element-context)))
            (push (list (org-element-type ctx)
                        (org-element-property :call ctx)
                        (org-element-property :arguments ctx)
                        (org-element-property :inside-header ctx)
                        (org-element-property :end-header ctx)
                        (org-element-property :begin ctx)
                        (org-element-property :end ctx))
                  mapped)))
        (setq before (buffer-substring-no-properties (point-min) (point-max)))
        (goto-char (point-min))
        (org-babel-execute-buffer)
        (setq after-first
              (buffer-substring-no-properties (point-min) (point-max)))
        (goto-char (point-min))
        (search-forward "#+CALL")
        (setq call-table
              (save-excursion
                (goto-char (org-babel-where-is-src-block-result))
                (forward-line 1)
                (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "Inline")
        (setq inline-line
              (buffer-substring-no-properties
               (line-beginning-position) (line-end-position)))
        (search-forward "call_scalar")
        (org-babel-remove-inline-result)
        (setq removed-line
              (buffer-substring-no-properties
               (line-beginning-position) (line-end-position)))
        (goto-char (point-min))
        (search-forward "| b")
        (search-forward "4")
        (replace-match "6" t t)
        (goto-char (point-min))
        (search-forward "#+CALL")
        (org-babel-lob-execute-maybe)
        (setq after-edit
              (buffer-substring-no-properties (point-min) (point-max)))
        (org-babel-map-executables nil
          (push (org-element-type (org-element-context)) executable-types))
        (setq parsed-results
              (org-element-map (org-element-parse-buffer)
                  '(babel-call inline-babel-call macro table)
                (lambda (el)
                  (list (org-element-type el)
                        (org-element-property :call el)
                        (org-element-property :key el)
                        (org-element-property :value el)
                        (org-element-property :begin el)
                        (org-element-property :end el)))))
        (goto-char (point-min))
        (search-forward "#+CALL")
        (org-babel-remove-result nil t)
        (setq after-remove
              (buffer-substring-no-properties (point-min) (point-max)))
        (list (nreverse mapped)
              before
              after-first
              call-table
              inline-line
              removed-line
              after-edit
              (sort (mapcar #'symbol-name executable-types) #'string<)
              parsed-results
              after-remove)))))"##,
        expect,
    );
}

#[test]
fn org_babel_cache_file_var_result_lifecycle_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (let* ((root (make-temp-file "org-babel-cache" t))
         (cache-file (expand-file-name "cache.org" root))
         (out-file (expand-file-name "output.txt" root))
         (org-confirm-babel-evaluate nil)
         (norm (lambda (s)
                 (replace-regexp-in-string
                  "[0-9a-f]\\{40\\}" "HASH"
                  (replace-regexp-in-string
                   "(27[0-9]+ [0-9]+ [0-9]+ [0-9]+)"
                   "(TIMESTAMP)"
                   (replace-regexp-in-string
                    (regexp-quote root) "<root>" s))))))
    (unwind-protect
        (progn
          (with-temp-file cache-file
            (insert "#+PROPERTY: header-args:emacs-lisp :results value replace\n\n")
            (insert "#+NAME: counter\n")
            (insert "#+begin_src emacs-lisp :var n=1 :cache yes\n")
            (insert "(list :count n :double (* 2 n) :ts (format \"%s\" (current-time)))\n")
            (insert "#+end_src\n\n")
            (insert "#+RESULTS[abc]: counter\n")
            (insert ":cached-placeholder\n\n")
            (insert "#+NAME: adder\n")
            (insert "#+begin_src emacs-lisp :var a=1 b=2 :results value replace\n")
            (insert "(list :sum (+ a b) :product (* a b) :diff (- a b))\n")
            (insert "#+end_src\n\n")
            (insert "#+NAME: writer\n")
            (insert "#+begin_src emacs-lisp :var data=adder :file " out-file "\n")
            (insert "(with-temp-file \"" out-file "\"\n")
            (insert "  (insert (format \"sum=%s prod=%s\" (plist-get data :sum) (plist-get data :product))))\n")
            (insert "\"done\")\n")
            (insert "#+end_src\n\n"))
          (with-current-buffer (find-file-noselect cache-file)
            (org-mode)
            (let ((snap (lambda ()
                          (funcall norm
                                   (buffer-substring-no-properties
                                    (point-min) (point-max))))))
              (let ((before (funcall snap)))
                ;; Execute counter
                (goto-char (point-min))
                (search-forward "counter")
                (org-babel-execute-src-block)
                (let ((after-counter (funcall snap)))
                  ;; Execute adder
                  (goto-char (point-min))
                  (search-forward "adder")
                  (org-babel-execute-src-block)
                  (let ((after-adder (funcall snap)))
                    ;; Execute writer
                    (goto-char (point-min))
                    (search-forward "writer")
                    (org-babel-execute-src-block)
                    (let ((after-writer (funcall snap))
                          (file-content
                           (when (file-exists-p out-file)
                             (with-temp-buffer
                               (insert-file-contents out-file)
                               (funcall norm (buffer-string))))))
                      ;; Re-execute counter with different var
                      (goto-char (point-min))
                      (search-forward "counter")
                      (org-babel-execute-src-block '(4))
                      (let ((after-re-exec (funcall snap)))
                        (list before
                              after-counter
                              after-adder
                              after-writer
                              (or file-content "no-file")
                              after-re-exec))))))))
            (kill-buffer)))
      (delete-directory root t)))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_result_value_insertion_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"(list :sum (+ a b) :product (* a b) :diff (- a b))\\n\" \"(princ (format \\\"item1=%s item2=%s\\\" 42 99))\\n\" \"'((\\\"X\\\" \\\"Y\\\") hline (1 2) (3 4) (5 6))\\n\" (\"| :sum | 3 | :product | 2 | :diff | -1 |\\n\" \": item1=42 item2=99\\n\" \"| X | Y |\\n|---+---|\\n| 1 | 2 |\\n| 3 | 4 |\\n| 5 | 6 |\\n\") ((\"adder\" \"emacs-lisp\" \":var a=1 b=2 :results value replace\") (\"lister\" \"emacs-lisp\" \":results output replace\") (\"tabler\" \"emacs-lisp\" \":results value table replace\")) \"#+NAME: adder\\n#+begin_src emacs-lisp :var a=1 b=2 :results value replace\\n(list :sum (+ a b) :product (* a b) :diff (- a b))\\n#+end_src\\n\\n#+RESULTS: adder\\n| :sum | 3 | :product | 2 | :diff | -1 |\\n\\n#+NAME: lister\\n#+begin_src emacs-lisp :results output replace\\n(princ (format \\\"item1=%s item2=%s\\\" 42 99))\\n#+end_src\\n\\n#+RESULTS: lister\\n: item1=42 item2=99\\n\\n#+NAME: tabler\\n#+begin_src emacs-lisp :results value table replace\\n'((\\\"X\\\" \\\"Y\\\") hline (1 2) (3 4) (5 6))\\n#+end_src\\n\\n#+RESULTS: tabler\\n| X | Y |\\n|---+---|\\n| 1 | 2 |\\n| 3 | 4 |\\n| 5 | 6 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      (insert "#+NAME: adder\n")
      (insert "#+begin_src emacs-lisp :var a=1 b=2 :results value replace\n")
      (insert "(list :sum (+ a b) :product (* a b) :diff (- a b))\n")
      (insert "#+end_src\n\n")
      (insert "#+NAME: lister\n")
      (insert "#+begin_src emacs-lisp :results output replace\n")
      (insert "(princ (format \"item1=%s item2=%s\" 42 99))\n")
      (insert "#+end_src\n\n")
      (insert "#+NAME: tabler\n")
      (insert "#+begin_src emacs-lisp :results value table replace\n")
      (insert "'((\"X\" \"Y\") hline (1 2) (3 4) (5 6))\n")
      (insert "#+end_src\n\n")
      ;; Execute adder
      (goto-char (point-min))
      (search-forward "adder")
      (org-babel-execute-src-block)
      (let ((adder-result (org-babel-read-result))
            (adder-buf (buffer-substring-no-properties
                        (point-min) (point-max))))
        ;; Execute lister
        (goto-char (point-min))
        (search-forward "lister")
        (org-babel-execute-src-block)
        (let ((lister-result (org-babel-read-result))
              (lister-buf (buffer-substring-no-properties
                           (point-min) (point-max))))
          ;; Execute tabler
          (goto-char (point-min))
          (search-forward "tabler")
          (org-babel-execute-src-block)
          (let ((tabler-result (org-babel-read-result))
                (tabler-buf (buffer-substring-no-properties
                             (point-min) (point-max))))
            ;; Extract all RESULTS blocks
            (let ((results-blocks nil))
              (goto-char (point-min))
              (while (re-search-forward "^#\\+RESULTS:" nil t)
                (forward-line 1)
                (let ((beg (point)))
                  (if (re-search-forward "^$" nil t)
                      (push (buffer-substring-no-properties beg (point))
                            results-blocks)
                    (push (buffer-substring-no-properties beg (point-max))
                          results-blocks))))
              (list adder-result
                    lister-result
                    tabler-result
                    (nreverse results-blocks)
                    (org-element-map (org-element-parse-buffer) 'src-block
                      (lambda (sb)
                        (list (org-element-property :name sb)
                              (org-element-property :language sb)
                              (org-element-property :parameters sb))))
                    tabler-buf))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_dir_default_dir_header_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"(expand-file-name \\\"probe.txt\\\")\\n\" \"(princ (format \\\"default-dir=%s\\\" default-directory))\\n\" \"#+begin_src emacs-lisp :dir <root> :results value replace\\n(expand-file-name \\\"probe.txt\\\")\\n#+end_src\\n\\n#+RESULTS:\\n: <root>/probe.txt\\n\\n#+begin_src emacs-lisp :results output replace\\n(princ (format \\\"default-dir=%s\\\" default-directory))\\n#+end_src\\n\\n\" \"#+begin_src emacs-lisp :dir <root> :results value replace\\n(expand-file-name \\\"probe.txt\\\")\\n#+end_src\\n\\n#+RESULTS:\\n: <root>/probe.txt\\n\\n#+begin_src emacs-lisp :results output replace\\n(princ (format \\\"default-dir=%s\\\" default-directory))\\n#+end_src\\n\\n#+RESULTS:\\n: default-dir=[ORACLE-PROJECT-ROOT]/crates/neovm-oracle-tests/\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (let* ((root (make-temp-file "org-babel-dir" t))
         (probe (expand-file-name "probe.txt" root))
         (org-confirm-babel-evaluate nil))
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          (insert "#+begin_src emacs-lisp :dir " root " :results value replace\n")
          (insert "(expand-file-name \"probe.txt\")\n")
          (insert "#+end_src\n\n")
          (insert "#+begin_src emacs-lisp :results output replace\n")
          (insert "(princ (format \"default-dir=%s\" default-directory))\n")
          (insert "#+end_src\n\n")
          ;; Execute dir block
          (goto-char (point-min))
          (search-forward "expand-file-name")
          (org-babel-execute-src-block)
          (let ((dir-result (org-babel-read-result))
                (after-dir (buffer-substring-no-properties
                            (point-min) (point-max))))
            ;; Execute default-dir block
            (goto-char (point-min))
            (search-forward "default-dir=")
            (org-babel-execute-src-block)
            (let ((default-result (org-babel-read-result))
                  (after-default (buffer-substring-no-properties
                                  (point-min) (point-max))))
              (list (replace-regexp-in-string
                     (regexp-quote root) "<root>" dir-result)
                    (replace-regexp-in-string
                     (regexp-quote root) "<root>" default-result)
                    (replace-regexp-in-string
                     (regexp-quote root) "<root>" after-dir)
                    (replace-regexp-in-string
                     (regexp-quote root) "<root>" after-default)))))
      (delete-directory root t))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_var_table_output_header_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"(cons '(\\\"X\\\" \\\"Y\\\" \\\"Product\\\")\\n      (cons 'hline\\n            (mapcar (lambda (row)\\n                      (list (car row) (cadr row) (* (cadr row) factor)))\\n                    tbl)))\\n)\\n\" \"#+NAME: data\\n| X | Y |\\n|---+---|\\n| 1 | 10 |\\n| 2 | 20 |\\n| 3 | 30 |\\n\\n#+NAME: compute\\n#+begin_src emacs-lisp :var tbl=data factor=2 :results value table replace\\n(cons '(\\\"X\\\" \\\"Y\\\" \\\"Product\\\")\\n      (cons 'hline\\n            (mapcar (lambda (row)\\n                      (list (car row) (cadr row) (* (cadr row) factor)))\\n                    tbl)))\\n)\\n#+end_src\\n\\n#+RESULTS: compute\\n| X |  Y | Product |\\n|---+----+---------|\\n| 1 | 10 |      20 |\\n| 2 | 20 |      40 |\\n| 3 | 30 |      60 |\\n\\n#+begin_src emacs-lisp :results output replace\\n(dotimes (i 3)\\n  (princ (format \\\"line %d\\\\n\\\" i)))\\n#+end_src\\n\\n\" \"(dotimes (i 3)\\n  (princ (format \\\"line %d\\\\n\\\" i)))\\n\" \"#+NAME: data\\n| X | Y |\\n|---+---|\\n| 1 | 10 |\\n| 2 | 20 |\\n| 3 | 30 |\\n\\n#+NAME: compute\\n#+begin_src emacs-lisp :var tbl=data factor=2 :results value table replace\\n(cons '(\\\"X\\\" \\\"Y\\\" \\\"Product\\\")\\n      (cons 'hline\\n            (mapcar (lambda (row)\\n                      (list (car row) (cadr row) (* (cadr row) factor)))\\n                    tbl)))\\n)\\n#+end_src\\n\\n#+RESULTS: compute\\n| X |  Y | Product |\\n|---+----+---------|\\n| 1 | 10 |      20 |\\n| 2 | 20 |      40 |\\n| 3 | 30 |      60 |\\n\\n#+begin_src emacs-lisp :results output replace\\n(dotimes (i 3)\\n  (princ (format \\\"line %d\\\\n\\\" i)))\\n#+end_src\\n\\n#+RESULTS:\\n: line 0\\n: line 1\\n: line 2\\n\\n\" ((src-block \"(cons '(\\\"X\\\" \\\"Y\\\" \\\"Product\\\")\\n      (cons 'hline\\n            (mapcar (lambda (row)\\n                      (list (car row) (cadr row) (* (cadr row) factor)))\\n                    tbl)))\\n)\\n\") (src-block \"(dotimes (i 3)\\n  (princ (format \\\"line %d\\\\n\\\" i)))\\n\") (fixed-width \"line 0\\nline 1\\nline 2\")))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      (insert "#+NAME: data\n")
      (insert "| X | Y |\n|---+---|\n| 1 | 10 |\n| 2 | 20 |\n| 3 | 30 |\n\n")
      (insert "#+NAME: compute\n")
      (insert "#+begin_src emacs-lisp :var tbl=data factor=2 :results value table replace\n")
      (insert "(cons '(\"X\" \"Y\" \"Product\")\n")
      (insert "      (cons 'hline\n")
      (insert "            (mapcar (lambda (row)\n")
      (insert "                      (list (car row) (cadr row) (* (cadr row) factor)))\n")
      (insert "                    tbl)))\n")
      (insert ")\n")
      (insert "#+end_src\n\n")
      (insert "#+begin_src emacs-lisp :results output replace\n")
      (insert "(dotimes (i 3)\n  (princ (format \"line %d\\n\" i)))\n")
      (insert "#+end_src\n\n")
      ;; Execute compute
      (goto-char (point-min))
      (search-forward "compute")
      (org-babel-execute-src-block)
      (let ((compute-result (org-babel-read-result))
            (after-compute (buffer-substring-no-properties
                            (point-min) (point-max))))
        ;; Execute output
        (goto-char (point-min))
        (search-forward "dotimes")
        (org-babel-execute-src-block)
        (let ((output-result (org-babel-read-result))
              (after-output (buffer-substring-no-properties
                             (point-min) (point-max)))
              ;; Parse results
              (results-els
               (org-element-map (org-element-parse-buffer)
                   '(fixed-width src-block)
                 (lambda (el)
                   (list (org-element-type el)
                         (org-element-property :value el))))))
           (list compute-result
                 after-compute
                 output-result
                 after-output
                 results-els))))))"##,
        expect,
    );
}

#[test]
fn org_babel_header_arg_merge_property_inherit_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Set file-level property
      (insert "#+PROPERTY: header-args :results value replace\n\n")
      ;; Block with its own header
      (insert "#+NAME: merged\n")
      (insert "#+begin_src emacs-lisp :var x=10\n")
      (insert "(list :x x :doubled (* x 2))\n")
      (insert "#+end_src\n\n")
      ;; Block with :results output override
      (insert "#+NAME: output-block\n")
      (insert "#+begin_src emacs-lisp :results output\n")
      (insert "(princ (format \"output-mode=%s\" 'output))\n")
      (insert "#+end_src\n\n")
      ;; Block with :file
      (let* ((root (make-temp-file "org-babel-merge" t))
             (out-file (expand-file-name "result.txt" root)))
        (unwind-protect
            (progn
              (insert "#+NAME: file-writer\n")
              (insert "#+begin_src emacs-lisp :file " out-file "\n")
              (insert "(with-temp-file \"" out-file "\"\n  (insert \"file-content\"))\n  \"done\"\n")
              (insert "#+end_src\n\n")
              ;; Execute merged
              (goto-char (point-min))
              (search-forward "merged")
              (org-babel-execute-src-block)
              (let ((merged-result (org-babel-read-result)))
                ;; Execute output-block
                (goto-char (point-min))
                (search-forward "output-block")
                (org-babel-execute-src-block)
                (let ((output-result (org-babel-read-result)))
                  ;; Execute file-writer
                  (goto-char (point-min))
                  (search-forward "file-writer")
                  (org-babel-execute-src-block)
                  (let ((file-result (org-babel-read-result))
                        (file-content
                         (when (file-exists-p out-file)
                           (with-temp-buffer
                             (insert-file-contents out-file)
                             (buffer-string)))))
                    (list merged-result
                          output-result
                          file-result
                          (replace-regexp-in-string
                           (regexp-quote root) "<root>"
                           (or file-content "no-file"))
                          (buffer-substring-no-properties
                           (point-min) (point-max))))))))
           (delete-directory root t)))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_header_override_noweb_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"(let ((m (progn <<config>> multiplier)))\\n  (* x m))\\n\" \"#+NAME: config\\n#+begin_src emacs-lisp\\n(defconst multiplier 3)\\n#+end_src\\n\\n#+NAME: compute\\n#+begin_src emacs-lisp :var x=5 :noweb yes :results value replace\\n(let ((m (progn <<config>> multiplier)))\\n  (* x m))\\n#+end_src\\n\\n#+RESULTS: compute\\n: 15\\n\\n#+begin_src emacs-lisp :results output replace :var val=compute\\n(princ (format \\\"result=%s\\\" val))\\n#+end_src\\n\\n\" \"(princ (format \\\"result=%s\\\" val))\\n\" \"#+NAME: config\\n#+begin_src emacs-lisp\\n(defconst multiplier 3)\\n#+end_src\\n\\n#+NAME: compute\\n#+begin_src emacs-lisp :var x=5 :noweb yes :results value replace\\n(let ((m (progn <<config>> multiplier)))\\n  (* x m))\\n#+end_src\\n\\n#+RESULTS: compute\\n: 15\\n\\n#+begin_src emacs-lisp :results output replace :var val=compute\\n(princ (format \\\"result=%s\\\" val))\\n#+end_src\\n\\n#+RESULTS:\\n: result=15\\n\\n\" ((src-block \"config\" \"(defconst multiplier 3)\\n\") (src-block \"compute\" \"(let ((m (progn <<config>> multiplier)))\\n  (* x m))\\n\") (fixed-width nil \"15\") (src-block nil \"(princ (format \\\"result=%s\\\" val))\\n\") (fixed-width nil \"result=15\")))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      (insert "#+NAME: config\n")
      (insert "#+begin_src emacs-lisp\n")
      (insert "(defconst multiplier 3)\n")
      (insert "#+end_src\n\n")
      (insert "#+NAME: compute\n")
      (insert "#+begin_src emacs-lisp :var x=5 :noweb yes :results value replace\n")
      (insert "(let ((m (progn <<config>> multiplier)))\n")
      (insert "  (* x m))\n")
      (insert "#+end_src\n\n")
      (insert "#+begin_src emacs-lisp :results output replace :var val=compute\n")
      (insert "(princ (format \"result=%s\" val))\n")
      (insert "#+end_src\n\n")
      ;; Execute compute with noweb
      (goto-char (point-min))
      (search-forward "compute")
      (org-babel-execute-src-block)
      (let ((compute-result (org-babel-read-result))
            (after-compute (buffer-substring-no-properties
                            (point-min) (point-max))))
        ;; Execute output with var reference
        (goto-char (point-min))
        (search-forward "princ")
        (org-babel-execute-src-block)
        (let ((output-result (org-babel-read-result))
              (after-output (buffer-substring-no-properties
                             (point-min) (point-max)))
              ;; Parse results
              (results
               (org-element-map (org-element-parse-buffer)
                   '(fixed-width src-block)
                 (lambda (el)
                   (list (org-element-type el)
                         (org-element-property :name el)
                         (org-element-property :value el))))))
           (list compute-result
                 after-compute
                 output-result
                 after-output
                 results))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multiple_blocks_result_chain_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":x\" 1) (\":y\" 2) (\":z\" 3)) ((\":x\" 10) (\":y\" 20) (\":z\" 30)) \":x=10\\n:y=20\\n:z=30\") ((src-block \"producer\" \"'((:x 1) (:y 2) (:z 3))\\n\") (src-block \"consumer\" \"(mapcar (lambda (item)\\n          (list (car item) (* 10 (cadr item))))\\n        data)\\n\") (src-block \"final\" \"(dolist (item data)\\n  (princ (format \\\"%s=%s\\\\n\\\" (car item) (cadr item))))\\n\") (fixed-width nil \":x=10\\n:y=20\\n:z=30\")) \"#+NAME: producer\\n#+begin_src emacs-lisp :results value replace\\n'((:x 1) (:y 2) (:z 3))\\n#+end_src\\n\\n#+RESULTS: producer\\n| :x | 1 |\\n| :y | 2 |\\n| :z | 3 |\\n\\n#+NAME: consumer\\n#+begin_src emacs-lisp :var data=producer :results value replace\\n(mapcar (lambda (item)\\n          (list (car item) (* 10 (cadr item))))\\n        data)\\n#+end_src\\n\\n#+RESULTS: consumer\\n| :x | 10 |\\n| :y | 20 |\\n| :z | 30 |\\n\\n#+NAME: final\\n#+begin_src emacs-lisp :var data=consumer :results output replace\\n(dolist (item data)\\n  (princ (format \\\"%s=%s\\\\n\\\" (car item) (cadr item))))\\n#+end_src\\n\\n#+RESULTS: final\\n: :x=10\\n: :y=20\\n: :z=30\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Block A: produces a list
      (insert "#+NAME: producer\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((:x 1) (:y 2) (:z 3))\n")
      (insert "#+end_src\n\n")
      ;; Block B: uses producer result
      (insert "#+NAME: consumer\n")
      (insert "#+begin_src emacs-lisp :var data=producer :results value replace\n")
      (insert "(mapcar (lambda (item)\n")
      (insert "          (list (car item) (* 10 (cadr item))))\n")
      (insert "        data)\n")
      (insert "#+end_src\n\n")
      ;; Block C: uses consumer result
      (insert "#+NAME: final\n")
      (insert "#+begin_src emacs-lisp :var data=consumer :results output replace\n")
      (insert "(dolist (item data)\n")
      (insert "  (princ (format \"%s=%s\\n\" (car item) (cadr item))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all in order
      (dolist (name '("producer" "consumer" "final"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read all results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        ;; Parse elements
        (let ((elements
               (org-element-map (org-element-parse-buffer)
                   '(src-block fixed-width)
                 (lambda (el)
                   (list (org-element-type el)
                         (org-element-property :name el)
                         (org-element-property :value el))))))
           (list (nreverse results)
                 elements
                 (buffer-substring-no-properties
                  (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_rank_edit_reexecute_v19() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"a\" 180) (\"b\" 165) (\"c\" 190) (\"d\" 155) (\"e\" 175)) ((1 \"c\" 190) (2 \"a\" 180) (3 \"e\" 175) (4 \"b\" 165) (5 \"d\" 155)) ((\"a\" 180) (\"b\" 165) (\"c\" 190) (\"d\" 195) (\"e\" 175)) ((1 \"d\" 195) (2 \"c\" 190) (3 \"a\" 180) (4 \"e\" 175) (5 \"b\" 165)) \"#+NAME: heights\\n#+begin_src emacs-lisp :results value replace\\n'((a 180) (b 165) (c 190) (d 195) (e 175))\\n#+end_src\\n\\n#+RESULTS: heights\\n| a | 180 |\\n| b | 165 |\\n| c | 190 |\\n| d | 195 |\\n| e | 175 |\\n\\n#+NAME: ranked\\n#+begin_src emacs-lisp :var data=heights :results value replace\\n(let ((sorted (sort (copy-sequence data) (lambda (x y) (> (cadr x) (cadr y))))))\\n  (let ((i 0))\\n    (mapcar (lambda (r) (setq i (1+ i)) (list i (car r) (cadr r))) sorted)))\\n#+end_src\\n\\n#+RESULTS: ranked\\n| 1 | d | 195 |\\n| 2 | c | 190 |\\n| 3 | a | 180 |\\n| 4 | e | 175 |\\n| 5 | b | 165 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: heights
      (insert "#+NAME: heights\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a 180) (b 165) (c 190) (d 155) (e 175))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: ranked
      (insert "#+NAME: ranked\n")
      (insert "#+begin_src emacs-lisp :var data=heights :results value replace\n")
      (insert "(let ((sorted (sort (copy-sequence data) (lambda (x y) (> (cadr x) (cadr y))))))\n  (let ((i 0))\n    (mapcar (lambda (r) (setq i (1+ i)) (list i (car r) (cadr r))) sorted)))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("heights" "ranked"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: heights")
      (forward-line 1)
      (let ((heights1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: ranked")
        (forward-line 1)
        (let ((ranked1 (org-babel-read-result)))
          ;; Edit: change d from 155 to 195
          (goto-char (point-min))
          (search-forward "(d 155)")
          (replace-match "(d 195)")
          ;; Re-execute
          (dolist (name '("heights" "ranked"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: heights")
          (forward-line 1)
          (let ((heights2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: ranked")
            (forward-line 1)
            (let ((ranked2 (org-babel-read-result)))
              (list heights1 ranked1 heights2 ranked2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_weighted_avg_edit_reexecute_v18() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"t1\" 10 2) (\"t2\" 20 3) (\"t3\" 15 1) (\"t4\" 25 4) (\"t5\" 5 1)) ((\"t1\" 10 2 5.0) (\"t2\" 20 3 6.666666666666667) (\"t3\" 15 1 15.0) (\"t4\" 25 4 6.25) (\"t5\" 5 1 5.0)) ((\"t1\" 10 2) (\"t2\" 20 3) (\"t3\" 15 1) (\"t4\" 25 2) (\"t5\" 5 1)) ((\"t1\" 10 2 5.0) (\"t2\" 20 3 6.666666666666667) (\"t3\" 15 1 15.0) (\"t4\" 25 2 12.5) (\"t5\" 5 1 5.0)) \"#+NAME: measures\\n#+begin_src emacs-lisp :results value replace\\n'((t1 10 2) (t2 20 3) (t3 15 1) (t4 25 2) (t5 5 1))\\n#+end_src\\n\\n#+RESULTS: measures\\n| t1 | 10 | 2 |\\n| t2 | 20 | 3 |\\n| t3 | 15 | 1 |\\n| t4 | 25 | 2 |\\n| t5 |  5 | 1 |\\n\\n#+NAME: wavg\\n#+begin_src emacs-lisp :var data=measures :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (caddr r) (/ (* 1.0 (cadr r)) (caddr r)))) data)\\n#+end_src\\n\\n#+RESULTS: wavg\\n| t1 | 10 | 2 |               5.0 |\\n| t2 | 20 | 3 | 6.666666666666667 |\\n| t3 | 15 | 1 |              15.0 |\\n| t4 | 25 | 2 |              12.5 |\\n| t5 |  5 | 1 |               5.0 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: measurements
      (insert "#+NAME: measures\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((t1 10 2) (t2 20 3) (t3 15 1) (t4 25 4) (t5 5 1))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: weighted avg
      (insert "#+NAME: wavg\n")
      (insert "#+begin_src emacs-lisp :var data=measures :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (caddr r) (/ (* 1.0 (cadr r)) (caddr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("measures" "wavg"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: measures")
      (forward-line 1)
      (let ((measures1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: wavg")
        (forward-line 1)
        (let ((wavg1 (org-babel-read-result)))
          ;; Edit: change t4 from (25 4) to (25 2)
          (goto-char (point-min))
          (search-forward "(t4 25 4)")
          (replace-match "(t4 25 2)")
          ;; Re-execute
          (dolist (name '("measures" "wavg"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: measures")
          (forward-line 1)
          (let ((measures2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: wavg")
            (forward-line 1)
            (let ((wavg2 (org-babel-read-result)))
              (list measures1 wavg1 measures2 wavg2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_interest_edit_reexecute_v17() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"p1\" 1000) (\"p2\" 2000) (\"p3\" 1500) (\"p4\" 3000) (\"p5\" 500)) ((\"p1\" 1000 1050.0) (\"p2\" 2000 2100.0) (\"p3\" 1500 1575.0) (\"p4\" 3000 3150.0) (\"p5\" 500 525.0)) ((\"p1\" 1000) (\"p2\" 2000) (\"p3\" 1500) (\"p4\" 5000) (\"p5\" 500)) ((\"p1\" 1000 1050.0) (\"p2\" 2000 2100.0) (\"p3\" 1500 1575.0) (\"p4\" 5000 5250.0) (\"p5\" 500 525.0)) \"#+NAME: principals\\n#+begin_src emacs-lisp :results value replace\\n'((p1 1000) (p2 2000) (p3 1500) (p4 5000) (p5 500))\\n#+end_src\\n\\n#+RESULTS: principals\\n| p1 | 1000 |\\n| p2 | 2000 |\\n| p3 | 1500 |\\n| p4 | 5000 |\\n| p5 |  500 |\\n\\n#+NAME: interest\\n#+begin_src emacs-lisp :var data=principals :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (* 1.05 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: interest\\n| p1 | 1000 | 1050.0 |\\n| p2 | 2000 | 2100.0 |\\n| p3 | 1500 | 1575.0 |\\n| p4 | 5000 | 5250.0 |\\n| p5 |  500 |  525.0 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: principals
      (insert "#+NAME: principals\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((p1 1000) (p2 2000) (p3 1500) (p4 3000) (p5 500))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: with interest (5%)
      (insert "#+NAME: interest\n")
      (insert "#+begin_src emacs-lisp :var data=principals :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (* 1.05 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("principals" "interest"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: principals")
      (forward-line 1)
      (let ((principals1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: interest")
        (forward-line 1)
        (let ((interest1 (org-babel-read-result)))
          ;; Edit: change p4 from 3000 to 5000
          (goto-char (point-min))
          (search-forward "(p4 3000)")
          (replace-match "(p4 5000)")
          ;; Re-execute
          (dolist (name '("principals" "interest"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: principals")
          (forward-line 1)
          (let ((principals2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: interest")
            (forward-line 1)
            (let ((interest2 (org-babel-read-result)))
              (list principals1 interest1 principals2 interest2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_grade_edit_reexecute_v16() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"s1\" 92) (\"s2\" 78) (\"s3\" 85) (\"s4\" 67) (\"s5\" 95)) ((\"s1\" 92 \"A\") (\"s2\" 78 \"C\") (\"s3\" 85 \"B\") (\"s4\" 67 \"D\") (\"s5\" 95 \"A\")) ((\"s1\" 92) (\"s2\" 78) (\"s3\" 85) (\"s4\" 82) (\"s5\" 95)) ((\"s1\" 92 \"A\") (\"s2\" 78 \"C\") (\"s3\" 85 \"B\") (\"s4\" 82 \"B\") (\"s5\" 95 \"A\")) \"#+NAME: scores\\n#+begin_src emacs-lisp :results value replace\\n'((s1 92) (s2 78) (s3 85) (s4 82) (s5 95))\\n#+end_src\\n\\n#+RESULTS: scores\\n| s1 | 92 |\\n| s2 | 78 |\\n| s3 | 85 |\\n| s4 | 82 |\\n| s5 | 95 |\\n\\n#+NAME: graded\\n#+begin_src emacs-lisp :var data=scores :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r)\\n  (cond ((>= (cadr r) 90) \\\"A\\\")\\n        ((>= (cadr r) 80) \\\"B\\\")\\n        ((>= (cadr r) 70) \\\"C\\\")\\n        (t \\\"D\\\")))) data)\\n#+end_src\\n\\n#+RESULTS: graded\\n| s1 | 92 | A |\\n| s2 | 78 | C |\\n| s3 | 85 | B |\\n| s4 | 82 | B |\\n| s5 | 95 | A |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: scores
      (insert "#+NAME: scores\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((s1 92) (s2 78) (s3 85) (s4 67) (s5 95))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: graded
      (insert "#+NAME: graded\n")
      (insert "#+begin_src emacs-lisp :var data=scores :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r)\n  (cond ((>= (cadr r) 90) \"A\")\n        ((>= (cadr r) 80) \"B\")\n        ((>= (cadr r) 70) \"C\")\n        (t \"D\")))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("scores" "graded"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: scores")
      (forward-line 1)
      (let ((scores1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: graded")
        (forward-line 1)
        (let ((graded1 (org-babel-read-result)))
          ;; Edit: change s4 from 67 to 82
          (goto-char (point-min))
          (search-forward "(s4 67)")
          (replace-match "(s4 82)")
          ;; Re-execute
          (dolist (name '("scores" "graded"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: scores")
          (forward-line 1)
          (let ((scores2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: graded")
            (forward-line 1)
            (let ((graded2 (org-babel-read-result)))
              (list scores1 graded1 scores2 graded2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_tax_edit_reexecute_v15() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"w1\" 50000) (\"w2\" 75000) (\"w3\" 60000) (\"w4\" 90000) (\"w5\" 45000)) ((\"w1\" 50000 37500.0) (\"w2\" 75000 56250.0) (\"w3\" 60000 45000.0) (\"w4\" 90000 67500.0) (\"w5\" 45000 33750.0)) ((\"w1\" 50000) (\"w2\" 75000) (\"w3\" 60000) (\"w4\" 120000) (\"w5\" 45000)) ((\"w1\" 50000 37500.0) (\"w2\" 75000 56250.0) (\"w3\" 60000 45000.0) (\"w4\" 120000 90000.0) (\"w5\" 45000 33750.0)) \"#+NAME: incomes\\n#+begin_src emacs-lisp :results value replace\\n'((w1 50000) (w2 75000) (w3 60000) (w4 120000) (w5 45000))\\n#+end_src\\n\\n#+RESULTS: incomes\\n| w1 |  50000 |\\n| w2 |  75000 |\\n| w3 |  60000 |\\n| w4 | 120000 |\\n| w5 |  45000 |\\n\\n#+NAME: taxed\\n#+begin_src emacs-lisp :var data=incomes :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (* 0.75 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: taxed\\n| w1 |  50000 | 37500.0 |\\n| w2 |  75000 | 56250.0 |\\n| w3 |  60000 | 45000.0 |\\n| w4 | 120000 | 90000.0 |\\n| w5 |  45000 | 33750.0 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: incomes
      (insert "#+NAME: incomes\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((w1 50000) (w2 75000) (w3 60000) (w4 90000) (w5 45000))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: taxed
      (insert "#+NAME: taxed\n")
      (insert "#+begin_src emacs-lisp :var data=incomes :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (* 0.75 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("incomes" "taxed"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: incomes")
      (forward-line 1)
      (let ((incomes1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: taxed")
        (forward-line 1)
        (let ((taxed1 (org-babel-read-result)))
          ;; Edit: change w4 from 90000 to 120000
          (goto-char (point-min))
          (search-forward "(w4 90000)")
          (replace-match "(w4 120000)")
          ;; Re-execute
          (dolist (name '("incomes" "taxed"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: incomes")
          (forward-line 1)
          (let ((incomes2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: taxed")
            (forward-line 1)
            (let ((taxed2 (org-babel-read-result)))
              (list incomes1 taxed1 incomes2 taxed2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_ratio_edit_reexecute_v14() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"a\" 60) (\"b\" 80) (\"c\" 45) (\"d\" 90) (\"e\" 70)) ((\"a\" 60 17.391304347826086) (\"b\" 80 23.18840579710145) (\"c\" 45 13.043478260869565) (\"d\" 90 26.08695652173913) (\"e\" 70 20.28985507246377)) ((\"a\" 60) (\"b\" 80) (\"c\" 100) (\"d\" 90) (\"e\" 70)) ((\"a\" 60 15.0) (\"b\" 80 20.0) (\"c\" 100 25.0) (\"d\" 90 22.5) (\"e\" 70 17.5)) \"#+NAME: nums\\n#+begin_src emacs-lisp :results value replace\\n'((a 60) (b 80) (c 100) (d 90) (e 70))\\n#+end_src\\n\\n#+RESULTS: nums\\n| a |  60 |\\n| b |  80 |\\n| c | 100 |\\n| d |  90 |\\n| e |  70 |\\n\\n#+NAME: ratios\\n#+begin_src emacs-lisp :var data=nums :results value replace\\n(let ((total (apply #'+ (mapcar #'cadr data))))\\n  (mapcar (lambda (r) (list (car r) (cadr r) (/ (* 100.0 (cadr r)) total))) data))\\n#+end_src\\n\\n#+RESULTS: ratios\\n| a |  60 | 15.0 |\\n| b |  80 | 20.0 |\\n| c | 100 | 25.0 |\\n| d |  90 | 22.5 |\\n| e |  70 | 17.5 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: numbers
      (insert "#+NAME: nums\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a 60) (b 80) (c 45) (d 90) (e 70))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: ratios
      (insert "#+NAME: ratios\n")
      (insert "#+begin_src emacs-lisp :var data=nums :results value replace\n")
      (insert "(let ((total (apply #'+ (mapcar #'cadr data))))\n  (mapcar (lambda (r) (list (car r) (cadr r) (/ (* 100.0 (cadr r)) total))) data))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("nums" "ratios"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: nums")
      (forward-line 1)
      (let ((nums1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: ratios")
        (forward-line 1)
        (let ((ratios1 (org-babel-read-result)))
          ;; Edit: change c from 45 to 100
          (goto-char (point-min))
          (search-forward "(c 45)")
          (replace-match "(c 100)")
          ;; Re-execute
          (dolist (name '("nums" "ratios"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: nums")
          (forward-line 1)
          (let ((nums2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: ratios")
            (forward-line 1)
            (let ((ratios2 (org-babel-read-result)))
              (list nums1 ratios1 nums2 ratios2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_discount_edit_reexecute_v13() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"item1\" 100) (\"item2\" 200) (\"item3\" 150) (\"item4\" 300) (\"item5\" 50)) ((\"item1\" 100 80.0) (\"item2\" 200 160.0) (\"item3\" 150 120.0) (\"item4\" 300 240.0) (\"item5\" 50 40.0)) ((\"item1\" 100) (\"item2\" 200) (\"item3\" 150) (\"item4\" 500) (\"item5\" 50)) ((\"item1\" 100 80.0) (\"item2\" 200 160.0) (\"item3\" 150 120.0) (\"item4\" 500 400.0) (\"item5\" 50 40.0)) \"#+NAME: prices\\n#+begin_src emacs-lisp :results value replace\\n'((item1 100) (item2 200) (item3 150) (item4 500) (item5 50))\\n#+end_src\\n\\n#+RESULTS: prices\\n| item1 | 100 |\\n| item2 | 200 |\\n| item3 | 150 |\\n| item4 | 500 |\\n| item5 |  50 |\\n\\n#+NAME: discounted\\n#+begin_src emacs-lisp :var data=prices :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (* 0.8 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: discounted\\n| item1 | 100 |  80.0 |\\n| item2 | 200 | 160.0 |\\n| item3 | 150 | 120.0 |\\n| item4 | 500 | 400.0 |\\n| item5 |  50 |  40.0 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: prices
      (insert "#+NAME: prices\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((item1 100) (item2 200) (item3 150) (item4 300) (item5 50))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: discounted
      (insert "#+NAME: discounted\n")
      (insert "#+begin_src emacs-lisp :var data=prices :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (* 0.8 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("prices" "discounted"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: prices")
      (forward-line 1)
      (let ((prices1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: discounted")
        (forward-line 1)
        (let ((discounted1 (org-babel-read-result)))
          ;; Edit: change item4 from 300 to 500
          (goto-char (point-min))
          (search-forward "(item4 300)")
          (replace-match "(item4 500)")
          ;; Re-execute
          (dolist (name '("prices" "discounted"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: prices")
          (forward-line 1)
          (let ((prices2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: discounted")
            (forward-line 1)
            (let ((discounted2 (org-babel-read-result)))
              (list prices1 discounted1 prices2 discounted2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_percent_edit_reexecute_v12() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"s1\" 80) (\"s2\" 65) (\"s3\" 90) (\"s4\" 45) (\"s5\" 70)) ((\"s1\" 80 80.0) (\"s2\" 65 65.0) (\"s3\" 90 90.0) (\"s4\" 45 45.0) (\"s5\" 70 70.0)) ((\"s1\" 80) (\"s2\" 65) (\"s3\" 90) (\"s4\" 88) (\"s5\" 70)) ((\"s1\" 80 80.0) (\"s2\" 65 65.0) (\"s3\" 90 90.0) (\"s4\" 88 88.0) (\"s5\" 70 70.0)) \"#+NAME: scores\\n#+begin_src emacs-lisp :results value replace\\n'((s1 80) (s2 65) (s3 90) (s4 88) (s5 70))\\n#+end_src\\n\\n#+RESULTS: scores\\n| s1 | 80 |\\n| s2 | 65 |\\n| s3 | 90 |\\n| s4 | 88 |\\n| s5 | 70 |\\n\\n#+NAME: pct\\n#+begin_src emacs-lisp :var data=scores :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (/ (* 100.0 (cadr r)) 100))) data)\\n#+end_src\\n\\n#+RESULTS: pct\\n| s1 | 80 | 80.0 |\\n| s2 | 65 | 65.0 |\\n| s3 | 90 | 90.0 |\\n| s4 | 88 | 88.0 |\\n| s5 | 70 | 70.0 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: scores
      (insert "#+NAME: scores\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((s1 80) (s2 65) (s3 90) (s4 45) (s5 70))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: percent
      (insert "#+NAME: pct\n")
      (insert "#+begin_src emacs-lisp :var data=scores :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (/ (* 100.0 (cadr r)) 100))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("scores" "pct"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: scores")
      (forward-line 1)
      (let ((scores1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: pct")
        (forward-line 1)
        (let ((pct1 (org-babel-read-result)))
          ;; Edit: change s4 from 45 to 88
          (goto-char (point-min))
          (search-forward "(s4 45)")
          (replace-match "(s4 88)")
          ;; Re-execute
          (dolist (name '("scores" "pct"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: scores")
          (forward-line 1)
          (let ((scores2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: pct")
            (forward-line 1)
            (let ((pct2 (org-babel-read-result)))
              (list scores1 pct1 scores2 pct2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_cumulative_edit_reexecute_v11() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"a\" 100) (\"b\" 250) (\"c\" 75) (\"d\" 300) (\"e\" 150)) ((\"a\" 100 100) (\"b\" 250 350) (\"c\" 75 425) (\"d\" 300 725) (\"e\" 150 875)) ((\"a\" 100) (\"b\" 250) (\"c\" 200) (\"d\" 300) (\"e\" 150)) ((\"a\" 100 100) (\"b\" 250 350) (\"c\" 200 550) (\"d\" 300 850) (\"e\" 150 1000)) \"#+NAME: amounts\\n#+begin_src emacs-lisp :results value replace\\n'((a 100) (b 250) (c 200) (d 300) (e 150))\\n#+end_src\\n\\n#+RESULTS: amounts\\n| a | 100 |\\n| b | 250 |\\n| c | 200 |\\n| d | 300 |\\n| e | 150 |\\n\\n#+NAME: cumulative\\n#+begin_src emacs-lisp :var data=amounts :results value replace\\n(let ((sum 0))\\n  (mapcar (lambda (r) (setq sum (+ sum (cadr r))) (list (car r) (cadr r) sum)) data))\\n#+end_src\\n\\n#+RESULTS: cumulative\\n| a | 100 |  100 |\\n| b | 250 |  350 |\\n| c | 200 |  550 |\\n| d | 300 |  850 |\\n| e | 150 | 1000 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: amounts
      (insert "#+NAME: amounts\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a 100) (b 250) (c 75) (d 300) (e 150))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: cumulative
      (insert "#+NAME: cumulative\n")
      (insert "#+begin_src emacs-lisp :var data=amounts :results value replace\n")
      (insert "(let ((sum 0))\n  (mapcar (lambda (r) (setq sum (+ sum (cadr r))) (list (car r) (cadr r) sum)) data))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("amounts" "cumulative"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: amounts")
      (forward-line 1)
      (let ((amounts1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: cumulative")
        (forward-line 1)
        (let ((cumulative1 (org-babel-read-result)))
          ;; Edit: change c from 75 to 200
          (goto-char (point-min))
          (search-forward "(c 75)")
          (replace-match "(c 200)")
          ;; Re-execute
          (dolist (name '("amounts" "cumulative"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: amounts")
          (forward-line 1)
          (let ((amounts2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: cumulative")
            (forward-line 1)
            (let ((cumulative2 (org-babel-read-result)))
              (list amounts1 cumulative1 amounts2 cumulative2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_mapcar_edit_reexecute_v10() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"a\" 2) (\"b\" 5) (\"c\" 8) (\"d\" 3) (\"e\" 7)) ((\"a\" 2 4) (\"b\" 5 25) (\"c\" 8 64) (\"d\" 3 9) (\"e\" 7 49)) ((\"a\" 2) (\"b\" 5) (\"c\" 8) (\"d\" 12) (\"e\" 7)) ((\"a\" 2 4) (\"b\" 5 25) (\"c\" 8 64) (\"d\" 12 144) (\"e\" 7 49)) \"#+NAME: raw\\n#+begin_src emacs-lisp :results value replace\\n'((a 2) (b 5) (c 8) (d 12) (e 7))\\n#+end_src\\n\\n#+RESULTS: raw\\n| a |  2 |\\n| b |  5 |\\n| c |  8 |\\n| d | 12 |\\n| e |  7 |\\n\\n#+NAME: squared\\n#+begin_src emacs-lisp :var data=raw :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (* (cadr r) (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: squared\\n| a |  2 |   4 |\\n| b |  5 |  25 |\\n| c |  8 |  64 |\\n| d | 12 | 144 |\\n| e |  7 |  49 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: raw
      (insert "#+NAME: raw\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a 2) (b 5) (c 8) (d 3) (e 7))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: squared
      (insert "#+NAME: squared\n")
      (insert "#+begin_src emacs-lisp :var data=raw :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (* (cadr r) (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("raw" "squared"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: raw")
      (forward-line 1)
      (let ((raw1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: squared")
        (forward-line 1)
        (let ((squared1 (org-babel-read-result)))
          ;; Edit: change d from 3 to 12
          (goto-char (point-min))
          (search-forward "(d 3)")
          (replace-match "(d 12)")
          ;; Re-execute
          (dolist (name '("raw" "squared"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: raw")
          (forward-line 1)
          (let ((raw2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: squared")
            (forward-line 1)
            (let ((squared2 (org-babel-read-result)))
              (list raw1 squared1 raw2 squared2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_three_format_execute_edit_reexecute_v9() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((50 (\"Alice\" \"Bob\" \"Carol\" \"Dave\") ((\"X\" \"Y\") hline (1 2) (3 4) (5 6))) (100 (\"Eve\" \"Frank\") ((\"X\" \"Y\") hline (1 2) (3 4) (5 6))) \"#+NAME: sum\\n#+begin_src emacs-lisp :results scalar replace\\n(apply #'+ (mapcar #'cadr '((a 10) (b 20) (c 30) (d 40))))\\n#+end_src\\n\\n#+RESULTS: sum\\n: 100\\n\\n#+NAME: names\\n#+begin_src emacs-lisp :results list replace\\n'(\\\"Eve\\\" \\\"Frank\\\")\\n#+end_src\\n\\n#+RESULTS: names\\n- Eve\\n- Frank\\n\\n#+NAME: data\\n#+begin_src emacs-lisp :results table replace\\n(list '(\\\"X\\\" \\\"Y\\\") 'hline '(1 2) '(3 4) '(5 6))\\n#+end_src\\n\\n#+RESULTS: data\\n| X | Y |\\n|---+---|\\n| 1 | 2 |\\n| 3 | 4 |\\n| 5 | 6 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Scalar: sum
      (insert "#+NAME: sum\n")
      (insert "#+begin_src emacs-lisp :results scalar replace\n")
      (insert "(apply #'+ (mapcar #'cadr '((a 5) (b 10) (c 15) (d 20))))\n")
      (insert "#+end_src\n\n")
      ;; List: names
      (insert "#+NAME: names\n")
      (insert "#+begin_src emacs-lisp :results list replace\n")
      (insert "'(\"Alice\" \"Bob\" \"Carol\" \"Dave\")\n")
      (insert "#+end_src\n\n")
      ;; Table: data
      (insert "#+NAME: data\n")
      (insert "#+begin_src emacs-lisp :results table replace\n")
      (insert "(list '(\"X\" \"Y\") 'hline '(1 2) '(3 4) '(5 6))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("sum" "names" "data"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      (let ((r1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) r1))
        ;; Edit: change sum expression
        (goto-char (point-min))
        (search-forward "(a 5) (b 10) (c 15) (d 20)")
        (replace-match "(a 10) (b 20) (c 30) (d 40)")
        ;; Edit: change names
        (goto-char (point-min))
        (search-forward "'(\"Alice\" \"Bob\" \"Carol\" \"Dave\")")
        (replace-match "'(\"Eve\" \"Frank\")")
        ;; Re-execute
        (dolist (name '("sum" "names" "data"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((r2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) r2))
          (list (nreverse r1) (nreverse r2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_weighted_sum_edit_reexecute_v8() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"x\" 10) (\"y\" 20) (\"z\" 30) (\"w\" 40)) ((\"x\" 10 30 130) (\"y\" 20 60 160) (\"z\" 30 90 190) (\"w\" 40 120 220)) ((\"x\" 10) (\"y\" 20) (\"z\" 50) (\"w\" 40)) ((\"x\" 10 30 130) (\"y\" 20 60 160) (\"z\" 50 150 250) (\"w\" 40 120 220)) \"#+NAME: vals\\n#+begin_src emacs-lisp :results value replace\\n'((x 10) (y 20) (z 50) (w 40))\\n#+end_src\\n\\n#+RESULTS: vals\\n| x | 10 |\\n| y | 20 |\\n| z | 50 |\\n| w | 40 |\\n\\n#+NAME: weighted\\n#+begin_src emacs-lisp :var data=vals :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (* 3 (cadr r)) (+ 100 (* 3 (cadr r))))) data)\\n#+end_src\\n\\n#+RESULTS: weighted\\n| x | 10 |  30 | 130 |\\n| y | 20 |  60 | 160 |\\n| z | 50 | 150 | 250 |\\n| w | 40 | 120 | 220 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: values
      (insert "#+NAME: vals\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((x 10) (y 20) (z 30) (w 40))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: weighted
      (insert "#+NAME: weighted\n")
      (insert "#+begin_src emacs-lisp :var data=vals :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (* 3 (cadr r)) (+ 100 (* 3 (cadr r))))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("vals" "weighted"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: vals")
      (forward-line 1)
      (let ((vals1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: weighted")
        (forward-line 1)
        (let ((weighted1 (org-babel-read-result)))
          ;; Edit: change z from 30 to 50
          (goto-char (point-min))
          (search-forward "(z 30)")
          (replace-match "(z 50)")
          ;; Re-execute
          (dolist (name '("vals" "weighted"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: vals")
          (forward-line 1)
          (let ((vals2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: weighted")
            (forward-line 1)
            (let ((weighted2 (org-babel-read-result)))
              (list vals1 weighted1 vals2 weighted2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_map_edit_reexecute_v7() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"p\" 3 4) (\"q\" 1 7) (\"r\" 5 2) (\"s\" 8 6)) ((\"p\" 3 4 7) (\"q\" 1 7 8) (\"r\" 5 2 7) (\"s\" 8 6 14)) ((\"p\" 3 4) (\"q\" 1 7) (\"r\" 5 10) (\"s\" 8 6)) ((\"p\" 3 4 7) (\"q\" 1 7 8) (\"r\" 5 10 15) (\"s\" 8 6 14)) \"#+NAME: coords\\n#+begin_src emacs-lisp :results value replace\\n'((p 3 4) (q 1 7) (r 5 10) (s 8 6))\\n#+end_src\\n\\n#+RESULTS: coords\\n| p | 3 |  4 |\\n| q | 1 |  7 |\\n| r | 5 | 10 |\\n| s | 8 |  6 |\\n\\n#+NAME: computed\\n#+begin_src emacs-lisp :var data=coords :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (caddr r) (+ (cadr r) (caddr r)))) data)\\n#+end_src\\n\\n#+RESULTS: computed\\n| p | 3 |  4 |  7 |\\n| q | 1 |  7 |  8 |\\n| r | 5 | 10 | 15 |\\n| s | 8 |  6 | 14 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: coords
      (insert "#+NAME: coords\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((p 3 4) (q 1 7) (r 5 2) (s 8 6))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: computed
      (insert "#+NAME: computed\n")
      (insert "#+begin_src emacs-lisp :var data=coords :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (caddr r) (+ (cadr r) (caddr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("coords" "computed"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: coords")
      (forward-line 1)
      (let ((coords1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: computed")
        (forward-line 1)
        (let ((computed1 (org-babel-read-result)))
          ;; Edit: change r from (5 2) to (5 10)
          (goto-char (point-min))
          (search-forward "(r 5 2)")
          (replace-match "(r 5 10)")
          ;; Re-execute
          (dolist (name '("coords" "computed"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: coords")
          (forward-line 1)
          (let ((coords2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: computed")
            (forward-line 1)
            (let ((computed2 (org-babel-read-result)))
              (list coords1 computed1 coords2 computed2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_sort_index_edit_reexecute_v6() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"c\" 30) (\"a\" 10) (\"d\" 40) (\"b\" 20)) ((1 \"a\" 10) (2 \"b\" 20) (3 \"c\" 30) (4 \"d\" 40)) ((\"c\" 5) (\"a\" 10) (\"d\" 40) (\"b\" 20)) ((1 \"c\" 5) (2 \"a\" 10) (3 \"b\" 20) (4 \"d\" 40)) \"#+NAME: raw\\n#+begin_src emacs-lisp :results value replace\\n'((c 5) (a 10) (d 40) (b 20))\\n#+end_src\\n\\n#+RESULTS: raw\\n| c |  5 |\\n| a | 10 |\\n| d | 40 |\\n| b | 20 |\\n\\n#+NAME: indexed\\n#+begin_src emacs-lisp :var data=raw :results value replace\\n(let ((sorted (sort (copy-sequence data) (lambda (x y) (< (cadr x) (cadr y))))))\\n  (let ((i 0))\\n    (mapcar (lambda (r) (setq i (1+ i)) (list i (car r) (cadr r))) sorted)))\\n#+end_src\\n\\n#+RESULTS: indexed\\n| 1 | c |  5 |\\n| 2 | a | 10 |\\n| 3 | b | 20 |\\n| 4 | d | 40 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: raw data
      (insert "#+NAME: raw\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((c 30) (a 10) (d 40) (b 20))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: sorted and indexed
      (insert "#+NAME: indexed\n")
      (insert "#+begin_src emacs-lisp :var data=raw :results value replace\n")
      (insert "(let ((sorted (sort (copy-sequence data) (lambda (x y) (< (cadr x) (cadr y))))))\n")
      (insert "  (let ((i 0))\n")
      (insert "    (mapcar (lambda (r) (setq i (1+ i)) (list i (car r) (cadr r))) sorted)))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("raw" "indexed"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: raw")
      (forward-line 1)
      (let ((raw1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: indexed")
        (forward-line 1)
        (let ((indexed1 (org-babel-read-result)))
          ;; Edit: change c from 30 to 5
          (goto-char (point-min))
          (search-forward "(c 30)")
          (replace-match "(c 5)")
          ;; Re-execute
          (dolist (name '("raw" "indexed"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: raw")
          (forward-line 1)
          (let ((raw2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: indexed")
            (forward-line 1)
            (let ((indexed2 (org-babel-read-result)))
              (list raw1 indexed1 raw2 indexed2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_filter_edit_reexecute_v5() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"Red\" 5) (\"Blue\" 12) (\"Green\" 3) (\"Yellow\" 8) (\"Purple\" 15)) ((\"Blue\" 12) (\"Yellow\" 8) (\"Purple\" 15)) ((\"Red\" 5) (\"Blue\" 12) (\"Green\" 10) (\"Yellow\" 8) (\"Purple\" 15)) ((\"Blue\" 12) (\"Green\" 10) (\"Yellow\" 8) (\"Purple\" 15)) \"#+NAME: items\\n#+begin_src emacs-lisp :results value replace\\n'((\\\"Red\\\" 5) (\\\"Blue\\\" 12) (\\\"Green\\\" 10) (\\\"Yellow\\\" 8) (\\\"Purple\\\" 15))\\n#+end_src\\n\\n#+RESULTS: items\\n| Red    |  5 |\\n| Blue   | 12 |\\n| Green  | 10 |\\n| Yellow |  8 |\\n| Purple | 15 |\\n\\n#+NAME: big\\n#+begin_src emacs-lisp :var data=items :results value replace\\n(seq-filter (lambda (r) (> (cadr r) 6)) data)\\n#+end_src\\n\\n#+RESULTS: big\\n| Blue   | 12 |\\n| Green  | 10 |\\n| Yellow |  8 |\\n| Purple | 15 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: items
      (insert "#+NAME: items\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((\"Red\" 5) (\"Blue\" 12) (\"Green\" 3) (\"Yellow\" 8) (\"Purple\" 15))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: filtered
      (insert "#+NAME: big\n")
      (insert "#+begin_src emacs-lisp :var data=items :results value replace\n")
      (insert "(seq-filter (lambda (r) (> (cadr r) 6)) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("items" "big"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: items")
      (forward-line 1)
      (let ((items1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: big")
        (forward-line 1)
        (let ((big1 (org-babel-read-result)))
          ;; Edit: change Green from 3 to 10
          (goto-char (point-min))
          (search-forward "(\"Green\" 3)")
          (replace-match "(\"Green\" 10)")
          ;; Re-execute
          (dolist (name '("items" "big"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: items")
          (forward-line 1)
          (let ((items2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: big")
            (forward-line 1)
            (let ((big2 (org-babel-read-result)))
              (list items1 big1 items2 big2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_double_edit_reexecute_v4() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"n1\" 4) (\"n2\" 7) (\"n3\" 2) (\"n4\" 9) (\"n5\" 5)) ((\"n1\" 4 14 8) (\"n2\" 7 17 14) (\"n3\" 2 12 4) (\"n4\" 9 19 18) (\"n5\" 5 15 10)) ((\"n1\" 4) (\"n2\" 7) (\"n3\" 20) (\"n4\" 9) (\"n5\" 5)) ((\"n1\" 4 14 8) (\"n2\" 7 17 14) (\"n3\" 20 30 40) (\"n4\" 9 19 18) (\"n5\" 5 15 10)) \"#+NAME: nums\\n#+begin_src emacs-lisp :results value replace\\n'((n1 4) (n2 7) (n3 20) (n4 9) (n5 5))\\n#+end_src\\n\\n#+RESULTS: nums\\n| n1 |  4 |\\n| n2 |  7 |\\n| n3 | 20 |\\n| n4 |  9 |\\n| n5 |  5 |\\n\\n#+NAME: comp\\n#+begin_src emacs-lisp :var data=nums :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (+ 10 (cadr r)) (* 2 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: comp\\n| n1 |  4 | 14 |  8 |\\n| n2 |  7 | 17 | 14 |\\n| n3 | 20 | 30 | 40 |\\n| n4 |  9 | 19 | 18 |\\n| n5 |  5 | 15 | 10 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: numbers
      (insert "#+NAME: nums\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((n1 4) (n2 7) (n3 2) (n4 9) (n5 5))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: compute
      (insert "#+NAME: comp\n")
      (insert "#+begin_src emacs-lisp :var data=nums :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (+ 10 (cadr r)) (* 2 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("nums" "comp"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read
      (goto-char (point-min))
      (search-forward "#+RESULTS: nums")
      (forward-line 1)
      (let ((nums1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: comp")
        (forward-line 1)
        (let ((comp1 (org-babel-read-result)))
          ;; Edit: change n3 from 2 to 20
          (goto-char (point-min))
          (search-forward "(n3 2)")
          (replace-match "(n3 20)")
          ;; Re-execute
          (dolist (name '("nums" "comp"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: nums")
          (forward-line 1)
          (let ((nums2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: comp")
            (forward-line 1)
            (let ((comp2 (org-babel-read-result)))
              (list nums1 comp1 nums2 comp2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_four_stage_intermediate_read_back_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"a\" 3) (\"b\" 7) (\"c\" 1) (\"d\" 9)) ((\"a\" 6) (\"b\" 14) (\"c\" 2) (\"d\" 18)) ((\"a\" 6 106) (\"b\" 14 114) (\"c\" 2 102) (\"d\" 18 118)) ((\"a\" 106) (\"b\" 114) (\"c\" 102) (\"d\" 118))) (((\"a\" 3) (\"b\" 7) (\"c\" 20) (\"d\" 9)) ((\"a\" 6) (\"b\" 14) (\"c\" 40) (\"d\" 18)) ((\"a\" 6 106) (\"b\" 14 114) (\"c\" 40 140) (\"d\" 18 118)) ((\"a\" 106) (\"b\" 114) (\"c\" 140) (\"d\" 118))) \"#+NAME: raw\\n#+begin_src emacs-lisp :results value replace\\n'((a 3) (b 7) (c 20) (d 9))\\n#+end_src\\n\\n#+RESULTS: raw\\n| a |  3 |\\n| b |  7 |\\n| c | 20 |\\n| d |  9 |\\n\\n#+NAME: doubled\\n#+begin_src emacs-lisp :var data=raw :results value replace\\n(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: doubled\\n| a |  6 |\\n| b | 14 |\\n| c | 40 |\\n| d | 18 |\\n\\n#+NAME: added\\n#+begin_src emacs-lisp :var data=doubled :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (+ 100 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: added\\n| a |  6 | 106 |\\n| b | 14 | 114 |\\n| c | 40 | 140 |\\n| d | 18 | 118 |\\n\\n#+NAME: final\\n#+begin_src emacs-lisp :var data=added :results value replace\\n(mapcar (lambda (r) (list (car r) (caddr r))) data)\\n#+end_src\\n\\n#+RESULTS: final\\n| a | 106 |\\n| b | 114 |\\n| c | 140 |\\n| d | 118 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: raw data
      (insert "#+NAME: raw\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a 3) (b 7) (c 1) (d 9))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: doubled
      (insert "#+NAME: doubled\n")
      (insert "#+begin_src emacs-lisp :var data=raw :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Stage 3: added
      (insert "#+NAME: added\n")
      (insert "#+begin_src emacs-lisp :var data=doubled :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (+ 100 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Stage 4: final
      (insert "#+NAME: final\n")
      (insert "#+begin_src emacs-lisp :var data=added :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (caddr r))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("raw" "doubled" "added" "final"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read all intermediates
      (let ((snap (lambda ()
                    (let ((results nil))
                      (goto-char (point-min))
                      (while (re-search-forward "#\\+RESULTS:" nil t)
                        (forward-line 1)
                        (push (org-babel-read-result) results))
                      (nreverse results)))))
        (let ((results1 (funcall snap)))
          ;; Edit: change c from 1 to 20
          (goto-char (point-min))
          (search-forward "(c 1)")
          (replace-match "(c 20)")
          ;; Re-execute
          (dolist (name '("raw" "doubled" "added" "final"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          (let ((results2 (funcall snap)))
            (list results1 results2
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_three_stage_intermediate_read_back_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"m\" 4) (\"n\" 7) (\"o\" 2) (\"p\" 9)) ((\"m\" 8) (\"n\" 14) (\"o\" 4) (\"p\" 18)) ((\"m\" 8 58) (\"n\" 14 64) (\"o\" 4 54) (\"p\" 18 68)) ((\"m\" 4) (\"n\" 7) (\"o\" 15) (\"p\" 9)) ((\"m\" 8) (\"n\" 14) (\"o\" 30) (\"p\" 18)) ((\"m\" 8 58) (\"n\" 14 64) (\"o\" 30 80) (\"p\" 18 68)) \"#+NAME: raw\\n#+begin_src emacs-lisp :results value replace\\n'((m 4) (n 7) (o 15) (p 9))\\n#+end_src\\n\\n#+RESULTS: raw\\n| m |  4 |\\n| n |  7 |\\n| o | 15 |\\n| p |  9 |\\n\\n#+NAME: doubled\\n#+begin_src emacs-lisp :var data=raw :results value replace\\n(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: doubled\\n| m |  8 |\\n| n | 14 |\\n| o | 30 |\\n| p | 18 |\\n\\n#+NAME: added\\n#+begin_src emacs-lisp :var data=doubled :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (+ 50 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: added\\n| m |  8 | 58 |\\n| n | 14 | 64 |\\n| o | 30 | 80 |\\n| p | 18 | 68 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: raw
      (insert "#+NAME: raw\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((m 4) (n 7) (o 2) (p 9))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: doubled
      (insert "#+NAME: doubled\n")
      (insert "#+begin_src emacs-lisp :var data=raw :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Stage 3: added
      (insert "#+NAME: added\n")
      (insert "#+begin_src emacs-lisp :var data=doubled :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (+ 50 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("raw" "doubled" "added"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read all intermediates
      (goto-char (point-min))
      (search-forward "#+RESULTS: raw")
      (forward-line 1)
      (let ((raw1 (org-babel-read-result)))
        (goto-char (point-min))
        (search-forward "#+RESULTS: doubled")
        (forward-line 1)
        (let ((doubled1 (org-babel-read-result)))
          (goto-char (point-min))
          (search-forward "#+RESULTS: added")
          (forward-line 1)
          (let ((added1 (org-babel-read-result)))
            ;; Edit: change o from 2 to 15
            (goto-char (point-min))
            (search-forward "(o 2)")
            (replace-match "(o 15)")
            ;; Re-execute
            (dolist (name '("raw" "doubled" "added"))
              (goto-char (point-min))
              (search-forward name)
              (org-babel-execute-src-block))
            ;; Re-read
            (goto-char (point-min))
            (search-forward "#+RESULTS: raw")
            (forward-line 1)
            (let ((raw2 (org-babel-read-result)))
              (goto-char (point-min))
              (search-forward "#+RESULTS: doubled")
              (forward-line 1)
              (let ((doubled2 (org-babel-read-result)))
                (goto-char (point-min))
                (search-forward "#+RESULTS: added")
                (forward-line 1)
                (let ((added2 (org-babel-read-result)))
                  (list raw1 doubled1 added1
                        raw2 doubled2 added2
                        (buffer-substring-no-properties
                         (point-min) (point-max))))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_intermediate_read_edit_reexecute_v2() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"p\" 2) (\"q\" 5) (\"r\" 8) (\"s\" 3) (\"t\" 7)) ((\"p\" 102) (\"q\" 105) (\"r\" 108) (\"s\" 103) (\"t\" 107)) ((\"p\" 2) (\"q\" 5) (\"r\" 20) (\"s\" 3) (\"t\" 7)) ((\"p\" 102) (\"q\" 105) (\"r\" 120) (\"s\" 103) (\"t\" 107)) \"#+NAME: gen\\n#+begin_src emacs-lisp :results value replace\\n'((p 2) (q 5) (r 20) (s 3) (t 7))\\n#+end_src\\n\\n#+RESULTS: gen\\n| p |  2 |\\n| q |  5 |\\n| r | 20 |\\n| s |  3 |\\n| t |  7 |\\n\\n#+NAME: xform\\n#+begin_src emacs-lisp :var data=gen :results value replace\\n(mapcar (lambda (r) (list (car r) (+ 100 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: xform\\n| p | 102 |\\n| q | 105 |\\n| r | 120 |\\n| s | 103 |\\n| t | 107 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: generate data
      (insert "#+NAME: gen\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((p 2) (q 5) (r 8) (s 3) (t 7))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: transform
      (insert "#+NAME: xform\n")
      (insert "#+begin_src emacs-lisp :var data=gen :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (+ 100 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("gen" "xform"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read intermediate
      (goto-char (point-min))
      (search-forward "#+RESULTS: gen")
      (forward-line 1)
      (let ((gen-result (org-babel-read-result)))
        ;; Read final
        (goto-char (point-min))
        (search-forward "#+RESULTS: xform")
        (forward-line 1)
        (let ((xform-result (org-babel-read-result)))
          ;; Edit: change r from 8 to 20
          (goto-char (point-min))
          (search-forward "(r 8)")
          (replace-match "(r 20)")
          ;; Re-execute
          (dolist (name '("gen" "xform"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          ;; Re-read
          (goto-char (point-min))
          (search-forward "#+RESULTS: gen")
          (forward-line 1)
          (let ((gen-result2 (org-babel-read-result)))
            (goto-char (point-min))
            (search-forward "#+RESULTS: xform")
            (forward-line 1)
            (let ((xform-result2 (org-babel-read-result)))
              (list gen-result xform-result
                    gen-result2 xform-result2
                    (buffer-substring-no-properties
                     (point-min) (point-max))))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_two_stage_intermediate_results_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"x\" 10) (\"y\" 20) (\"z\" 30) (\"w\" 40)) ((\"x\" 10 100) (\"y\" 20 400) (\"z\" 30 900) (\"w\" 40 1600))) ((\"x\" 10) (\"y\" 20) (\"z\" 30) (\"w\" 40)) (((\"x\" 10) (\"y\" 25) (\"z\" 30) (\"w\" 40)) ((\"x\" 10 100) (\"y\" 25 625) (\"z\" 30 900) (\"w\" 40 1600))) \"#+NAME: pairs\\n#+begin_src emacs-lisp :results value replace\\n'((x 10) (y 25) (z 30) (w 40))\\n#+end_src\\n\\n#+RESULTS: pairs\\n| x | 10 |\\n| y | 25 |\\n| z | 30 |\\n| w | 40 |\\n\\n#+NAME: computed\\n#+begin_src emacs-lisp :var data=pairs :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (* (cadr r) (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: computed\\n| x | 10 |  100 |\\n| y | 25 |  625 |\\n| z | 30 |  900 |\\n| w | 40 | 1600 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: generate pairs
      (insert "#+NAME: pairs\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((x 10) (y 20) (z 30) (w 40))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: compute
      (insert "#+NAME: computed\n")
      (insert "#+begin_src emacs-lisp :var data=pairs :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (* (cadr r) (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("pairs" "computed"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Read intermediate: check pairs result
        (goto-char (point-min))
        (search-forward "#+RESULTS: pairs")
        (forward-line 1)
        (let ((pairs-result (org-babel-read-result)))
          ;; Edit: change y from 20 to 25
          (goto-char (point-min))
          (search-forward "(y 20)")
          (replace-match "(y 25)")
          ;; Re-execute
          (dolist (name '("pairs" "computed"))
            (goto-char (point-min))
            (search-forward name)
            (org-babel-execute-src-block))
          (let ((results2 nil))
            (goto-char (point-min))
            (while (re-search-forward "#\\+RESULTS:" nil t)
              (forward-line 1)
              (push (org-babel-read-result) results2))
            (list (nreverse results1)
                  pairs-result
                  (nreverse results2)
                  (buffer-substring-no-properties
                   (point-min) (point-max)))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_four_stage_chain_filter_sort_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Alice\" 85) (\"Bob\" 92) (\"Carol\" 78) (\"Dave\" 95) (\"Eve\" 88)) ((\"Dave\" 95) (\"Bob\" 92) (\"Eve\" 88) (\"Alice\" 85) (\"Carol\" 78)) ((1 \"Dave\" 95) (2 \"Bob\" 92) (3 \"Eve\" 88) (4 \"Alice\" 85) (5 \"Carol\" 78)) ((1 \"Dave\" 95) (2 \"Bob\" 92) (3 \"Eve\" 88))) (((\"Alice\" 85) (\"Bob\" 92) (\"Carol\" 99) (\"Dave\" 95) (\"Eve\" 88)) ((\"Carol\" 99) (\"Dave\" 95) (\"Bob\" 92) (\"Eve\" 88) (\"Alice\" 85)) ((1 \"Carol\" 99) (2 \"Dave\" 95) (3 \"Bob\" 92) (4 \"Eve\" 88) (5 \"Alice\" 85)) ((1 \"Carol\" 99) (2 \"Dave\" 95) (3 \"Bob\" 92))) \"#+NAME: scores\\n#+begin_src emacs-lisp :results value replace\\n'((\\\"Alice\\\" 85) (\\\"Bob\\\" 92) (\\\"Carol\\\" 99) (\\\"Dave\\\" 95) (\\\"Eve\\\" 88))\\n#+end_src\\n\\n#+RESULTS: scores\\n| Alice | 85 |\\n| Bob   | 92 |\\n| Carol | 99 |\\n| Dave  | 95 |\\n| Eve   | 88 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=scores :results value replace\\n(sort (copy-sequence data) (lambda (x y) (> (cadr x) (cadr y))))\\n#+end_src\\n\\n#+RESULTS: sorted\\n| Carol | 99 |\\n| Dave  | 95 |\\n| Bob   | 92 |\\n| Eve   | 88 |\\n| Alice | 85 |\\n\\n#+NAME: ranked\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(let ((i 0))\\n  (mapcar (lambda (r) (setq i (1+ i)) (list i (car r) (cadr r))) data))\\n#+end_src\\n\\n#+RESULTS: ranked\\n| 1 | Carol | 99 |\\n| 2 | Dave  | 95 |\\n| 3 | Bob   | 92 |\\n| 4 | Eve   | 88 |\\n| 5 | Alice | 85 |\\n\\n#+NAME: top3\\n#+begin_src emacs-lisp :var data=ranked :results value replace\\n(seq-take data 3)\\n#+end_src\\n\\n#+RESULTS: top3\\n| 1 | Carol | 99 |\\n| 2 | Dave  | 95 |\\n| 3 | Bob   | 92 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: raw scores
      (insert "#+NAME: scores\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((\"Alice\" 85) (\"Bob\" 92) (\"Carol\" 78) (\"Dave\" 95) (\"Eve\" 88))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: sorted
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=scores :results value replace\n")
      (insert "(sort (copy-sequence data) (lambda (x y) (> (cadr x) (cadr y))))\n")
      (insert "#+end_src\n\n")
      ;; Stage 3: ranked
      (insert "#+NAME: ranked\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(let ((i 0))\n  (mapcar (lambda (r) (setq i (1+ i)) (list i (car r) (cadr r))) data))\n")
      (insert "#+end_src\n\n")
      ;; Stage 4: top3
      (insert "#+NAME: top3\n")
      (insert "#+begin_src emacs-lisp :var data=ranked :results value replace\n")
      (insert "(seq-take data 3)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("scores" "sorted" "ranked" "top3"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit: change Carol score to 99
        (goto-char (point-min))
        (search-forward "(\"Carol\" 78)")
        (replace-match "(\"Carol\" 99)")
        ;; Re-execute
        (dolist (name '("scores" "sorted" "ranked" "top3"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_three_stage_chain_edit_reexecute_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"a\" 3) (\"b\" 7) (\"c\" 1) (\"d\" 9) (\"e\" 5)) ((\"c\" 1) (\"a\" 3) (\"e\" 5) (\"b\" 7) (\"d\" 9)) ((1 \"c\" 1) (2 \"a\" 3) (3 \"e\" 5) (4 \"b\" 7) (5 \"d\" 9))) (((\"a\" 3) (\"b\" 7) (\"c\" 10) (\"d\" 9) (\"e\" 5)) ((\"a\" 3) (\"e\" 5) (\"b\" 7) (\"d\" 9) (\"c\" 10)) ((1 \"a\" 3) (2 \"e\" 5) (3 \"b\" 7) (4 \"d\" 9) (5 \"c\" 10))) \"#+NAME: raw\\n#+begin_src emacs-lisp :results value replace\\n'((a 3) (b 7) (c 10) (d 9) (e 5))\\n#+end_src\\n\\n#+RESULTS: raw\\n| a |  3 |\\n| b |  7 |\\n| c | 10 |\\n| d |  9 |\\n| e |  5 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=raw :results value replace\\n(sort (copy-sequence data) (lambda (x y) (< (cadr x) (cadr y))))\\n#+end_src\\n\\n#+RESULTS: sorted\\n| a |  3 |\\n| e |  5 |\\n| b |  7 |\\n| d |  9 |\\n| c | 10 |\\n\\n#+NAME: indexed\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(let ((i 0))\\n  (mapcar (lambda (r) (setq i (1+ i)) (list i (car r) (cadr r))) data))\\n#+end_src\\n\\n#+RESULTS: indexed\\n| 1 | a |  3 |\\n| 2 | e |  5 |\\n| 3 | b |  7 |\\n| 4 | d |  9 |\\n| 5 | c | 10 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: raw data
      (insert "#+NAME: raw\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a 3) (b 7) (c 1) (d 9) (e 5))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: sorted
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=raw :results value replace\n")
      (insert "(sort (copy-sequence data) (lambda (x y) (< (cadr x) (cadr y))))\n")
      (insert "#+end_src\n\n")
      ;; Stage 3: indexed
      (insert "#+NAME: indexed\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(let ((i 0))\n  (mapcar (lambda (r) (setq i (1+ i)) (list i (car r) (cadr r))) data))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("raw" "sorted" "indexed"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit: change c from 1 to 10
        (goto-char (point-min))
        (search-forward "(c 1)")
        (replace-match "(c 10)")
        ;; Re-execute
        (dolist (name '("raw" "sorted" "indexed"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_scalar_list_table_edit_reexecute_chain_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((60 (\"alpha\" \"beta\" \"gamma\" \"delta\" \"epsilon\") ((\"X\" \"Y\" \"Z\") hline (1 2 3) (4 5 6) (7 8 9))) (300 (\"one\" \"two\" \"three\") ((\"X\" \"Y\" \"Z\") hline (1 2 3) (4 5 6) (7 8 9))) \"#+NAME: count\\n#+begin_src emacs-lisp :results scalar replace\\n(+ 100 200)\\n#+end_src\\n\\n#+RESULTS: count\\n: 300\\n\\n#+NAME: items\\n#+begin_src emacs-lisp :results list replace\\n'(one two three)\\n#+end_src\\n\\n#+RESULTS: items\\n- one\\n- two\\n- three\\n\\n#+NAME: matrix\\n#+begin_src emacs-lisp :results table replace\\n(list '(\\\"X\\\" \\\"Y\\\" \\\"Z\\\") 'hline '(1 2 3) '(4 5 6) '(7 8 9))\\n#+end_src\\n\\n#+RESULTS: matrix\\n| X | Y | Z |\\n|---+---+---|\\n| 1 | 2 | 3 |\\n| 4 | 5 | 6 |\\n| 7 | 8 | 9 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Scalar
      (insert "#+NAME: count\n")
      (insert "#+begin_src emacs-lisp :results scalar replace\n")
      (insert "(+ 10 20 30)\n")
      (insert "#+end_src\n\n")
      ;; List
      (insert "#+NAME: items\n")
      (insert "#+begin_src emacs-lisp :results list replace\n")
      (insert "'(alpha beta gamma delta epsilon)\n")
      (insert "#+end_src\n\n")
      ;; Table
      (insert "#+NAME: matrix\n")
      (insert "#+begin_src emacs-lisp :results table replace\n")
      (insert "(list '(\"X\" \"Y\" \"Z\") 'hline '(1 2 3) '(4 5 6) '(7 8 9))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("count" "items" "matrix"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit: change scalar
        (goto-char (point-min))
        (search-forward "(+ 10 20 30)")
        (replace-match "(+ 100 200)")
        ;; Edit: change list
        (goto-char (point-min))
        (search-forward "'(alpha beta gamma delta epsilon)")
        (replace-match "'(one two three)")
        ;; Re-execute
        (dolist (name '("count" "items" "matrix"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_nested_variable_chain_edit_reexecute_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"a\" 5) (\"b\" 10) (\"c\" 15) (\"d\" 20) (\"e\" 25)) ((\"a\" 10) (\"b\" 20) (\"c\" 30) (\"d\" 40) (\"e\" 50)) ((\"a\" 110) (\"b\" 120) (\"c\" 130) (\"d\" 140) (\"e\" 150)) ((\"b\" 120) (\"c\" 130) (\"d\" 140) (\"e\" 150))) (((\"a\" 5) (\"b\" 10) (\"c\" 50) (\"d\" 20) (\"e\" 25)) ((\"a\" 10) (\"b\" 20) (\"c\" 100) (\"d\" 40) (\"e\" 50)) ((\"a\" 110) (\"b\" 120) (\"c\" 200) (\"d\" 140) (\"e\" 150)) ((\"b\" 120) (\"c\" 200) (\"d\" 140) (\"e\" 150))) \"#+NAME: raw\\n#+begin_src emacs-lisp :results value replace\\n'((a 5) (b 10) (c 50) (d 20) (e 25))\\n#+end_src\\n\\n#+RESULTS: raw\\n| a |  5 |\\n| b | 10 |\\n| c | 50 |\\n| d | 20 |\\n| e | 25 |\\n\\n#+NAME: doubled\\n#+begin_src emacs-lisp :var data=raw :results value replace\\n(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: doubled\\n| a |  10 |\\n| b |  20 |\\n| c | 100 |\\n| d |  40 |\\n| e |  50 |\\n\\n#+NAME: shifted\\n#+begin_src emacs-lisp :var data=doubled :results value replace\\n(mapcar (lambda (r) (list (car r) (+ 100 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: shifted\\n| a | 110 |\\n| b | 120 |\\n| c | 200 |\\n| d | 140 |\\n| e | 150 |\\n\\n#+NAME: filtered\\n#+begin_src emacs-lisp :var data=shifted :results value replace\\n(seq-filter (lambda (r) (> (cadr r) 110)) data)\\n#+end_src\\n\\n#+RESULTS: filtered\\n| b | 120 |\\n| c | 200 |\\n| d | 140 |\\n| e | 150 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Stage 1: raw data
      (insert "#+NAME: raw\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a 5) (b 10) (c 15) (d 20) (e 25))\n")
      (insert "#+end_src\n\n")
      ;; Stage 2: double
      (insert "#+NAME: doubled\n")
      (insert "#+begin_src emacs-lisp :var data=raw :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Stage 3: add constant
      (insert "#+NAME: shifted\n")
      (insert "#+begin_src emacs-lisp :var data=doubled :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (+ 100 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Stage 4: filter
      (insert "#+NAME: filtered\n")
      (insert "#+begin_src emacs-lisp :var data=shifted :results value replace\n")
      (insert "(seq-filter (lambda (r) (> (cadr r) 110)) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("raw" "doubled" "shifted" "filtered"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit: change c from 15 to 50
        (goto-char (point-min))
        (search-forward "(c 15)")
        (replace-match "(c 50)")
        ;; Re-execute
        (dolist (name '("raw" "doubled" "shifted" "filtered"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_multi_format_results_edit_reexecute_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((15 (\"a\" \"b\" \"c\" \"d\" \"e\") ((\"X\" \"Y\") hline (1 2) (3 4) (5 6))) (60 (\"x\" \"y\" \"z\") ((\"X\" \"Y\") hline (1 2) (3 4) (5 6))) \"#+NAME: scalar\\n#+begin_src emacs-lisp :results scalar replace\\n(+ 10 20 30)\\n#+end_src\\n\\n#+RESULTS: scalar\\n: 60\\n\\n#+NAME: list-res\\n#+begin_src emacs-lisp :results list replace\\n'(x y z)\\n#+end_src\\n\\n#+RESULTS: list-res\\n- x\\n- y\\n- z\\n\\n#+NAME: table-res\\n#+begin_src emacs-lisp :results table replace\\n(list '(\\\"X\\\" \\\"Y\\\") 'hline '(1 2) '(3 4) '(5 6))\\n#+end_src\\n\\n#+RESULTS: table-res\\n| X | Y |\\n|---+---|\\n| 1 | 2 |\\n| 3 | 4 |\\n| 5 | 6 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Scalar result
      (insert "#+NAME: scalar\n")
      (insert "#+begin_src emacs-lisp :results scalar replace\n")
      (insert "(+ 1 2 3 4 5)\n")
      (insert "#+end_src\n\n")
      ;; List result
      (insert "#+NAME: list-res\n")
      (insert "#+begin_src emacs-lisp :results list replace\n")
      (insert "'(a b c d e)\n")
      (insert "#+end_src\n\n")
      ;; Table result
      (insert "#+NAME: table-res\n")
      (insert "#+begin_src emacs-lisp :results table replace\n")
      (insert "(list '(\"X\" \"Y\") 'hline '(1 2) '(3 4) '(5 6))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dolist (name '("scalar" "list-res" "table-res"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit: change scalar
        (goto-char (point-min))
        (search-forward "(+ 1 2 3 4 5)")
        (replace-match "(+ 10 20 30)")
        ;; Edit: change list
        (goto-char (point-min))
        (search-forward "'(a b c d e)")
        (replace-match "'(x y z)")
        ;; Re-execute
        (dolist (name '("scalar" "list-res" "table-res"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_list_results_chain_edit_reexecute_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (((\"Alice\" \"Bob\" \"Carol\" \"Dave\" \"Eve\") (\"Alice\\n5\" \"Bob\\n3\" \"Carol\\n5\" \"Dave\\n4\" \"Eve\\n3\")) ((\"X\" \"Y\" \"Z\") (\"X\\n1\" \"Y\\n1\" \"Z\\n1\")) \"#+NAME: names\\n#+begin_src emacs-lisp :results value list replace\\n'(\\\"X\\\" \\\"Y\\\" \\\"Z\\\")\\n#+end_src\\n\\n#+RESULTS: names\\n- X\\n- Y\\n- Z\\n\\n#+NAME: scored\\n#+begin_src emacs-lisp :var names=names :results value list replace\\n(mapcar (lambda (n) (list n (length (format \\\"%s\\\" n)))) names)\\n#+end_src\\n\\n#+RESULTS: scored\\n- X\\n  1\\n- Y\\n  1\\n- Z\\n  1\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source: generate names
      (insert "#+NAME: names\n")
      (insert "#+begin_src emacs-lisp :results value list replace\n")
      (insert "'(\"Alice\" \"Bob\" \"Carol\" \"Dave\" \"Eve\")\n")
      (insert "#+end_src\n\n")
      ;; Transform: add score
      (insert "#+NAME: scored\n")
      (insert "#+begin_src emacs-lisp :var names=names :results value list replace\n")
      (insert "(mapcar (lambda (n) (list n (length (format \"%s\" n)))) names)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("names" "scored"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit: change names
        (goto-char (point-min))
        (search-forward "\"Alice\" \"Bob\" \"Carol\" \"Dave\" \"Eve\"")
        (replace-match "\"X\" \"Y\" \"Z\"")
        ;; Re-execute
        (dolist (name '("names" "scored"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_results_edit_reexecute_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Name\" \"Score\") hline (\"Alice\" 85) (\"Bob\" 92) (\"Carol\" 78)) ((\"Name\" \"Score\" \"Grade\") hline)) (((\"Name\" \"Score\") hline (\"Alice\" 85) (\"Bob\" 95) (\"Carol\" 78)) ((\"Name\" \"Score\" \"Grade\") hline)) \"#+NAME: gen-table\\n#+begin_src emacs-lisp :results value table replace\\n(list '(\\\"Name\\\" \\\"Score\\\")\\n      'hline\\n      '(\\\"Alice\\\" 85)\\n      '(\\\"Bob\\\" 95)\\n      '(\\\"Carol\\\" 78))\\n#+end_src\\n\\n#+RESULTS: gen-table\\n| Name  | Score |\\n|-------+-------|\\n| Alice |    85 |\\n| Bob   |    95 |\\n| Carol |    78 |\\n\\n#+NAME: graded\\n#+begin_src emacs-lisp :var data=gen-table :results value table replace\\n(cons '(\\\"Name\\\" \\\"Score\\\" \\\"Grade\\\")\\n      (cons 'hline\\n            (mapcar (lambda (r)\\n                      (list (car r) (cadr r)\\n                            (if (>= (cadr r) 90) \\\"A\\\"\\n                              (if (>= (cadr r) 80) \\\"B\\\" \\\"C\\\"))))\\n                    (cdr (memq 'hline data)))))\\n#+end_src\\n\\n#+RESULTS: graded\\n| Name | Score | Grade |\\n|------+-------+-------|\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source: generate table
      (insert "#+NAME: gen-table\n")
      (insert "#+begin_src emacs-lisp :results value table replace\n")
      (insert "(list '(\"Name\" \"Score\")\n")
      (insert "      'hline\n")
      (insert "      '(\"Alice\" 85)\n")
      (insert "      '(\"Bob\" 92)\n")
      (insert "      '(\"Carol\" 78))\n")
      (insert "#+end_src\n\n")
      ;; Transform: add grade column
      (insert "#+NAME: graded\n")
      (insert "#+begin_src emacs-lisp :var data=gen-table :results value table replace\n")
      (insert "(cons '(\"Name\" \"Score\" \"Grade\")\n")
      (insert "      (cons 'hline\n")
      (insert "            (mapcar (lambda (r)\n")
      (insert "                      (list (car r) (cadr r)\n")
      (insert "                            (if (>= (cadr r) 90) \"A\"\n")
      (insert "                              (if (>= (cadr r) 80) \"B\" \"C\"))))\n")
      (insert "                    (cdr (memq 'hline data)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("gen-table" "graded"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit: change Bob score to 95
        (goto-char (point-min))
        (search-forward "(\"Bob\" 92)")
        (replace-match "(\"Bob\" 95)")
        ;; Re-execute
        (dolist (name '("gen-table" "graded"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_header_args_results_format_edit_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((15 (\"a\" \"b\" \"c\" \"d\" \"e\")) (60 (\"a\" \"b\" \"c\" \"d\" \"e\")) \"#+NAME: scalar-out\\n#+begin_src emacs-lisp :results value scalar replace\\n(+ 10 20 30)\\n#+end_src\\n\\n#+RESULTS: scalar-out\\n: 60\\n\\n#+NAME: list-out\\n#+begin_src emacs-lisp :results value list replace\\n'(a b c d e)\\n#+end_src\\n\\n#+RESULTS: list-out\\n- a\\n- b\\n- c\\n- d\\n- e\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Output scalar
      (insert "#+NAME: scalar-out\n")
      (insert "#+begin_src emacs-lisp :results value scalar replace\n")
      (insert "(+ 1 2 3 4 5)\n")
      (insert "#+end_src\n\n")
      ;; Output file
      (insert "#+NAME: list-out\n")
      (insert "#+begin_src emacs-lisp :results value list replace\n")
      (insert "'(a b c d e)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("scalar-out" "list-out"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit scalar
        (goto-char (point-min))
        (search-forward "(+ 1 2 3 4 5)")
        (replace-match "(+ 10 20 30)")
        ;; Re-execute
        (dolist (name '("scalar-out" "list-out"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_variable_propagation_edit_reexecute_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"x\" 10) (\"y\" 20) (\"z\" 30)) ((\"x\" 20) (\"y\" 40) (\"z\" 60)) ((\"x\" 120) (\"y\" 140) (\"z\" 160))) (((\"x\" 5) (\"y\" 15) (\"z\" 25)) ((\"x\" 10) (\"y\" 30) (\"z\" 50)) ((\"x\" 110) (\"y\" 130) (\"z\" 150))) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'((x 5) (y 15) (z 25))\\n#+end_src\\n\\n#+RESULTS: seed\\n| x |  5 |\\n| y | 15 |\\n| z | 25 |\\n\\n#+NAME: transform-a\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: transform-a\\n| x | 10 |\\n| y | 30 |\\n| z | 50 |\\n\\n#+NAME: transform-b\\n#+begin_src emacs-lisp :var data=transform-a :results value replace\\n(mapcar (lambda (r) (list (car r) (+ 100 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: transform-b\\n| x | 110 |\\n| y | 130 |\\n| z | 150 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((x 10) (y 20) (z 30))\n")
      (insert "#+end_src\n\n")
      ;; Transform A
      (insert "#+NAME: transform-a\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Transform B
      (insert "#+NAME: transform-b\n")
      (insert "#+begin_src emacs-lisp :var data=transform-a :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (+ 100 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("seed" "transform-a" "transform-b"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit: change seed values
        (goto-char (point-min))
        (search-forward "'((x 10) (y 20) (z 30))")
        (replace-match "'((x 5) (y 15) (z 25))")
        ;; Re-execute chain
        (dolist (name '("seed" "transform-a" "transform-b"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_edit_code_reexecute_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"a\" 1) (\"b\" 2) (\"c\" 3) (\"d\" 4) (\"e\" 5)) ((\"a\" 2) (\"b\" 4) (\"c\" 6) (\"d\" 8) (\"e\" 10))) (((\"a\" 1) (\"b\" 2) (\"c\" 3) (\"d\" 4) (\"e\" 5)) ((\"a\" 10) (\"b\" 20) (\"c\" 30) (\"d\" 40) (\"e\" 50))) \"#+NAME: data\\n#+begin_src emacs-lisp :results value replace\\n'((a 1) (b 2) (c 3) (d 4) (e 5))\\n#+end_src\\n\\n#+RESULTS: data\\n| a | 1 |\\n| b | 2 |\\n| c | 3 |\\n| d | 4 |\\n| e | 5 |\\n\\n#+NAME: doubled\\n#+begin_src emacs-lisp :var data=data :results value replace\\n(mapcar (lambda (r) (list (car r) (* 10 (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: doubled\\n| a | 10 |\\n| b | 20 |\\n| c | 30 |\\n| d | 40 |\\n| e | 50 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Data source
      (insert "#+NAME: data\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a 1) (b 2) (c 3) (d 4) (e 5))\n")
      (insert "#+end_src\n\n")
      ;; Transform
      (insert "#+NAME: doubled\n")
      (insert "#+begin_src emacs-lisp :var data=data :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("data" "doubled"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      (let ((results1 nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results1))
        ;; Edit: change multiplier from 2 to 10
        (goto-char (point-min))
        (search-forward "(* 2 (cadr r))")
        (replace-match "(* 10 (cadr r))")
        ;; Re-execute
        (dolist (name '("data" "doubled"))
          (goto-char (point-min))
          (search-forward name)
          (org-babel-execute-src-block))
        (let ((results2 nil))
          (goto-char (point-min))
          (while (re-search-forward "#\\+RESULTS:" nil t)
            (forward-line 1)
            (push (org-babel-read-result) results2))
          (list (nreverse results1)
                (nreverse results2)
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_fifteen_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5 6 7 8 9 10)) ((1 1) (2 3) (3 6) (4 10) (5 15) (6 21) (7 28) (8 36) (9 45) (10 55))) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(10 5 8 3 7 1 9 2 6 4)\\n#+end_src\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(sort (copy-sequence data) #'<)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |\\n\\n#+NAME: running\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(let ((sum 0))\\n  (mapcar (lambda (x) (setq sum (+ sum x)) (list x sum)) data))\\n#+end_src\\n\\n#+RESULTS: running\\n|  1 |  1 |\\n|  2 |  3 |\\n|  3 |  6 |\\n|  4 | 10 |\\n|  5 | 15 |\\n|  6 | 21 |\\n|  7 | 28 |\\n|  8 | 36 |\\n|  9 | 45 |\\n| 10 | 55 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(10 5 8 3 7 1 9 2 6 4)\n")
      (insert "#+end_src\n\n")
      ;; Sorted
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(sort (copy-sequence data) #'<)\n")
      (insert "#+end_src\n\n")
      ;; Running sum
      (insert "#+NAME: running\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(let ((sum 0))\n  (mapcar (lambda (x) (setq sum (+ sum x)) (list x sum)) data))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("sorted" "running"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_fourteen_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5 6 7 8 9)) ((1 1) (2 2) (3 3) (4 4) (5 5) (6 6) (7 7) (8 8) (9 9)) ((1 1 1) (2 2 4) (3 3 9) (4 4 16) (5 5 25) (6 6 36) (7 7 49) (8 8 64) (9 9 81))) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(4 7 2 9 1 5 8 3 6)\\n#+end_src\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(sort (copy-sequence data) #'<)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |\\n\\n#+NAME: indexed\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(let ((i 0))\\n  (mapcar (lambda (x) (setq i (1+ i)) (list i x)) data))\\n#+end_src\\n\\n#+RESULTS: indexed\\n| 1 | 1 |\\n| 2 | 2 |\\n| 3 | 3 |\\n| 4 | 4 |\\n| 5 | 5 |\\n| 6 | 6 |\\n| 7 | 7 |\\n| 8 | 8 |\\n| 9 | 9 |\\n\\n#+NAME: squared\\n#+begin_src emacs-lisp :var data=indexed :results value replace\\n(mapcar (lambda (r) (list (car r) (cadr r) (* (cadr r) (cadr r)))) data)\\n#+end_src\\n\\n#+RESULTS: squared\\n| 1 | 1 |  1 |\\n| 2 | 2 |  4 |\\n| 3 | 3 |  9 |\\n| 4 | 4 | 16 |\\n| 5 | 5 | 25 |\\n| 6 | 6 | 36 |\\n| 7 | 7 | 49 |\\n| 8 | 8 | 64 |\\n| 9 | 9 | 81 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(4 7 2 9 1 5 8 3 6)\n")
      (insert "#+end_src\n\n")
      ;; Sorted
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(sort (copy-sequence data) #'<)\n")
      (insert "#+end_src\n\n")
      ;; Index pairs
      (insert "#+NAME: indexed\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(let ((i 0))\n  (mapcar (lambda (x) (setq i (1+ i)) (list i x)) data))\n")
      (insert "#+end_src\n\n")
      ;; Compute squares
      (insert "#+NAME: squared\n")
      (insert "#+begin_src emacs-lisp :var data=indexed :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (cadr r) (* (cadr r) (cadr r)))) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("sorted" "indexed" "squared"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_thirteen_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5 6 7 8 9)) ((1 2 3) (4 5 6) (7 8 9)) ((6 15 24))) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(6 2 8 1 9 3 7 4 5)\\n#+end_src\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(sort (copy-sequence data) #'<)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |\\n\\n#+NAME: partitioned\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(seq-partition data 3)\\n#+end_src\\n\\n#+RESULTS: partitioned\\n| 1 | 2 | 3 |\\n| 4 | 5 | 6 |\\n| 7 | 8 | 9 |\\n\\n#+NAME: sums\\n#+begin_src emacs-lisp :var data=partitioned :results value replace\\n(mapcar (lambda (g) (apply #'+ g)) data)\\n#+end_src\\n\\n#+RESULTS: sums\\n| 6 | 15 | 24 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(6 2 8 1 9 3 7 4 5)\n")
      (insert "#+end_src\n\n")
      ;; Sorted
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(sort (copy-sequence data) #'<)\n")
      (insert "#+end_src\n\n")
      ;; Partition
      (insert "#+NAME: partitioned\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(seq-partition data 3)\n")
      (insert "#+end_src\n\n")
      ;; Sum each group
      (insert "#+NAME: sums\n")
      (insert "#+begin_src emacs-lisp :var data=partitioned :results value replace\n")
      (insert "(mapcar (lambda (g) (apply #'+ g)) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("sorted" "partitioned" "sums"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_twelve_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((8 5 9 6 7)) ((8 64) (5 25) (9 81) (6 36) (7 49)) ((9 81) (8 64) (7 49) (6 36) (5 25))) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(8 3 5 1 9 6 2 7 4)\\n#+end_src\\n\\n#+NAME: filtered\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(cl-remove-if-not (lambda (x) (> x 4)) data)\\n#+end_src\\n\\n#+RESULTS: filtered\\n| 8 | 5 | 9 | 6 | 7 |\\n\\n#+NAME: mapped\\n#+begin_src emacs-lisp :var data=filtered :results value replace\\n(mapcar (lambda (x) (list x (* x x))) data)\\n#+end_src\\n\\n#+RESULTS: mapped\\n| 8 | 64 |\\n| 5 | 25 |\\n| 9 | 81 |\\n| 6 | 36 |\\n| 7 | 49 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=mapped :results value replace\\n(sort (copy-sequence data) (lambda (a b) (> (cadr a) (cadr b))))\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 9 | 81 |\\n| 8 | 64 |\\n| 7 | 49 |\\n| 6 | 36 |\\n| 5 | 25 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(8 3 5 1 9 6 2 7 4)\n")
      (insert "#+end_src\n\n")
      ;; Filtered > 4
      (insert "#+NAME: filtered\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(cl-remove-if-not (lambda (x) (> x 4)) data)\n")
      (insert "#+end_src\n\n")
      ;; Mapped
      (insert "#+NAME: mapped\n")
      (insert "#+begin_src emacs-lisp :var data=filtered :results value replace\n")
      (insert "(mapcar (lambda (x) (list x (* x x))) data)\n")
      (insert "#+end_src\n\n")
      ;; Sorted by square desc
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=mapped :results value replace\n")
      (insert "(sort (copy-sequence data) (lambda (a b) (> (cadr a) (cadr b))))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("filtered" "mapped" "sorted"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_eleven_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((3 7 1 9 4 8 2 6 5)) ((1 2 3 4 5 6 7 8 9)) ((1 1) (2 4) (3 9) (4 16) (5 25) (6 36) (7 49) (8 64) (9 81)) ((5 25) (6 36) (7 49) (8 64) (9 81))) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(3 7 1 9 4 8 2 6 5)\\n#+end_src\\n\\n#+NAME: unique\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(seq-uniq data)\\n#+end_src\\n\\n#+RESULTS: unique\\n| 3 | 7 | 1 | 9 | 4 | 8 | 2 | 6 | 5 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=unique :results value replace\\n(sort (copy-sequence data) #'<)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |\\n\\n#+NAME: mapped\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(mapcar (lambda (x) (list x (* x x))) data)\\n#+end_src\\n\\n#+RESULTS: mapped\\n| 1 |  1 |\\n| 2 |  4 |\\n| 3 |  9 |\\n| 4 | 16 |\\n| 5 | 25 |\\n| 6 | 36 |\\n| 7 | 49 |\\n| 8 | 64 |\\n| 9 | 81 |\\n\\n#+NAME: filtered\\n#+begin_src emacs-lisp :var data=mapped :results value replace\\n(cl-remove-if-not (lambda (r) (> (cadr r) 20)) data)\\n#+end_src\\n\\n#+RESULTS: filtered\\n| 5 | 25 |\\n| 6 | 36 |\\n| 7 | 49 |\\n| 8 | 64 |\\n| 9 | 81 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(3 7 1 9 4 8 2 6 5)\n")
      (insert "#+end_src\n\n")
      ;; Unique
      (insert "#+NAME: unique\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(seq-uniq data)\n")
      (insert "#+end_src\n\n")
      ;; Sorted
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=unique :results value replace\n")
      (insert "(sort (copy-sequence data) #'<)\n")
      (insert "#+end_src\n\n")
      ;; Mapped
      (insert "#+NAME: mapped\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(mapcar (lambda (x) (list x (* x x))) data)\n")
      (insert "#+end_src\n\n")
      ;; Filtered
      (insert "#+NAME: filtered\n")
      (insert "#+begin_src emacs-lisp :var data=mapped :results value replace\n")
      (insert "(cl-remove-if-not (lambda (r) (> (cadr r) 20)) data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("unique" "sorted" "mapped" "filtered"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_ten_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((12 14 16 18 20)) ((6 7 8 9 10)) ((10 9 8 7 6)) 40) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(2 4 6 8 10 12 14 16 18 20)\\n#+end_src\\n\\n#+NAME: filtered\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(cl-remove-if-not (lambda (x) (> x 10)) data)\\n#+end_src\\n\\n#+RESULTS: filtered\\n| 12 | 14 | 16 | 18 | 20 |\\n\\n#+NAME: halved\\n#+begin_src emacs-lisp :var data=filtered :results value replace\\n(mapcar (lambda (x) (/ x 2)) data)\\n#+end_src\\n\\n#+RESULTS: halved\\n| 6 | 7 | 8 | 9 | 10 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=halved :results value replace\\n(sort (copy-sequence data) #'>)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 10 | 9 | 8 | 7 | 6 |\\n\\n#+NAME: total\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(apply #'+ data)\\n#+end_src\\n\\n#+RESULTS: total\\n: 40\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(2 4 6 8 10 12 14 16 18 20)\n")
      (insert "#+end_src\n\n")
      ;; Filter > 10
      (insert "#+NAME: filtered\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(cl-remove-if-not (lambda (x) (> x 10)) data)\n")
      (insert "#+end_src\n\n")
      ;; Halved
      (insert "#+NAME: halved\n")
      (insert "#+begin_src emacs-lisp :var data=filtered :results value replace\n")
      (insert "(mapcar (lambda (x) (/ x 2)) data)\n")
      (insert "#+end_src\n\n")
      ;; Sorted desc
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=halved :results value replace\n")
      (insert "(sort (copy-sequence data) #'>)\n")
      (insert "#+end_src\n\n")
      ;; Total
      (insert "#+NAME: total\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(apply #'+ data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("filtered" "halved" "sorted" "total"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_nine_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 3 5 7 9)) ((1 27 125 343 729)) ((125 343 729)) ((729 343 125)) 1197) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(1 2 3 4 5 6 7 8 9 10)\\n#+end_src\\n\\n#+NAME: odds\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(seq-remove #'evenp data)\\n#+end_src\\n\\n#+RESULTS: odds\\n| 1 | 3 | 5 | 7 | 9 |\\n\\n#+NAME: cubed\\n#+begin_src emacs-lisp :var data=odds :results value replace\\n(mapcar (lambda (x) (* x x x)) data)\\n#+end_src\\n\\n#+RESULTS: cubed\\n| 1 | 27 | 125 | 343 | 729 |\\n\\n#+NAME: big\\n#+begin_src emacs-lisp :var data=cubed :results value replace\\n(cl-remove-if-not (lambda (x) (> x 100)) data)\\n#+end_src\\n\\n#+RESULTS: big\\n| 125 | 343 | 729 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=big :results value replace\\n(sort (copy-sequence data) #'>)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 729 | 343 | 125 |\\n\\n#+NAME: total\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(apply #'+ data)\\n#+end_src\\n\\n#+RESULTS: total\\n: 1197\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(1 2 3 4 5 6 7 8 9 10)\n")
      (insert "#+end_src\n\n")
      ;; Odd
      (insert "#+NAME: odds\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(seq-remove #'evenp data)\n")
      (insert "#+end_src\n\n")
      ;; Cubed
      (insert "#+NAME: cubed\n")
      (insert "#+begin_src emacs-lisp :var data=odds :results value replace\n")
      (insert "(mapcar (lambda (x) (* x x x)) data)\n")
      (insert "#+end_src\n\n")
      ;; Big
      (insert "#+NAME: big\n")
      (insert "#+begin_src emacs-lisp :var data=cubed :results value replace\n")
      (insert "(cl-remove-if-not (lambda (x) (> x 100)) data)\n")
      (insert "#+end_src\n\n")
      ;; Sorted desc
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=big :results value replace\n")
      (insert "(sort (copy-sequence data) #'>)\n")
      (insert "#+end_src\n\n")
      ;; Total
      (insert "#+NAME: total\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(apply #'+ data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("odds" "cubed" "big" "sorted" "total"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_eight_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5 6 7 8 9)) ((2 4 6 8 10 12 14 16 18)) ((10 12 14 16 18)) ((18 16 14 12 10)) 70) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(5 3 8 1 9 2 7 4 6)\\n#+end_src\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(sort (copy-sequence data) #'<)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |\\n\\n#+NAME: doubled\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(mapcar (lambda (x) (* x 2)) data)\\n#+end_src\\n\\n#+RESULTS: doubled\\n| 2 | 4 | 6 | 8 | 10 | 12 | 14 | 16 | 18 |\\n\\n#+NAME: big\\n#+begin_src emacs-lisp :var data=doubled :results value replace\\n(cl-remove-if-not (lambda (x) (> x 8)) data)\\n#+end_src\\n\\n#+RESULTS: big\\n| 10 | 12 | 14 | 16 | 18 |\\n\\n#+NAME: reversed\\n#+begin_src emacs-lisp :var data=big :results value replace\\n(reverse data)\\n#+end_src\\n\\n#+RESULTS: reversed\\n| 18 | 16 | 14 | 12 | 10 |\\n\\n#+NAME: total\\n#+begin_src emacs-lisp :var data=reversed :results value replace\\n(apply #'+ data)\\n#+end_src\\n\\n#+RESULTS: total\\n: 70\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(5 3 8 1 9 2 7 4 6)\n")
      (insert "#+end_src\n\n")
      ;; Sorted asc
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(sort (copy-sequence data) #'<)\n")
      (insert "#+end_src\n\n")
      ;; Doubled
      (insert "#+NAME: doubled\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(mapcar (lambda (x) (* x 2)) data)\n")
      (insert "#+end_src\n\n")
      ;; Filter > 8
      (insert "#+NAME: big\n")
      (insert "#+begin_src emacs-lisp :var data=doubled :results value replace\n")
      (insert "(cl-remove-if-not (lambda (x) (> x 8)) data)\n")
      (insert "#+end_src\n\n")
      ;; Reversed
      (insert "#+NAME: reversed\n")
      (insert "#+begin_src emacs-lisp :var data=big :results value replace\n")
      (insert "(reverse data)\n")
      (insert "#+end_src\n\n")
      ;; Total
      (insert "#+NAME: total\n")
      (insert "#+begin_src emacs-lisp :var data=reversed :results value replace\n")
      (insert "(apply #'+ data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("sorted" "doubled" "big" "reversed" "total"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_seven_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((3 1 4 5 9 2 6)) ((1 2 3 4 5 6 9)) ((2 4 6 8 10 12 18)) ((8 10 12 18)) 48) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(3 1 4 1 5 9 2 6 5 3)\\n#+end_src\\n\\n#+NAME: unique\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(seq-uniq data)\\n#+end_src\\n\\n#+RESULTS: unique\\n| 3 | 1 | 4 | 5 | 9 | 2 | 6 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=unique :results value replace\\n(sort (copy-sequence data) #'<)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 1 | 2 | 3 | 4 | 5 | 6 | 9 |\\n\\n#+NAME: doubled\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(mapcar (lambda (x) (* x 2)) data)\\n#+end_src\\n\\n#+RESULTS: doubled\\n| 2 | 4 | 6 | 8 | 10 | 12 | 18 |\\n\\n#+NAME: filtered\\n#+begin_src emacs-lisp :var data=doubled :results value replace\\n(cl-remove-if-not (lambda (x) (> x 6)) data)\\n#+end_src\\n\\n#+RESULTS: filtered\\n| 8 | 10 | 12 | 18 |\\n\\n#+NAME: total\\n#+begin_src emacs-lisp :var data=filtered :results value replace\\n(apply #'+ data)\\n#+end_src\\n\\n#+RESULTS: total\\n: 48\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(3 1 4 1 5 9 2 6 5 3)\n")
      (insert "#+end_src\n\n")
      ;; Unique
      (insert "#+NAME: unique\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(seq-uniq data)\n")
      (insert "#+end_src\n\n")
      ;; Sorted
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=unique :results value replace\n")
      (insert "(sort (copy-sequence data) #'<)\n")
      (insert "#+end_src\n\n")
      ;; Doubled
      (insert "#+NAME: doubled\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(mapcar (lambda (x) (* x 2)) data)\n")
      (insert "#+end_src\n\n")
      ;; Filtered
      (insert "#+NAME: filtered\n")
      (insert "#+begin_src emacs-lisp :var data=doubled :results value replace\n")
      (insert "(cl-remove-if-not (lambda (x) (> x 6)) data)\n")
      (insert "#+end_src\n\n")
      ;; Total
      (insert "#+NAME: total\n")
      (insert "#+begin_src emacs-lisp :var data=filtered :results value replace\n")
      (insert "(apply #'+ data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("unique" "sorted" "doubled" "filtered" "total"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_six_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((2 4 6 8 10)) ((6 8 10)) ((10 8 6)) 24 8) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(1 2 3 4 5)\\n#+end_src\\n\\n#+NAME: doubled\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(mapcar (lambda (x) (* x 2)) data)\\n#+end_src\\n\\n#+RESULTS: doubled\\n| 2 | 4 | 6 | 8 | 10 |\\n\\n#+NAME: filtered\\n#+begin_src emacs-lisp :var data=doubled :results value replace\\n(cl-remove-if-not (lambda (x) (> x 4)) data)\\n#+end_src\\n\\n#+RESULTS: filtered\\n| 6 | 8 | 10 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=filtered :results value replace\\n(sort (copy-sequence data) #'>)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 10 | 8 | 6 |\\n\\n#+NAME: total\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(apply #'+ data)\\n#+end_src\\n\\n#+RESULTS: total\\n: 24\\n\\n#+NAME: avg\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(/ (apply #'+ data) (length data))\\n#+end_src\\n\\n#+RESULTS: avg\\n: 8\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(1 2 3 4 5)\n")
      (insert "#+end_src\n\n")
      ;; Doubled
      (insert "#+NAME: doubled\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(mapcar (lambda (x) (* x 2)) data)\n")
      (insert "#+end_src\n\n")
      ;; Filtered
      (insert "#+NAME: filtered\n")
      (insert "#+begin_src emacs-lisp :var data=doubled :results value replace\n")
      (insert "(cl-remove-if-not (lambda (x) (> x 4)) data)\n")
      (insert "#+end_src\n\n")
      ;; Sorted
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=filtered :results value replace\n")
      (insert "(sort (copy-sequence data) #'>)\n")
      (insert "#+end_src\n\n")
      ;; Total
      (insert "#+NAME: total\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(apply #'+ data)\n")
      (insert "#+end_src\n\n")
      ;; Avg
      (insert "#+NAME: avg\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(/ (apply #'+ data) (length data))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("doubled" "filtered" "sorted" "total" "avg"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_five_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((2 4 6 8 10)) ((4 16 36 64 100)) ((100 64 36 16 4)) 220) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(1 2 3 4 5 6 7 8 9 10)\\n#+end_src\\n\\n#+NAME: evens\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(seq-filter #'evenp data)\\n#+end_src\\n\\n#+RESULTS: evens\\n| 2 | 4 | 6 | 8 | 10 |\\n\\n#+NAME: squared\\n#+begin_src emacs-lisp :var data=evens :results value replace\\n(mapcar (lambda (x) (* x x)) data)\\n#+end_src\\n\\n#+RESULTS: squared\\n| 4 | 16 | 36 | 64 | 100 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=squared :results value replace\\n(sort (copy-sequence data) #'>)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 100 | 64 | 36 | 16 | 4 |\\n\\n#+NAME: total\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(apply #'+ data)\\n#+end_src\\n\\n#+RESULTS: total\\n: 220\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(1 2 3 4 5 6 7 8 9 10)\n")
      (insert "#+end_src\n\n")
      ;; Even
      (insert "#+NAME: evens\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(seq-filter #'evenp data)\n")
      (insert "#+end_src\n\n")
      ;; Squared
      (insert "#+NAME: squared\n")
      (insert "#+begin_src emacs-lisp :var data=evens :results value replace\n")
      (insert "(mapcar (lambda (x) (* x x)) data)\n")
      (insert "#+end_src\n\n")
      ;; Sorted
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=squared :results value replace\n")
      (insert "(sort (copy-sequence data) #'>)\n")
      (insert "#+end_src\n\n")
      ;; Total
      (insert "#+NAME: total\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(apply #'+ data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("evens" "squared" "sorted" "total"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_four_block_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 4 9 16 25)) ((16 25)) 41) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(1 2 3 4 5)\\n#+end_src\\n\\n#+NAME: squared\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(mapcar (lambda (x) (* x x)) data)\\n#+end_src\\n\\n#+RESULTS: squared\\n| 1 | 4 | 9 | 16 | 25 |\\n\\n#+NAME: big\\n#+begin_src emacs-lisp :var data=squared :results value replace\\n(cl-remove-if-not (lambda (x) (> x 10)) data)\\n#+end_src\\n\\n#+RESULTS: big\\n| 16 | 25 |\\n\\n#+NAME: total\\n#+begin_src emacs-lisp :var data=big :results value replace\\n(apply #'+ data)\\n#+end_src\\n\\n#+RESULTS: total\\n: 41\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(1 2 3 4 5)\n")
      (insert "#+end_src\n\n")
      ;; Square
      (insert "#+NAME: squared\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(mapcar (lambda (x) (* x x)) data)\n")
      (insert "#+end_src\n\n")
      ;; Filter > 10
      (insert "#+NAME: big\n")
      (insert "#+begin_src emacs-lisp :var data=squared :results value replace\n")
      (insert "(cl-remove-if-not (lambda (x) (> x 10)) data)\n")
      (insert "#+end_src\n\n")
      ;; Sum
      (insert "#+NAME: total\n")
      (insert "#+begin_src emacs-lisp :var data=big :results value replace\n")
      (insert "(apply #'+ data)\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("squared" "big" "total"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_three_table_join_aggregate_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"((Eng . 15) (Mkt . 5))\") \"#+NAME: depts\\n| ID | Name |\\n|----+------|\\n| A | Eng |\\n| B | Mkt |\\n\\n#+NAME: workers\\n| ID | Name | Dept |\\n|----+------+------|\\n| 1 | W1 | A |\\n| 2 | W2 | B |\\n| 3 | W3 | A |\\n\\n#+NAME: hours\\n| ID | Hrs |\\n|----+-----|\\n| 1 | 8 |\\n| 2 | 5 |\\n| 3 | 7 |\\n\\n#+NAME: dept-hrs\\n#+begin_src emacs-lisp :var d=depts w=workers h=hours :results value replace\\n(let ((groups nil))\\n  (dolist (hour h)\\n    (let* ((wid (car hour))\\n           (hrs (cadr hour))\\n           (wk (assoc wid w))\\n           (did (caddr wk))\\n           (dept (assoc did d))\\n           (dname (cadr dept)))\\n      (let ((entry (assoc dname groups)))\\n        (if entry\\n            (setcdr entry (+ (cdr entry) hrs))\\n            (push (cons dname hrs) groups)))))\\n  (sort groups (lambda (a b) (string< (car a) (car b)))))\\n#+end_src\\n\\n#+RESULTS: dept-hrs\\n: ((Eng . 15) (Mkt . 5))\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Departments
      (insert "#+NAME: depts\n")
      (insert "| ID | Name |\n")
      (insert "|----+------|\n")
      (insert "| A | Eng |\n")
      (insert "| B | Mkt |\n\n")
      ;; Workers
      (insert "#+NAME: workers\n")
      (insert "| ID | Name | Dept |\n")
      (insert "|----+------+------|\n")
      (insert "| 1 | W1 | A |\n")
      (insert "| 2 | W2 | B |\n")
      (insert "| 3 | W3 | A |\n\n")
      ;; Hours
      (insert "#+NAME: hours\n")
      (insert "| ID | Hrs |\n")
      (insert "|----+-----|\n")
      (insert "| 1 | 8 |\n")
      (insert "| 2 | 5 |\n")
      (insert "| 3 | 7 |\n\n")
      ;; Three-table join
      (insert "#+NAME: dept-hrs\n")
      (insert "#+begin_src emacs-lisp :var d=depts w=workers h=hours :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (hour h)\n")
      (insert "    (let* ((wid (car hour))\n")
      (insert "           (hrs (cadr hour))\n")
      (insert "           (wk (assoc wid w))\n")
      (insert "           (did (caddr wk))\n")
      (insert "           (dept (assoc did d))\n")
      (insert "           (dname (cadr dept)))\n")
      (insert "      (let ((entry (assoc dname groups)))\n")
      (insert "        (if entry\n")
      (insert "            (setcdr entry (+ (cdr entry) hrs))\n")
      (insert "            (push (cons dname hrs) groups)))))\n")
      (insert "  (sort groups (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "dept-hrs")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_two_table_join_sort_aggregate_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|--------+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Sales
      (insert "#+NAME: sales\n")
      (insert "| Region | Amt |\n")
      (insert("|--------+-----|\n")
      (insert "| N | 100 |\n")
      (insert "| S | 200 |\n")
      (insert "| N | 150 |\n")
      (insert "| E | 300 |\n\n")
      ;; Group by region, sort desc
      (insert "#+NAME: grouped\n")
      (insert "#+begin_src emacs-lisp :var tbl=sales :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (row tbl)\n")
      (insert "    (let* ((region (car row))\n")
      (insert "           (amt (cadr row))\n")
      (insert "           (entry (assoc region groups)))\n")
      (insert "      (if entry\n")
      (insert "          (setcdr entry (+ (cdr entry) amt))\n")
      (insert "          (push (cons region amt) groups))))\n")
      (insert "  (sort groups (lambda (a b) (> (cdr a) (cdr b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "grouped")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_two_table_join_aggregate_chain_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"((A . 15) (B . 5))\") \"#+NAME: workers\\n| ID | Name | Dept |\\n|----+------+------|\\n| 1 | W1 | A |\\n| 2 | W2 | B |\\n| 3 | W3 | A |\\n\\n#+NAME: hours\\n| ID | Hrs |\\n|----+-----|\\n| 1 | 8 |\\n| 2 | 5 |\\n| 3 | 7 |\\n\\n#+NAME: dept-hrs\\n#+begin_src emacs-lisp :var w=workers h=hours :results value replace\\n(let ((groups nil))\\n  (dolist (hour h)\\n    (let* ((id (car hour))\\n           (hrs (cadr hour))\\n           (wk (assoc id w))\\n           (dept (caddr wk)))\\n      (let ((entry (assoc dept groups)))\\n        (if entry\\n            (setcdr entry (+ (cdr entry) hrs))\\n            (push (cons dept hrs) groups)))))\\n  (sort groups (lambda (a b) (string< (car a) (car b)))))\\n#+end_src\\n\\n#+RESULTS: dept-hrs\\n: ((A . 15) (B . 5))\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Workers
      (insert "#+NAME: workers\n")
      (insert "| ID | Name | Dept |\n")
      (insert "|----+------+------|\n")
      (insert "| 1 | W1 | A |\n")
      (insert "| 2 | W2 | B |\n")
      (insert "| 3 | W3 | A |\n\n")
      ;; Hours
      (insert "#+NAME: hours\n")
      (insert "| ID | Hrs |\n")
      (insert "|----+-----|\n")
      (insert "| 1 | 8 |\n")
      (insert "| 2 | 5 |\n")
      (insert "| 3 | 7 |\n\n")
      ;; Join and aggregate by dept
      (insert "#+NAME: dept-hrs\n")
      (insert "#+begin_src emacs-lisp :var w=workers h=hours :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (hour h)\n")
      (insert "    (let* ((id (car hour))\n")
      (insert "           (hrs (cadr hour))\n")
      (insert "           (wk (assoc id w))\n")
      (insert "           (dept (caddr wk)))\n")
      (insert "      (let ((entry (assoc dept groups)))\n")
      (insert "        (if entry\n")
      (insert "            (setcdr entry (+ (cdr entry) hrs))\n")
      (insert "            (push (cons dept hrs) groups)))))\n")
      (insert "  (sort groups (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "dept-hrs")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_two_table_join_sort_chain_deep_v3_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Ada\" 92) (\"Cal\" 85) (\"Bob\" 78))) \"#+NAME: students\\n| ID | Name |\\n|----+------|\\n| 1 | Ada |\\n| 2 | Bob |\\n| 3 | Cal |\\n\\n#+NAME: grades\\n| SID | Score |\\n|-----+-------|\\n| 1 | 92 |\\n| 2 | 78 |\\n| 3 | 85 |\\n\\n#+NAME: ranked\\n#+begin_src emacs-lisp :var s=students g=grades :results value replace\\n(sort\\n (mapcar\\n  (lambda (grade)\\n    (let* ((sid (car grade))\\n           (score (cadr grade))\\n           (stu (assoc sid s))\\n           (name (cadr stu)))\\n      (list name score)))\\n  g)\\n (lambda (a b) (> (cadr a) (cadr b))))\\n#+end_src\\n\\n#+RESULTS: ranked\\n| Ada | 92 |\\n| Cal | 85 |\\n| Bob | 78 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Students
      (insert "#+NAME: students\n")
      (insert "| ID | Name |\n")
      (insert "|----+------|\n")
      (insert "| 1 | Ada |\n")
      (insert "| 2 | Bob |\n")
      (insert "| 3 | Cal |\n\n")
      ;; Grades
      (insert "#+NAME: grades\n")
      (insert "| SID | Score |\n")
      (insert "|-----+-------|\n")
      (insert "| 1 | 92 |\n")
      (insert "| 2 | 78 |\n")
      (insert "| 3 | 85 |\n\n")
      ;; Join and rank
      (insert "#+NAME: ranked\n")
      (insert "#+begin_src emacs-lisp :var s=students g=grades :results value replace\n")
      (insert "(sort\n")
      (insert " (mapcar\n")
      (insert "  (lambda (grade)\n")
      (insert "    (let* ((sid (car grade))\n")
      (insert "           (score (cadr grade))\n")
      (insert "           (stu (assoc sid s))\n")
      (insert "           (name (cadr stu)))\n")
      (insert "      (list name score)))\n")
      (insert "  g)\n")
      (insert " (lambda (a b) (> (cadr a) (cadr b))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "ranked")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_table_join_sort_deep_v3_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Pb\" 5 8 40) (\"Pa\" 3 12 36) (\"Pc\" 2 15 30))) \"#+NAME: items\\n| ID | Name | Price |\\n|----+------+-------|\\n| 1 | Pa | 12 |\\n| 2 | Pb | 8 |\\n| 3 | Pc | 15 |\\n\\n#+NAME: ords\\n| PID | Qty |\\n|-----+-----|\\n| 1 | 3 |\\n| 2 | 5 |\\n| 3 | 2 |\\n\\n#+NAME: bills\\n#+begin_src emacs-lisp :var i=items o=ords :results value replace\\n(sort\\n (mapcar\\n  (lambda (ord)\\n    (let* ((pid (car ord))\\n           (qty (cadr ord))\\n           (itm (assoc pid i))\\n           (name (cadr itm))\\n           (price (caddr itm)))\\n      (list name qty price (* qty price))))\\n  o)\\n (lambda (a b) (> (cadddr a) (cadddr b))))\\n#+end_src\\n\\n#+RESULTS: bills\\n| Pb | 5 |  8 | 40 |\\n| Pa | 3 | 12 | 36 |\\n| Pc | 2 | 15 | 30 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Items
      (insert "#+NAME: items\n")
      (insert "| ID | Name | Price |\n")
      (insert "|----+------+-------|\n")
      (insert "| 1 | Pa | 12 |\n")
      (insert "| 2 | Pb | 8 |\n")
      (insert "| 3 | Pc | 15 |\n\n")
      ;; Orders
      (insert "#+NAME: ords\n")
      (insert "| PID | Qty |\n")
      (insert "|-----+-----|\n")
      (insert "| 1 | 3 |\n")
      (insert "| 2 | 5 |\n")
      (insert "| 3 | 2 |\n\n")
      ;; Join, compute, sort by total desc
      (insert "#+NAME: bills\n")
      (insert "#+begin_src emacs-lisp :var i=items o=ords :results value replace\n")
      (insert "(sort\n")
      (insert " (mapcar\n")
      (insert "  (lambda (ord)\n")
      (insert "    (let* ((pid (car ord))\n")
      (insert "           (qty (cadr ord))\n")
      (insert "           (itm (assoc pid i))\n")
      (insert "           (name (cadr itm))\n")
      (insert "           (price (caddr itm)))\n")
      (insert "      (list name qty price (* qty price))))\n")
      (insert "  o)\n")
      (insert " (lambda (a b) (> (cadddr a) (cadddr b))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "bills")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_two_table_join_sort_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"W1\" 8) (\"W3\" 7) (\"W2\" 5))) \"#+NAME: workers\\n| ID | Name | Dept |\\n|----+------+------|\\n| 1 | W1 | A |\\n| 2 | W2 | B |\\n| 3 | W3 | A |\\n\\n#+NAME: hours\\n| ID | Hrs |\\n|----+-----|\\n| 1 | 8 |\\n| 2 | 5 |\\n| 3 | 7 |\\n\\n#+NAME: ranked\\n#+begin_src emacs-lisp :var w=workers h=hours :results value replace\\n(sort\\n (mapcar\\n  (lambda (hour)\\n    (let* ((id (car hour))\\n           (hrs (cadr hour))\\n           (wk (assoc id w))\\n           (name (cadr wk)))\\n      (list name hrs)))\\n  h)\\n (lambda (a b) (> (cadr a) (cadr b))))\\n#+end_src\\n\\n#+RESULTS: ranked\\n| W1 | 8 |\\n| W3 | 7 |\\n| W2 | 5 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Workers
      (insert "#+NAME: workers\n")
      (insert "| ID | Name | Dept |\n")
      (insert "|----+------+------|\n")
      (insert "| 1 | W1 | A |\n")
      (insert "| 2 | W2 | B |\n")
      (insert "| 3 | W3 | A |\n\n")
      ;; Hours
      (insert "#+NAME: hours\n")
      (insert "| ID | Hrs |\n")
      (insert "|----+-----|\n")
      (insert "| 1 | 8 |\n")
      (insert "| 2 | 5 |\n")
      (insert "| 3 | 7 |\n\n")
      ;; Join and sort by hours desc
      (insert "#+NAME: ranked\n")
      (insert "#+begin_src emacs-lisp :var w=workers h=hours :results value replace\n")
      (insert "(sort\n")
      (insert " (mapcar\n")
      (insert "  (lambda (hour)\n")
      (insert "    (let* ((id (car hour))\n")
      (insert "           (hrs (cadr hour))\n")
      (insert "           (wk (assoc id w))\n")
      (insert "           (name (cadr wk)))\n")
      (insert "      (list name hrs)))\n")
      (insert "  h)\n")
      (insert " (lambda (a b) (> (cadr a) (cadr b))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "ranked")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_block_chain_aggregate_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((4 16 36 64 100)) ((\":count\" 5 \":total\" 220 \":avg\" 44))) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(1 2 3 4 5 6 7 8 9 10)\\n#+end_src\\n\\n#+NAME: squares\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(mapcar (lambda (x) (* x x))\\n        (seq-filter #'evenp data))\\n#+end_src\\n\\n#+RESULTS: squares\\n| 4 | 16 | 36 | 64 | 100 |\\n\\n#+NAME: agg\\n#+begin_src emacs-lisp :var data=squares :results value replace\\n(list :count (length data)\\n      :total (apply #'+ data)\\n      :avg (/ (apply #'+ data) (length data)))\\n#+end_src\\n\\n#+RESULTS: agg\\n| :count | 5 | :total | 220 | :avg | 44 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(1 2 3 4 5 6 7 8 9 10)\n")
      (insert "#+end_src\n\n")
      ;; Filter even, square
      (insert "#+NAME: squares\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(mapcar (lambda (x) (* x x))\n")
      (insert "        (seq-filter #'evenp data))\n")
      (insert "#+end_src\n\n")
      ;; Aggregate
      (insert "#+NAME: agg\n")
      (insert "#+begin_src emacs-lisp :var data=squares :results value replace\n")
      (insert "(list :count (length data)\n")
      (insert "      :total (apply #'+ data)\n")
      (insert "      :avg (/ (apply #'+ data) (length data)))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("squares" "agg"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_block_chain_table_var_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---+---|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed table
      (insert "#+NAME: seed\n")
      (insert "| A | B |\n")
      (insert("|---+---|\n")
      (insert "| 1 | 2 |\n")
      (insert "| 3 | 4 |\n\n")
      ;; Double
      (insert "#+NAME: doubled\n")
      (insert "#+begin_src emacs-lisp :var tbl=seed :results value replace\n")
      (insert "(mapcar (lambda (r) (list (car r) (* 2 (cadr r)))) tbl)\n")
      (insert "#+end_src\n\n")
      ;; Sum
      (insert "#+NAME: summed\n")
      (insert "#+begin_src emacs-lisp :var tbl=doubled :results value replace\n")
      (insert "(list :rows tbl :total (apply #'+ (mapcar #'cadr tbl)))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("doubled" "summed"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_join_filter_sort_chain_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Z\" 4 15 60) (\"X\" 3 10 30))) \"#+NAME: items\\n| ID | Name | Price |\\n|----+------+-------|\\n| 1 | X | 10 |\\n| 2 | Y | 25 |\\n| 3 | Z | 15 |\\n\\n#+NAME: orders\\n| IID | Qty |\\n|-----+-----|\\n| 1 | 3 |\\n| 2 | 1 |\\n| 3 | 4 |\\n\\n#+NAME: big\\n#+begin_src emacs-lisp :var i=items o=orders :results value replace\\n(let* ((big-orders (cl-remove-if-not\\n                   (lambda (r) (>= (cadr r) 3))\\n                   o))\\n       (bills (mapcar\\n               (lambda (order)\\n                 (let* ((iid (car order))\\n                        (qty (cadr order))\\n                        (item (assoc iid i))\\n                        (name (cadr item))\\n                        (price (caddr item)))\\n                   (list name qty price (* qty price))))\\n               big-orders)))\\n  (sort bills (lambda (a b) (> (cadddr a) (cadddr b)))))\\n#+end_src\\n\\n#+RESULTS: big\\n| Z | 4 | 15 | 60 |\\n| X | 3 | 10 | 30 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Items
      (insert "#+NAME: items\n")
      (insert "| ID | Name | Price |\n")
      (insert "|----+------+-------|\n")
      (insert "| 1 | X | 10 |\n")
      (insert "| 2 | Y | 25 |\n")
      (insert "| 3 | Z | 15 |\n\n")
      ;; Orders
      (insert "#+NAME: orders\n")
      (insert "| IID | Qty |\n")
      (insert "|-----+-----|\n")
      (insert "| 1 | 3 |\n")
      (insert "| 2 | 1 |\n")
      (insert "| 3 | 4 |\n\n")
      ;; Filter qty >= 3, join, sort by total desc
      (insert "#+NAME: big\n")
      (insert "#+begin_src emacs-lisp :var i=items o=orders :results value replace\n")
      (insert "(let* ((big-orders (cl-remove-if-not\n")
      (insert "                   (lambda (r) (>= (cadr r) 3))\n")
      (insert "                   o))\n")
      (insert "       (bills (mapcar\n")
      (insert "               (lambda (order)\n")
      (insert "                 (let* ((iid (car order))\n")
      (insert "                        (qty (cadr order))\n")
      (insert "                        (item (assoc iid i))\n")
      (insert "                        (name (cadr item))\n")
      (insert "                        (price (caddr item)))\n")
      (insert "                   (list name qty price (* qty price))))\n")
      (insert "               big-orders)))\n")
      (insert "  (sort bills (lambda (a b) (> (cadddr a) (cadddr b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "big")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_join_compute_sort_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Pa\" 5 10 50) (\"Pc\" 3 15 45) (\"Pb\" 1 20 20))) \"#+NAME: parts\\n| ID | Name | Price |\\n|----+------+-------|\\n| 1 | Pa | 10 |\\n| 2 | Pb | 20 |\\n| 3 | Pc | 15 |\\n\\n#+NAME: orders\\n| PID | Qty |\\n|-----+-----|\\n| 1 | 5 |\\n| 2 | 1 |\\n| 3 | 3 |\\n\\n#+NAME: bills\\n#+begin_src emacs-lisp :var p=parts o=orders :results value replace\\n(sort\\n (mapcar\\n  (lambda (order)\\n    (let* ((pid (car order))\\n           (qty (cadr order))\\n           (part (assoc pid p))\\n           (name (cadr part))\\n           (price (caddr part)))\\n      (list name qty price (* qty price))))\\n  o)\\n (lambda (a b) (> (cadddr a) (cadddr b))))\\n#+end_src\\n\\n#+RESULTS: bills\\n| Pa | 5 | 10 | 50 |\\n| Pc | 3 | 15 | 45 |\\n| Pb | 1 | 20 | 20 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Parts
      (insert "#+NAME: parts\n")
      (insert "| ID | Name | Price |\n")
      (insert "|----+------+-------|\n")
      (insert "| 1 | Pa | 10 |\n")
      (insert "| 2 | Pb | 20 |\n")
      (insert "| 3 | Pc | 15 |\n\n")
      ;; Orders
      (insert "#+NAME: orders\n")
      (insert "| PID | Qty |\n")
      (insert "|-----+-----|\n")
      (insert "| 1 | 5 |\n")
      (insert "| 2 | 1 |\n")
      (insert "| 3 | 3 |\n\n")
      ;; Join, compute, sort by total desc
      (insert "#+NAME: bills\n")
      (insert "#+begin_src emacs-lisp :var p=parts o=orders :results value replace\n")
      (insert "(sort\n")
      (insert " (mapcar\n")
      (insert "  (lambda (order)\n")
      (insert "    (let* ((pid (car order))\n")
      (insert "           (qty (cadr order))\n")
      (insert "           (part (assoc pid p))\n")
      (insert "           (name (cadr part))\n")
      (insert "           (price (caddr part)))\n")
      (insert "      (list name qty price (* qty price))))\n")
      (insert "  o)\n")
      (insert " (lambda (a b) (> (cadddr a) (cadddr b))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "bills")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_list_table_join_aggregate_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":rows\" \"((Pa 3 5 15) (Pb 2 10 20) (Pc 4 15 60))\" \":total\" 95))) \"#+NAME: prices\\n| ID | Name | Price |\\n|----+------+-------|\\n| 1 | Pa | 5 |\\n| 2 | Pb | 10 |\\n| 3 | Pc | 15 |\\n\\n#+NAME: orders\\n| ID | Qty |\\n|----+-----|\\n| 1 | 3 |\\n| 2 | 2 |\\n| 3 | 4 |\\n\\n#+NAME: invoices\\n#+begin_src emacs-lisp :var p=prices o=orders :results value replace\\n(let ((rows (mapcar\\n              (lambda (order)\\n                (let* ((id (car order))\\n                       (qty (cadr order))\\n                       (prod (assoc id p))\\n                       (name (cadr prod))\\n                       (price (caddr prod)))\\n                  (list name qty price (* qty price))))\\n              o)))\\n  (list :rows rows\\n        :total (apply #'+ (mapcar #'cadddr rows))))\\n#+end_src\\n\\n#+RESULTS: invoices\\n| :rows | ((Pa 3 5 15) (Pb 2 10 20) (Pc 4 15 60)) | :total | 95 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Prices
      (insert "#+NAME: prices\n")
      (insert "| ID | Name | Price |\n")
      (insert "|----+------+-------|\n")
      (insert "| 1 | Pa | 5 |\n")
      (insert "| 2 | Pb | 10 |\n")
      (insert "| 3 | Pc | 15 |\n\n")
      ;; Orders
      (insert "#+NAME: orders\n")
      (insert "| ID | Qty |\n")
      (insert "|----+-----|\n")
      (insert "| 1 | 3 |\n")
      (insert "| 2 | 2 |\n")
      (insert "| 3 | 4 |\n\n")
      ;; Join and compute
      (insert "#+NAME: invoices\n")
      (insert "#+begin_src emacs-lisp :var p=prices o=orders :results value replace\n")
      (insert "(let ((rows (mapcar\n")
      (insert "              (lambda (order)\n")
      (insert "                (let* ((id (car order))\n")
      (insert "                       (qty (cadr order))\n")
      (insert "                       (prod (assoc id p))\n")
      (insert "                       (name (cadr prod))\n")
      (insert "                       (price (caddr prod)))\n")
      (insert "                  (list name qty price (* qty price))))\n")
      (insert "              o)))\n")
      (insert "  (list :rows rows\n")
      (insert "        :total (apply #'+ (mapcar #'cadddr rows))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "invoices")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_column_sort_filter_chain_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---+---|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Input table
      (insert "#+NAME: src\n")
      (insert "| N | V |\n")
      (insert("|---+---|\n")
      (insert "| 2 | 7 |\n")
      (insert "| 5 | 3 |\n")
      (insert "| 8 | 1 |\n")
      (insert "| 4 | 9 |\n\n")
      ;; Filter even N, sort V desc, compute
      (insert "#+NAME: result\n")
      (insert "#+begin_src emacs-lisp :var tbl=src :results value replace\n")
      (insert "(let* ((evens (cl-remove-if-not\n")
      (insert "               (lambda (r) (evenp (car r)))\n")
      (insert "               tbl))\n")
      (insert "       (sorted (sort (copy-sequence evens)\n")
      (insert "                     (lambda (a b) (> (cadr a) (cadr b)))))\n")
      (insert "       (total (apply #'+ (mapcar #'cadr sorted))))\n")
      (insert "  (list :rows sorted :total total))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "result")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_chain_table_aggregate_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---+---|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Table
      (insert "#+NAME: data\n")
      (insert "| A | B |\n")
      (insert("|---+---|\n")
      (insert "| 1 | 2 |\n")
      (insert "| 3 | 4 |\n")
      (insert "| 5 | 6 |\n\n")
      ;; Compute products and totals
      (insert "#+NAME: computed\n")
      (insert "#+begin_src emacs-lisp :var tbl=data :results value replace\n")
      (insert "(let ((prods (mapcar (lambda (r) (* (car r) (cadr r))) tbl)))\n")
      (insert "  (list :prods prods\n")
      (insert "        :total (apply #'+ prods)\n")
      (insert "        :avg (/ (apply #'+ prods) (length prods))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "computed")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_var_column_sort_chain_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---+---|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: src\n")
      (insert "| N | V |\n")
      (insert("|---+---|\n")
      (insert "| 4 | 8 |\n")
      (insert "| 1 | 5 |\n")
      (insert "| 3 | 9 |\n")
      (insert "| 2 | 6 |\n\n")
      ;; Sort by V desc, compute
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var tbl=src :results value replace\n")
      (insert "(let ((s (sort (copy-sequence tbl)\n")
      (insert "               (lambda (a b) (> (cadr a) (cadr b))))))\n")
      (insert "  (list :sorted s\n")
      (insert "        :v-total (apply #'+ (mapcar #'cadr s))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "sorted")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_filter_sort_chain_deep_v3_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---+---|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: src\n")
      (insert "| X | Y |\n")
      (insert("|---+---|\n")
      (insert "| 3 | 8 |\n")
      (insert "| 1 | 5 |\n")
      (insert "| 4 | 2 |\n")
      (insert "| 2 | 7 |\n\n")
      ;; Filter > 2, sort by Y desc, compute
      (insert "#+NAME: computed\n")
      (insert "#+begin_src emacs-lisp :var tbl=src :results value replace\n")
      (insert "(let* ((big (cl-remove-if-not\n")
      (insert "               (lambda (r) (> (cadr r) 3))\n")
      (insert "               tbl))\n")
      (insert "       (sorted (sort (copy-sequence big)\n")
      (insert "                     (lambda (a b) (> (cadr a) (cadr b)))))\n")
      (insert "       (total (apply #'+ (mapcar #'cadr sorted))))\n")
      (insert "  (list :rows sorted :total total))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "computed")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_var_column_aggregate_deep_v3_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|-----+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: data\n")
      (insert "| Cat | Val |\n")
      (insert("|-----+-----|\n")
      (insert "| A | 10 |\n")
      (insert "| B | 20 |\n")
      (insert "| A | 30 |\n")
      (insert "| C | 40 |\n")
      (insert "| B | 50 |\n\n")
      ;; Group and sum
      (insert "#+NAME: groups\n")
      (insert "#+begin_src emacs-lisp :var tbl=data :results value replace\n")
      (insert "(let ((g nil))\n")
      (insert "  (dolist (r tbl)\n")
      (insert "    (let* ((c (car r))\n")
      (insert "           (v (cadr r))\n")
      (insert "           (e (assoc c g)))\n")
      (insert "      (if e (setcdr e (+ (cdr e) v))\n")
      (insert "          (push (cons c v) g))))\n")
      (insert "  (sort g (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "groups")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_filter_sort_aggregate_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---+---|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Input table
      (insert "#+NAME: input\n")
      (insert "| N | V |\n")
      (insert("|---+---|\n")
      (insert "| 1 | 5 |\n")
      (insert "| 2 | 3 |\n")
      (insert "| 3 | 8 |\n")
      (insert "| 4 | 1 |\n")
      (insert "| 5 | 6 |\n\n")
      ;; Filter > 2, sort desc, sum
      (insert "#+NAME: result\n")
      (insert "#+begin_src emacs-lisp :var tbl=input :results value replace\n")
      (insert "(let* ((big (cl-remove-if-not (lambda (r) (> (cadr r) 2)) tbl))\n")
      (insert "       (sorted (sort (copy-sequence big)\n")
      (insert "                     (lambda (a b) (> (cadr a) (cadr b)))))\n")
      (insert "       (total (apply #'+ (mapcar #'cadr sorted))))\n")
      (insert "  (list :filtered sorted :total total))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "result")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_column_filter_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---+---|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: src\n")
      (insert "| X | Y |\n")
      (insert("|---+---|\n")
      (insert "| 1 | 5 |\n")
      (insert "| 3 | 2 |\n")
      (insert "| 6 | 4 |\n")
      (insert "| 2 | 8 |\n\n")
      ;; Filter even X, compute
      (insert "#+NAME: evens\n")
      (insert "#+begin_src emacs-lisp :var tbl=src :results value replace\n")
      (insert "(let ((rows (cl-remove-if-not\n")
      (insert "              (lambda (r) (evenp (car r)))\n")
      (insert "              tbl)))\n")
      (insert "  (mapcar (lambda (r)\n")
      (insert "            (list (car r) (cadr r) (* (car r) (cadr r))))\n")
      (insert "          rows))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "evens")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_table_chain_simple_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---+---|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Table
      (insert "#+NAME: data\n")
      (insert "| A | B |\n")
      (insert("|---+---|\n")
      (insert "| 2 | 3 |\n")
      (insert "| 4 | 5 |\n")
      (insert "| 6 | 7 |\n\n")
      ;; Compute products and sums
      (insert "#+NAME: computed\n")
      (insert "#+begin_src emacs-lisp :var tbl=data :results value replace\n")
      (insert "(let ((rows (mapcar (lambda (r)\n")
      (insert "                     (list (car r) (cadr r)\n")
      (insert "                           (* (car r) (cadr r))\n")
      (insert "                           (+ (car r) (cadr r))))\n")
      (insert "                   tbl)))\n")
      (insert "  (list :rows rows\n")
      (insert "        :prod-total (apply #'+ (mapcar #'caddr rows))\n")
      (insert "        :sum-total (apply #'+ (mapcar #'cadddr rows))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "computed")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_join_sort_chain_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Z\" 5 15 75) (\"Y\" 2 25 50) (\"X\" 4 10 40))) \"#+NAME: items\\n| ID | Name | Price |\\n|----+------+-------|\\n| 1 | X | 10 |\\n| 2 | Y | 25 |\\n| 3 | Z | 15 |\\n\\n#+NAME: orders\\n| IID | Qty |\\n|-----+-----|\\n| 1 | 4 |\\n| 2 | 2 |\\n| 3 | 5 |\\n\\n#+NAME: bills\\n#+begin_src emacs-lisp :var i=items o=orders :results value replace\\n(sort\\n (mapcar\\n  (lambda (order)\\n    (let* ((iid (car order))\\n           (qty (cadr order))\\n           (item (assoc iid i))\\n           (name (cadr item))\\n           (price (caddr item)))\\n      (list name qty price (* qty price))))\\n  o)\\n (lambda (a b) (> (cadddr a) (cadddr b))))\\n#+end_src\\n\\n#+RESULTS: bills\\n| Z | 5 | 15 | 75 |\\n| Y | 2 | 25 | 50 |\\n| X | 4 | 10 | 40 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Items
      (insert "#+NAME: items\n")
      (insert "| ID | Name | Price |\n")
      (insert "|----+------+-------|\n")
      (insert "| 1 | X | 10 |\n")
      (insert "| 2 | Y | 25 |\n")
      (insert "| 3 | Z | 15 |\n\n")
      ;; Orders
      (insert "#+NAME: orders\n")
      (insert "| IID | Qty |\n")
      (insert "|-----+-----|\n")
      (insert "| 1 | 4 |\n")
      (insert "| 2 | 2 |\n")
      (insert "| 3 | 5 |\n\n")
      ;; Compute and sort by total desc
      (insert "#+NAME: bills\n")
      (insert "#+begin_src emacs-lisp :var i=items o=orders :results value replace\n")
      (insert "(sort\n")
      (insert " (mapcar\n")
      (insert "  (lambda (order)\n")
      (insert "    (let* ((iid (car order))\n")
      (insert "           (qty (cadr order))\n")
      (insert "           (item (assoc iid i))\n")
      (insert "           (name (cadr item))\n")
      (insert "           (price (caddr item)))\n")
      (insert "      (list name qty price (* qty price))))\n")
      (insert "  o)\n")
      (insert " (lambda (a b) (> (cadddr a) (cadddr b))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "bills")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_table_join_sort_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Pc\" 6 5 30) (\"Pa\" 3 8 24) (\"Pb\" 1 12 12))) \"#+NAME: prods\\n| ID | Name | Cost |\\n|----+------+------|\\n| 1 | Pa | 8 |\\n| 2 | Pb | 12 |\\n| 3 | Pc | 5 |\\n\\n#+NAME: ords\\n| PID | Qty |\\n|-----+-----|\\n| 1 | 3 |\\n| 2 | 1 |\\n| 3 | 6 |\\n\\n#+NAME: invoices\\n#+begin_src emacs-lisp :var p=prods o=ords :results value replace\\n(sort\\n (mapcar\\n  (lambda (order)\\n    (let* ((pid (car order))\\n           (qty (cadr order))\\n           (prod (assoc pid p))\\n           (name (cadr prod))\\n           (cost (caddr prod)))\\n      (list name qty cost (* qty cost))))\\n  o)\\n (lambda (a b) (> (cadddr a) (cadddr b))))\\n#+end_src\\n\\n#+RESULTS: invoices\\n| Pc | 6 |  5 | 30 |\\n| Pa | 3 |  8 | 24 |\\n| Pb | 1 | 12 | 12 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Products
      (insert "#+NAME: prods\n")
      (insert "| ID | Name | Cost |\n")
      (insert "|----+------+------|\n")
      (insert "| 1 | Pa | 8 |\n")
      (insert "| 2 | Pb | 12 |\n")
      (insert "| 3 | Pc | 5 |\n\n")
      ;; Orders
      (insert "#+NAME: ords\n")
      (insert "| PID | Qty |\n")
      (insert "|-----+-----|\n")
      (insert "| 1 | 3 |\n")
      (insert "| 2 | 1 |\n")
      (insert "| 3 | 6 |\n\n")
      ;; Join, compute, sort by cost desc
      (insert "#+NAME: invoices\n")
      (insert "#+begin_src emacs-lisp :var p=prods o=ords :results value replace\n")
      (insert "(sort\n")
      (insert " (mapcar\n")
      (insert "  (lambda (order)\n")
      (insert "    (let* ((pid (car order))\n")
      (insert "           (qty (cadr order))\n")
      (insert "           (prod (assoc pid p))\n")
      (insert "           (name (cadr prod))\n")
      (insert "           (cost (caddr prod)))\n")
      (insert "      (list name qty cost (* qty cost))))\n")
      (insert "  o)\n")
      (insert " (lambda (a b) (> (cadddr a) (cadddr b))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "invoices")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_filter_sort_chain_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|------+-------|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: items\n")
      (insert "| Name | Price |\n")
      (insert("|------+-------|\n")
      (insert "| Z | 50 |\n")
      (insert "| A | 10 |\n")
      (insert "| M | 30 |\n")
      (insert "| B | 20 |\n\n")
      ;; Filter, sort, compute
      (insert "#+NAME: result\n")
      (insert "#+begin_src emacs-lisp :var tbl=items :results value replace\n")
      (insert "(let* ((big (cl-remove-if-not\n")
      (insert "               (lambda (r) (>= (cadr r) 20))\n")
      (insert "               tbl))\n")
      (insert "       (sorted (sort (copy-sequence big)\n")
      (insert "                     (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "       (total (apply #'+ (mapcar #'cadr sorted))))\n")
      (insert "  (list :items sorted :total total))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "result")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_join_sort_aggregate_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"S1\" 90) (\"S3\" 85) (\"S2\" 75))) \"#+NAME: students\\n| ID | Name |\\n|----+------|\\n| 1 | S1 |\\n| 2 | S2 |\\n| 3 | S3 |\\n\\n#+NAME: grades\\n| SID | Score |\\n|-----+-------|\\n| 1 | 90 |\\n| 2 | 75 |\\n| 3 | 85 |\\n\\n#+NAME: ranked\\n#+begin_src emacs-lisp :var s=students g=grades :results value replace\\n(let ((rows (mapcar\\n              (lambda (grade)\\n                (let* ((sid (car grade))\\n                       (score (cadr grade))\\n                       (student (assoc sid s))\\n                       (name (cadr student)))\\n                  (list name score)))\\n              g)))\\n  (sort rows (lambda (a b) (> (cadr a) (cadr b)))))\\n#+end_src\\n\\n#+RESULTS: ranked\\n| S1 | 90 |\\n| S3 | 85 |\\n| S2 | 75 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Students
      (insert "#+NAME: students\n")
      (insert "| ID | Name |\n")
      (insert "|----+------|\n")
      (insert "| 1 | S1 |\n")
      (insert "| 2 | S2 |\n")
      (insert "| 3 | S3 |\n\n")
      ;; Grades
      (insert "#+NAME: grades\n")
      (insert "| SID | Score |\n")
      (insert "|-----+-------|\n")
      (insert "| 1 | 90 |\n")
      (insert "| 2 | 75 |\n")
      (insert "| 3 | 85 |\n\n")
      ;; Join and rank
      (insert "#+NAME: ranked\n")
      (insert "#+begin_src emacs-lisp :var s=students g=grades :results value replace\n")
      (insert "(let ((rows (mapcar\n")
      (insert "              (lambda (grade)\n")
      (insert "                (let* ((sid (car grade))\n")
      (insert "                       (score (cadr grade))\n")
      (insert "                       (student (assoc sid s))\n")
      (insert "                       (name (cadr student)))\n")
      (insert "                  (list name score)))\n")
      (insert "              g)))\n")
      (insert "  (sort rows (lambda (a b) (> (cadr a) (cadr b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "ranked")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_table_column_chain_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Pc\" 8 15 120) (\"Pa\" 5 10 50) (\"Pb\" 2 20 40))) \"#+NAME: prods\\n| ID | Name | Price |\\n|----+------+-------|\\n| 1 | Pa | 10 |\\n| 2 | Pb | 20 |\\n| 3 | Pc | 15 |\\n\\n#+NAME: orders\\n| PID | Qty |\\n|-----+-----|\\n| 1 | 5 |\\n| 2 | 2 |\\n| 3 | 8 |\\n\\n#+NAME: invoices\\n#+begin_src emacs-lisp :var p=prods o=orders :results value replace\\n(let ((bills\\n       (mapcar\\n        (lambda (order)\\n          (let* ((pid (car order))\\n                 (qty (cadr order))\\n                 (prod (assoc pid p))\\n                 (name (cadr prod))\\n                 (price (caddr prod)))\\n            (list name qty price (* qty price))))\\n        o)))\\n  (sort bills (lambda (a b) (> (cadddr a) (cadddr b)))))\\n#+end_src\\n\\n#+RESULTS: invoices\\n| Pc | 8 | 15 | 120 |\\n| Pa | 5 | 10 |  50 |\\n| Pb | 2 | 20 |  40 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Products
      (insert "#+NAME: prods\n")
      (insert "| ID | Name | Price |\n")
      (insert "|----+------+-------|\n")
      (insert "| 1 | Pa | 10 |\n")
      (insert "| 2 | Pb | 20 |\n")
      (insert "| 3 | Pc | 15 |\n\n")
      ;; Orders
      (insert "#+NAME: orders\n")
      (insert "| PID | Qty |\n")
      (insert "|-----+-----|\n")
      (insert "| 1 | 5 |\n")
      (insert "| 2 | 2 |\n")
      (insert "| 3 | 8 |\n\n")
      ;; Compute invoices sorted by total desc
      (insert "#+NAME: invoices\n")
      (insert "#+begin_src emacs-lisp :var p=prods o=orders :results value replace\n")
      (insert "(let ((bills\n")
      (insert "       (mapcar\n")
      (insert "        (lambda (order)\n")
      (insert "          (let* ((pid (car order))\n")
      (insert "                 (qty (cadr order))\n")
      (insert "                 (prod (assoc pid p))\n")
      (insert "                 (name (cadr prod))\n")
      (insert "                 (price (caddr prod)))\n")
      (insert "            (list name qty price (* qty price))))\n")
      (insert "        o)))\n")
      (insert "  (sort bills (lambda (a b) (> (cadddr a) (cadddr b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "invoices")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_column_pivot_compute_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|------+---+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Revenue by dept and quarter
      (insert "#+NAME: revenue\n")
      (insert "| Dept | Q | Rev |\n")
      (insert("|------+---+-----|\n")
      (insert "| A | 1 | 100 |\n")
      (insert "| B | 1 | 200 |\n")
      (insert "| A | 2 | 150 |\n")
      (insert "| B | 2 | 250 |\n")
      (insert "| A | 3 | 300 |\n\n")
      ;; Total by dept
      (insert "#+NAME: by-dept\n")
      (insert "#+begin_src emacs-lisp :var tbl=revenue :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (row tbl)\n")
      (insert "    (let* ((dept (car row))\n")
      (insert "           (rev (caddr row))\n")
      (insert "           (entry (assoc dept groups)))\n")
      (insert "      (if entry\n")
      (insert "          (setcdr entry (+ (cdr entry) rev))\n")
      (insert "          (push (cons dept rev) groups))))\n")
      (insert "  (sort groups (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Total by quarter
      (insert "#+NAME: by-quarter\n")
      (insert "#+begin_src emacs-lisp :var tbl=revenue :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (row tbl)\n")
      (insert "    (let* ((q (cadr row))\n")
      (insert "           (rev (caddr row))\n")
      (insert "           (entry (assoc q groups)))\n")
      (insert "      (if entry\n")
      (insert "          (setcdr entry (+ (cdr entry) rev))\n")
      (insert "          (push (cons q rev) groups))))\n")
      (insert "  (sort groups (lambda (a b) (< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("by-dept" "by-quarter"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_row_col_pivot_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (invalid-function \"|--------+---------+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Sales by region and product
      (insert "#+NAME: sales\n")
      (insert "| Region | Product | Amt |\n")
      (insert("|--------+---------+-----|\n")
      (insert "| N | X | 100 |\n")
      (insert "| S | Y | 200 |\n")
      (insert "| N | Y | 150 |\n")
      (insert "| E | X | 300 |\n")
      (insert "| S | X | 250 |\n\n")
      ;; Pivot by region
      (insert "#+NAME: by-region\n")
      (insert "#+begin_src emacs-lisp :var tbl=sales :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (row tbl)\n")
      (insert "    (let* ((region (car row))\n")
      (insert "           (amt (caddr row))\n")
      (insert "           (entry (assoc region groups)))\n")
      (insert "      (if entry\n")
      (insert "          (setcdr entry (+ (cdr entry) amt))\n")
      (insert "          (push (cons region amt) groups))))\n")
      (insert "  (sort groups (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Pivot by product
      (insert "#+NAME: by-product\n")
      (insert "#+begin_src emacs-lisp :var tbl=sales :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (row tbl)\n")
      (insert "    (let* ((product (cadr row))\n")
      (insert "           (amt (caddr row))\n")
      (insert "           (entry (assoc product groups)))\n")
      (insert "      (if entry\n")
      (insert "          (setcdr entry (+ (cdr entry) amt))\n")
      (insert "          (push (cons product amt) groups))))\n")
      (insert "  (sort groups (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("by-region" "by-product"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_column_chain_aggregate_deep_v3_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (invalid-function \"|------+-------+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Orders table
      (insert "#+NAME: orders\n")
      (insert "| Item | Price | Qty |\n")
      (insert("|------+-------+-----|\n")
      (insert "| A | 5 | 10 |\n")
      (insert "| B | 8 | 3 |\n")
      (insert "| C | 4 | 7 |\n\n")
      ;; Compute subtotals
      (insert "#+NAME: subtotals\n")
      (insert "#+begin_src emacs-lisp :var tbl=orders :results value replace\n")
      (insert "(let ((rows (mapcar\n")
      (insert "              (lambda (r)\n")
      (insert "                (list (car r) (cadr r) (caddr r)\n")
      (insert "                      (* (cadr r) (caddr r))))\n")
      (insert "              tbl)))\n")
      (insert "  (list :rows rows\n")
      (insert "        :total (apply #'+ (mapcar #'cadddr rows))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "subtotals")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_seq_group_by_partition_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"nil\" 1 3 5 7) (\"t\" 2 4 6 8))) \"#+begin_src emacs-lisp :results value replace\\n(seq-group-by #'evenp '(1 2 3 4 5 6 7 8))\\n#+end_src\\n\\n#+RESULTS:\\n| nil | 1 | 3 | 5 | 7 |\\n| t   | 2 | 4 | 6 | 8 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(seq-partition '(a b c d e f g h i) 3)\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; seq-group-by
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-group-by #'evenp '(1 2 3 4 5 6 7 8))\n")
      (insert "#+end_src\n\n")
      ;; seq-partition
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-partition '(a b c d e f g h i) 3)\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_list_filter_map_reduce_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5 6 7 8 9 10))) \"#+NAME: data\\n#+begin_src emacs-lisp :results value replace\\n'(1 2 3 4 5 6 7 8 9 10)\\n#+end_src\\n\\n#+RESULTS: data\\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |\\n\\n#+NAME: result\\n#+begin_src emacs-lisp :var data=data :results value replace\\n(let* ((evens (seq-filter #'evenp data))\\n       (squares (mapcar (lambda (x) (* x x)) evens))\\n       (total (apply #'+ squares)))\\n  (list :evens evens :squares squares :total total))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Input list
      (insert "#+NAME: data\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(1 2 3 4 5 6 7 8 9 10)\n")
      (insert "#+end_src\n\n")
      ;; Filter even, square, sum
      (insert "#+NAME: result\n")
      (insert "#+begin_src emacs-lisp :var data=data :results value replace\n")
      (insert "(let* ((evens (seq-filter #'evenp data))\n")
      (insert "       (squares (mapcar (lambda (x) (* x x)) evens))\n")
      (insert "       (total (apply #'+ squares)))\n")
      (insert "  (list :evens evens :squares squares :total total))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("data" "result"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_join_sort_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"C\" \"X\" 6 50 300) (\"A\" \"X\" 4 50 200) (\"B\" \"Y\" 2 80 160) (\"D\" \"Z\" 3 30 90))) \"#+NAME: rates\\n| Svc | Rate |\\n|-----+------|\\n| X | 50 |\\n| Y | 80 |\\n| Z | 30 |\\n\\n#+NAME: usage\\n| Client | Svc | Hrs |\\n|--------+-----+-----|\\n| A | X | 4 |\\n| B | Y | 2 |\\n| C | X | 6 |\\n| D | Z | 3 |\\n\\n#+NAME: bills\\n#+begin_src emacs-lisp :var r=rates u=usage :results value replace\\n(let ((bills (mapcar\\n              (lambda (row)\\n                (let* ((client (car row))\\n                       (svc (cadr row))\\n                       (hrs (caddr row))\\n                       (rate (cadr (assoc svc r)))\\n                       (cost (* hrs rate)))\\n                  (list client svc hrs rate cost)))\\n              u)))\\n  (sort bills (lambda (a b) (> (nth 4 a) (nth 4 b)))))\\n#+end_src\\n\\n#+RESULTS: bills\\n| C | X | 6 | 50 | 300 |\\n| A | X | 4 | 50 | 200 |\\n| B | Y | 2 | 80 | 160 |\\n| D | Z | 3 | 30 |  90 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Rates
      (insert "#+NAME: rates\n")
      (insert "| Svc | Rate |\n")
      (insert "|-----+------|\n")
      (insert "| X | 50 |\n")
      (insert "| Y | 80 |\n")
      (insert "| Z | 30 |\n\n")
      ;; Usage
      (insert "#+NAME: usage\n")
      (insert "| Client | Svc | Hrs |\n")
      (insert "|--------+-----+-----|\n")
      (insert "| A | X | 4 |\n")
      (insert "| B | Y | 2 |\n")
      (insert "| C | X | 6 |\n")
      (insert "| D | Z | 3 |\n\n")
      ;; Compute and sort by cost desc
      (insert "#+NAME: bills\n")
      (insert "#+begin_src emacs-lisp :var r=rates u=usage :results value replace\n")
      (insert "(let ((bills (mapcar\n")
      (insert "              (lambda (row)\n")
      (insert "                (let* ((client (car row))\n")
      (insert "                       (svc (cadr row))\n")
      (insert "                       (hrs (caddr row))\n")
      (insert "                       (rate (cadr (assoc svc r)))\n")
      (insert "                       (cost (* hrs rate)))\n")
      (insert "                  (list client svc hrs rate cost)))\n")
      (insert "              u)))\n")
      (insert "  (sort bills (lambda (a b) (> (nth 4 a) (nth 4 b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "bills")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_var_column_sort_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|------+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Input table
      (insert "#+NAME: items\n")
      (insert "| Name | Val |\n")
      (insert("|------+-----|\n")
      (insert "| C | 30 |\n")
      (insert "| A | 10 |\n")
      (insert "| D | 40 |\n")
      (insert "| B | 20 |\n\n")
      ;; Sort by name
      (insert "#+NAME: byname\n")
      (insert "#+begin_src emacs-lisp :var tbl=items :results value replace\n")
      (insert "(sort (copy-sequence tbl)\n")
      (insert "      (lambda (a b) (string< (car a) (car b))))\n")
      (insert "#+end_src\n\n")
      ;; Sort by value desc
      (insert "#+NAME: byval\n")
      (insert "#+begin_src emacs-lisp :var tbl=byname :results value replace\n")
      (insert "(sort (copy-sequence tbl)\n")
      (insert "      (lambda (a b) (> (cadr a) (cadr b))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("byname" "byval"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_var_column_filter_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|------+------|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Input table
      (insert "#+NAME: input\n")
      (insert "| City | Temp |\n")
      (insert("|------+------|\n")
      (insert "| A | 15 |\n")
      (insert "| B | 28 |\n")
      (insert "| C | 22 |\n")
      (insert "| D | 30 |\n")
      (insert "| E | 18 |\n\n")
      ;; Filter warm, compute
      (insert "#+NAME: warm\n")
      (insert "#+begin_src emacs-lisp :var tbl=input :results value replace\n")
      (insert "(let ((warm (cl-remove-if-not\n")
      (insert "              (lambda (r) (>= (cadr r) 20))\n")
      (insert "              tbl)))\n")
      (insert "  (list :cities (mapcar #'car warm)\n")
      (insert "        :count (length warm)\n")
      (insert "        :avg (/ (apply #'+ (mapcar #'cadr warm))\n")
      (insert "                (length warm))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "warm")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_column_compute_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---+---|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source data
      (insert "#+NAME: vals\n")
      (insert "| A | B |\n")
      (insert("|---+---|\n")
      (insert "| 2 | 3 |\n")
      (insert "| 4 | 5 |\n")
      (insert "| 6 | 7 |\n\n")
      ;; Compute products and sums
      (insert "#+NAME: computed\n")
      (insert "#+begin_src emacs-lisp :var tbl=vals :results value replace\n")
      (insert "(let ((rows (mapcar\n")
      (insert "              (lambda (row)\n")
      (insert "                (list (car row) (cadr row)\n")
      (insert "                      (* (car row) (cadr row))\n")
      (insert "                      (+ (car row) (cadr row))))\n")
      (insert "              tbl)))\n")
      (insert "  (list :rows rows\n")
      (insert "        :total-prod (apply #'+ (mapcar #'caddr rows))\n")
      (insert "        :total-sum (apply #'+ (mapcar #'cadddr rows))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "computed")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_list_table_join_compute_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":lines\" \"((Pa 4 8 32) (Pb 2 12 24) (Pc 6 5 30))\" \":total\" 86))) \"#+NAME: products\\n| ID | Name | Cost |\\n|----+------+------|\\n| 1 | Pa | 8 |\\n| 2 | Pb | 12 |\\n| 3 | Pc | 5 |\\n\\n#+NAME: orders\\n| PID | Qty |\\n|-----+-----|\\n| 1 | 4 |\\n| 2 | 2 |\\n| 3 | 6 |\\n\\n#+NAME: invoices\\n#+begin_src emacs-lisp :var prods=products ords=orders :results value replace\\n(let ((lines (mapcar\\n              (lambda (order)\\n                (let* ((pid (car order))\\n                       (qty (cadr order))\\n                       (prod (assoc pid prods))\\n                       (name (cadr prod))\\n                       (cost (caddr prod)))\\n                  (list name qty cost (* qty cost))))\\n              ords)))\\n  (list :lines lines\\n        :total (apply #'+ (mapcar #'cadddr lines))))\\n#+end_src\\n\\n#+RESULTS: invoices\\n| :lines | ((Pa 4 8 32) (Pb 2 12 24) (Pc 6 5 30)) | :total | 86 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Products
      (insert "#+NAME: products\n")
      (insert "| ID | Name | Cost |\n")
      (insert "|----+------+------|\n")
      (insert "| 1 | Pa | 8 |\n")
      (insert "| 2 | Pb | 12 |\n")
      (insert "| 3 | Pc | 5 |\n\n")
      ;; Orders
      (insert "#+NAME: orders\n")
      (insert "| PID | Qty |\n")
      (insert "|-----+-----|\n")
      (insert "| 1 | 4 |\n")
      (insert "| 2 | 2 |\n")
      (insert "| 3 | 6 |\n\n")
      ;; Compute invoices
      (insert "#+NAME: invoices\n")
      (insert "#+begin_src emacs-lisp :var prods=products ords=orders :results value replace\n")
      (insert "(let ((lines (mapcar\n")
      (insert "              (lambda (order)\n")
      (insert "                (let* ((pid (car order))\n")
      (insert "                       (qty (cadr order))\n")
      (insert "                       (prod (assoc pid prods))\n")
      (insert "                       (name (cadr prod))\n")
      (insert "                       (cost (caddr prod)))\n")
      (insert "                  (list name qty cost (* qty cost))))\n")
      (insert "              ords)))\n")
      (insert "  (list :lines lines\n")
      (insert "        :total (apply #'+ (mapcar #'cadddr lines))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "invoices")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_column_aggregate_deep_v2_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|--------+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: log\n")
      (insert "| Action | Val |\n")
      (insert("|--------+-----|\n")
      (insert "| Add | 10 |\n")
      (insert "| Sub | 5 |\n")
      (insert "| Add | 20 |\n")
      (insert "| Mul | 3 |\n")
      (insert "| Add | 15 |\n\n")
      ;; Group by action
      (insert "#+NAME: summary\n")
      (insert "#+begin_src emacs-lisp :var tbl=log :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (row tbl)\n")
      (insert "    (let* ((action (car row))\n")
      (insert "           (val (cadr row))\n")
      (insert "           (entry (assoc action groups)))\n")
      (insert "      (if entry\n")
      (insert "          (setcdr entry (+ (cdr entry) val))\n")
      (insert "          (push (cons action val) groups))))\n")
      (insert "  (sort groups (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "summary")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_table_assoc_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"X\" 3 5 15) (\"Y\" 1 10 10) (\"Z\" 2 15 30))) \"#+NAME: prices\\n| Item | Price |\\n|------+-------|\\n| X | 5 |\\n| Y | 10 |\\n| Z | 15 |\\n\\n#+NAME: orders\\n| Item | Qty |\\n|------+-----|\\n| X | 3 |\\n| Y | 1 |\\n| Z | 2 |\\n\\n#+NAME: lines\\n#+begin_src emacs-lisp :var p=prices o=orders :results value replace\\n(mapcar (lambda (order)\\n          (let* ((item (car order))\\n                 (qty (cadr order))\\n                 (price (cadr (assoc item p))))\\n            (list item qty price (* qty price))))\\n        o)\\n#+end_src\\n\\n#+RESULTS: lines\\n| X | 3 |  5 | 15 |\\n| Y | 1 | 10 | 10 |\\n| Z | 2 | 15 | 30 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Prices
      (insert "#+NAME: prices\n")
      (insert "| Item | Price |\n")
      (insert "|------+-------|\n")
      (insert "| X | 5 |\n")
      (insert "| Y | 10 |\n")
      (insert "| Z | 15 |\n\n")
      ;; Orders
      (insert "#+NAME: orders\n")
      (insert "| Item | Qty |\n")
      (insert "|------+-----|\n")
      (insert "| X | 3 |\n")
      (insert "| Y | 1 |\n")
      (insert "| Z | 2 |\n\n")
      ;; Compute line totals
      (insert "#+NAME: lines\n")
      (insert "#+begin_src emacs-lisp :var p=prices o=orders :results value replace\n")
      (insert "(mapcar (lambda (order)\n")
      (insert "          (let* ((item (car order))\n")
      (insert "                 (qty (cadr order))\n")
      (insert "                 (price (cadr (assoc item p))))\n")
      (insert "            (list item qty price (* qty price))))\n")
      (insert "        o)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "lines")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_assoc_chain_aggregate_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|------+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: ledger\n")
      (insert "| Acct | Amt |\n")
      (insert("|------+-----|\n")
      (insert "| A | 100 |\n")
      (insert "| B | 200 |\n")
      (insert "| A | 50 |\n")
      (insert "| C | 300 |\n")
      (insert "| B | 150 |\n\n")
      ;; Group by account
      (insert "#+NAME: balances\n")
      (insert "#+begin_src emacs-lisp :var tbl=ledger :results value replace\n")
      (insert "(let ((acc nil))\n")
      (insert "  (dolist (row tbl)\n")
      (insert "    (let* ((acct (car row))\n")
      (insert "           (amt (cadr row))\n")
      (insert "           (entry (assoc acct acc)))\n")
      (insert "      (if entry\n")
      (insert "          (setcdr entry (+ (cdr entry) amt))\n")
      (insert "          (push (cons acct amt) acc))))\n")
      (insert "  (sort acc (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "balances")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_filter_sort_aggregate_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|------+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source data
      (insert "#+NAME: scores\n")
      (insert "| Name | Pts |\n")
      (insert("|------+-----|\n")
      (insert "| Z | 30 |\n")
      (insert "| A | 10 |\n")
      (insert "| M | 50 |\n")
      (insert "| B | 20 |\n")
      (insert "| C | 40 |\n\n")
      ;; Filter > 20, sort, aggregate
      (insert "#+NAME: top\n")
      (insert "#+begin_src emacs-lisp :var tbl=scores :results value replace\n")
      (insert "(let* ((big (cl-remove-if-not (lambda (r) (> (cadr r) 20)) tbl))\n")
      (insert "       (sorted (sort (copy-sequence big)\n")
      (insert "                     (lambda (a b) (> (cadr a) (cadr b)))))\n")
      (insert "       (total (apply #'+ (mapcar #'cadr sorted))))\n")
      (insert "  (list :top sorted :total total))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "top")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_group_aggregate_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|--------+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Sales table
      (insert "#+NAME: sales\n")
      (insert "| Region | Amt |\n")
      (insert("|--------+-----|\n")
      (insert "| N | 100 |\n")
      (insert "| S | 200 |\n")
      (insert "| N | 150 |\n")
      (insert "| E | 300 |\n")
      (insert "| S | 250 |\n\n")
      ;; Group and aggregate
      (insert "#+NAME: grouped\n")
      (insert "#+begin_src emacs-lisp :var tbl=sales :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (row tbl)\n")
      (insert "    (let* ((region (car row))\n")
      (insert "           (amt (cadr row))\n")
      (insert "           (entry (assoc region groups)))\n")
      (insert "      (if entry\n")
      (insert "          (setcdr entry (+ (cdr entry) amt))\n")
      (insert "          (push (cons region amt) groups))))\n")
      (insert "  (sort groups (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "grouped")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_var_chain_aggregate_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|-----+-----|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: items\n")
      (insert "| Cat | Val |\n")
      (insert("|-----+-----|\n")
      (insert "| A | 10 |\n")
      (insert "| B | 20 |\n")
      (insert "| A | 30 |\n")
      (insert "| C | 40 |\n")
      (insert "| B | 50 |\n\n")
      ;; Group and sum
      (insert "#+NAME: grouped\n")
      (insert "#+begin_src emacs-lisp :var tbl=items :results value replace\n")
      (insert "(let ((groups nil))\n")
      (insert "  (dolist (row tbl)\n")
      (insert "    (let* ((cat (car row))\n")
      (insert "           (val (cadr row))\n")
      (insert "           (entry (assoc cat groups)))\n")
      (insert "      (if entry\n")
      (insert "          (setcdr entry (+ (cdr entry) val))\n")
      (insert "          (push (cons cat val) groups))))\n")
      (insert "  (sort groups (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "grouped")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_join_compute_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-function \"|---------+------|\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Rates table
      (insert "#+NAME: rates\n")
      (insert "| Service | Rate |\n")
      (insert("|---------+------|\n")
      (insert "| Std | 100 |\n")
      (insert "| Prem | 200 |\n\n")
      ;; Usage table
      (insert "#+NAME: usage\n")
      (insert "| Client | Service | Hours |\n")
      (insert "|--------+---------+-------|\n")
      (insert "| X | Std | 5 |\n")
      (insert "| Y | Prem | 3 |\n")
      (insert "| Z | Std | 8 |\n\n")
      ;; Compute bills
      (insert "#+NAME: bills\n")
      (insert "#+begin_src emacs-lisp :var r=rates u=usage :results value replace\n")
      (insert "(mapcar (lambda (row)\n")
      (insert "          (let* ((client (car row))\n")
      (insert "                 (service (cadr row))\n")
      (insert "                 (hours (caddr row))\n")
      (insert "                 (rate (cadr (assoc service r)))\n")
      (insert "                 (bill (* hours rate)))\n")
      (insert "            (list client service hours rate bill)))\n")
      (insert "        u)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "bills")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_table_join_aggregate_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":rows\" \"((A 3 10 30) (B 1 20 20) (C 5 15 75))\" \":total\" 125))) \"#+NAME: products\\n| ID | Name | Price |\\n|----+------+-------|\\n| 1 | A | 10 |\\n| 2 | B | 20 |\\n| 3 | C | 15 |\\n\\n#+NAME: orders\\n| ProductID | Qty |\\n|-----------+-----|\\n| 1 | 3 |\\n| 2 | 1 |\\n| 3 | 5 |\\n\\n#+NAME: revenue\\n#+begin_src emacs-lisp :var prods=products ords=orders :results value replace\\n(let ((rows (mapcar\\n              (lambda (order)\\n                (let* ((pid (car order))\\n                       (qty (cadr order))\\n                       (prod (assoc pid prods))\\n                       (name (cadr prod))\\n                       (price (caddr prod)))\\n                  (list name qty price (* qty price))))\\n              ords)))\\n  (list :rows rows\\n        :total (apply #'+ (mapcar #'cadddr rows))))\\n#+end_src\\n\\n#+RESULTS: revenue\\n| :rows | ((A 3 10 30) (B 1 20 20) (C 5 15 75)) | :total | 125 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Products
      (insert "#+NAME: products\n")
      (insert "| ID | Name | Price |\n")
      (insert "|----+------+-------|\n")
      (insert "| 1 | A | 10 |\n")
      (insert "| 2 | B | 20 |\n")
      (insert "| 3 | C | 15 |\n\n")
      ;; Orders
      (insert "#+NAME: orders\n")
      (insert "| ProductID | Qty |\n")
      (insert "|-----------+-----|\n")
      (insert "| 1 | 3 |\n")
      (insert "| 2 | 1 |\n")
      (insert "| 3 | 5 |\n\n")
      ;; Compute revenue
      (insert "#+NAME: revenue\n")
      (insert "#+begin_src emacs-lisp :var prods=products ords=orders :results value replace\n")
      (insert "(let ((rows (mapcar\n")
      (insert "              (lambda (order)\n")
      (insert "                (let* ((pid (car order))\n")
      (insert "                       (qty (cadr order))\n")
      (insert "                       (prod (assoc pid prods))\n")
      (insert "                       (name (cadr prod))\n")
      (insert "                       (price (caddr prod)))\n")
      (insert "                  (list name qty price (* qty price))))\n")
      (insert "              ords)))\n")
      (insert "  (list :rows rows\n")
      (insert "        :total (apply #'+ (mapcar #'cadddr rows))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "revenue")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_map_reduce_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((3 4 5.0) (5 12 13.0) (8 15 17.0))) \"#+NAME: data\\n| X | Y |\\n|---+---|\\n| 3 | 4 |\\n| 5 | 12 |\\n| 8 | 15 |\\n\\n#+NAME: dists\\n#+begin_src emacs-lisp :var tbl=data :results value replace\\n(mapcar (lambda (row)\\n          (let ((x (car row)) (y (cadr row)))\\n            (list x y (sqrt (+ (* x x) (* y y))))))\\n        tbl)\\n#+end_src\\n\\n#+RESULTS: dists\\n| 3 |  4 |  5.0 |\\n| 5 | 12 | 13.0 |\\n| 8 | 15 | 17.0 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source data
      (insert "#+NAME: data\n")
      (insert "| X | Y |\n")
      (insert "|---+---|\n")
      (insert "| 3 | 4 |\n")
      (insert "| 5 | 12 |\n")
      (insert "| 8 | 15 |\n\n")
      ;; Compute distances
      (insert "#+NAME: dists\n")
      (insert "#+begin_src emacs-lisp :var tbl=data :results value replace\n")
      (insert "(mapcar (lambda (row)\n")
      (insert "          (let ((x (car row)) (y (cadr row)))\n")
      (insert "            (list x y (sqrt (+ (* x x) (* y y))))))\n")
      (insert "        tbl)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "dists")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_list_table_join_aggregate_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":rows\" \"((A 5 4 20) (B 8 2 16) (C 3 7 21))\" \":grand\" 57))) \"#+NAME: prices\\n| Item | Price |\\n|------+-------|\\n| A | 5 |\\n| B | 8 |\\n| C | 3 |\\n\\n#+NAME: qtys\\n| Item | Qty |\\n|------+-----|\\n| A | 4 |\\n| B | 2 |\\n| C | 7 |\\n\\n#+NAME: agg\\n#+begin_src emacs-lisp :var p=prices q=qtys :results value replace\\n(let* ((rows (mapcar\\n               (lambda (row)\\n                 (let* ((item (car row))\\n                        (price (cadr row))\\n                        (qty (cadr (assoc item q)))\\n                        (total (* price (or qty 0))))\\n                   (list item price (or qty 0) total)))\\n               p))\\n       (grand (apply #'+ (mapcar #'cadddr rows))))\\n  (list :rows rows :grand grand))\\n#+end_src\\n\\n#+RESULTS: agg\\n| :rows | ((A 5 4 20) (B 8 2 16) (C 3 7 21)) | :grand | 57 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Prices
      (insert "#+NAME: prices\n")
      (insert "| Item | Price |\n")
      (insert "|------+-------|\n")
      (insert "| A | 5 |\n")
      (insert "| B | 8 |\n")
      (insert "| C | 3 |\n\n")
      ;; Quantities
      (insert "#+NAME: qtys\n")
      (insert "| Item | Qty |\n")
      (insert "|------+-----|\n")
      (insert "| A | 4 |\n")
      (insert "| B | 2 |\n")
      (insert "| C | 7 |\n\n")
      ;; Aggregate
      (insert "#+NAME: agg\n")
      (insert "#+begin_src emacs-lisp :var p=prices q=qtys :results value replace\n")
      (insert "(let* ((rows (mapcar\n")
      (insert "               (lambda (row)\n")
      (insert "                 (let* ((item (car row))\n")
      (insert "                        (price (cadr row))\n")
      (insert "                        (qty (cadr (assoc item q)))\n")
      (insert "                        (total (* price (or qty 0))))\n")
      (insert "                   (list item price (or qty 0) total)))\n")
      (insert "               p))\n")
      (insert "       (grand (apply #'+ (mapcar #'cadddr rows))))\n")
      (insert "  (list :rows rows :grand grand))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "agg")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_column_join_stats_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Eng\" \"Ada\" 2 220 110) (\"Mkt\" \"Bob\" 1 90 90) (\"Sales\" \"Cal\" 1 110 110))) \"#+NAME: depts\\n| Dept | Head |\\n|------+------|\\n| Eng | Ada |\\n| Mkt | Bob |\\n| Sales | Cal |\\n\\n#+NAME: emps\\n| Name | Dept | Salary |\\n|------+--------|\\n| X1 | Eng | 100 |\\n| X2 | Eng | 120 |\\n| X3 | Mkt | 90 |\\n| X4 | Sales | 110 |\\n\\n#+NAME: stats\\n#+begin_src emacs-lisp :var d=depts e=emps :results value replace\\n(mapcar\\n (lambda (dept-row)\\n   (let* ((dept (car dept-row))\\n          (head (cadr dept-row))\\n          (members (cl-remove-if-not\\n                    (lambda (r) (string= (cadr r) dept))\\n                    e))\\n          (salaries (mapcar #'caddr members)))\\n     (list dept head (length members)\\n           (apply #'+ salaries)\\n           (/ (apply #'+ salaries) (max 1 (length members))))))\\n d)\\n#+end_src\\n\\n#+RESULTS: stats\\n| Eng   | Ada | 2 | 220 | 110 |\\n| Mkt   | Bob | 1 |  90 |  90 |\\n| Sales | Cal | 1 | 110 | 110 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Departments
      (insert "#+NAME: depts\n")
      (insert "| Dept | Head |\n")
      (insert "|------+------|\n")
      (insert "| Eng | Ada |\n")
      (insert "| Mkt | Bob |\n")
      (insert "| Sales | Cal |\n\n")
      ;; Employees
      (insert "#+NAME: emps\n")
      (insert "| Name | Dept | Salary |\n")
      (insert "|------+--------|\n")
      (insert "| X1 | Eng | 100 |\n")
      (insert "| X2 | Eng | 120 |\n")
      (insert "| X3 | Mkt | 90 |\n")
      (insert "| X4 | Sales | 110 |\n\n")
      ;; Compute dept stats
      (insert "#+NAME: stats\n")
      (insert "#+begin_src emacs-lisp :var d=depts e=emps :results value replace\n")
      (insert "(mapcar\n")
      (insert " (lambda (dept-row)\n")
      (insert "   (let* ((dept (car dept-row))\n")
      (insert "          (head (cadr dept-row))\n")
      (insert "          (members (cl-remove-if-not\n")
      (insert "                    (lambda (r) (string= (cadr r) dept))\n")
      (insert "                    e))\n")
      (insert "          (salaries (mapcar #'caddr members)))\n")
      (insert "     (list dept head (length members)\n")
      (insert "           (apply #'+ salaries)\n")
      (insert "           (/ (apply #'+ salaries) (max 1 (length members))))))\n")
      (insert " d)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "stats")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_table_assoc_compute_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Ada\" 95 \"A\") (\"Bob\" 78 \"C\") (\"Cal\" 92 \"A\") (\"Dee\" 85 \"B\"))) \"#+NAME: input\\n| Name | Score |\\n|------+-------|\\n| Ada | 95 |\\n| Bob | 78 |\\n| Cal | 92 |\\n| Dee | 85 |\\n\\n#+NAME: graded\\n#+begin_src emacs-lisp :var tbl=input :results value replace\\n(mapcar (lambda (row)\\n          (let* ((name (car row))\\n                 (score (cadr row))\\n                 (grade (cond ((>= score 90) 'A)\\n                              ((>= score 80) 'B)\\n                              ((>= score 70) 'C)\\n                              (t 'D))))\\n            (list name score grade)))\\n        tbl)\\n#+end_src\\n\\n#+RESULTS: graded\\n| Ada | 95 | A |\\n| Bob | 78 | C |\\n| Cal | 92 | A |\\n| Dee | 85 | B |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Input table
      (insert "#+NAME: input\n")
      (insert "| Name | Score |\n")
      (insert "|------+-------|\n")
      (insert "| Ada | 95 |\n")
      (insert "| Bob | 78 |\n")
      (insert "| Cal | 92 |\n")
      (insert "| Dee | 85 |\n\n")
      ;; Grade compute
      (insert "#+NAME: graded\n")
      (insert "#+begin_src emacs-lisp :var tbl=input :results value replace\n")
      (insert "(mapcar (lambda (row)\n")
      (insert "          (let* ((name (car row))\n")
      (insert "                 (score (cadr row))\n")
      (insert "                 (grade (cond ((>= score 90) 'A)\n")
      (insert "                              ((>= score 80) 'B)\n")
      (insert "                              ((>= score 70) 'C)\n")
      (insert "                              (t 'D))))\n")
      (insert "            (list name score grade)))\n")
      (insert "        tbl)\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "graded")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_pcase_cond_cl_case_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"b\" 6 \"(key val)\"))) \"#+begin_src emacs-lisp :results value replace\\n(list (pcase 2 (1 'a) (2 'b) (3 'c))\\n      (pcase '(1 2 3)\\n        (`(,x ,y ,z) (+ x y z))\\n        (_ 0))\\n      (pcase '(key . val)\\n        (`(,k . ,v) (list k v))))\\n#+end_src\\n\\n#+RESULTS:\\n| b | 6 | (key val) |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((x 5))\\n  (cond ((< x 3) 'small)\\n        ((< x 7) 'medium)\\n        (t 'large)))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; pcase
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (pcase 2 (1 'a) (2 'b) (3 'c))\n")
      (insert "      (pcase '(1 2 3)\n")
      (insert "        (`(,x ,y ,z) (+ x y z))\n")
      (insert "        (_ 0))\n")
      (insert "      (pcase '(key . val)\n")
      (insert "        (`(,k . ,v) (list k v))))\n")
      (insert "#+end_src\n\n")
      ;; cond
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((x 5))\n")
      (insert "  (cond ((< x 3) 'small)\n")
      (insert "        ((< x 7) 'medium)\n")
      (insert "        (t 'large)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_table_join_stats_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":rows\" \"((N A 10 50) (S B 20 160) (N B 15 120) (E A 25 125))\" \":total-rev\" 455 \":n-regions\" 3))) \"#+NAME: sales\\n| Region | Product | Qty |\\n|--------+---------+-----|\\n| N | A | 10 |\\n| S | B | 20 |\\n| N | B | 15 |\\n| E | A | 25 |\\n\\n#+NAME: prices\\n| Product | Price |\\n|---------+-------|\\n| A | 5 |\\n| B | 8 |\\n\\n#+NAME: revenue\\n#+begin_src emacs-lisp :var s=sales p=prices :results value replace\\n(let ((rows (mapcar\\n              (lambda (row)\\n                (let* ((region (car row))\\n                       (product (cadr row))\\n                       (qty (caddr row))\\n                       (price (cadr (assoc product p))))\\n                  (list region product qty (* qty price))))\\n              s)))\\n  (list :rows rows\\n        :total-rev (apply #'+ (mapcar #'cadddr rows))\\n        :n-regions (length (seq-uniq (mapcar #'car s))))))\\n#+end_src\\n\\n#+RESULTS: revenue\\n| :rows | ((N A 10 50) (S B 20 160) (N B 15 120) (E A 25 125)) | :total-rev | 455 | :n-regions | 3 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Sales table
      (insert "#+NAME: sales\n")
      (insert "| Region | Product | Qty |\n")
      (insert "|--------+---------+-----|\n")
      (insert "| N | A | 10 |\n")
      (insert "| S | B | 20 |\n")
      (insert "| N | B | 15 |\n")
      (insert "| E | A | 25 |\n\n")
      ;; Price table
      (insert "#+NAME: prices\n")
      (insert "| Product | Price |\n")
      (insert "|---------+-------|\n")
      (insert "| A | 5 |\n")
      (insert "| B | 8 |\n\n")
      ;; Compute revenue
      (insert "#+NAME: revenue\n")
      (insert "#+begin_src emacs-lisp :var s=sales p=prices :results value replace\n")
      (insert "(let ((rows (mapcar\n")
      (insert "              (lambda (row)\n")
      (insert "                (let* ((region (car row))\n")
      (insert "                       (product (cadr row))\n")
      (insert "                       (qty (caddr row))\n")
      (insert "                       (price (cadr (assoc product p))))\n")
      (insert "                  (list region product qty (* qty price))))\n")
      (insert "              s)))\n")
      (insert "  (list :rows rows\n")
      (insert "        :total-rev (apply #'+ (mapcar #'cadddr rows))\n")
      (insert "        :n-regions (length (seq-uniq (mapcar #'car s))))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "revenue")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_list_map_filter_reduce_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5 6 7 8 9 10)) ((4 16 36 64 100)) ((\":count\" 5 \":total\" 220 \":avg\" 44))) \"#+NAME: raw\\n#+begin_src emacs-lisp :results value replace\\n'(1 2 3 4 5 6 7 8 9 10)\\n#+end_src\\n\\n#+RESULTS: raw\\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 | 10 |\\n\\n#+NAME: squares\\n#+begin_src emacs-lisp :var data=raw :results value replace\\n(let ((evens (seq-filter #'evenp data)))\\n  (mapcar (lambda (x) (* x x)) evens))\\n#+end_src\\n\\n#+RESULTS: squares\\n| 4 | 16 | 36 | 64 | 100 |\\n\\n#+NAME: total\\n#+begin_src emacs-lisp :var data=squares :results value replace\\n(list :count (length data)\\n      :total (apply #'+ data)\\n      :avg (/ (apply #'+ data) (length data)))\\n#+end_src\\n\\n#+RESULTS: total\\n| :count | 5 | :total | 220 | :avg | 44 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Build data
      (insert "#+NAME: raw\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(1 2 3 4 5 6 7 8 9 10)\n")
      (insert "#+end_src\n\n")
      ;; Filter even, map square
      (insert "#+NAME: squares\n")
      (insert "#+begin_src emacs-lisp :var data=raw :results value replace\n")
      (insert "(let ((evens (seq-filter #'evenp data)))\n")
      (insert "  (mapcar (lambda (x) (* x x)) evens))\n")
      (insert "#+end_src\n\n")
      ;; Reduce sum
      (insert "#+NAME: total\n")
      (insert "#+begin_src emacs-lisp :var data=squares :results value replace\n")
      (insert "(list :count (length data)\n")
      (insert "      :total (apply #'+ data)\n")
      (insert "      :avg (/ (apply #'+ data) (length data)))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("raw" "squares" "total"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_row_sort_filter_compute_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":sorted\" \"((A 10) (B 40) (M 20) (Z 30))\" \":big\" \"((B 40) (M 20) (Z 30))\" \":big-total\" 90))) \"#+NAME: data\\n| Name | Val |\\n|------+-----|\\n| Z | 30 |\\n| A | 10 |\\n| M | 20 |\\n| B | 40 |\\n\\n#+NAME: result\\n#+begin_src emacs-lisp :var tbl=data :results value replace\\n(let* ((sorted (sort (copy-sequence tbl)\\n                     (lambda (a b) (string< (car a) (car b)))))\\n       (big (cl-remove-if-not (lambda (r) (>= (cadr r) 20)) sorted)))\\n  (list :sorted sorted\\n        :big big\\n        :big-total (apply #'+ (mapcar #'cadr big))))\\n#+end_src\\n\\n#+RESULTS: result\\n| :sorted | ((A 10) (B 40) (M 20) (Z 30)) | :big | ((B 40) (M 20) (Z 30)) | :big-total | 90 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: data\n")
      (insert "| Name | Val |\n")
      (insert "|------+-----|\n")
      (insert "| Z | 30 |\n")
      (insert "| A | 10 |\n")
      (insert "| M | 20 |\n")
      (insert "| B | 40 |\n\n")
      ;; Sort + filter + compute
      (insert "#+NAME: result\n")
      (insert "#+begin_src emacs-lisp :var tbl=data :results value replace\n")
      (insert "(let* ((sorted (sort (copy-sequence tbl)\n")
      (insert "                     (lambda (a b) (string< (car a) (car b)))))\n")
      (insert "       (big (cl-remove-if-not (lambda (r) (>= (cadr r) 20)) sorted)))\n")
      (insert "  (list :sorted sorted\n")
      (insert "        :big big\n")
      (insert "        :big-total (apply #'+ (mapcar #'cadr big))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "result")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_filter_chain_table_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"X\" \"Y\" \"X+Y\" \"X*Y\") hline (2 7 9 14) (8 1 9 8) (4 9 13 36))) \"#+NAME: raw\\n| X | Y |\\n|---+---|\\n| 2 | 7 |\\n| 5 | 3 |\\n| 8 | 1 |\\n| 4 | 9 |\\n\\n#+NAME: computed\\n#+begin_src emacs-lisp :var tbl=raw :results value table replace\\n(let ((filtered (cl-remove-if-not\\n                 (lambda (row) (evenp (car row)))\\n                 tbl)))\\n  (cons '(\\\"X\\\" \\\"Y\\\" \\\"X+Y\\\" \\\"X*Y\\\")\\n        (cons 'hline\\n              (mapcar (lambda (r)\\n                        (list (car r) (cadr r)\\n                              (+ (car r) (cadr r))\\n                              (* (car r) (cadr r))))\\n                      filtered))))\\n#+end_src\\n\\n#+RESULTS: computed\\n| X | Y | X+Y | X*Y |\\n|---+---+-----+-----|\\n| 2 | 7 |   9 |  14 |\\n| 8 | 1 |   9 |   8 |\\n| 4 | 9 |  13 |  36 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Data
      (insert "#+NAME: raw\n")
      (insert "| X | Y |\n")
      (insert "|---+---|\n")
      (insert "| 2 | 7 |\n")
      (insert "| 5 | 3 |\n")
      (insert "| 8 | 1 |\n")
      (insert "| 4 | 9 |\n\n")
      ;; Filter even X, compute
      (insert "#+NAME: computed\n")
      (insert "#+begin_src emacs-lisp :var tbl=raw :results value table replace\n")
      (insert "(let ((filtered (cl-remove-if-not\n")
      (insert "                 (lambda (row) (evenp (car row)))\n")
      (insert "                 tbl)))\n")
      (insert "  (cons '(\"X\" \"Y\" \"X+Y\" \"X*Y\")\n")
      (insert "        (cons 'hline\n")
      (insert "              (mapcar (lambda (r)\n")
      (insert "                        (list (car r) (cadr r)\n")
      (insert "                              (+ (car r) (cadr r))\n")
      (insert "                              (* (car r) (cadr r))))\n")
      (insert "                      filtered))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "computed")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_letrec_named_let_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"t\" \"t\" \"nil\" \"nil\"))) \"#+begin_src emacs-lisp :results value replace\\n(letrec ((is-even (lambda (n) (if (= n 0) t (funcall is-odd (1- n)))))\\n         (is-odd (lambda (n) (if (= n 0) nil (funcall is-even (1- n))))))\\n  (list (funcall is-even 4) (funcall is-odd 5)\\n        (funcall is-even 7) (funcall is-odd 2)))\\n#+end_src\\n\\n#+RESULTS:\\n| t | t | nil | nil |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let loop ((i 0) (acc nil))\\n  (if (> i 5)\\n      (nreverse acc)\\n      (loop (1+ i) (cons (* i i) acc))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; letrec
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(letrec ((is-even (lambda (n) (if (= n 0) t (funcall is-odd (1- n)))))\n")
      (insert "         (is-odd (lambda (n) (if (= n 0) nil (funcall is-even (1- n))))))\n")
      (insert "  (list (funcall is-even 4) (funcall is-odd 5)\n")
      (insert "        (funcall is-even 7) (funcall is-odd 2)))\n")
      (insert "#+end_src\n\n")
      ;; named let (loop)
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let loop ((i 0) (acc nil))\n")
      (insert "  (if (> i 5)\n")
      (insert "      (nreverse acc)\n")
      (insert "      (loop (1+ i) (cons (* i i) acc))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_column_join_compute_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Item\" \"Cost\" \"Qty\" \"Total\") hline (\"A\" 5 4 20) (\"B\" 8 2 16) (\"C\" 3 7 21))) \"#+NAME: prices\\n| Item | Cost |\\n|------+------|\\n| A | 5 |\\n| B | 8 |\\n| C | 3 |\\n\\n#+NAME: qtys\\n| Item | N |\\n|------+---|\\n| A | 4 |\\n| B | 2 |\\n| C | 7 |\\n\\n#+NAME: joined\\n#+begin_src emacs-lisp :var p=prices q=qtys :results value table replace\\n(cons '(\\\"Item\\\" \\\"Cost\\\" \\\"Qty\\\" \\\"Total\\\")\\n      (cons 'hline\\n            (mapcar (lambda (row)\\n                      (let* ((item (car row))\\n                             (cost (cadr row))\\n                             (qty (cadr (assoc item q))))\\n                        (list item cost qty (* cost qty))))\\n                    p)))\\n#+end_src\\n\\n#+RESULTS: joined\\n| Item | Cost | Qty | Total |\\n|------+------+-----+-------|\\n| A    |    5 |   4 |    20 |\\n| B    |    8 |   2 |    16 |\\n| C    |    3 |   7 |    21 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Prices table
      (insert "#+NAME: prices\n")
      (insert "| Item | Cost |\n")
      (insert "|------+------|\n")
      (insert "| A | 5 |\n")
      (insert "| B | 8 |\n")
      (insert "| C | 3 |\n\n")
      ;; Quantities table
      (insert "#+NAME: qtys\n")
      (insert "| Item | N |\n")
      (insert "|------+---|\n")
      (insert "| A | 4 |\n")
      (insert "| B | 2 |\n")
      (insert "| C | 7 |\n\n")
      ;; Join and compute
      (insert "#+NAME: joined\n")
      (insert "#+begin_src emacs-lisp :var p=prices q=qtys :results value table replace\n")
      (insert "(cons '(\"Item\" \"Cost\" \"Qty\" \"Total\")\n")
      (insert "      (cons 'hline\n")
      (insert "            (mapcar (lambda (row)\n")
      (insert "                      (let* ((item (car row))\n")
      (insert "                             (cost (cadr row))\n")
      (insert "                             (qty (cadr (assoc item q))))\n")
      (insert "                        (list item cost qty (* cost qty))))\n")
      (insert "                    p)))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "joined")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_block_var_chain_table_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((5 3 8 1 9 2 7 4 6)) ((1 2 3 4 5 6 7 8 9)) ((\":first\" 1 \":last\" 9 \":mid\" 5 \":sum\" 45))) \"#+NAME: nums\\n#+begin_src emacs-lisp :results value replace\\n'(5 3 8 1 9 2 7 4 6)\\n#+end_src\\n\\n#+RESULTS: nums\\n| 5 | 3 | 8 | 1 | 9 | 2 | 7 | 4 | 6 |\\n\\n#+NAME: sorted\\n#+begin_src emacs-lisp :var data=nums :results value replace\\n(sort (copy-sequence data) #'<)\\n#+end_src\\n\\n#+RESULTS: sorted\\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |\\n\\n#+NAME: stats\\n#+begin_src emacs-lisp :var data=sorted :results value replace\\n(list :first (car data)\\n      :last (car (last data))\\n      :mid (nth (/ (length data) 2) data)\\n      :sum (apply #'+ data))\\n#+end_src\\n\\n#+RESULTS: stats\\n| :first | 1 | :last | 9 | :mid | 5 | :sum | 45 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source numbers
      (insert "#+NAME: nums\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(5 3 8 1 9 2 7 4 6)\n")
      (insert "#+end_src\n\n")
      ;; Sort
      (insert "#+NAME: sorted\n")
      (insert "#+begin_src emacs-lisp :var data=nums :results value replace\n")
      (insert "(sort (copy-sequence data) #'<)\n")
      (insert "#+end_src\n\n")
      ;; Stats
      (insert "#+NAME: stats\n")
      (insert "#+begin_src emacs-lisp :var data=sorted :results value replace\n")
      (insert "(list :first (car data)\n")
      (insert "      :last (car (last data))\n")
      (insert "      :mid (nth (/ (length data) 2) data)\n")
      (insert "      :sum (apply #'+ data))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("nums" "sorted" "stats"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_row_filter_map_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"A\" 10 3 30) (\"B\" 20 1 20) (\"D\" 15 2 30))) \"#+NAME: items\\n| Name | Price | Qty |\\n|------+-------+-----|\\n| A | 10 | 3 |\\n| B | 20 | 1 |\\n| C | 5 | 8 |\\n| D | 15 | 2 |\\n\\n#+NAME: expensive\\n#+begin_src emacs-lisp :var tbl=items :results value replace\\n(let ((filtered (cl-remove-if-not\\n                 (lambda (row) (>= (cadr row) 10))\\n                 tbl)))\\n  (mapcar (lambda (row)\\n            (list (car row) (cadr row) (caddr row)\\n                  (* (cadr row) (caddr row))))\\n          filtered))\\n#+end_src\\n\\n#+RESULTS: expensive\\n| A | 10 | 3 | 30 |\\n| B | 20 | 1 | 20 |\\n| D | 15 | 2 | 30 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: items\n")
      (insert "| Name | Price | Qty |\n")
      (insert "|------+-------+-----|\n")
      (insert "| A | 10 | 3 |\n")
      (insert "| B | 20 | 1 |\n")
      (insert "| C | 5 | 8 |\n")
      (insert "| D | 15 | 2 |\n\n")
      ;; Filter + map
      (insert "#+NAME: expensive\n")
      (insert "#+begin_src emacs-lisp :var tbl=items :results value replace\n")
      (insert "(let ((filtered (cl-remove-if-not\n")
      (insert "                 (lambda (row) (>= (cadr row) 10))\n")
      (insert "                 tbl)))\n")
      (insert "  (mapcar (lambda (row)\n")
      (insert "            (list (car row) (cadr row) (caddr row)\n")
      (insert "                  (* (cadr row) (caddr row))))\n")
      (insert "          filtered))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "expensive")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_column_stats_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":x\" \"(2 3 4)\" \":y\" \"(5 7 9)\" \":z\" \"(1 5 8)\"))) ((\"X\" \"Y\" \"Z\") hline (\"2\" \"5\" \"8\") (\"3\" \"7\" \"1\") (\"4\" \"9\" \"6\")) \"#+NAME: data\\n| X | Y | Z |\\n|---+---+---|\\n| 2 | 5 | 8 |\\n| 3 | 7 | 1 |\\n| 4 | 9 | 6 |\\n\\n#+NAME: stats\\n#+begin_src emacs-lisp :var tbl=data :results value replace\\n(let ((xs (mapcar #'car tbl))\\n      (ys (mapcar #'cadr tbl))\\n      (zs (mapcar #'caddr tbl)))\\n  (list :x (list (apply #'min xs) (/ (apply #'+ xs) (length xs)) (apply #'max xs))\\n        :y (list (apply #'min ys) (/ (apply #'+ ys) (length ys)) (apply #'max ys))\\n        :z (list (apply #'min zs) (/ (apply #'+ zs) (length zs)) (apply #'max zs))))\\n#+end_src\\n\\n#+RESULTS: stats\\n| :x | (2 3 4) | :y | (5 7 9) | :z | (1 5 8) |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Data table
      (insert "#+NAME: data\n")
      (insert "| X | Y | Z |\n")
      (insert "|---+---+---|\n")
      (insert "| 2 | 5 | 8 |\n")
      (insert "| 3 | 7 | 1 |\n")
      (insert "| 4 | 9 | 6 |\n\n")
      ;; Column stats
      (insert "#+NAME: stats\n")
      (insert "#+begin_src emacs-lisp :var tbl=data :results value replace\n")
      (insert "(let ((xs (mapcar #'car tbl))\n")
      (insert "      (ys (mapcar #'cadr tbl))\n")
      (insert "      (zs (mapcar #'caddr tbl)))\n")
      (insert "  (list :x (list (apply #'min xs) (/ (apply #'+ xs) (length xs)) (apply #'max xs))\n")
      (insert "        :y (list (apply #'min ys) (/ (apply #'+ ys) (length ys)) (apply #'max ys))\n")
      (insert "        :z (list (apply #'min zs) (/ (apply #'+ zs) (length zs)) (apply #'max zs))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "stats")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        ;; Table lisp
        (goto-char (point-min))
        (search-forward "| X")
        (let ((table-lisp (org-table-to-lisp)))
          (list (nreverse results)
                table-lisp
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_assoc_list_transform_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"(((x . 10) (y . 20) (z . 30)) ((x . 20) (y . 40) (z . 60)) (y . 20) (z . 60) (10 20 30))\") \"#+begin_src emacs-lisp :results value replace\\n(let* ((al (list (cons 'x 10) (cons 'y 20) (cons 'z 30)))\\n       (doubled (mapcar (lambda (p) (cons (car p) (* 2 (cdr p)))) al)))\\n  (list al\\n        doubled\\n        (assq 'y al)\\n        (assoc 'z doubled)\\n        (mapcar #'cdr al)))\\n#+end_src\\n\\n#+RESULTS:\\n: (((x . 10) (y . 20) (z . 30)) ((x . 20) (y . 40) (z . 60)) (y . 20) (z . 60) (10 20 30))\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((db (list (cons 'users (list (cons 'a 1) (cons 'b 2)))\\n               (cons 'scores (list 10 20 30)))))\\n  (list (cdr (assoc 'users db))\\n        (cdr (assoc 'scores db))\\n        (cdr (assoc 'a (cdr (assoc 'users db))))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Alist creation and transform
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let* ((al (list (cons 'x 10) (cons 'y 20) (cons 'z 30)))\n")
      (insert "       (doubled (mapcar (lambda (p) (cons (car p) (* 2 (cdr p)))) al)))\n")
      (insert "  (list al\n")
      (insert "        doubled\n")
      (insert "        (assq 'y al)\n")
      (insert "        (assoc 'z doubled)\n")
      (insert "        (mapcar #'cdr al)))\n")
      (insert "#+end_src\n\n")
      ;; Nested alist
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((db (list (cons 'users (list (cons 'a 1) (cons 'b 2)))\n")
      (insert "               (cons 'scores (list 10 20 30)))))\n")
      (insert "  (list (cdr (assoc 'users db))\n")
      (insert "        (cdr (assoc 'scores db))\n")
      (insert "        (cdr (assoc 'a (cdr (assoc 'users db))))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_cl_loop_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 1 1) (2 1 2) (2 2 4) (3 1 3) (3 2 6) (3 3 9) (4 1 4) (4 2 8) (4 3 12) (4 4 16))) \"#+begin_src emacs-lisp :results value replace\\n(cl-loop for i from 1 to 4\\n         append (cl-loop for j from 1 to i\\n                         collect (list i j (* i j))))\\n#+end_src\\n\\n#+RESULTS:\\n| 1 | 1 |  1 |\\n| 2 | 1 |  2 |\\n| 2 | 2 |  4 |\\n| 3 | 1 |  3 |\\n| 3 | 2 |  6 |\\n| 3 | 3 |  9 |\\n| 4 | 1 |  4 |\\n| 4 | 2 |  8 |\\n| 4 | 3 | 12 |\\n| 4 | 4 | 16 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(cl-loop for x in '(a b c)\\n         for y in '(1 2 3)\\n         collect (cons x y) into result\\n         finally return result)\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; cl-loop with append
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(cl-loop for i from 1 to 4\n")
      (insert "         append (cl-loop for j from 1 to i\n")
      (insert "                         collect (list i j (* i j))))\n")
      (insert "#+end_src\n\n")
      ;; cl-loop with nconc
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(cl-loop for x in '(a b c)\n")
      (insert "         for y in '(1 2 3)\n")
      (insert "         collect (cons x y) into result\n")
      (insert "         finally return result)\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_map_assoc_chain_table_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Ada\" 95 \"A\") (\"Bob\" 87 \"B\") (\"Cal\" 92 \"A\")) ((\":count\" 3 \":a-count\" 2 \":b-count\" 1 \":avg\" 91))) \"#+NAME: input\\n| Name | Score |\\n|------+-------|\\n| Ada | 95 |\\n| Bob | 87 |\\n| Cal | 92 |\\n\\n#+NAME: graded\\n#+begin_src emacs-lisp :var tbl=input :results value replace\\n(mapcar (lambda (row)\\n          (let* ((name (car row))\\n                 (score (cadr row))\\n                 (grade (cond ((>= score 90) 'A)\\n                              ((>= score 80) 'B)\\n                              (t 'C))))\\n            (list name score grade)))\\n        tbl)\\n#+end_src\\n\\n#+RESULTS: graded\\n| Ada | 95 | A |\\n| Bob | 87 | B |\\n| Cal | 92 | A |\\n\\n#+NAME: agg\\n#+begin_src emacs-lisp :var data=graded :results value replace\\n(let ((grades (mapcar #'caddr data)))\\n  (list :count (length grades)\\n        :a-count (cl-count 'A grades)\\n        :b-count (cl-count 'B grades)\\n        :avg (/ (apply #'+ (mapcar #'cadr data)) (length data))))\\n#+end_src\\n\\n#+RESULTS: agg\\n| :count | 3 | :a-count | 2 | :b-count | 1 | :avg | 91 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Input table
      (insert "#+NAME: input\n")
      (insert "| Name | Score |\n")
      (insert "|------+-------|\n")
      (insert "| Ada | 95 |\n")
      (insert "| Bob | 87 |\n")
      (insert "| Cal | 92 |\n\n")
      ;; Map scores to grades
      (insert "#+NAME: graded\n")
      (insert "#+begin_src emacs-lisp :var tbl=input :results value replace\n")
      (insert "(mapcar (lambda (row)\n")
      (insert "          (let* ((name (car row))\n")
      (insert "                 (score (cadr row))\n")
      (insert "                 (grade (cond ((>= score 90) 'A)\n")
      (insert "                              ((>= score 80) 'B)\n")
      (insert "                              (t 'C))))\n")
      (insert "            (list name score grade)))\n")
      (insert "        tbl)\n")
      (insert "#+end_src\n\n")
      ;; Aggregate
      (insert "#+NAME: agg\n")
      (insert "#+begin_src emacs-lisp :var data=graded :results value replace\n")
      (insert "(let ((grades (mapcar #'caddr data)))\n")
      (insert "  (list :count (length grades)\n")
      (insert "        :a-count (cl-count 'A grades)\n")
      (insert "        :b-count (cl-count 'B grades)\n")
      (insert "        :avg (/ (apply #'+ (mapcar #'cadr data)) (length data))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("graded" "agg"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_table_column_compute_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":math\" \"(78 87 95)\" \":lang\" \"(85 88 92)\" \":sci\" \"(80 88 95)\"))) \"#+NAME: scores\\n| Student | Math | Lang | Sci |\\n|---------+--------+------|\\n| Ada | 95 | 88 | 92 |\\n| Bob | 78 | 85 | 80 |\\n| Cal | 90 | 92 | 88 |\\n| Dee | 85 | 90 | 95 |\\n\\n#+NAME: stats\\n#+begin_src emacs-lisp :var tbl=scores :results value replace\\n(let ((math (mapcar #'cadr tbl))\\n      (lang (mapcar #'caddr tbl))\\n      (sci (mapcar #'cadddr tbl)))\\n  (list :math (list (apply #'min math) (/ (apply #'+ math) (length math)) (apply #'max math))\\n        :lang (list (apply #'min lang) (/ (apply #'+ lang) (length lang)) (apply #'max lang))\\n        :sci (list (apply #'min sci) (/ (apply #'+ sci) (length sci)) (apply #'max sci))))\\n#+end_src\\n\\n#+RESULTS: stats\\n| :math | (78 87 95) | :lang | (85 88 92) | :sci | (80 88 95) |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: scores\n")
      (insert "| Student | Math | Lang | Sci |\n")
      (insert "|---------+--------+------|\n")
      (insert "| Ada | 95 | 88 | 92 |\n")
      (insert "| Bob | 78 | 85 | 80 |\n")
      (insert "| Cal | 90 | 92 | 88 |\n")
      (insert "| Dee | 85 | 90 | 95 |\n\n")
      ;; Column stats
      (insert "#+NAME: stats\n")
      (insert "#+begin_src emacs-lisp :var tbl=scores :results value replace\n")
      (insert "(let ((math (mapcar #'cadr tbl))\n")
      (insert "      (lang (mapcar #'caddr tbl))\n")
      (insert "      (sci (mapcar #'cadddr tbl)))\n")
      (insert "  (list :math (list (apply #'min math) (/ (apply #'+ math) (length math)) (apply #'max math))\n")
      (insert "        :lang (list (apply #'min lang) (/ (apply #'+ lang) (length lang)) (apply #'max lang))\n")
      (insert "        :sci (list (apply #'min sci) (/ (apply #'+ sci) (length sci)) (apply #'max sci))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "stats")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_map_tree_assoc_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"((name . Project) (tasks (a 1 2 3) (b 4 5 6) (c 7 8 9)))\" ((\":name\" \"Project\" \":sums\" \"((a . 6) (b . 15) (c . 24))\" \":total\" 45))) \"#+NAME: data\\n#+begin_src emacs-lisp :results value replace\\n'((name . \\\"Project\\\")\\n  (tasks . ((a 1 2 3)\\n            (b 4 5 6)\\n            (c 7 8 9))))\\n#+end_src\\n\\n#+RESULTS: data\\n: ((name . Project) (tasks (a 1 2 3) (b 4 5 6) (c 7 8 9)))\\n\\n#+NAME: proc\\n#+begin_src emacs-lisp :var d=data :results value replace\\n(let* ((tasks (cdr (assoc 'tasks d)))\\n       (sums (mapcar (lambda (item)\\n                       (cons (car item)\\n                             (apply #'+ (cdr item))))\\n                     tasks)))\\n  (list :name (cdr (assoc 'name d))\\n        :sums sums\\n        :total (apply #'+ (mapcar #'cdr sums))))\\n#+end_src\\n\\n#+RESULTS: proc\\n| :name | Project | :sums | ((a . 6) (b . 15) (c . 24)) | :total | 45 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Build data
      (insert "#+NAME: data\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((name . \"Project\")\n")
      (insert "  (tasks . ((a 1 2 3)\n")
      (insert "            (b 4 5 6)\n")
      (insert "            (c 7 8 9))))\n")
      (insert "#+end_src\n\n")
      ;; Process
      (insert "#+NAME: proc\n")
      (insert "#+begin_src emacs-lisp :var d=data :results value replace\n")
      (insert "(let* ((tasks (cdr (assoc 'tasks d)))\n")
      (insert "       (sums (mapcar (lambda (item)\n")
      (insert "                       (cons (car item)\n")
      (insert "                             (apply #'+ (cdr item))))\n")
      (insert "                     tasks)))\n")
      (insert "  (list :name (cdr (assoc 'name d))\n")
      (insert "        :sums sums\n")
      (insert "        :total (apply #'+ (mapcar #'cdr sums))))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("data" "proc"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_tree_map_reduce_pipeline_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"dept-a\" \"(emp . 10)\" \"(budget . 100)\") (\"dept-b\" \"(emp . 20)\" \"(budget . 200)\") (\"dept-c\" \"(emp . 15)\" \"(budget . 150)\")) ((\"dept-a\" 10 100 10) (\"dept-b\" 20 200 10) (\"dept-c\" 15 150 10)) ((\":total-emp\" 45 \":total-budget\" 450 \":avg-per-person\" 10))) \"#+NAME: tree\\n#+begin_src emacs-lisp :results value replace\\n'((dept-a . ((emp . 10) (budget . 100)))\\n  (dept-b . ((emp . 20) (budget . 200)))\\n  (dept-c . ((emp . 15) (budget . 150))))\\n#+end_src\\n\\n#+RESULTS: tree\\n| dept-a | (emp . 10) | (budget . 100) |\\n| dept-b | (emp . 20) | (budget . 200) |\\n| dept-c | (emp . 15) | (budget . 150) |\\n\\n#+NAME: mapped\\n#+begin_src emacs-lisp :var data=tree :results value replace\\n(mapcar (lambda (dept)\\n          (let ((name (car dept))\\n                (emp (cdr (assoc 'emp (cdr dept))))\\n                (budget (cdr (assoc 'budget (cdr dept)))))\\n            (list name emp budget (/ budget emp))))\\n        data)\\n#+end_src\\n\\n#+RESULTS: mapped\\n| dept-a | 10 | 100 | 10 |\\n| dept-b | 20 | 200 | 10 |\\n| dept-c | 15 | 150 | 10 |\\n\\n#+NAME: agg\\n#+begin_src emacs-lisp :var data=mapped :results value replace\\n(let ((emps (mapcar #'cadr data))\\n      (budgets (mapcar #'caddr data)))\\n  (list :total-emp (apply #'+ emps)\\n        :total-budget (apply #'+ budgets)\\n        :avg-per-person (/ (apply #'+ budgets)\\n                          (apply #'+ emps))))\\n#+end_src\\n\\n#+RESULTS: agg\\n| :total-emp | 45 | :total-budget | 450 | :avg-per-person | 10 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Build tree
      (insert "#+NAME: tree\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((dept-a . ((emp . 10) (budget . 100)))\n")
      (insert "  (dept-b . ((emp . 20) (budget . 200)))\n")
      (insert "  (dept-c . ((emp . 15) (budget . 150))))\n")
      (insert "#+end_src\n\n")
      ;; Map: extract and transform
      (insert "#+NAME: mapped\n")
      (insert "#+begin_src emacs-lisp :var data=tree :results value replace\n")
      (insert "(mapcar (lambda (dept)\n")
      (insert "          (let ((name (car dept))\n")
      (insert "                (emp (cdr (assoc 'emp (cdr dept))))\n")
      (insert "                (budget (cdr (assoc 'budget (cdr dept)))))\n")
      (insert "            (list name emp budget (/ budget emp))))\n")
      (insert "        data)\n")
      (insert "#+end_src\n\n")
      ;; Reduce: aggregate
      (insert "#+NAME: agg\n")
      (insert "#+begin_src emacs-lisp :var data=mapped :results value replace\n")
      (insert "(let ((emps (mapcar #'cadr data))\n")
      (insert "      (budgets (mapcar #'caddr data)))\n")
      (insert "  (list :total-emp (apply #'+ emps)\n")
      (insert "        :total-budget (apply #'+ budgets)\n")
      (insert "        :avg-per-person (/ (apply #'+ budgets)\n")
      (insert "                          (apply #'+ emps))))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("tree" "mapped" "agg"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_table_var_chain_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Item\" \"Cost\" \"Qty\" \"Total\") hline (\"A\" 10 3 30) (\"B\" 20 1 20) (\"C\" 30 5 150))) \"#+NAME: prices\\n| Item | Cost |\\n|------+------|\\n| A | 10 |\\n| B | 20 |\\n| C | 30 |\\n\\n#+NAME: quantities\\n| Item | Qty |\\n|------+-----|\\n| A | 3 |\\n| B | 1 |\\n| C | 5 |\\n\\n#+NAME: totals\\n#+begin_src emacs-lisp :var p=prices q=quantities :results value table replace\\n(cons '(\\\"Item\\\" \\\"Cost\\\" \\\"Qty\\\" \\\"Total\\\")\\n      (cons 'hline\\n            (mapcar (lambda (row)\\n                      (let* ((item (car row))\\n                             (cost (cadr row))\\n                             (qty-row (assoc item q))\\n                             (qty (if qty-row (cadr qty-row) 0)))\\n                        (list item cost qty (* cost qty))))\\n                    p)))\\n#+end_src\\n\\n#+RESULTS: totals\\n| Item | Cost | Qty | Total |\\n|------+------+-----+-------|\\n| A    |   10 |   3 |    30 |\\n| B    |   20 |   1 |    20 |\\n| C    |   30 |   5 |   150 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Table A
      (insert "#+NAME: prices\n")
      (insert "| Item | Cost |\n")
      (insert "|------+------|\n")
      (insert "| A | 10 |\n")
      (insert "| B | 20 |\n")
      (insert "| C | 30 |\n\n")
      ;; Table B
      (insert "#+NAME: quantities\n")
      (insert "| Item | Qty |\n")
      (insert "|------+-----|\n")
      (insert "| A | 3 |\n")
      (insert "| B | 1 |\n")
      (insert "| C | 5 |\n\n")
      ;; Compute join
      (insert "#+NAME: totals\n")
      (insert "#+begin_src emacs-lisp :var p=prices q=quantities :results value table replace\n")
      (insert "(cons '(\"Item\" \"Cost\" \"Qty\" \"Total\")\n")
      (insert "      (cons 'hline\n")
      (insert "            (mapcar (lambda (row)\n")
      (insert "                      (let* ((item (car row))\n")
      (insert "                             (cost (cadr row))\n")
      (insert "                             (qty-row (assoc item q))\n")
      (insert "                             (qty (if qty-row (cadr qty-row) 0)))\n")
      (insert "                        (list item cost qty (* cost qty))))\n")
      (insert "                    p)))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (goto-char (point-min))
      (search-forward "totals")
      (org-babel-execute-src-block)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_sequence_slice_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"a\" \"b\" \"c\" \"\" \"\" \"\" \"\" \"\") (\"f\" \"g\" \"h\" \"\" \"\" \"\" \"\" \"\") (\"b\" \"c\" \"d\" \"\" \"\" \"\" \"\" \"\") (\"c\" \"d\" \"e\" \"f\" \"g\" \"h\" \"\" \"\") (\"a\" \"b\" \"c\" \"d\" \"e\" \"f\" \"g\" \"h\") (\"d\" \"e\" \"f\" \"g\" \"h\" \"\" \"\" \"\"))) \"#+begin_src emacs-lisp :results value replace\\n(let ((s '(a b c d e f g h)))\\n  (list (seq-take s 3) (seq-drop s 5)\\n        (seq-subseq s 1 4) (seq-subseq s 2)\\n        (seq-take-while #'symbolp s)\\n        (seq-drop-while (lambda (x) (not (eq x 'd))) s)))\\n#+end_src\\n\\n#+RESULTS:\\n| a | b | c |   |   |   |   |   |\\n| f | g | h |   |   |   |   |   |\\n| b | c | d |   |   |   |   |   |\\n| c | d | e | f | g | h |   |   |\\n| a | b | c | d | e | f | g | h |\\n| d | e | f | g | h |   |   |   |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((s '(3 1 4 1 5 9 2 6 5 3 5)))\\n  (list (seq-find (lambda (x) (> x 4)) s)\\n        (seq-count (lambda (x) (= x 5)) s)\\n        (seq-contains-p s 9)\\n        (seq-contains-p s 0)\\n        (seq-positions s 5)\\n        (seq-uniq s)))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Sequence slicing
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((s '(a b c d e f g h)))\n")
      (insert "  (list (seq-take s 3) (seq-drop s 5)\n")
      (insert "        (seq-subseq s 1 4) (seq-subseq s 2)\n")
      (insert "        (seq-take-while #'symbolp s)\n")
      (insert "        (seq-drop-while (lambda (x) (not (eq x 'd))) s)))\n")
      (insert "#+end_src\n\n")
      ;; Sequence search
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((s '(3 1 4 1 5 9 2 6 5 3 5)))\n")
      (insert "  (list (seq-find (lambda (x) (> x 4)) s)\n")
      (insert "        (seq-count (lambda (x) (= x 5)) s)\n")
      (insert "        (seq-contains-p s 9)\n")
      (insert "        (seq-contains-p s 0)\n")
      (insert "        (seq-positions s 5)\n")
      (insert "        (seq-uniq s)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_io_process_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"hello\"))) \"#+begin_src emacs-lisp :results value replace\\n(with-temp-buffer\\n  (insert \\\"hello\\\\nworld\\\\nfoo\\\\n\\\")\\n  (list (buffer-string)\\n        (count-lines (point-min) (point-max))\\n        (progn (goto-char (point-min))\\n               (search-forward \\\"world\\\")\\n               (line-number-at-pos))))\\n#+end_src\\n\\n#+RESULTS:\\n| hello |\\nworld\\nfoo\\n| 3 | 2 |\\nworld\\nfoo\\n| 3 | 2 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((f \\\"[SESSION-TMPDIR]/org-babel-io/test.txt\\\"))\\n  (with-temp-file f (insert \\\"line1\\\\nline2\\\\n\\\"))\\n  (list (file-exists-p f)\\n        (with-temp-buffer\\n          (insert-file-contents f)\\n          (buffer-string))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Buffer operations
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(with-temp-buffer\n")
      (insert "  (insert \"hello\\nworld\\nfoo\\n\")\n")
      (insert "  (list (buffer-string)\n")
      (insert "        (count-lines (point-min) (point-max))\n")
      (insert "        (progn (goto-char (point-min))\n")
      (insert "               (search-forward \"world\")\n")
      (insert "               (line-number-at-pos))))\n")
      (insert "#+end_src\n\n")
      ;; File operations
      (let* ((root (expand-file-name "org-babel-io" temporary-file-directory))
             (tmp (expand-file-name "test.txt" root)))
        (make-directory root t)
        (insert "#+begin_src emacs-lisp :results value replace\n")
        (insert "(let ((f \"" tmp "\"))\n")
        (insert "  (with-temp-file f (insert \"line1\\nline2\\n\"))\n")
        (insert "  (list (file-exists-p f)\n")
        (insert "        (with-temp-buffer\n")
        (insert "          (insert-file-contents f)\n")
        (insert "          (buffer-string))))\n")
        (insert "#+end_src\n\n"))
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_vector_operations_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((10 50 5 \"[1 2 3]\" \"(10 20 30 40 50)\"))) \"#+begin_src emacs-lisp :results value replace\\n(let ((v [10 20 30 40 50]))\\n  (list (aref v 0) (aref v 4) (length v)\\n        (seq-into '(1 2 3) 'vector)\\n        (append v nil)))\\n#+end_src\\n\\n#+RESULTS:\\n| 10 | 50 | 5 | [1 2 3] | (10 20 30 40 50) |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((v1 [1 2 3]) (v2 [4 5 6]))\\n  (list (vconcat v1 v2)\\n        (mapcar (lambda (x) (* x 2)) (append v1 nil))\\n        (seq-sort #'< (vconcat v2 v1))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Vector create and access
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((v [10 20 30 40 50]))\n")
      (insert "  (list (aref v 0) (aref v 4) (length v)\n")
      (insert "        (seq-into '(1 2 3) 'vector)\n")
      (insert "        (append v nil)))\n")
      (insert "#+end_src\n\n")
      ;; Vector operations
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((v1 [1 2 3]) (v2 [4 5 6]))\n")
      (insert "  (list (vconcat v1 v2)\n")
      (insert "        (mapcar (lambda (x) (* x 2)) (append v1 nil))\n")
      (insert "        (seq-sort #'< (vconcat v2 v1))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_type_check_coerce_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"integer\" \"float\" \"string\" \"cons\" \"vector\" \"symbol\" \"symbol\" \"hash-table\"))) \"#+begin_src emacs-lisp :results value replace\\n(list (type-of 42) (type-of 3.14) (type-of \\\"hi\\\")\\n      (type-of '(1 2)) (type-of [1 2]) (type-of t)\\n      (type-of nil) (type-of (make-hash-table)))\\n#+end_src\\n\\n#+RESULTS:\\n| integer | float | string | cons | vector | symbol | symbol | hash-table |\\n\\n#+begin_src emacs-lisp :results value replace\\n(list (string-to-number \\\"42\\\") (string-to-number \\\"3.14\\\")\\n      (number-to-string 42) (number-to-string 3.14)\\n      (int-to-string 42) (string-to-int \\\"42\\\")\\n      (float 42) (truncate 3.7) (round 3.7))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Type checks
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (type-of 42) (type-of 3.14) (type-of \"hi\")\n")
      (insert "      (type-of '(1 2)) (type-of [1 2]) (type-of t)\n")
      (insert "      (type-of nil) (type-of (make-hash-table)))\n")
      (insert "#+end_src\n\n")
      ;; Type coercion
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (string-to-number \"42\") (string-to-number \"3.14\")\n")
      (insert "      (number-to-string 42) (number-to-string 3.14)\n")
      (insert "      (int-to-string 42) (string-to-int \"42\")\n")
      (insert "      (float 42) (truncate 3.7) (round 3.7))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_string_predicates_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"t\" \"nil\" \"t\" \"nil\" \"t\" \"t\" \"t\" \"t\" 3))) \"#+begin_src emacs-lisp :results value replace\\n(list (stringp \\\"hello\\\") (stringp 42)\\n      (string-empty-p \\\"\\\") (string-empty-p \\\"x\\\")\\n      (string< \\\"a\\\" \\\"b\\\") (string> \\\"z\\\" \\\"a\\\")\\n      (string-prefix-p \\\"Hel\\\" \\\"Hello\\\")\\n      (string-suffix-p \\\"lo\\\" \\\"Hello\\\")\\n      (string-match-p \\\"[0-9]+\\\" \\\"abc123\\\"))\\n#+end_src\\n\\n#+RESULTS:\\n| t | nil | t | nil | t | t | t | t | 3 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((s \\\"  Hello World  \\\"))\\n  (list (string-trim s)\\n        (string-trim-left s)\\n        (string-trim-right s)\\n        (string-pad \\\"hi\\\" 5)\\n        (string-fill \\\"Hello World\\\" 5)))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; String predicates
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (stringp \"hello\") (stringp 42)\n")
      (insert "      (string-empty-p \"\") (string-empty-p \"x\")\n")
      (insert "      (string< \"a\" \"b\") (string> \"z\" \"a\")\n")
      (insert "      (string-prefix-p \"Hel\" \"Hello\")\n")
      (insert "      (string-suffix-p \"lo\" \"Hello\")\n")
      (insert "      (string-match-p \"[0-9]+\" \"abc123\"))\n")
      (insert "#+end_src\n\n")
      ;; String operations
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((s \"  Hello World  \"))\n")
      (insert "  (list (string-trim s)\n")
      (insert "        (string-trim-left s)\n")
      (insert "        (string-trim-right s)\n")
      (insert "        (string-pad \"hi\" 5)\n")
      (insert "        (string-fill \"Hello World\" 5)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_number_predicates_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"t\" \"t\" \"t\" \"t\" \"t\" \"t\" \"t\" \"t\" \"t\" \"t\"))) \"#+begin_src emacs-lisp :results value replace\\n(list (numberp 42) (integerp 42) (floatp 3.14)\\n      (natnump 0) (wholenump 5) (zerop 0)\\n      (plusp 1) (minusp -1) (oddp 3) (evenp 4))\\n#+end_src\\n\\n#+RESULTS:\\n| t | t | t | t | t | t | t | t | t | t |\\n\\n#+begin_src emacs-lisp :results value replace\\n(list (/ 10 3) (% 10 3) (mod 10 3)\\n      (abs -7) (max 1 3 2) (min 5 2 8)\\n      (expt 2 10) (sqrt 144) (log 100 10))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Number predicates
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (numberp 42) (integerp 42) (floatp 3.14)\n")
      (insert "      (natnump 0) (wholenump 5) (zerop 0)\n")
      (insert "      (plusp 1) (minusp -1) (oddp 3) (evenp 4))\n")
      (insert "#+end_src\n\n")
      ;; Arithmetic with edge cases
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (/ 10 3) (% 10 3) (mod 10 3)\n")
      (insert "      (abs -7) (max 1 3 2) (min 5 2 8)\n")
      (insert "      (expt 2 10) (sqrt 144) (log 100 10))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_seq_count_position_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((4 3))) \"#+begin_src emacs-lisp :results value replace\\n(list (seq-count #'evenp '(1 2 3 4 5 6 7 8))\\n      (seq-count (lambda (x) (> x 5)) '(1 2 3 4 5 6 7 8)))\\n#+end_src\\n\\n#+RESULTS:\\n| 4 | 3 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((s '(a b c d e f)))\\n  (list (seq-position s 'c)\\n        (seq-position s 'z)\\n        (seq-index-of s 'e)\\n        (seq-contains-p s 'd)\\n        (seq-contains-p s 'g)))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; seq-count with predicate
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (seq-count #'evenp '(1 2 3 4 5 6 7 8))\n")
      (insert "      (seq-count (lambda (x) (> x 5)) '(1 2 3 4 5 6 7 8)))\n")
      (insert "#+end_src\n\n")
      ;; seq-position and seq-index-of
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((s '(a b c d e f)))\n")
      (insert "  (list (seq-position s 'c)\n")
      (insert "        (seq-position s 'z)\n")
      (insert "        (seq-index-of s 'e)\n")
      (insert "        (seq-contains-p s 'd)\n")
      (insert "        (seq-contains-p s 'g)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_seq_partition_flatten_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3) (4 5 6) (7 8 9))) \"#+begin_src emacs-lisp :results value replace\\n(seq-partition '(1 2 3 4 5 6 7 8 9) 3)\\n#+end_src\\n\\n#+RESULTS:\\n| 1 | 2 | 3 |\\n| 4 | 5 | 6 |\\n| 7 | 8 | 9 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((nested '((1 2) (3 (4 5)) (6))))\\n  (list (apply #'append nested)\\n        (cl-loop for x in nested append x)\\n        (seq-uniq (apply #'append (apply #'append nested)))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; seq-partition
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-partition '(1 2 3 4 5 6 7 8 9) 3)\n")
      (insert "#+end_src\n\n")
      ;; Flatten nested
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((nested '((1 2) (3 (4 5)) (6))))\n")
      (insert "  (list (apply #'append nested)\n")
      (insert "        (cl-loop for x in nested append x)\n")
      (insert "        (seq-uniq (apply #'append (apply #'append nested)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_map_assoc_sort_reverse_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"(3 . 9)\" \"(1 . 1)\" \"(4 . 16)\" \"(1 . 1)\" \"(5 . 25)\" \"(9 . 81)\" \"(2 . 4)\" \"(6 . 36)\") (\"(1 . 1)\" \"(1 . 1)\" \"(2 . 4)\" \"(3 . 9)\" \"(4 . 16)\" \"(5 . 25)\" \"(6 . 36)\" \"(9 . 81)\") (\"(9 . 81)\" \"(6 . 36)\" \"(5 . 25)\" \"(4 . 16)\" \"(3 . 9)\" \"(2 . 4)\" \"(1 . 1)\" \"(1 . 1)\"))) \"#+begin_src emacs-lisp :results value replace\\n(let ((data (mapcar (lambda (x) (cons x (* x x)))\\n                    '(3 1 4 1 5 9 2 6))))\\n  (list data\\n        (sort (copy-sequence data) (lambda (a b) (< (cdr a) (cdr b))))\\n        (reverse (sort (copy-sequence data) (lambda (a b) (< (cdr a) (cdr b)))))))\\n#+end_src\\n\\n#+RESULTS:\\n| (3 . 9)  | (1 . 1)  | (4 . 16) | (1 . 1)  | (5 . 25) | (9 . 81) | (2 . 4)  | (6 . 36) |\\n| (1 . 1)  | (1 . 1)  | (2 . 4)  | (3 . 9)  | (4 . 16) | (5 . 25) | (6 . 36) | (9 . 81) |\\n| (9 . 81) | (6 . 36) | (5 . 25) | (4 . 16) | (3 . 9)  | (2 . 4)  | (1 . 1)  | (1 . 1)  |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((matrix (mapcar (lambda (row)\\n                        (mapcar (lambda (cell) (* cell cell))\\n                                row))\\n                      '((1 2 3) (4 5 6) (7 8 9)))))\\n  (list matrix\\n        (apply #'+ (mapcar (lambda (row) (apply #'+ row)) matrix))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Map + assoc
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((data (mapcar (lambda (x) (cons x (* x x)))\n")
      (insert "                    '(3 1 4 1 5 9 2 6))))\n")
      (insert "  (list data\n")
      (insert "        (sort (copy-sequence data) (lambda (a b) (< (cdr a) (cdr b))))\n")
      (insert "        (reverse (sort (copy-sequence data) (lambda (a b) (< (cdr a) (cdr b)))))))\n")
      (insert "#+end_src\n\n")
      ;; Nested mapcar with lambda
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((matrix (mapcar (lambda (row)\n")
      (insert "                        (mapcar (lambda (cell) (* cell cell))\n")
      (insert "                                row))\n")
      (insert "                      '((1 2 3) (4 5 6) (7 8 9)))))\n")
      (insert "  (list matrix\n")
      (insert "        (apply #'+ (mapcar (lambda (row) (apply #'+ row)) matrix))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_string_replace_concat_length_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Hello Elisp Foo Bar\" \"XXXXX XXXXX XXX XXX\" 19))) \"#+begin_src emacs-lisp :results value replace\\n(let ((s \\\"Hello World Foo Bar\\\"))\\n  (list (replace-regexp-in-string \\\"World\\\" \\\"Elisp\\\" s)\\n        (replace-regexp-in-string \\\"[A-Z]\\\" \\\"X\\\" s)\\n        (length s)))\\n#+end_src\\n\\n#+RESULTS:\\n| Hello Elisp Foo Bar | XXXXX XXXXX XXX XXX | 19 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(list (concat \\\"a\\\" \\\"-\\\" \\\"b\\\" \\\"-\\\" \\\"c\\\")\\n      (format \\\"%05d\\\" 42)\\n      (format \\\"%.3f\\\" 3.14159)\\n      (format \\\"%-10s|\\\" \\\"left\\\"))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; String replace
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((s \"Hello World Foo Bar\"))\n")
      (insert "  (list (replace-regexp-in-string \"World\" \"Elisp\" s)\n")
      (insert "        (replace-regexp-in-string \"[A-Z]\" \"X\" s)\n")
      (insert "        (length s)))\n")
      (insert "#+end_src\n\n")
      ;; String concat and format
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (concat \"a\" \"-\" \"b\" \"-\" \"c\")\n")
      (insert "      (format \"%05d\" 42)\n")
      (insert "      (format \"%.3f\" 3.14159)\n")
      (insert "      (format \"%-10s|\" \"left\"))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_var_header_chain_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"((a . 5) (b . 10) (c . 15))\" ((\"a\" 5 10) (\"b\" 10 20) (\"c\" 15 30)) ((\":count\" 3 \":sum\" 30 \":min\" 5 \":max\" 15))) \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'((a . 5) (b . 10) (c . 15))\\n#+end_src\\n\\n#+RESULTS: seed\\n: ((a . 5) (b . 10) (c . 15))\\n\\n#+NAME: xform\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(mapcar (lambda (pair)\\n          (cons (car pair) (list (cdr pair) (* (cdr pair) 2))))\\n        data)\\n#+end_src\\n\\n#+RESULTS: xform\\n| a |  5 | 10 |\\n| b | 10 | 20 |\\n| c | 15 | 30 |\\n\\n#+NAME: agg\\n#+begin_src emacs-lisp :var data=xform :results value replace\\n(let ((vals (mapcar #'cadr data)))\\n  (list :count (length vals)\\n        :sum (apply #'+ vals)\\n        :min (apply #'min vals)\\n        :max (apply #'max vals)))\\n#+end_src\\n\\n#+RESULTS: agg\\n| :count | 3 | :sum | 30 | :min | 5 | :max | 15 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed block
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a . 5) (b . 10) (c . 15))\n")
      (insert "#+end_src\n\n")
      ;; Transform with default var
      (insert "#+NAME: xform\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(mapcar (lambda (pair)\n")
      (insert "          (cons (car pair) (list (cdr pair) (* (cdr pair) 2))))\n")
      (insert "        data)\n")
      (insert "#+end_src\n\n")
      ;; Aggregate
      (insert "#+NAME: agg\n")
      (insert "#+begin_src emacs-lisp :var data=xform :results value replace\n")
      (insert "(let ((vals (mapcar #'cadr data)))\n")
      (insert "  (list :count (length vals)\n")
      (insert "        :sum (apply #'+ vals)\n")
      (insert "        :min (apply #'min vals)\n")
      (insert "        :max (apply #'max vals)))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("seed" "xform" "agg"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_row_col_access_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":names\" \"(Ada Bob Cal)\" \":math-avg\" 86 \":sci-avg\" 89 \":best-math\" 95 \":best-sci\" 92))) ((\"Name\" \"Math\" \"Sci\") hline (\"Ada\" \"95\" \"90\") (\"Bob\" \"87\" \"92\") (\"Cal\" \"78\" \"85\")) \"#+NAME: scores\\n| Name | Math | Sci |\\n|------+-----------|\\n| Ada | 95 | 90 |\\n| Bob | 87 | 92 |\\n| Cal | 78 | 85 |\\n\\n#+NAME: analyze\\n#+begin_src emacs-lisp :var tbl=scores :results value replace\\n(let ((names (mapcar #'car tbl))\\n      (math (mapcar #'cadr tbl))\\n      (sci (mapcar #'caddr tbl)))\\n  (list :names names\\n        :math-avg (/ (apply #'+ math) (length math))\\n        :sci-avg (/ (apply #'+ sci) (length sci))\\n        :best-math (car (sort (copy-sequence math) #'>))\\n        :best-sci (car (sort (copy-sequence sci) #'>))))\\n#+end_src\\n\\n#+RESULTS: analyze\\n| :names | (Ada Bob Cal) | :math-avg | 86 | :sci-avg | 89 | :best-math | 95 | :best-sci | 92 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Table as data
      (insert "#+NAME: scores\n")
      (insert "| Name | Math | Sci |\n")
      (insert "|------+-----------|\n")
      (insert "| Ada | 95 | 90 |\n")
      (insert "| Bob | 87 | 92 |\n")
      (insert "| Cal | 78 | 85 |\n\n")
      ;; Row access and column compute
      (insert "#+NAME: analyze\n")
      (insert "#+begin_src emacs-lisp :var tbl=scores :results value replace\n")
      (insert "(let ((names (mapcar #'car tbl))\n")
      (insert "      (math (mapcar #'cadr tbl))\n")
      (insert "      (sci (mapcar #'caddr tbl)))\n")
      (insert "  (list :names names\n")
      (insert "        :math-avg (/ (apply #'+ math) (length math))\n")
      (insert "        :sci-avg (/ (apply #'+ sci) (length sci))\n")
      (insert "        :best-math (car (sort (copy-sequence math) #'>))\n")
      (insert "        :best-sci (car (sort (copy-sequence sci) #'>))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("analyze"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        ;; Table to lisp
        (goto-char (point-min))
        (search-forward "| Name")
        (let ((table-lisp (org-table-to-lisp)))
          (list (nreverse results)
                table-lisp
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_plist_alist_transform_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"((:a . 1) (:b . 4) (:c . 9) (:d . 16))\") \"#+begin_src emacs-lisp :results value replace\\n(let ((pl (list :a 1 :b 2 :c 3 :d 4)))\\n  (cl-loop for (k v) on pl by #'cddr\\n           collect (cons k (* v v))))\\n#+end_src\\n\\n#+RESULTS:\\n: ((:a . 1) (:b . 4) (:c . 9) (:d . 16))\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((al '((x . 10) (y . 20) (z . 30) (w . 40))))\\n  (cl-loop for (k . v) in al\\n           when (> v 15)\\n           collect (cons k (list v (* v 2)))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Plist transform
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((pl (list :a 1 :b 2 :c 3 :d 4)))\n")
      (insert "  (cl-loop for (k v) on pl by #'cddr\n")
      (insert "           collect (cons k (* v v))))\n")
      (insert "#+end_src\n\n")
      ;; Alist filter + transform
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((al '((x . 10) (y . 20) (z . 30) (w . 40))))\n")
      (insert "  (cl-loop for (k . v) in al\n")
      (insert "           when (> v 15)\n")
      (insert "           collect (cons k (list v (* v 2)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_seq_union_intersection_diff_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5))) \"#+begin_src emacs-lisp :results value replace\\n(seq-uniq (append '(1 2 3) '(2 3 4 5)))\\n#+end_src\\n\\n#+RESULTS:\\n| 1 | 2 | 3 | 4 | 5 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(seq-intersection '(1 2 3 4 5) '(3 4 5 6 7))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(seq-difference '(1 2 3 4 5) '(2 4))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Union
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-uniq (append '(1 2 3) '(2 3 4 5)))\n")
      (insert "#+end_src\n\n")
      ;; Intersection
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-intersection '(1 2 3 4 5) '(3 4 5 6 7))\n")
      (insert "#+end_src\n\n")
      ;; Difference
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-difference '(1 2 3 4 5) '(2 4))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_map_table_reduce_pipeline_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"X\" \"Y\" \"Product\") hline (2 3 6) (4 5 20) (6 7 42)) nil) ((\"X\" \"Y\") hline (\"2\" \"3\") (\"4\" \"5\") (\"6\" \"7\")) \"#+NAME: raw\\n| X | Y |\\n|---+---|\\n| 2 | 3 |\\n| 4 | 5 |\\n| 6 | 7 |\\n\\n#+NAME: products\\n#+begin_src emacs-lisp :var data=raw :results value table replace\\n(cons '(\\\"X\\\" \\\"Y\\\" \\\"Product\\\")\\n      (cons 'hline\\n            (mapcar (lambda (r) (list (car r) (cadr r) (* (car r) (cadr r)))) data)))\\n#+end_src\\n\\n#+RESULTS: products\\n| X | Y | Product |\\n|---+---+---------|\\n| 2 | 3 |       6 |\\n| 4 | 5 |      20 |\\n| 6 | 7 |      42 |\\n\\n#+NAME: total\\n#+begin_src emacs-lisp :var data=products :results value replace\\n#+end_src\\n\\n#+RESULTS: total\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source data as table
      (insert "#+NAME: raw\n")
      (insert "| X | Y |\n|---+---|\n| 2 | 3 |\n| 4 | 5 |\n| 6 | 7 |\n\n")
      ;; Map: compute products
      (insert "#+NAME: products\n")
      (insert "#+begin_src emacs-lisp :var data=raw :results value table replace\n")
      (insert "(cons '(\"X\" \"Y\" \"Product\")\n")
      (insert "      (cons 'hline\n")
      (insert "            (mapcar (lambda (r) (list (car r) (cadr r) (* (car r) (cadr r)))) data)))\n")
      (insert "#+end_src\n\n")
      ;; Reduce: sum products
      (insert "#+NAME: total\n")
      (insert "#+begin_src emacs-lisp :var data=products :results value replace\n")
      (apply #'concat
             (list "(let ((nums (mapcar #'caddr (cdr (memq 'hline data)))))\n"
                   "  (list :count (length nums)\n"
                   "        :total (apply #'+ nums)\n"
                   "        :avg (/ (apply #'+ nums) (length nums))))\n"))
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("products" "total"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        ;; Get table lisp
        (goto-char (point-min))
        (search-forward "| X")
        (let ((raw-lisp (org-table-to-lisp)))
          (list (nreverse results)
                raw-lisp
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_struct_access_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Ada\" 30 \"elisp\" \"t\" \"t\"))) \"#+begin_src emacs-lisp :results value replace\\n(progn\\n  (cl-defstruct person name age lang)\\n  (let ((p (make-person :name \\\"Ada\\\" :age 30 :lang \\\"elisp\\\")))\\n    (list (person-name p) (person-age p) (person-lang p)\\n          (person-p p) (cl-typep p 'person))))\\n#+end_src\\n\\n#+RESULTS:\\n| Ada | 30 | elisp | t | t |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((data (list (cons 'users (list (cons 'name \\\"Ada\\\") (cons 'age 30)))\\n                 (cons 'scores (list 95 87 92))\\n                 (cons 'meta (list (cons 'v 2) (cons 'ts \\\"now\\\"))))))\\n  (list (cdr (assoc 'name (cdr (assoc 'users data))))\\n        (cdr (assoc 'scores data))\\n        (cdr (assoc 'v (cdr (assoc 'meta data))))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Record-like structure via cl-struct
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(progn\n  (cl-defstruct person name age lang)\n")
      (insert "  (let ((p (make-person :name \"Ada\" :age 30 :lang \"elisp\")))\n")
      (insert "    (list (person-name p) (person-age p) (person-lang p)\n")
      (insert "          (person-p p) (cl-typep p 'person))))\n")
      (insert "#+end_src\n\n")
      ;; Nested structure
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((data (list (cons 'users (list (cons 'name \"Ada\") (cons 'age 30)))\n")
      (insert "                 (cons 'scores (list 95 87 92))\n")
      (insert "                 (cons 'meta (list (cons 'v 2) (cons 'ts \"now\"))))))\n")
      (insert "  (list (cdr (assoc 'name (cdr (assoc 'users data))))\n")
      (insert "        (cdr (assoc 'scores data))\n")
      (insert "        (cdr (assoc 'v (cdr (assoc 'meta data))))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_catch_throw_block_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((5) \"#+begin_src emacs-lisp :results value replace\\n(catch 'done\\n  (dotimes (i 10)\\n    (when (= i 5) (throw 'done i))))\\n#+end_src\\n\\n#+RESULTS:\\n: 5\\n\\n#+begin_src emacs-lisp :results value replace\\n(list\\n  (catch 'a (throw 'a 42))\\n  (catch 'b (+ 1 2 3))\\n  (catch 'c (throw 'c (list 'x 'y))))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(catch 'outer\\n  (catch 'inner\\n    (throw 'outer 'escaped))\\n  'not-reached)\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; catch/throw success
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(catch 'done\n  (dotimes (i 10)\n    (when (= i 5) (throw 'done i))))\n")
      (insert "#+end_src\n\n")
      ;; catch/throw with handler
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list\n  (catch 'a (throw 'a 42))\n  (catch 'b (+ 1 2 3))\n  (catch 'c (throw 'c (list 'x 'y))))\n")
      (insert "#+end_src\n\n")
      ;; Nested catch
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(catch 'outer\n  (catch 'inner\n    (throw 'outer 'escaped))\n  'not-reached)\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_cl_loop_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 4 9 16 25))) \"#+begin_src emacs-lisp :results value replace\\n(cl-loop for i from 1 to 5 collect (* i i))\\n#+end_src\\n\\n#+RESULTS:\\n| 1 | 4 | 9 | 16 | 25 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(cl-loop for x in '(10 20 30 40 50) sum x)\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(cl-loop for i from 1 to 10\\n         when (evenp i)\\n         collect i into evens\\n         finally (return (list evens (length evens))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; cl-loop collect
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(cl-loop for i from 1 to 5 collect (* i i))\n")
      (insert "#+end_src\n\n")
      ;; cl-loop with sum
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(cl-loop for x in '(10 20 30 40 50) sum x)\n")
      (insert "#+end_src\n\n")
      ;; cl-loop with when/append
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(cl-loop for i from 1 to 10\n         when (evenp i)\n         collect i into evens\n         finally (return (list evens (length evens))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_type_result_chain_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((60 \"total=60\" ((8 \"TOTAL=60\" \"total=60!\")) \"8\\nTOTAL=60\\ntotal=60!\") \"#+NAME: num\\n#+begin_src emacs-lisp :results value replace\\n(+ 10 20 30)\\n#+end_src\\n\\n#+RESULTS: num\\n: 60\\n\\n#+NAME: str\\n#+begin_src emacs-lisp :var n=num :results value replace\\n(format \\\"total=%d\\\" n)\\n#+end_src\\n\\n#+RESULTS: str\\n: total=60\\n\\n#+NAME: lst\\n#+begin_src emacs-lisp :var s=str :results value replace\\n(list (length s) (upcase s) (concat s \\\"!\\\"))\\n#+end_src\\n\\n#+RESULTS: lst\\n| 8 | TOTAL=60 | total=60! |\\n\\n#+NAME: out\\n#+begin_src emacs-lisp :var data=lst :results output replace\\n(dolist (item data)\\n  (princ (format \\\"%s\\\\n\\\" item)))\\n#+end_src\\n\\n#+RESULTS: out\\n: 8\\n: TOTAL=60\\n: total=60!\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Number block
      (insert "#+NAME: num\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(+ 10 20 30)\n")
      (insert "#+end_src\n\n")
      ;; String block using num
      (insert "#+NAME: str\n")
      (insert "#+begin_src emacs-lisp :var n=num :results value replace\n")
      (insert "(format \"total=%d\" n)\n")
      (insert "#+end_src\n\n")
      ;; List block using str
      (insert "#+NAME: lst\n")
      (insert "#+begin_src emacs-lisp :var s=str :results value replace\n")
      (insert "(list (length s) (upcase s) (concat s \"!\"))\n")
      (insert "#+end_src\n\n")
      ;; Output block using lst
      (insert "#+NAME: out\n")
      (insert "#+begin_src emacs-lisp :var data=lst :results output replace\n")
      (insert "(dolist (item data)\n  (princ (format \"%s\\n\" item)))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("num" "str" "lst" "out"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_list_append_reverse_sort_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5) (3 2 1 \"\" \"\") (1 2 3 \"\" \"\") (1 2 3 \"\" \"\"))) \"#+begin_src emacs-lisp :results value replace\\n(let ((l (list 1 2 3)))\\n  (list (append l '(4 5))\\n        (reverse l)\\n        (append nil l)\\n        (append l nil)))\\n#+end_src\\n\\n#+RESULTS:\\n| 1 | 2 | 3 | 4 | 5 |\\n| 3 | 2 | 1 |   |   |\\n| 1 | 2 | 3 |   |   |\\n| 1 | 2 | 3 |   |   |\\n\\n#+begin_src emacs-lisp :results value replace\\n(list (sort '(3 1 4 1 5 9 2 6) #'<)\\n      (sort '(\\\"banana\\\" \\\"apple\\\" \\\"cherry\\\") #'string<)\\n      (sort '((b . 2) (a . 1) (c . 3))\\n            (lambda (x y) (< (cdr x) (cdr y))))))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(let* ((a '(1 2 3))\\n       (b '(4 5 6))\\n       (c (append a b)))\\n  (list (length c)\\n        (nth 3 c)\\n        (last c 2)\\n        (butlast c 2)\\n        (subseq c 1 4)))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; List append and reverse
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((l (list 1 2 3)))\n")
      (insert "  (list (append l '(4 5))\n")
      (insert "        (reverse l)\n")
      (insert "        (append nil l)\n")
      (insert "        (append l nil)))\n")
      (insert "#+end_src\n\n")
      ;; Sort with predicate
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (sort '(3 1 4 1 5 9 2 6) #'<)\n")
      (insert "      (sort '(\"banana\" \"apple\" \"cherry\") #'string<)\n")
      (insert "      (sort '((b . 2) (a . 1) (c . 3))\n")
      (insert "            (lambda (x y) (< (cdr x) (cdr y))))))\n")
      (insert "#+end_src\n\n")
      ;; Nested list operations
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let* ((a '(1 2 3))\n")
      (insert "       (b '(4 5 6))\n")
      (insert "       (c (append a b)))\n")
      (insert "  (list (length c)\n")
      (insert "        (nth 3 c)\n")
      (insert "        (last c 2)\n")
      (insert "        (butlast c 2)\n")
      (insert "        (subseq c 1 4)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_condition_case_unwind_protect_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((3) \"#+begin_src emacs-lisp :results value replace\\n(condition-case err\\n    (+ 1 2)\\n  (error (list :err (cdr err))))\\n#+end_src\\n\\n#+RESULTS:\\n: 3\\n\\n#+begin_src emacs-lisp :results value replace\\n(condition-case err\\n    (/ 1 0)\\n  (error (list :caught t :msg (error-message-string err))))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((result nil) (cleanup nil))\\n  (unwind-protect\\n      (progn (setq result 42) result)\\n    (setq cleanup t))\\n  (list result cleanup))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; condition-case success
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(condition-case err\n    (+ 1 2)\n  (error (list :err (cdr err))))\n")
      (insert "#+end_src\n\n")
      ;; condition-case error
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(condition-case err\n    (/ 1 0)\n  (error (list :caught t :msg (error-message-string err))))\n")
      (insert "#+end_src\n\n")
      ;; unwind-protect
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((result nil) (cleanup nil))\n")
      (insert "  (unwind-protect\n")
      (insert "      (progn (setq result 42) result)\n")
      (insert "    (setq cleanup t))\n")
      (insert "  (list result cleanup))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multi_block_dependency_chain_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 1) (2 4) (3 9) (4 16) (5 25)) \"((1 . 101) (2 . 104) (3 . 109) (4 . 116) (5 . 125))\" \"(:count 5 :first (1 . 101) :last (5 . 125) :total 555)\") \"#+NAME: gen\\n#+begin_src emacs-lisp :results value replace\\n(mapcar (lambda (i) (list i (* i i))) (number-sequence 1 5))\\n#+end_src\\n\\n#+RESULTS: gen\\n| 1 |  1 |\\n| 2 |  4 |\\n| 3 |  9 |\\n| 4 | 16 |\\n| 5 | 25 |\\n\\n#+NAME: xform\\n#+begin_src emacs-lisp :var data=gen :results value replace\\n(mapcar (lambda (r) (cons (car r) (+ (cadr r) 100))) data)\\n#+end_src\\n\\n#+RESULTS: xform\\n: ((1 . 101) (2 . 104) (3 . 109) (4 . 116) (5 . 125))\\n\\n#+NAME: summary\\n#+begin_src emacs-lisp :var data=xform :results value replace\\n(list :count (length data)\\n      :first (car data)\\n      :last (car (last data))\\n      :total (apply #'+ (mapcar #'cdr data)))\\n#+end_src\\n\\n#+RESULTS: summary\\n: (:count 5 :first (1 . 101) :last (5 . 125) :total 555)\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Block A: generates list
      (insert "#+NAME: gen\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(mapcar (lambda (i) (list i (* i i))) (number-sequence 1 5))\n")
      (insert "#+end_src\n\n")
      ;; Block B: transforms gen result
      (insert "#+NAME: xform\n")
      (insert "#+begin_src emacs-lisp :var data=gen :results value replace\n")
      (insert "(mapcar (lambda (r) (cons (car r) (+ (cadr r) 100))) data)\n")
      (insert "#+end_src\n\n")
      ;; Block C: summarizes xform
      (insert "#+NAME: summary\n")
      (insert "#+begin_src emacs-lisp :var data=xform :results value replace\n")
      (insert "(list :count (length data)\n")
      (insert "      :first (car data)\n")
      (insert "      :last (car (last data))\n")
      (insert "      :total (apply #'+ (mapcar #'cdr data)))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("gen" "xform" "summary"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read all results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_seq_sort_group_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5 6 7 8 9))) \"#+begin_src emacs-lisp :results value replace\\n(seq-sort #'< '(5 2 8 1 9 3 7 4 6))\\n#+end_src\\n\\n#+RESULTS:\\n| 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 | 9 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(seq-group-by #'evenp '(1 2 3 4 5 6 7 8 9 10))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((s '(1 2 3 4 5 6 7 8 9 10)))\\n  (list (seq-take s 3) (seq-drop s 7)\\n        (seq-take-while #'< (list 1 2 3 0 4 5))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; seq-sort
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-sort #'< '(5 2 8 1 9 3 7 4 6))\n")
      (insert "#+end_src\n\n")
      ;; seq-group-by
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-group-by #'evenp '(1 2 3 4 5 6 7 8 9 10))\n")
      (insert "#+end_src\n\n")
      ;; seq-take seq-drop seq-take-while
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((s '(1 2 3 4 5 6 7 8 9 10)))\n")
      (insert "  (list (seq-take s 3) (seq-drop s 7)\n")
      (insert "        (seq-take-while #'< (list 1 2 3 0 4 5))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_hash_table_operations_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"Ada\" 30 \"default\" 3 \"nil\" 2))) \"#+begin_src emacs-lisp :results value replace\\n(let ((ht (make-hash-table :test #'equal)))\\n  (puthash \\\"name\\\" \\\"Ada\\\" ht)\\n  (puthash \\\"age\\\" 30 ht)\\n  (puthash \\\"lang\\\" \\\"elisp\\\" ht)\\n  (list (gethash \\\"name\\\" ht)\\n        (gethash \\\"age\\\" ht)\\n        (gethash \\\"missing\\\" ht \\\"default\\\")\\n        (hash-table-count ht)\\n        (remhash \\\"age\\\" ht)\\n        (hash-table-count ht)))\\n#+end_src\\n\\n#+RESULTS:\\n| Ada | 30 | default | 3 | nil | 2 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((ht (make-hash-table)))\\n  (puthash 'x 10 ht) (puthash 'y 20 ht) (puthash 'z 30 ht)\\n  (let ((al nil))\\n    (maphash (lambda (k v) (push (cons k v) al)) ht)\\n    (sort al (lambda (a b) (string< (symbol-name (car a))\\n                                      (symbol-name (car b))))))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Create and populate hash
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((ht (make-hash-table :test #'equal)))\n")
      (insert "  (puthash \"name\" \"Ada\" ht)\n")
      (insert "  (puthash \"age\" 30 ht)\n")
      (insert "  (puthash \"lang\" \"elisp\" ht)\n")
      (insert "  (list (gethash \"name\" ht)\n")
      (insert "        (gethash \"age\" ht)\n")
      (insert "        (gethash \"missing\" ht \"default\")\n")
      (insert "        (hash-table-count ht)\n")
      (insert "        (remhash \"age\" ht)\n")
      (insert "        (hash-table-count ht)))\n")
      (insert "#+end_src\n\n")
      ;; Hash to alist
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((ht (make-hash-table)))\n")
      (insert "  (puthash 'x 10 ht) (puthash 'y 20 ht) (puthash 'z 30 ht)\n")
      (insert "  (let ((al nil))\n")
      (insert "    (maphash (lambda (k v) (push (cons k v) al)) ht)\n")
      (insert "    (sort al (lambda (a b) (string< (symbol-name (car a))\n")
      (insert "                                      (symbol-name (car b))))))))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_property_list_alist_conversion_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"((:a . 1) (:b . 2) (:c . 3))\" 2 \"(:c 3)\"))) \"#+begin_src emacs-lisp :results value replace\\n(let ((pl (list :a 1 :b 2 :c 3)))\\n  (list (cl-loop for (k v) on pl by #'cddr collect (cons k v))\\n        (plist-get pl :b)\\n        (plist-member pl :c)))\\n#+end_src\\n\\n#+RESULTS:\\n| ((:a . 1) (:b . 2) (:c . 3)) | 2 | (:c 3) |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((al '((x . 10) (y . 20) (z . 30))))\\n  (list (apply #'append al)\\n        (cdr (assq 'y al))\\n        (assoc 'w al)))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Plist to alist
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((pl (list :a 1 :b 2 :c 3)))\n")
      (insert "  (list (cl-loop for (k v) on pl by #'cddr collect (cons k v))\n")
      (insert "        (plist-get pl :b)\n")
      (insert "        (plist-member pl :c)))\n")
      (insert "#+end_src\n\n")
      ;; Alist to plist
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((al '((x . 10) (y . 20) (z . 30))))\n")
      (insert "  (list (apply #'append al)\n")
      (insert "        (cdr (assq 'y al))\n")
      (insert "        (assoc 'w al)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_string_split_join_match_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"hello\" \"world\" \"foo\" \"bar\"))) \"#+begin_src emacs-lisp :results value replace\\n(split-string \\\"hello world foo bar\\\" \\\" \\\")\\n#+end_src\\n\\n#+RESULTS:\\n| hello | world | foo | bar |\\n\\n#+begin_src emacs-lisp :results value replace\\n(mapconcat #'upcase '(\\\"hello\\\" \\\"world\\\") \\\"-\\\")\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(list (string-match \\\"[0-9]+\\\" \\\"abc123def456\\\")\\n      (match-string 0 \\\"abc123def456\\\")\\n      (replace-regexp-in-string \\\"[0-9]+\\\" \\\"#\\\" \\\"a1b2c3\\\"))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; String split
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(split-string \"hello world foo bar\" \" \")\n")
      (insert "#+end_src\n\n")
      ;; String join
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(mapconcat #'upcase '(\"hello\" \"world\") \"-\")\n")
      (insert "#+end_src\n\n")
      ;; String match
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (string-match \"[0-9]+\" \"abc123def456\")\n")
      (insert "      (match-string 0 \"abc123def456\")\n")
      (insert "      (replace-regexp-in-string \"[0-9]+\" \"#\" \"a1b2c3\"))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_setq_let_star_dolist_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((10 20 30))) \"#+begin_src emacs-lisp :results value replace\\n(progn (setq a 10 b 20 c 30) (list a b c))\\n#+end_src\\n\\n#+RESULTS:\\n| 10 | 20 | 30 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let* ((x 5) (y (* x 2)) (z (+ x y)))\\n  (list x y z))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((acc nil))\\n  (dolist (item '(a b c d e))\\n    (push (symbol-name item) acc))\\n  (nreverse acc))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; setq
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(progn (setq a 10 b 20 c 30) (list a b c))\n")
      (insert "#+end_src\n\n")
      ;; let*
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let* ((x 5) (y (* x 2)) (z (+ x y)))\n  (list x y z))\n")
      (insert "#+end_src\n\n")
      ;; dolist accumulation
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((acc nil))\n  (dolist (item '(a b c d e))\n    (push (symbol-name item) acc))\n  (nreverse acc))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_char_alist_plist_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"A\" 66 67 \"α\"))) \"#+begin_src emacs-lisp :results value replace\\n(list (char-to-string 65) (string-to-char \\\"B\\\") ?C (char-to-string 945))\\n#+end_src\\n\\n#+RESULTS:\\n| A | 66 | 67 | α |\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((al '((x . 10) (y . 20) (z . 30))))\\n  (list (assq 'y al) (rassoc 30 al) (length al)))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((pl (list :name \\\"Ada\\\" :age 30 :lang \\\"elisp\\\")))\\n  (list (plist-get pl :name) (plist-get pl :age) (plist-member pl :lang)))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Character operations
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (char-to-string 65) (string-to-char \"B\") ?C (char-to-string 945))\n")
      (insert "#+end_src\n\n")
      ;; Alist
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((al '((x . 10) (y . 20) (z . 30))))\n")
      (insert "  (list (assq 'y al) (rassoc 30 al) (length al)))\n")
      (insert "#+end_src\n\n")
      ;; Plist
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((pl (list :name \"Ada\" :age 30 :lang \"elisp\")))\n")
      (insert "  (list (plist-get pl :name) (plist-get pl :age) (plist-member pl :lang)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_recursive_fib_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((0 1 1 2 3 5 8 13 21 34 55)) ((1 1 2 6 24 120 720 5040 40320))) \"#+NAME: fib\\n#+begin_src emacs-lisp :results value replace\\n(progn\\n  (defun fib (n)\\n    (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))\\n  (mapcar #'fib '(0 1 2 3 4 5 6 7 8 9 10)))\\n#+end_src\\n\\n#+RESULTS: fib\\n| 0 | 1 | 1 | 2 | 3 | 5 | 8 | 13 | 21 | 34 | 55 |\\n\\n#+NAME: fact\\n#+begin_src emacs-lisp :results value replace\\n(progn\\n  (defun fact (n) (if (<= n 1) 1 (* n (fact (1- n)))))\\n  (mapcar #'fact '(0 1 2 3 4 5 6 7 8)))\\n#+end_src\\n\\n#+RESULTS: fact\\n| 1 | 1 | 2 | 6 | 24 | 120 | 720 | 5040 | 40320 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Recursive fibonacci
      (insert "#+NAME: fib\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(progn\n  (defun fib (n)\n    (if (<= n 1) n (+ (fib (- n 1)) (fib (- n 2)))))\n  (mapcar #'fib '(0 1 2 3 4 5 6 7 8 9 10)))\n")
      (insert "#+end_src\n\n")
      ;; Recursive factorial
      (insert "#+NAME: fact\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(progn\n  (defun fact (n) (if (<= n 1) 1 (* n (fact (1- n)))))\n  (mapcar #'fact '(0 1 2 3 4 5 6 7 8)))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("fib" "fact"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_type_coercion_boundary_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":zero\" 0 \":neg\" -1 \":float\" 3.14 \":big\" 9999999999))) \"#+begin_src emacs-lisp :results value replace\\n(list :zero 0 :neg -1 :float 3.14 :big 9999999999)\\n#+end_src\\n\\n#+RESULTS:\\n| :zero | 0 | :neg | -1 | :float | 3.14 | :big | 9999999999 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(list :nil nil :empty-list '() :empty-str \\\"\\\" :empty-vec [])\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(list (cons 'a nil) (cons nil 'b) (list nil nil nil))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Zero
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list :zero 0 :neg -1 :float 3.14 :big 9999999999)\n")
      (insert "#+end_src\n\n")
      ;; Empty structures
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list :nil nil :empty-list '() :empty-str \"\" :empty-vec [])\n")
      (insert "#+end_src\n\n")
      ;; Nested nil
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (cons 'a nil) (cons nil 'b) (list nil nil nil))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_mapcar_filter_reduce_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 4 9 16 25))) \"#+begin_src emacs-lisp :results value replace\\n(mapcar (lambda (x) (* x x)) '(1 2 3 4 5))\\n#+end_src\\n\\n#+RESULTS:\\n| 1 | 4 | 9 | 16 | 25 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(seq-filter (lambda (x) (> x 10)) '(5 12 8 20 3 15))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(seq-reduce #'+ '(1 2 3 4 5) 0)\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(let* ((data '(1 2 3 4 5 6 7 8 9 10))\\n       (evens (seq-filter #'evenp data))\\n       (squares (mapcar (lambda (x) (* x x)) evens))\\n       (total (seq-reduce #'+ squares 0)))\\n  (list :evens evens :squares squares :total total))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; mapcar
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(mapcar (lambda (x) (* x x)) '(1 2 3 4 5))\n")
      (insert "#+end_src\n\n")
      ;; filter (remove-if-not)
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-filter (lambda (x) (> x 10)) '(5 12 8 20 3 15))\n")
      (insert "#+end_src\n\n")
      ;; reduce
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(seq-reduce #'+ '(1 2 3 4 5) 0)\n")
      (insert "#+end_src\n\n")
      ;; Combined
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let* ((data '(1 2 3 4 5 6 7 8 9 10))\n")
      (insert "       (evens (seq-filter #'evenp data))\n")
      (insert "       (squares (mapcar (lambda (x) (* x x)) evens))\n")
      (insert "       (total (seq-reduce #'+ squares 0)))\n")
      (insert "  (list :evens evens :squares squares :total total))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 4)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_file_output_tangle_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    crate::common::assert_oracle_parity(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (require 'ob-tangle)
  (let* ((root (make-temp-file "org-babel-file" t))
         (out-el (expand-file-name "out.el" root))
         (out-txt (expand-file-name "out.txt" root))
         (org-confirm-babel-evaluate nil))
    (unwind-protect
        (with-temp-buffer
          (org-mode)
          ;; File output block
          (insert "#+begin_src emacs-lisp :file " out-txt " :results file\n")
          (insert "(with-temp-file \"" out-txt "\"\n")
          (insert "  (insert \"hello from babel\"))\n")
          (insert "\"" out-txt "\")\n")
          (insert "#+end_src\n\n")
          ;; Tangle block
          (insert "#+begin_src emacs-lisp :tangle " out-el "\n")
          (insert "(defun tangled-func () 42)\n")
          (insert "#+end_src\n\n")
          ;; Execute file block
          (goto-char (point-min))
          (search-forward "begin_src")
          (org-babel-execute-src-block)
          (let ((file-result (org-babel-read-result)))
            ;; Tangle
            (let ((tangle-result (org-babel-tangle)))
              ;; Read outputs
              (let ((txt-content
                     (when (file-exists-p out-txt)
                       (with-temp-buffer
                         (insert-file-contents out-txt)
                         (buffer-string))))
                    (el-content
                     (when (file-exists-p out-el)
                       (with-temp-buffer
                         (insert-file-contents out-el)
                         (buffer-string)))))
                (list (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (or file-result "nil"))
                      (mapcar (lambda (f)
                                (replace-regexp-in-string
                                 (regexp-quote root) "<root>" f))
                              tangle-result)
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (or txt-content "no-txt"))
                      (replace-regexp-in-string
                       (regexp-quote root) "<root>"
                       (or el-content "no-el"))
                      (buffer-substring-no-properties
                       (point-min) (point-max))))))))
      (delete-directory root t)))"##,
    );
}

#[test]
fn org_babel_execute_string_list_table_mixed_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"Hello World\" ((11 \"HELLO WORLD\" \"hello world\")) ((\"Metric\" \"Value\") hline (\"integer\" 11) (\"string\" \"HELLO WORLD\") (\"string\" \"hello world\"))) \"#+NAME: str\\n#+begin_src emacs-lisp :results value replace\\n(concat \\\"Hello\\\" \\\" \\\" \\\"World\\\")\\n#+end_src\\n\\n#+RESULTS: str\\n: Hello World\\n\\n#+NAME: lst\\n#+begin_src emacs-lisp :var s=str :results value replace\\n(list (length s) (upcase s) (downcase s))\\n#+end_src\\n\\n#+RESULTS: lst\\n| 11 | HELLO WORLD | hello world |\\n\\n#+NAME: tbl\\n#+begin_src emacs-lisp :var data=lst :results value table replace\\n(cons '(\\\"Metric\\\" \\\"Value\\\")\\n      (cons 'hline\\n            (mapcar (lambda (v) (list (type-of v) v)) data)))\\n#+end_src\\n\\n#+RESULTS: tbl\\n| Metric  | Value       |\\n|---------+-------------|\\n| integer | 11          |\\n| string  | HELLO WORLD |\\n| string  | hello world |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; String result
      (insert "#+NAME: str\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(concat \"Hello\" \" \" \"World\")\n")
      (insert "#+end_src\n\n")
      ;; List result using named ref
      (insert "#+NAME: lst\n")
      (insert "#+begin_src emacs-lisp :var s=str :results value replace\n")
      (insert "(list (length s) (upcase s) (downcase s))\n")
      (insert "#+end_src\n\n")
      ;; Table result using named ref
      (insert "#+NAME: tbl\n")
      (insert "#+begin_src emacs-lisp :var data=lst :results value table replace\n")
      (insert "(cons '(\"Metric\" \"Value\")\n")
      (insert "      (cons 'hline\n")
      (insert "            (mapcar (lambda (v) (list (type-of v) v)) data)))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("str" "lst" "tbl"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_let_lambda_defun_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((30) \"#+begin_src emacs-lisp :results value replace\\n(let ((a 10) (b 20))\\n  (+ a b))\\n#+end_src\\n\\n#+RESULTS:\\n: 30\\n\\n#+begin_src emacs-lisp :results value replace\\n(funcall (lambda (x y) (+ (* x x) (* y y))) 3 4)\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(progn\\n  (defun factorial (n)\\n    (if (<= n 1) 1 (* n (factorial (1- n)))))\\n  (factorial 10))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; let binding
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((a 10) (b 20))\n  (+ a b))\n")
      (insert "#+end_src\n\n")
      ;; lambda
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(funcall (lambda (x y) (+ (* x x) (* y y))) 3 4)\n")
      (insert "#+end_src\n\n")
      ;; defun + call
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(progn\n  (defun factorial (n)\n    (if (<= n 1) 1 (* n (factorial (1- n)))))\n  (factorial 10))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_cond_assoc_lookup_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"medium\") \"#+begin_src emacs-lisp :results value replace\\n(let ((x 5))\\n  (cond ((< x 3) 'small)\\n        ((< x 10) 'medium)\\n        (t 'large)))\\n#+end_src\\n\\n#+RESULTS:\\n: medium\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((table '((a . 1) (b . 2) (c . 3))))\\n  (list (cdr (assoc 'b table))\\n        (assoc 'd table)\\n        (assq 'a table)))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((acc 0))\\n  (dotimes (i 10) (setq acc (+ acc i)))\\n  acc)\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Conditional
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((x 5))\n  (cond ((< x 3) 'small)\n        ((< x 10) 'medium)\n        (t 'large)))\n")
      (insert "#+end_src\n\n")
      ;; Assoc lookup
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((table '((a . 1) (b . 2) (c . 3))))\n  (list (cdr (assoc 'b table))\n        (assoc 'd table)\n        (assq 'a table)))\n")
      (insert "#+end_src\n\n")
      ;; Loop construct
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((acc 0))\n  (dotimes (i 10) (setq acc (+ acc i)))\n  acc)\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_arithmetic_comparison_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":add\" 6 \":mul\" 20 \":div\" 3 \":mod\" 1))) \"#+begin_src emacs-lisp :results value replace\\n(list :add (+ 1 2 3) :mul (* 4 5) :div (/ 10 3) :mod (% 10 3))\\n#+end_src\\n\\n#+RESULTS:\\n| :add | 6 | :mul | 20 | :div | 3 | :mod | 1 |\\n\\n#+begin_src emacs-lisp :results value replace\\n(list (< 1 2 3) (> 3 2 1) (= 5 5) (<= 3 3) (>= 4 3))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(list (abs -5) (max 1 3 2) (min 5 2 8) (expt 2 10) (sqrt 144))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Arithmetic
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list :add (+ 1 2 3) :mul (* 4 5) :div (/ 10 3) :mod (% 10 3))\n")
      (insert "#+end_src\n\n")
      ;; Comparison
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (< 1 2 3) (> 3 2 1) (= 5 5) (<= 3 3) (>= 4 3))\n")
      (insert "#+end_src\n\n")
      ;; Math functions
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list (abs -5) (max 1 3 2) (min 5 2 8) (expt 2 10) (sqrt 144))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_string_concat_format_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"Hello World !\") \"#+begin_src emacs-lisp :results value replace\\n(concat \\\"Hello\\\" \\\" \\\" \\\"World\\\" \\\" \\\" \\\"!\\\")\\n#+end_src\\n\\n#+RESULTS:\\n: Hello World !\\n\\n#+begin_src emacs-lisp :results value replace\\n(format \\\"name=%s age=%d score=%.2f\\\" \\\"Ada\\\" 30 95.678)\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((s \\\"Hello World\\\"))\\n  (list (upcase s) (downcase s) (length s) (substring s 0 5)))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; String concatenation
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(concat \"Hello\" \" \" \"World\" \" \" \"!\")\n")
      (insert "#+end_src\n\n")
      ;; Format with multiple args
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(format \"name=%s age=%d score=%.2f\" \"Ada\" 30 95.678)\n")
      (insert "#+end_src\n\n")
      ;; String operations
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((s \"Hello World\"))\n")
      (insert "  (list (upcase s) (downcase s) (length s) (substring s 0 5)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 3)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_map_accumulate_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5)) ((1 1 1) (2 4 8) (3 9 27) (4 16 64) (5 25 125)) ((\":total-squares\" 55 \":total-cubes\" 225 \":count\" 5)) \"squares=55 cubes=225 n=5\") \"#+NAME: seed\\n#+begin_src emacs-lisp :results value replace\\n'(1 2 3 4 5)\\n#+end_src\\n\\n#+RESULTS: seed\\n| 1 | 2 | 3 | 4 | 5 |\\n\\n#+NAME: mapper\\n#+begin_src emacs-lisp :var data=seed :results value replace\\n(mapcar (lambda (x) (list x (* x x) (* x x x))) data)\\n#+end_src\\n\\n#+RESULTS: mapper\\n| 1 |  1 |   1 |\\n| 2 |  4 |   8 |\\n| 3 |  9 |  27 |\\n| 4 | 16 |  64 |\\n| 5 | 25 | 125 |\\n\\n#+NAME: accumulator\\n#+begin_src emacs-lisp :var data=mapper :results value replace\\n(list :total-squares (apply #'+ (mapcar #'cadr data))\\n      :total-cubes (apply #'+ (mapcar #'caddr data))\\n      :count (length data))\\n#+end_src\\n\\n#+RESULTS: accumulator\\n| :total-squares | 55 | :total-cubes | 225 | :count | 5 |\\n\\n#+NAME: displayer\\n#+begin_src emacs-lisp :var acc=accumulator :results output replace\\n(princ (format \\\"squares=%d cubes=%d n=%d\\\"\\n               (plist-get acc :total-squares)\\n               (plist-get acc :total-cubes)\\n               (plist-get acc :count)))\\n#+end_src\\n\\n#+RESULTS: displayer\\n: squares=55 cubes=225 n=5\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Seed block
      (insert "#+NAME: seed\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(1 2 3 4 5)\n")
      (insert "#+end_src\n\n")
      ;; Map block
      (insert "#+NAME: mapper\n")
      (insert "#+begin_src emacs-lisp :var data=seed :results value replace\n")
      (insert "(mapcar (lambda (x) (list x (* x x) (* x x x))) data)\n")
      (insert "#+end_src\n\n")
      ;; Accumulate block
      (insert "#+NAME: accumulator\n")
      (insert "#+begin_src emacs-lisp :var data=mapper :results value replace\n")
      (insert "(list :total-squares (apply #'+ (mapcar #'cadr data))\n")
      (insert "      :total-cubes (apply #'+ (mapcar #'caddr data))\n")
      (insert "      :count (length data))\n")
      (insert "#+end_src\n\n")
      ;; Output block
      (insert "#+NAME: displayer\n")
      (insert "#+begin_src emacs-lisp :var acc=accumulator :results output replace\n")
      (insert "(princ (format \"squares=%d cubes=%d n=%d\"\n")
      (insert "               (plist-get acc :total-squares)\n")
      (insert "               (plist-get acc :total-cubes)\n")
      (insert "               (plist-get acc :count)))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("seed" "mapper" "accumulator" "displayer"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_complex_list_structure_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\"users\" \"(name . Ada)\" \"(age . 30)\" \"\" \"\") (\"scores\" 95 87 92 88) (\"meta\" \"(created . 2026-05-27)\" \"(version . 2)\" \"\" \"\"))) \"#+begin_src emacs-lisp :results value replace\\n'((users . ((name . \\\"Ada\\\") (age . 30)))\\n  (scores . (95 87 92 88))\\n  (meta . ((created . \\\"2026-05-27\\\") (version . 2))))\\n#+end_src\\n\\n#+RESULTS:\\n| users  | (name . Ada)           | (age . 30)    |    |    |\\n| scores | 95                     | 87            | 92 | 88 |\\n| meta   | (created . 2026-05-27) | (version . 2) |    |    |\\n\\n#+begin_src emacs-lisp :var data=src_1 :results value replace\\n(list :user-name (cdr (assoc 'name (cdr (assoc 'users data))))\\n      :avg-score (/ (apply #'+ (cdr (assoc 'scores data)))\\n                    (length (cdr (assoc 'scores data))))\\n      :version (cdr (assoc 'version (cdr (assoc 'meta data)))))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Nested structure
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((users . ((name . \"Ada\") (age . 30)))\n")
      (insert "  (scores . (95 87 92 88))\n")
      (insert "  (meta . ((created . \"2026-05-27\") (version . 2))))\n")
      (insert "#+end_src\n\n")
      ;; Process structure
      (insert "#+begin_src emacs-lisp :var data=src_1 :results value replace\n")
      (insert "(list :user-name (cdr (assoc 'name (cdr (assoc 'users data))))\n")
      (insert "      :avg-score (/ (apply #'+ (cdr (assoc 'scores data)))\n")
      (insert "                    (length (cdr (assoc 'scores data))))\n")
      (insert "      :version (cdr (assoc 'version (cdr (assoc 'meta data)))))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dotimes (_ 2)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_result_type_boolean_vector_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"t\") \"#+begin_src emacs-lisp :results value replace\\nt\\n#+end_src\\n\\n#+RESULTS:\\n: t\\n\\n#+begin_src emacs-lisp :results value replace\\nnil\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n[1 2 3 4 5]\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n(let ((ht (make-hash-table)))\\n  (puthash 'a 1 ht)\\n  (puthash 'b 2 ht)\\n  ht)\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Boolean true
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "t\n")
      (insert "#+end_src\n\n")
      ;; Boolean false
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "nil\n")
      (insert "#+end_src\n\n")
      ;; Vector
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "[1 2 3 4 5]\n")
      (insert "#+end_src\n\n")
      ;; Hash table
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(let ((ht (make-hash-table)))\n  (puthash 'a 1 ht)\n  (puthash 'b 2 ht)\n  ht)\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 4)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_block_var_ref_result_order_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"((a . 10) (b . 20) (c . 30))\" \"((a . 20) (b . 40) (c . 60))\" 120) \"#+NAME: producer\\n#+begin_src emacs-lisp :results value replace\\n'((a . 10) (b . 20) (c . 30))\\n#+end_src\\n\\n#+RESULTS: producer\\n: ((a . 10) (b . 20) (c . 30))\\n\\n#+NAME: consumer\\n#+begin_src emacs-lisp :var data=producer :results value replace\\n(mapcar (lambda (pair)\\n          (cons (car pair) (* 2 (cdr pair))))\\n        data)\\n#+end_src\\n\\n#+RESULTS: consumer\\n: ((a . 20) (b . 40) (c . 60))\\n\\n#+NAME: aggregator\\n#+begin_src emacs-lisp :var data=consumer :results value replace\\n(apply #'+ (mapcar #'cdr data))\\n#+end_src\\n\\n#+RESULTS: aggregator\\n: 120\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Producer block
      (insert "#+NAME: producer\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a . 10) (b . 20) (c . 30))\n")
      (insert "#+end_src\n\n")
      ;; Consumer block with var ref
      (insert "#+NAME: consumer\n")
      (insert "#+begin_src emacs-lisp :var data=producer :results value replace\n")
      (insert "(mapcar (lambda (pair)\n")
      (insert "          (cons (car pair) (* 2 (cdr pair))))\n")
      (insert "        data)\n")
      (insert "#+end_src\n\n")
      ;; Aggregator block
      (insert "#+NAME: aggregator\n")
      (insert "#+begin_src emacs-lisp :var data=consumer :results value replace\n")
      (insert "(apply #'+ (mapcar #'cdr data))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("producer" "consumer" "aggregator"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_named_result_header_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":count\" 3 \":sum\" 274 \":avg\" 91 \":max\" 95 \":min\" 87)) \"n=3 sum=274 avg=91 max=95 min=87\") ((\"Name\" \"Score\") hline (\"Alice\" \"95\") (\"Bob\" \"87\") (\"Carol\" \"92\")) \"#+NAME: scores\\n| Name | Score |\\n|------+-------|\\n| Alice | 95 |\\n| Bob | 87 |\\n| Carol | 92 |\\n\\n#+NAME: analysis\\n#+begin_src emacs-lisp :var tbl=scores :results value replace\\n(let ((scores (mapcar #'cadr tbl)))\\n  (list :count (length scores)\\n        :sum (apply #'+ scores)\\n        :avg (/ (apply #'+ scores) (length scores))\\n        :max (apply #'max scores)\\n        :min (apply #'min scores)))\\n#+end_src\\n\\n#+RESULTS: analysis\\n| :count | 3 | :sum | 274 | :avg | 91 | :max | 95 | :min | 87 |\\n\\n#+NAME: report\\n#+begin_src emacs-lisp :var stats=analysis :results output replace\\n(princ (format \\\"n=%d sum=%d avg=%d max=%d min=%d\\\"\\n               (plist-get stats :count)\\n               (plist-get stats :sum)\\n               (plist-get stats :avg)\\n               (plist-get stats :max)\\n               (plist-get stats :min)))\\n#+end_src\\n\\n#+RESULTS: report\\n: n=3 sum=274 avg=91 max=95 min=87\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Named table
      (insert "#+NAME: scores\n")
      (insert "| Name | Score |\n")
      (insert "|------+-------|\n")
      (insert "| Alice | 95 |\n")
      (insert "| Bob | 87 |\n")
      (insert "| Carol | 92 |\n\n")
      ;; Use table as var
      (insert "#+NAME: analysis\n")
      (insert "#+begin_src emacs-lisp :var tbl=scores :results value replace\n")
      (insert "(let ((scores (mapcar #'cadr tbl)))\n")
      (insert "  (list :count (length scores)\n")
      (insert "        :sum (apply #'+ scores)\n")
      (insert "        :avg (/ (apply #'+ scores) (length scores))\n")
      (insert "        :max (apply #'max scores)\n")
      (insert "        :min (apply #'min scores)))\n")
      (insert "#+end_src\n\n")
      ;; Output from analysis
      (insert "#+NAME: report\n")
      (insert "#+begin_src emacs-lisp :var stats=analysis :results output replace\n")
      (insert "(princ (format \"n=%d sum=%d avg=%d max=%d min=%d\"\n")
      (insert "               (plist-get stats :count)\n")
      (insert "               (plist-get stats :sum)\n")
      (insert "               (plist-get stats :avg)\n")
      (insert "               (plist-get stats :max)\n")
      (insert "               (plist-get stats :min)))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("analysis" "report"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        ;; Parse table
        (let ((table-lisp
               (progn
                 (goto-char (point-min))
                 (search-forward "| Name")
                 (org-table-to-lisp))))
          (list (nreverse results)
                table-lisp
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_result_insert_update_replace_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#+NAME: counter\\n#+begin_src emacs-lisp :results value replace\\n(progn (setq neo-oracle-babel-counter (1+ (if (boundp 'neo-oracle-babel-counter) neo-oracle-babel-counter 0))) neo-oracle-babel-counter)\\n#+end_src\\n\\n#+RESULTS: counter\\n: 1\\n\\n\" nil \"#+NAME: counter\\n#+begin_src emacs-lisp :results value replace\\n(progn (setq neo-oracle-babel-counter (1+ (if (boundp 'neo-oracle-babel-counter) neo-oracle-babel-counter 0))) neo-oracle-babel-counter)\\n#+end_src\\n\\n#+RESULTS: counter\\n: 2\\n\\n\" nil \"#+NAME: counter\\n#+begin_src emacs-lisp :results value replace\\n(progn (setq neo-oracle-babel-counter (1+ (if (boundp 'neo-oracle-babel-counter) neo-oracle-babel-counter 0))) neo-oracle-babel-counter)\\n#+end_src\\n\\n\" \"#+NAME: counter\\n#+begin_src emacs-lisp :results value replace\\n(progn (setq neo-oracle-babel-counter (1+ (if (boundp 'neo-oracle-babel-counter) neo-oracle-babel-counter 0))) neo-oracle-babel-counter)\\n#+end_src\\n\\n#+RESULTS: counter\\n: 3\\n\\n\" nil)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Block with placeholder result
      (insert "#+NAME: counter\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(progn (setq neo-oracle-babel-counter (1+ (if (boundp 'neo-oracle-babel-counter) neo-oracle-babel-counter 0))) neo-oracle-babel-counter)\n")
      (insert "#+end_src\n\n")
      (insert "#+RESULTS: counter\n")
      (insert ": placeholder\n\n")
      ;; Execute - should replace placeholder
      (goto-char (point-min))
      (search-forward "counter")
      (org-babel-execute-src-block)
      (let ((after-exec (buffer-substring-no-properties
                         (point-min) (point-max)))
            (result-1 (org-babel-read-result)))
        ;; Execute again - should replace previous result
        (goto-char (point-min))
        (search-forward "counter")
        (org-babel-execute-src-block)
        (let ((after-reexec (buffer-substring-no-properties
                             (point-min) (point-max)))
              (result-2 (org-babel-read-result)))
          ;; Remove result
          (goto-char (point-min))
          (search-forward "counter")
          (org-babel-remove-result)
          (let ((after-remove (buffer-substring-no-properties
                               (point-min) (point-max))))
            ;; Execute again - should create new result
            (goto-char (point-min))
            (search-forward "counter")
            (org-babel-execute-src-block)
            (let ((after-new (buffer-substring-no-properties
                              (point-min) (point-max)))
                  (result-3 (org-babel-read-result)))
              (list after-exec
                    (integerp result-1)
                    after-reexec
                    (integerp result-2)
                    after-remove
                    after-new
                    (integerp result-3)))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_result_type_string_number_list_deep_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((42) \"#+begin_src emacs-lisp :results value replace\\n42\\n#+end_src\\n\\n#+RESULTS:\\n: 42\\n\\n#+begin_src emacs-lisp :results value replace\\n\\\"hello world\\\"\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n'((name . \\\"Ada\\\") (age . 30) (lang . \\\"elisp\\\"))\\n#+end_src\\n\\n#+begin_src emacs-lisp :results value replace\\n'((a 1) (b (2 3)) (c (d 4)))\\n#+end_src\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Number
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "42\n")
      (insert "#+end_src\n\n")
      ;; String
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "\"hello world\"\n")
      (insert "#+end_src\n\n")
      ;; Association list
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((name . \"Ada\") (age . 30) (lang . \"elisp\"))\n")
      (insert "#+end_src\n\n")
      ;; Nested list
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((a 1) (b (2 3)) (c (d 4)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dotimes (_ 4)
        (goto-char (point-min))
        (search-forward "begin_src")
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_nested_var_reference_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((1 2 3 4 5)) ((\":count\" 5 \":sum\" 15 \":avg\" 3)) \"count=5 sum=15 avg=3\") \"#+NAME: numbers\\n#+begin_src emacs-lisp :results value replace\\n'(1 2 3 4 5)\\n#+end_src\\n\\n#+RESULTS: numbers\\n| 1 | 2 | 3 | 4 | 5 |\\n\\n#+NAME: stats\\n#+begin_src emacs-lisp :var nums=numbers :results value replace\\n(list :count (length nums)\\n      :sum (apply #'+ nums)\\n      :avg (/ (apply #'+ nums) (length nums)))\\n#+end_src\\n\\n#+RESULTS: stats\\n| :count | 5 | :sum | 15 | :avg | 3 |\\n\\n#+NAME: display\\n#+begin_src emacs-lisp :var s=stats :results output replace\\n(princ (format \\\"count=%d sum=%d avg=%d\\\"\\n               (plist-get s :count)\\n               (plist-get s :sum)\\n               (plist-get s :avg)))\\n#+end_src\\n\\n#+RESULTS: display\\n: count=5 sum=15 avg=3\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source data
      (insert "#+NAME: numbers\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'(1 2 3 4 5)\n")
      (insert "#+end_src\n\n")
      ;; Compute with var ref
      (insert "#+NAME: stats\n")
      (insert "#+begin_src emacs-lisp :var nums=numbers :results value replace\n")
      (insert "(list :count (length nums)\n")
      (insert "      :sum (apply #'+ nums)\n")
      (insert "      :avg (/ (apply #'+ nums) (length nums)))\n")
      (insert "#+end_src\n\n")
      ;; Format output
      (insert "#+NAME: display\n")
      (insert "#+begin_src emacs-lisp :var s=stats :results output replace\n")
      (insert "(princ (format \"count=%d sum=%d avg=%d\"\n")
      (insert "               (plist-get s :count)\n")
      (insert "               (plist-get s :sum)\n")
      (insert "               (plist-get s :avg)))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("numbers" "stats" "display"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_output_format_string_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"line-0\\nline-1\\nline-2\\nline-3\\nline-4\" \"key=test val=42 pi=3.14\") \"#+NAME: multiline\\n#+begin_src emacs-lisp :results output replace\\n(dotimes (i 5)\\n  (princ (format \\\"line-%d\\\\n\\\" i)))\\n#+end_src\\n\\n#+RESULTS: multiline\\n: line-0\\n: line-1\\n: line-2\\n: line-3\\n: line-4\\n\\n#+NAME: formatted\\n#+begin_src emacs-lisp :results output replace\\n(princ (format \\\"key=%s val=%d pi=%.2f\\\" \\\"test\\\" 42 3.14159))\\n#+end_src\\n\\n#+RESULTS: formatted\\n: key=test val=42 pi=3.14\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Multi-line output
      (insert "#+NAME: multiline\n")
      (insert "#+begin_src emacs-lisp :results output replace\n")
      (insert "(dotimes (i 5)\n  (princ (format \"line-%d\\n\" i)))\n")
      (insert "#+end_src\n\n")
      ;; Formatted output
      (insert "#+NAME: formatted\n")
      (insert "#+begin_src emacs-lisp :results output replace\n")
      (insert "(princ (format \"key=%s val=%d pi=%.2f\" \"test\" 42 3.14159))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("multiline" "formatted"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_multiple_named_results_order_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"first-val\" ((\"first-val\" \"second-val\")) \"chain=(\\\"first-val\\\" second-val)\") \"#+NAME: first\\n#+begin_src emacs-lisp :results value replace\\n'first-val\\n#+end_src\\n\\n#+RESULTS: first\\n: first-val\\n\\n#+NAME: second\\n#+begin_src emacs-lisp :var prev=first :results value replace\\n(list prev 'second-val)\\n#+end_src\\n\\n#+RESULTS: second\\n| first-val | second-val |\\n\\n#+NAME: third\\n#+begin_src emacs-lisp :var prev=second :results output replace\\n(princ (format \\\"chain=%S\\\" prev))\\n#+end_src\\n\\n#+RESULTS: third\\n: chain=(\\\"first-val\\\" second-val)\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      (insert "#+NAME: first\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'first-val\n")
      (insert "#+end_src\n\n")
      (insert "#+NAME: second\n")
      (insert "#+begin_src emacs-lisp :var prev=first :results value replace\n")
      (insert "(list prev 'second-val)\n")
      (insert "#+end_src\n\n")
      (insert "#+NAME: third\n")
      (insert "#+begin_src emacs-lisp :var prev=second :results output replace\n")
      (insert "(princ (format \"chain=%S\" prev))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dolist (name '("first" "second" "third"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read all results in order
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_noweb_var_chain_output_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((30 \"val=30\") \"#+NAME: config\\n#+begin_src emacs-lisp\\n(defconst base-val 10)\\n#+end_src\\n\\n#+NAME: compute\\n#+begin_src emacs-lisp :var x=3 :noweb yes :results value replace\\n(let ((b (progn <<config>> base-val)))\\n  (* x b))\\n#+end_src\\n\\n#+RESULTS: compute\\n: 30\\n\\n#+NAME: displayer\\n#+begin_src emacs-lisp :var val=compute :results output replace\\n(princ (format \\\"val=%d\\\" val))\\n#+end_src\\n\\n#+RESULTS: displayer\\n: val=30\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Config block
      (insert "#+NAME: config\n")
      (insert "#+begin_src emacs-lisp\n")
      (insert "(defconst base-val 10)\n")
      (insert "#+end_src\n\n")
      ;; Noweb block
      (insert "#+NAME: compute\n")
      (insert "#+begin_src emacs-lisp :var x=3 :noweb yes :results value replace\n")
      (insert "(let ((b (progn <<config>> base-val)))\n")
      (insert "  (* x b))\n")
      (insert "#+end_src\n\n")
      ;; Output block
      (insert "#+NAME: displayer\n")
      (insert "#+begin_src emacs-lisp :var val=compute :results output replace\n")
      (insert "(princ (format \"val=%d\" val))\n")
      (insert "#+end_src\n\n")
      ;; Execute chain
      (dolist (name '("compute" "displayer"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_table_var_format_spec_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":row-count\" 3 \":x-sum\" 9 \":y-sum\" 12)) \"rows=3 x=9 y=12\") ((\"X\" \"Y\") hline (\"1\" \"2\") (\"3\" \"4\") (\"5\" \"6\")) \"#+NAME: data\\n| X | Y |\\n|---+---|\\n| 1 | 2 |\\n| 3 | 4 |\\n| 5 | 6 |\\n\\n#+NAME: summer\\n#+begin_src emacs-lisp :var tbl=data :results value replace\\n(list :row-count (length tbl)\\n      :x-sum (apply #'+ (mapcar #'car tbl))\\n      :y-sum (apply #'+ (mapcar #'cadr tbl)))\\n#+end_src\\n\\n#+RESULTS: summer\\n| :row-count | 3 | :x-sum | 9 | :y-sum | 12 |\\n\\n#+NAME: formatter\\n#+begin_src emacs-lisp :var stats=summer :results output replace\\n(princ (format \\\"rows=%d x=%d y=%d\\\"\\n               (plist-get stats :row-count)\\n               (plist-get stats :x-sum)\\n               (plist-get stats :y-sum)))\\n#+end_src\\n\\n#+RESULTS: formatter\\n: rows=3 x=9 y=12\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Source table
      (insert "#+NAME: data\n")
      (insert "| X | Y |\n|---+---|\n| 1 | 2 |\n| 3 | 4 |\n| 5 | 6 |\n\n")
      ;; Sum block
      (insert "#+NAME: summer\n")
      (insert "#+begin_src emacs-lisp :var tbl=data :results value replace\n")
      (insert "(list :row-count (length tbl)\n")
      (insert "      :x-sum (apply #'+ (mapcar #'car tbl))\n")
      (insert "      :y-sum (apply #'+ (mapcar #'cadr tbl)))\n")
      (insert "#+end_src\n\n")
      ;; Format spec block
      (insert "#+NAME: formatter\n")
      (insert "#+begin_src emacs-lisp :var stats=summer :results output replace\n")
      (insert "(princ (format \"rows=%d x=%d y=%d\"\n")
      (insert "               (plist-get stats :row-count)\n")
      (insert "               (plist-get stats :x-sum)\n")
      (insert "               (plist-get stats :y-sum)))\n")
      (insert "#+end_src\n\n")
      ;; Execute
      (dolist (name '("summer" "formatter"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        ;; Parse table
        (let ((table-lisp
               (progn
                 (goto-char (point-min))
                 (search-forward "| X")
                 (org-table-to-lisp))))
          (list (nreverse results)
                table-lisp
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_header_arg_inherit_override_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; File-level property
      (insert "#+PROPERTY: header-args :results value replace\n\n")
      ;; Block inheriting file-level
      (insert "#+NAME: inherited\n")
      (insert "#+begin_src emacs-lisp\n")
      (insert "(+ 1 2)\n")
      (insert "#+end_src\n\n")
      ;; Block with output override
      (insert "#+NAME: output-override\n")
      (insert "#+begin_src emacs-lisp :results output replace\n")
      (insert "(princ \"overridden to output\")\n")
      (insert "#+end_src\n\n")
      ;; Block with file override
      (let* ((root (make-temp-file "org-babel-inherit" t))
             (out-file (expand-file-name "result.txt" root)))
        (unwind-protect
            (progn
              (insert "#+NAME: file-override\n")
              (insert "#+begin_src emacs-lisp :file " out-file "\n")
              (insert "(with-temp-file \"" out-file "\"\n  (insert \"file content\"))\n  \"done\"\n")
              (insert "#+end_src\n\n")
              ;; Execute all
              (dolist (name '("inherited" "output-override" "file-override"))
                (goto-char (point-min))
                (search-forward name)
                (org-babel-execute-src-block))
              ;; Read results
              (let ((results nil))
                (goto-char (point-min))
                (while (re-search-forward "#\\+RESULTS:" nil t)
                  (forward-line 1)
                  (push (org-babel-read-result) results))
                (let ((file-content
                       (when (file-exists-p out-file)
                         (with-temp-buffer
                           (insert-file-contents out-file)
                           (buffer-string)))))
                  (list (nreverse results)
                        (replace-regexp-in-string
                         (regexp-quote root) "<root>"
                         (or file-content "no-file"))
                        (replace-regexp-in-string
                         (regexp-quote root) "<root>"
                         (buffer-substring-no-properties
                          (point-min) (point-max))))))
          (delete-directory root t)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_assign_header_var_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((((\":input\" 5 \":doubled\" 10 \":squared\" 25)) ((\":input\" 10 \":tripled\" 30 \":squared\" 100)) ((\":input\" 20 \":doubled\" 40 \":squared\" 400))) \"#+NAME: doubler\\n#+begin_src emacs-lisp :var n=5 :results value replace\\n(list :input n :doubled (* n 2) :squared (* n n))\\n#+end_src\\n\\n#+RESULTS: doubler\\n| :input | 5 | :doubled | 10 | :squared | 25 |\\n\\n#+NAME: tripler\\n#+begin_src emacs-lisp :var n=10 :results value replace\\n(list :input n :tripled (* n 3) :squared (* n n))\\n#+end_src\\n\\n#+RESULTS: tripler\\n| :input | 10 | :tripled | 30 | :squared | 100 |\\n\\n#+CALL: doubler(n=20) :results value replace\\n\\n#+RESULTS:\\n| :input | 20 | :doubled | 40 | :squared | 400 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Named block with default var
      (insert "#+NAME: doubler\n")
      (insert "#+begin_src emacs-lisp :var n=5 :results value replace\n")
      (insert "(list :input n :doubled (* n 2) :squared (* n n))\n")
      (insert "#+end_src\n\n")
      ;; Override var
      (insert "#+NAME: tripler\n")
      (insert "#+begin_src emacs-lisp :var n=10 :results value replace\n")
      (insert "(list :input n :tripled (* n 3) :squared (* n n))\n")
      (insert "#+end_src\n\n")
      ;; Call with different var
      (insert "#+CALL: doubler(n=20) :results value replace\n\n")
      ;; Execute
      (dolist (name '("doubler" "tripler"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Execute call
      (goto-char (point-min))
      (search-forward "CALL:")
      (org-babel-lob-execute-maybe)
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        (list (nreverse results)
              (buffer-substring-no-properties
               (point-min) (point-max)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_error_handling_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((30 nil nil) ((src-block \"valid\" \"(+ 10 20)\\n\") (fixed-width nil \"30\") (src-block \"niler\" \"nil\\n\") (src-block \"emptier\" \"'()\\n\")) \"#+NAME: valid\\n#+begin_src emacs-lisp :results value replace\\n(+ 10 20)\\n#+end_src\\n\\n#+RESULTS: valid\\n: 30\\n\\n#+NAME: niler\\n#+begin_src emacs-lisp :results value replace\\nnil\\n#+end_src\\n\\n#+RESULTS: niler\\n\\n#+NAME: emptier\\n#+begin_src emacs-lisp :results value replace\\n'()\\n#+end_src\\n\\n#+RESULTS: emptier\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil)
          (org-babel-default-header-args '((:results . "value replace"))))
      ;; Valid block
      (insert "#+NAME: valid\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(+ 10 20)\n")
      (insert "#+end_src\n\n")
      ;; Block that returns nil
      (insert "#+NAME: niler\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "nil\n")
      (insert "#+end_src\n\n")
      ;; Block that returns empty list
      (insert "#+NAME: emptier\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'()\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dolist (name '("valid" "niler" "emptier"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        ;; Get buffer state
        (let ((buf-text (buffer-substring-no-properties
                         (point-min) (point-max)))
              (elements
               (org-element-map (org-element-parse-buffer)
                   '(src-block fixed-width)
                 (lambda (el)
                   (list (org-element-type el)
                         (org-element-property :name el)
                         (org-element-property :value el))))))
          (list (nreverse results)
                elements
                buf-text))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_result_insert_replace_remove_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#+NAME: calc\\n#+begin_src emacs-lisp :results value replace\\n(list :a 1 :b 2 :c 3)\\n#+end_src\\n\\n#+RESULTS: calc\\n| :a | 1 | :b | 2 | :c | 3 |\\n:placeholder\\n\\n\" \"(list :a 1 :b 2 :c 3)\\n\" \"#+NAME: calc\\n#+begin_src emacs-lisp :results value replace\\n(list :a 1 :b 2 :c 3)\\n#+end_src\\n:placeholder\\n\\n\" \"#+NAME: calc\\n#+begin_src emacs-lisp :results value replace\\n(list :a 1 :b 2 :c 3)\\n#+end_src\\n\\n#+RESULTS: calc\\n| :a | 1 | :b | 2 | :c | 3 |\\n\\n:placeholder\\n\\n\" \"(list :a 1 :b 2 :c 3)\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      (insert "#+NAME: calc\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "(list :a 1 :b 2 :c 3)\n")
      (insert "#+end_src\n\n")
      (insert "#+RESULTS: calc\n")
      (insert ":placeholder\n\n")
      ;; Execute - should replace placeholder
      (goto-char (point-min))
      (search-forward "calc")
      (org-babel-execute-src-block)
      (let ((after-exec (buffer-substring-no-properties
                         (point-min) (point-max)))
            (result-val (org-babel-read-result)))
        ;; Remove result
        (goto-char (point-min))
        (search-forward "calc")
        (org-babel-remove-result)
        (let ((after-remove (buffer-substring-no-properties
                             (point-min) (point-max))))
          ;; Re-execute
          (goto-char (point-min))
          (search-forward "calc")
          (org-babel-execute-src-block)
          (let ((after-reexec (buffer-substring-no-properties
                               (point-min) (point-max)))
                (reexec-val (org-babel-read-result)))
             (list after-exec
                   result-val
                   after-remove
                   after-reexec
                   reexec-val)))))))"##,
        expect,
    );
}

#[test]
fn org_babel_execute_output_var_list_table_deep_state_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK ((\"line-0\\nline-1\\nline-2\" ((\"x\" 1) (\"y\" 2) (\"z\" 3)) ((\"Key\" \"Val\") hline (\"x\" 10) (\"y\" 20) (\"z\" 30))) ((src-block \"out\" \"(dotimes (i 3)\\n  (princ (format \\\"line-%d\\\\n\\\" i)))\\n\") (fixed-width nil \"line-0\\nline-1\\nline-2\") (src-block \"lst\" \"'((x 1) (y 2) (z 3))\\n\") (src-block \"tbl\" \"(cons '(\\\"Key\\\" \\\"Val\\\")\\n      (cons 'hline\\n            (mapcar (lambda (r) (list (car r) (* 10 (cadr r)))) data)))\\n\")) \"#+NAME: out\\n#+begin_src emacs-lisp :results output replace\\n(dotimes (i 3)\\n  (princ (format \\\"line-%d\\\\n\\\" i)))\\n#+end_src\\n\\n#+RESULTS: out\\n: line-0\\n: line-1\\n: line-2\\n\\n#+NAME: lst\\n#+begin_src emacs-lisp :results value replace\\n'((x 1) (y 2) (z 3))\\n#+end_src\\n\\n#+RESULTS: lst\\n| x | 1 |\\n| y | 2 |\\n| z | 3 |\\n\\n#+NAME: tbl\\n#+begin_src emacs-lisp :var data=lst :results value table replace\\n(cons '(\\\"Key\\\" \\\"Val\\\")\\n      (cons 'hline\\n            (mapcar (lambda (r) (list (car r) (* 10 (cadr r)))) data)))\\n#+end_src\\n\\n#+RESULTS: tbl\\n| Key | Val |\\n|-----+-----|\\n| x   |  10 |\\n| y   |  20 |\\n| z   |  30 |\\n\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn
  (require 'org)
  (require 'ob-core)
  (require 'ob-emacs-lisp)
  (with-temp-buffer
    (org-mode)
    (let ((org-confirm-babel-evaluate nil))
      ;; Output mode
      (insert "#+NAME: out\n")
      (insert "#+begin_src emacs-lisp :results output replace\n")
      (insert "(dotimes (i 3)\n  (princ (format \"line-%d\\n\" i)))\n")
      (insert "#+end_src\n\n")
      ;; Value list mode
      (insert "#+NAME: lst\n")
      (insert "#+begin_src emacs-lisp :results value replace\n")
      (insert "'((x 1) (y 2) (z 3))\n")
      (insert "#+end_src\n\n")
      ;; Value table mode
      (insert "#+NAME: tbl\n")
      (insert "#+begin_src emacs-lisp :var data=lst :results value table replace\n")
      (insert "(cons '(\"Key\" \"Val\")\n")
      (insert "      (cons 'hline\n")
      (insert "            (mapcar (lambda (r) (list (car r) (* 10 (cadr r)))) data)))\n")
      (insert "#+end_src\n\n")
      ;; Execute all
      (dolist (name '("out" "lst" "tbl"))
        (goto-char (point-min))
        (search-forward name)
        (org-babel-execute-src-block))
      ;; Read results
      (let ((results nil))
        (goto-char (point-min))
        (while (re-search-forward "#\\+RESULTS:" nil t)
          (forward-line 1)
          (push (org-babel-read-result) results))
        ;; Parse elements
        (let ((elements
               (org-element-map (org-element-parse-buffer)
                   '(src-block fixed-width)
                 (lambda (el)
                   (list (org-element-type el)
                         (org-element-property :name el)
                         (org-element-property :value el))))))
          (list (nreverse results)
                elements
                (buffer-substring-no-properties
                 (point-min) (point-max))))))))"##,
        expect,
    );
}
