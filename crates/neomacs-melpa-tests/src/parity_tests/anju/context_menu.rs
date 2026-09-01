use expect_test::expect;

use super::ParityBatchCase;

fn anju_org_context_menu_tracks_heading_item_checkbox_table_link_and_body_workflows()
-> ParityBatchCase {
    ParityBatchCase::value(
        "anju_org_context_menu_tracks_heading_item_checkbox_table_link_and_body_workflows",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (org-mode)
             (insert (nth 1 case))
             (goto-char (nth 2 case))
             (let ((menu (make-sparse-keymap "Context"))
                   adjusted-clicks)
               (cl-letf
                   (((symbol-function 'anju-adjust-point-for-click)
                     (lambda (click)
                       (push click adjusted-clicks))))
                 (anju-context-menu-org-mode menu 'mouse-event)
                 (list
                  (car case)
                  (org-element-type (org-element-context))
                  (nreverse adjusted-clicks)
                  (anju-test-menu-entries menu))))))
         '((heading "* TODO Ship release\nBody\n" 3)
           (item "- deploy service\n" 4)
           (checkbox "- [ ] deploy service\n" 8)
           (table "| service | status |\n| api | ready |\n" 25)
           (link "See [[https://example.test/path][runbook]].\n" 12)
           (body "Ordinary body text\n" 5)))"##,
        expect![[
            r#"OK ((heading headline (mouse-event) ((org-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (org-todo "TODO…" org-todo :enable nil :visible nil :style nil :selected nil :help "Change the TODO state of an item") (org-toggle-heading "Change to Body" org-toggle-heading :enable nil :visible nil :style nil :selected nil :help "Convert headings to normal text, or items or text to headings") (org-clock-in "Clock In" org-clock-in :enable nil :visible (not (org-clocking-p)) :style nil :selected nil :help "Clock in") (org-clock-out "Clock Out" org-clock-out :enable nil :visible (org-clocking-p) :style nil :selected nil :help "Clock out") (org-sort-entries "Sort…" org-sort-entries :enable nil :visible nil :style nil :selected nil :help "Sort entries on a certain level of an outline tree") (org-do-demote "Demote →" org-do-demote :enable nil :visible nil :style nil :selected nil :help "Demote") (org-do-promote "Promote ←" org-do-promote :enable nil :visible nil :style nil :selected nil :help "Promote") (org-demote-subtree "Demote Subtree →" org-demote-subtree :enable nil :visible nil :style nil :selected nil :help "Demote heading subtree") (org-promote-subtree "Promote Subtree ←" org-promote-subtree :enable nil :visible nil :style nil :selected nil :help "Promote heading subtree"))) (item paragraph (mouse-event) ((org-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (org-toggle-item "Change to Body" org-toggle-item :enable nil :visible (not (or (use-region-p) (anju-line-empty-p))) :style nil :selected nil :help "Convert item to normal line") (org-cycle-list-bullet "Cycle Bullet" org-cycle-list-bullet :enable nil :visible nil :style nil :selected nil :help "Cycle through the different itemize/enumerate bullets") (org-sort-list "Sort…" org-sort-list :enable nil :visible nil :style nil :selected nil :help "Sort list items") (casual-org-toggle-list-to-checkbox #1=(if (org-at-item-checkbox-p) "Change to Item" "Change to Checkbox") casual-org-toggle-list-to-checkbox :enable nil :visible nil :style nil :selected nil :help "Toggle Item/Checkbox") (org-indent-item "Demote →" org-indent-item :enable nil :visible nil :style nil :selected nil :help "Demote") (org-outdent-item "Promote ←" org-outdent-item :enable nil :visible nil :style nil :selected nil :help "Promote") (org-indent-item-tree "Demote Subtree →" org-indent-item-tree :enable nil :visible nil :style nil :selected nil :help "Demote item subtree") (org-outdent-item-tree "Promote Subtree ←" org-outdent-item-tree :enable nil :visible nil :style nil :selected nil :help "Promote item subtree"))) (checkbox paragraph (mouse-event) ((org-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (casual-org-checkbox-in-progress "In-Progress [-]" casual-org-checkbox-in-progress :enable nil :visible nil :style nil :selected nil :help "Change checkbox state to in-progress [-]") (org-cycle-list-bullet "Cycle Bullet" org-cycle-list-bullet :enable nil :visible nil :style nil :selected nil :help "Cycle through the different itemize/enumerate bullets") (org-sort-list "Sort…" org-sort-list :enable nil :visible nil :style nil :selected nil :help "Sort list items") (casual-org-toggle-list-to-checkbox #1# casual-org-toggle-list-to-checkbox :enable nil :visible nil :style nil :selected nil :help "Toggle Item/Checkbox") (org-indent-item "Demote →" org-indent-item :enable nil :visible nil :style nil :selected nil :help "Demote") (org-outdent-item "Promote ←" org-outdent-item :enable nil :visible nil :style nil :selected nil :help "Promote") (org-indent-item-tree "Demote Subtree →" org-indent-item-tree :enable nil :visible nil :style nil :selected nil :help "Demote item subtree") (org-outdent-item-tree "Promote Subtree ←" org-outdent-item-tree :enable nil :visible nil :style nil :selected nil :help "Promote item subtree"))) (table table-cell (mouse-event) ((org-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (casual-org-table-copy-reference-dwim (casual-org-table--reference-dwim) casual-org-table-copy-reference-dwim :enable nil :visible nil :style nil :selected nil :help "Copy Org table reference (field or range) into kill ring via mouse") (Org\ Table\ Region "Org Table Region" <submenu> :enable nil :visible nil :style nil :selected nil :help nil) (org-table-sort-lines "Sort…" org-table-sort-lines :enable nil :visible nil :style nil :selected nil :help "Sort table lines according to the column at point") (org-table-toggle-coordinate-overlays "Show Coordinates" org-table-toggle-coordinate-overlays :enable nil :visible nil :style nil :selected nil :help "Toggle the display of row/column numbers in tables") (anju-org-table-recalculate "Recalculate" anju-org-table-recalculate :enable nil :visible nil :style nil :selected nil :help "Recalculate table") (org-table-edit-formulas "Edit Table Formulas" org-table-edit-formulas :enable nil :visible nil :style nil :selected nil :help "Edit the formulas of the current table in a separate buffer") (org-plot/gnuplot "Run gnuplot" org-plot/gnuplot :enable nil :visible nil :style nil :selected nil :help "Plot table using gnuplot"))) (link link (mouse-event) ((org-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (org-toggle-heading "Change to Heading" org-toggle-heading :enable nil :visible #2=(not (or (use-region-p) (anju-line-empty-p))) :style nil :selected nil :help "Convert headings to normal text, or items or text to headings") (org-toggle-item "Change to Item" org-toggle-item :enable nil :visible #3=(not (or (use-region-p) (anju-line-empty-p))) :style nil :selected nil :help "Convert headings or normal lines to items, items to normal lines") (org-insert-link "Link…" org-insert-link :enable nil :visible nil :style nil :selected nil :help "Insert a link.  At the prompt, enter the link") (anju-copy-raw-link "Copy Link Address…" anju-copy-raw-link :enable nil :visible nil :style nil :selected nil :help "Copy link address from an Org hyperlink"))) (body paragraph (mouse-event) ((org-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (org-toggle-heading "Change to Heading" org-toggle-heading :enable nil :visible #2# :style nil :selected nil :help "Convert headings to normal text, or items or text to headings") (org-toggle-item "Change to Item" org-toggle-item :enable nil :visible #3# :style nil :selected nil :help "Convert headings or normal lines to items, items to normal lines"))))"#
        ]],
    )
}

fn anju_copy_raw_org_link_and_exported_region_drive_real_clipboard_workflows() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_copy_raw_org_link_and_exported_region_drive_real_clipboard_workflows",
        r##"(let (selections messages)
         (cl-letf (((symbol-function 'gui-set-selection)
                    (lambda (type value)
                      (push (list type value) selections)))
                   ((symbol-function 'message)
                    (lambda (format-string &rest arguments)
                      (let ((rendered
                             (apply #'format format-string arguments)))
                        (when
                            (string-prefix-p "Copied '" rendered)
                          (push rendered messages))))))
           (list
            (with-temp-buffer
              (org-mode)
              (insert "Read [[https://example.test/guide?q=1][the guide]].")
              (search-backward "guide?q")
              (let ((kill-ring nil))
                (anju-copy-raw-link)
                (list (car kill-ring) (nreverse messages))))
            (with-temp-buffer
              (org-mode)
              (require 'ox-md)
              (insert "* Release\n- [X] API\n- [ ] Worker\n")
              (goto-char 1)
              (set-mark (point-max))
              (activate-mark)
              (anju-org-copy-region-as-markdown)
              (car selections)))))"##,
        expect![[
            r#"OK (("https://example.test/guide?q=1" ("Copied 'https://example.test/guide?q=1' to clipboard")) (CLIPBOARD "\n# Release\n\n-   [X] API\n-   [ ] Worker\n\n"))"#
        ]],
    )
}

fn anju_elisp_context_understands_defuns_ert_tests_lambdas_and_numeric_literals() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anju_elisp_context_understands_defuns_ert_tests_lambdas_and_numeric_literals",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (emacs-lisp-mode)
             (insert (nth 1 case))
             (goto-char (nth 2 case))
             (let ((menu (make-sparse-keymap "Context")))
               (cl-letf
                   (((symbol-function 'anju-adjust-point-for-click)
                     #'ignore))
                 (list
                  (car case)
                  (anju-form-delaration-at-point)
                  (anju-form-name-at-point)
                  (anju-point-in-ertdeftest-p)
                  (anju-point-on-lambda-p)
                  (anju-test-menu-entries
                   (anju-context-menu-elisp menu 'mouse-event)))))))
         '((defun "(defun deploy (target)\n  (message \"%s\" target))\n" 20)
           (ert "(ert-deftest deploy-works ()\n  (should t))\n" 22)
           (lambda "(mapcar (lambda (item) (* item 2)) '(1 2))\n" 10)
           (number "(+ 42 1)\n" 4)))"##,
        expect![[
            r#"OK ((defun defun deploy nil nil ((emacs-lisp-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (eval-last-sexp "Eval Last Sexp" eval-last-sexp :enable nil :visible nil :style nil :selected nil :help "Evaluate sexp before point; print value in the echo area") (eval-defun #1=(format "Eval “%s”" (anju-form-name-at-point)) eval-defun :enable nil :visible #2=(anju-form-name-at-point) :style nil :selected nil :help "Evaluate the top level form point is in") (anju-edebug-defun #3=(format "Edebug “%s”" (anju-form-name-at-point)) anju-edebug-defun :enable nil :visible #4=(anju-form-name-at-point) :style nil :selected nil :help "Evaluate the top level form point is in, stepping through with Edebug") (elisp-eval-region-or-buffer #5=(if (use-region-p) "Eval Region" "Eval Buffer") elisp-eval-region-or-buffer :enable nil :visible nil :style nil :selected nil :help "Evaluate region or buffer") (Hide/Show "Hide/Show" <submenu> :enable nil :visible hs-minor-mode :style nil :selected nil :help nil) (xref-find-references-and-replace #6=(format "Rename “%s”" (thing-at-point 'symbol)) xref-find-references-and-replace :enable nil :visible #7=(let ((thing (thing-at-point 'symbol))) (and thing (not (string-match-p "^[-+]?[[:digit:]]*\\.?[[:digit:]]+$" thing)) (not (member (substring-no-properties thing) '("lambda" "nil"))))) :style nil :selected nil :help "Rename xref symbol") (anju-ert-run-test-at-point #8=(format "Test “%s”" (anju-form-name-at-point)) anju-ert-run-test-at-point :enable nil :visible #9=(anju-point-in-ertdeftest-p) :style nil :selected nil :help "ERT") (anju-extract-lambda-to-defun "Extract 𝜆…" anju-extract-lambda-to-defun :enable nil :visible #10=(anju-point-on-lambda-p) :style nil :selected nil :help "Convert lambda expression into a function") (eval-expression "Eval Expression…" eval-expression :enable nil :visible nil :style nil :selected nil :help "Evaluate expression and print result in mini-buffer"))) (ert ert-deftest deploy-works t nil ((emacs-lisp-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (eval-last-sexp "Eval Last Sexp" eval-last-sexp :enable nil :visible nil :style nil :selected nil :help "Evaluate sexp before point; print value in the echo area") (eval-defun #1# eval-defun :enable nil :visible #2# :style nil :selected nil :help "Evaluate the top level form point is in") (anju-edebug-defun #3# anju-edebug-defun :enable nil :visible #4# :style nil :selected nil :help "Evaluate the top level form point is in, stepping through with Edebug") (elisp-eval-region-or-buffer #5# elisp-eval-region-or-buffer :enable nil :visible nil :style nil :selected nil :help "Evaluate region or buffer") (Hide/Show "Hide/Show" <submenu> :enable nil :visible hs-minor-mode :style nil :selected nil :help nil) (xref-find-references-and-replace #6# xref-find-references-and-replace :enable nil :visible #7# :style nil :selected nil :help "Rename xref symbol") (anju-ert-run-test-at-point #8# anju-ert-run-test-at-point :enable nil :visible #9# :style nil :selected nil :help "ERT") (anju-extract-lambda-to-defun "Extract 𝜆…" anju-extract-lambda-to-defun :enable nil :visible #10# :style nil :selected nil :help "Convert lambda expression into a function") (eval-expression "Eval Expression…" eval-expression :enable nil :visible nil :style nil :selected nil :help "Evaluate expression and print result in mini-buffer"))) (lambda mapcar nil nil t ((emacs-lisp-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (eval-last-sexp "Eval Last Sexp" eval-last-sexp :enable nil :visible nil :style nil :selected nil :help "Evaluate sexp before point; print value in the echo area") (eval-defun #1# eval-defun :enable nil :visible #2# :style nil :selected nil :help "Evaluate the top level form point is in") (anju-edebug-defun #3# anju-edebug-defun :enable nil :visible #4# :style nil :selected nil :help "Evaluate the top level form point is in, stepping through with Edebug") (elisp-eval-region-or-buffer #5# elisp-eval-region-or-buffer :enable nil :visible nil :style nil :selected nil :help "Evaluate region or buffer") (Hide/Show "Hide/Show" <submenu> :enable nil :visible hs-minor-mode :style nil :selected nil :help nil) (xref-find-references-and-replace #6# xref-find-references-and-replace :enable nil :visible #7# :style nil :selected nil :help "Rename xref symbol") (anju-ert-run-test-at-point #8# anju-ert-run-test-at-point :enable nil :visible #9# :style nil :selected nil :help "ERT") (anju-extract-lambda-to-defun "Extract 𝜆…" anju-extract-lambda-to-defun :enable nil :visible #10# :style nil :selected nil :help "Convert lambda expression into a function") (eval-expression "Eval Expression…" eval-expression :enable nil :visible nil :style nil :selected nil :help "Evaluate expression and print result in mini-buffer"))) (number + nil nil nil ((emacs-lisp-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (eval-last-sexp "Eval Last Sexp" eval-last-sexp :enable nil :visible nil :style nil :selected nil :help "Evaluate sexp before point; print value in the echo area") (eval-defun #1# eval-defun :enable nil :visible #2# :style nil :selected nil :help "Evaluate the top level form point is in") (anju-edebug-defun #3# anju-edebug-defun :enable nil :visible #4# :style nil :selected nil :help "Evaluate the top level form point is in, stepping through with Edebug") (elisp-eval-region-or-buffer #5# elisp-eval-region-or-buffer :enable nil :visible nil :style nil :selected nil :help "Evaluate region or buffer") (Hide/Show "Hide/Show" <submenu> :enable nil :visible hs-minor-mode :style nil :selected nil :help nil) (xref-find-references-and-replace #6# xref-find-references-and-replace :enable nil :visible #7# :style nil :selected nil :help "Rename xref symbol") (anju-ert-run-test-at-point #8# anju-ert-run-test-at-point :enable nil :visible #9# :style nil :selected nil :help "ERT") (anju-extract-lambda-to-defun "Extract 𝜆…" anju-extract-lambda-to-defun :enable nil :visible #10# :style nil :selected nil :help "Convert lambda expression into a function") (eval-expression "Eval Expression…" eval-expression :enable nil :visible nil :style nil :selected nil :help "Evaluate expression and print result in mini-buffer"))))"#
        ]],
    )
}

fn anju_extract_lambda_replaces_the_call_site_and_builds_an_editable_defun() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_extract_lambda_replaces_the_call_site_and_builds_an_editable_defun",
        r##"(let ((source (generate-new-buffer " *anju-source*"))
               (created nil))
         (unwind-protect
             (with-current-buffer source
               (emacs-lisp-mode)
               (insert "(mapcar (lambda (item)\n          (* item 2))\n        values)")
               (search-backward "lambda")
               (cl-letf
                   (((symbol-function 'switch-to-buffer-other-window)
                     (lambda (buffer &rest _)
                       (setq created buffer)
                       (set-buffer buffer))))
                 (anju-extract-lambda-to-defun "double-item")
                 (list
                  (with-current-buffer source (buffer-string))
                  (and created
                       (with-current-buffer created
                         (list major-mode (buffer-string) (point)))))))
           (when (buffer-live-p source)
             (kill-buffer source))
           (when (buffer-live-p created)
             (kill-buffer created))))"##,
        expect![[
            r#"OK ("(mapcar #'double-item\n        values)" (emacs-lisp-mode "(defun double-item (item) (* item 2))" 1))"#
        ]],
    )
}

fn anju_narrow_context_selects_region_defun_org_markdown_and_widen_actions() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_narrow_context_selects_region_defun_org_markdown_and_widen_actions",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (funcall (nth 1 case))
             (insert (nth 2 case))
             (goto-char (nth 3 case))
             (pcase (car case)
               ('region
                (set-mark (point-max))
                (activate-mark))
               ('narrowed
                (narrow-to-region 2 (1- (point-max)))))
             (let ((menu (make-sparse-keymap "Context")))
               (list
                (car case)
                (buffer-narrowed-p)
                (anju-test-menu-entries
                 (anju-context-menu-narrow menu nil))))))
         '((region text-mode "alpha beta gamma" 2)
           (defun emacs-lisp-mode "(defun alpha ()\n  1)\n" 8)
           (org org-mode "* Alpha\nBody\n" 3)
           (markdown markdown-mode "# Alpha\nBody\n" 3)
           (narrowed text-mode "alpha beta gamma\n" 5)))"##,
        expect![[
            r#"OK ((region nil ((narrow-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (narrow-to-region (anju-menu-label "Narrow Region") narrow-to-region :enable nil :visible nil :style nil :selected nil :help "Restrict editing in this buffer to the current region"))) (defun nil ((narrow-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (narrow-to-defun "Narrow to defun" narrow-to-defun :enable nil :visible nil :style nil :selected nil :help "Restrict editing in this buffer to the current defun"))) (org nil ((narrow-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (org-narrow-to-subtree "Narrow to subtree" org-narrow-to-subtree :enable nil :visible nil :style nil :selected nil :help "Restrict editing in this buffer to the current subtree"))) (markdown nil ((narrow-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (markdown-narrow-to-subtree "Narrow to subtree" markdown-narrow-to-subtree :enable nil :visible nil :style nil :selected nil :help "Restrict editing in this buffer to the current subtree"))) (narrowed t ((narrow-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (widen "Widen buffer" widen :enable nil :visible nil :style nil :selected nil :help "Remove narrowing restrictions from current buffer"))))"#
        ]],
    )
}

fn anju_region_context_builds_practical_plain_org_markdown_and_read_only_menus() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anju_region_context_builds_practical_plain_org_markdown_and_read_only_menus",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (funcall (nth 1 case))
             (insert (nth 2 case))
             (goto-char (nth 3 case))
             (set-mark (nth 4 case))
             (activate-mark)
             (setq buffer-read-only (nth 5 case))
             (let ((menu (make-sparse-keymap "Context")))
               (list
                (car case)
                (anju-menu-label "Occur")
                (anju-test-menu-entries
                 (anju-context-menu-region menu nil))))))
         '((plain text-mode "deploy alpha service" 8 13 nil)
           (org org-mode "* Release\nDeploy API\n" 11 17 nil)
           (markdown markdown-mode "# Release\nDeploy worker\n" 11 17 nil)
           (readonly text-mode "immutable text" 1 10 t)))"##,
        expect![[
            r#"OK ((plain "Occur “alpha”" ((transform-text-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (anju-occur-selected-region #1=(anju-menu-label "Occur") anju-occur-selected-region :enable nil :visible #2=(eq (count-lines (region-beginning) (region-end)) 1) :style nil :selected nil :help "Show all lines in the current buffer containing a match for selected word") (Transform\ Text "Transform Text" <submenu> :enable #3=(and (use-region-p) (not buffer-read-only)) :visible nil :style nil :selected nil :help nil) (query-replace "Query Replace…" query-replace :enable nil :visible #4=(not buffer-read-only) :style nil :selected nil :help "Replace some occurrences of FROM-STRING with TO-STRING") (query-replace-regexp "Query Replace Regexp…" query-replace-regexp :enable nil :visible #5=(not buffer-read-only) :style nil :selected nil :help "Replace some things after point matching REGEXP with TO-STRING") (write-region "Write Region…" write-region :enable nil :visible nil :style nil :selected nil :help "Write current region into specified file"))) (org "Occur “Deploy”" ((transform-text-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (anju-occur-selected-region #1# anju-occur-selected-region :enable nil :visible #2# :style nil :selected nil :help "Show all lines in the current buffer containing a match for selected word") (Style "Style" <submenu> :enable #6=(and (use-region-p) (not buffer-read-only)) :visible #7=(anju-style-mode-supported-p) :style nil :selected nil :help nil) (Transform\ Text "Transform Text" <submenu> :enable #3# :visible nil :style nil :selected nil :help nil) (query-replace "Query Replace…" query-replace :enable nil :visible #4# :style nil :selected nil :help "Replace some occurrences of FROM-STRING with TO-STRING") (query-replace-regexp "Query Replace Regexp…" query-replace-regexp :enable nil :visible #5# :style nil :selected nil :help "Replace some things after point matching REGEXP with TO-STRING") (comment-dwim "Toggle Comment" comment-dwim :enable nil :visible (not buffer-read-only) :style nil :selected nil :help "Toggle comment on selected region") (write-region "Write Region…" write-region :enable nil :visible nil :style nil :selected nil :help "Write current region into specified file"))) (markdown "Occur “Deploy”" ((transform-text-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (anju-occur-selected-region #1# anju-occur-selected-region :enable nil :visible #2# :style nil :selected nil :help "Show all lines in the current buffer containing a match for selected word") (Style "Style" <submenu> :enable #6# :visible #7# :style nil :selected nil :help nil) (Transform\ Text "Transform Text" <submenu> :enable #3# :visible nil :style nil :selected nil :help nil) (query-replace "Query Replace…" query-replace :enable nil :visible #4# :style nil :selected nil :help "Replace some occurrences of FROM-STRING with TO-STRING") (query-replace-regexp "Query Replace Regexp…" query-replace-regexp :enable nil :visible #5# :style nil :selected nil :help "Replace some things after point matching REGEXP with TO-STRING") (write-region "Write Region…" write-region :enable nil :visible nil :style nil :selected nil :help "Write current region into specified file"))) (readonly "Occur “immutable”" ((transform-text-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (anju-occur-selected-region #1# anju-occur-selected-region :enable nil :visible #2# :style nil :selected nil :help "Show all lines in the current buffer containing a match for selected word") (query-replace "Query Replace…" query-replace :enable nil :visible #4# :style nil :selected nil :help "Replace some occurrences of FROM-STRING with TO-STRING") (query-replace-regexp "Query Replace Regexp…" query-replace-regexp :enable nil :visible #5# :style nil :selected nil :help "Replace some things after point matching REGEXP with TO-STRING") (write-region "Write Region…" write-region :enable nil :visible nil :style nil :selected nil :help "Write current region into specified file"))))"#
        ]],
    )
}

fn anju_dired_duplicate_file_performs_a_real_deterministic_copy_and_builds_file_menu()
-> ParityBatchCase {
    ParityBatchCase::value(
        "anju_dired_duplicate_file_performs_a_real_deterministic_copy_and_builds_file_menu",
        r##"(let* ((root
                  (file-name-as-directory
                   (expand-file-name
                    "dired"
                    (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
                 (source (expand-file-name "release.notes.md" root))
                 (copy (expand-file-name "release.notes copy.md" root))
                 buffer)
         (make-directory root t)
         (write-region "ship alpha\n" nil source nil 'silent)
         (setq buffer (dired-noselect root))
         (unwind-protect
             (with-current-buffer buffer
               (dired-goto-file source)
               (anju-dired-duplicate-file)
               (let ((menu (make-sparse-keymap "Context")))
                 (cl-letf
                     (((symbol-function 'mouse-set-point) #'ignore))
                   (anju-context-menu-dired menu 'mouse-event)
                   (list
                    (file-exists-p copy)
                    (with-temp-buffer
                      (insert-file-contents copy)
                      (buffer-string))
                    (anju-test-menu-entries menu)))))
           (when (buffer-live-p buffer)
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK (t "ship alpha\n" ((dired-do-rename "Rename to…" dired-do-rename :enable nil :visible nil :style nil :selected nil :help "Rename or move file") (dired-do-copy "Copy to…" dired-do-copy :enable nil :visible nil :style nil :selected nil :help "Copy file") (dired-do-relsymlink "Symlink…" dired-do-relsymlink :enable nil :visible nil :style nil :selected nil :help "Make relative symlink") (dired-copy-filename-as-kill "Copy name" dired-copy-filename-as-kill :enable nil :visible nil :style nil :selected nil :help "Copy names of marked (or next ARG) files into the kill ring") (image-dired-dired-toggle-marked-thumbs "Toggle Thumbnail" image-dired-dired-toggle-marked-thumbs :enable nil :visible (string-match-p (image-file-name-regexp) (dired-get-filename)) :style nil :selected nil :help "Toggle thumbnails in front of marked file names in the Dired buffer") (Duplicate (format "Duplicate “%s”" (anju-filename-from-path (dired-get-filename))) anju-dired-duplicate-file :enable nil :visible nil :style nil :selected nil :help "Duplicate selected item") (dired-maybe-insert-subdir (format "Insert “%s” View" (anju-filename-from-path (dired-get-filename))) dired-maybe-insert-subdir :enable nil :visible (file-directory-p (dired-file-name-at-point)) :style nil :selected nil :help "Insert subdir (sub-directory)") (trash-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (dired-do-delete "Move to Trash…" dired-do-delete :enable nil :visible (file-writable-p (dired-file-name-at-point)) :style nil :selected nil :help "Delete all marked files") (dired-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (Sort\ By "Sort By" <submenu> :enable nil :visible (and (derived-mode-p 'dired-mode) (not dired-sort-inhibit)) :style nil :selected nil :help nil) (dired-omit-mode "Omit Mode" dired-omit-mode :enable nil :visible nil :style nil :selected nil :help "Omit mode") (dired-hide-details-mode "Hide Details" dired-hide-details-mode :enable nil :visible nil :style nil :selected nil :help "Hide directory details") (dired "📁 Dired…" dired :enable nil :visible nil :style nil :selected nil :help "Open Dired")))"#
        ]],
    )
}

fn anju_compilation_makefile_info_markup_and_wordcount_contexts_are_exact() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_compilation_makefile_info_markup_and_wordcount_contexts_are_exact",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (nth 2 case))
             (funcall (nth 1 case))
             (goto-char (point-min))
             (let ((menu (make-sparse-keymap "Context")))
               (cl-letf
                   (((symbol-function 'anju-adjust-point-for-click)
                     #'ignore)
                    ((symbol-function 'casual-compile--compilation-running-p)
                     (lambda () nil)))
                 (funcall (nth 3 case) menu 'mouse-event)
                 (list
                  (car case)
                  major-mode
                  (anju-test-menu-entries menu))))))
         '((compile compilation-mode "cc main.c\n" anju-context-menu-compile)
           (make makefile-gmake-mode "all:\n\t@echo ok\n" anju-context-menu-make-mode)
           (info Info-mode "Node text\n" anju-context-menu-info-mode)
           (org-markup org-mode "* Heading\nBody\n" anju-context-menu-markup)
           (markdown-markup markdown-mode "# Heading\nBody\n" anju-context-menu-markup)
           (words text-mode "one two three\n" anju-context-menu-wordcount)))"##,
        expect![[
            r#"OK ((compile compilation-mode ((compile-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (recompile (casual-compile--select-mode-label "Recompile" "Refresh") recompile :enable (not (casual-compile--compilation-running-p)) :visible nil :style nil :selected nil :help "Re-compile the program including the current buffer") (compile "Compile…" compile :enable (not (casual-compile--compilation-running-p)) :visible (not (derived-mode-p 'grep-mode)) :style nil :selected nil :help "Compile the program including the current buffer.  Default: run ‘make’") (kill-compilation (casual-compile-unicode-get :kill) kill-compilation :enable nil :visible (casual-compile--compilation-running-p) :style nil :selected nil :help "Kill the current compilation or grep process"))) (make makefile-gmake-mode ((context-makefile--separator1 "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (compile "Compile…" compile :enable nil :visible nil :style nil :selected nil :help "Compile the program including the current buffer.  Default: run ‘make’") (makefile-insert-target-ref "Insert target…" makefile-insert-target-ref :enable (not buffer-read-only) :visible nil :style nil :selected nil :help "Complete on a list of known targets, then insert TARGET-NAME at point") (makefile-insert-macro-ref "Insert macro…" makefile-insert-macro-ref :enable (not buffer-read-only) :visible nil :style nil :selected nil :help "Complete on a list of known macros, then insert complete ref at point") (makefile-backslash-region "\\ Region" makefile-backslash-region :enable (not buffer-read-only) :visible (use-region-p) :style nil :selected nil :help "Insert, align, or delete end-of-line backslashes on the lines in the region") (makefile-insert-gmake-function "Insert GNU make function…" makefile-insert-gmake-function :enable (not buffer-read-only) :visible (derived-mode-p 'makefile-gmake-mode) :style nil :selected nil :help "Insert a GNU make function call") (casual-make-identify-autovar-region "Identify Auto Var" casual-make-identify-autovar-region :enable nil :visible (use-region-p) :style nil :selected nil :help "Identify GNU Make automatic variable in region from START to END") (context-makefile--separator2 "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (makefile-pickup-everything "Refresh targets and macros" makefile-pickup-everything :enable nil :visible nil :style nil :selected nil :help "Notice names of all macros and targets in Makefile") (makefile-pickup-filenames-as-targets "Include file names as targets" makefile-pickup-filenames-as-targets :enable nil :visible nil :style nil :selected nil :help "Scan the current directory for filenames to use as targets") (makefile-create-up-to-date-overview "Overview" makefile-create-up-to-date-overview :enable nil :visible nil :style nil :selected nil :help "Create a buffer containing an overview of the state of all known targets") (Makefile\ Type (format "Makefile Type (%s)" (casual-make-mode-label major-mode)) <submenu> :enable nil :visible nil :style nil :selected nil :help nil))) (info Info-mode ((info-mode-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (Info-top-node "Top" Info-top-node :enable nil :visible nil :style nil :selected nil :help "Go to the Top node of this file") (Info-toc "Table of Contents" Info-toc :enable nil :visible nil :style nil :selected nil :help "Go to a node with table of contents of the current Info file") (Info-up "↑ Node" Info-up :enable nil :visible nil :style nil :selected nil :help "Go to the superior node of this node") (Info-backward-node "← Node" Info-backward-node :enable nil :visible nil :style nil :selected nil :help "Go backward one node, considering all nodes as forming one sequence") (Info-forward-node "→ Node" Info-forward-node :enable nil :visible nil :style nil :selected nil :help "Go forward one node, considering all nodes as forming one sequence") (info-apropos "Apropos…" info-apropos :enable nil :visible nil :style nil :selected nil :help "Search indices of all known Info files on your system for STRING") (Info-copy-current-node-name "Copy node name" Info-copy-current-node-name :enable nil :visible nil :style nil :selected nil :help "Put the name of the current Info node into the kill ring") (anju-info-goto-node-web "Open node in web" anju-info-goto-node-web :enable nil :visible nil :style nil :selected nil :help "Open node in web browser"))) (org-markup org-mode ((org-mode-operations-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (casual-org-toggle-images "Toggle Images" casual-org-toggle-images :enable nil :visible nil :style nil :selected nil :help "Toggle images") (visible-mode "Show Markup" visible-mode :enable nil :visible nil :style nil :selected nil :help "Toggle making all invisible text temporarily visible (Visible mode)"))) (markdown-markup markdown-mode ((markdown-mode-operations-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (markdown-toggle-markup-hiding "Hide Markup" markdown-toggle-markup-hiding :enable nil :visible nil :style nil :selected nil :help "Toggle the display or hiding of markup"))) (words text-mode ((count-words-separator "--" nil :enable nil :visible nil :style nil :selected nil :help nil) (count-words "Count Words" count-words :enable nil :visible nil :style nil :selected nil :help "Count words"))))"#
        ]],
    )
}

fn anju_context_inventory_reconfiguration_is_ordered_idempotent_and_resettable() -> ParityBatchCase
{
    ParityBatchCase::value(
        "anju_context_inventory_reconfiguration_is_ordered_idempotent_and_resettable",
        r##"(let ((before-default
                (default-value 'context-menu-functions))
               (before-type
                (copy-tree
                 (get 'context-menu-functions 'custom-type)))
               (before-saved
                (get 'context-menu-functions 'saved-value)))
         (unwind-protect
             (progn
               (set-default
                'context-menu-functions
                '(context-menu-global
                  context-menu-local
                  context-menu-minor
                  context-menu-middle-separator
                  context-menu-undo))
               (put 'context-menu-functions 'saved-value nil)
               (anju-reconfigure-context-menu-functions)
               (let ((once
                      (copy-sequence
                       (default-value 'context-menu-functions)))
                     (once-type
                      (copy-tree
                       (get 'context-menu-functions 'custom-type))))
                 (anju-reconfigure-context-menu-functions)
                 (let ((twice
                        (copy-sequence
                         (default-value 'context-menu-functions))))
                   (anju-reset-context-menu-functions)
                   (list
                    once
                    (equal once twice)
                    (length once-type)
                    (length
                     (delete-dups
                      (copy-sequence (cdr (nth 1 once-type)))))
                    (default-value 'context-menu-functions)))))
           (set-default 'context-menu-functions before-default)
           (put 'context-menu-functions 'custom-type before-type)
           (put 'context-menu-functions 'saved-value before-saved)))"##,
        expect![
            "OK ((anju-context-menu-dired anju-context-menu-org-mode anju-context-menu-org-agenda anju-context-menu-info-mode anju-context-menu-make-mode anju-context-menu-compile anju-context-menu-elisp anju-context-menu-edebug-eval anju-context-menu-xref anju-context-menu-scratch anju-context-menu-buffers anju-context-menu-region anju-context-menu-dictionary anju-context-menu-narrow anju-context-menu-open-in anju-context-menu-vc anju-context-menu-markup anju-context-menu-wordcount anju-context-menu-rectangle anju-context-menu-window context-menu-global context-menu-local context-menu-minor context-menu-middle-separator context-menu-undo) t 2 38 (context-menu-global context-menu-local context-menu-minor context-menu-middle-separator context-menu-undo))"
        ],
    )
}

fn anju_tty_click_distance_and_point_adjustment_follow_actual_event_positions() -> ParityBatchCase {
    ParityBatchCase::value(
        "anju_tty_click_distance_and_point_adjustment_follow_actual_event_positions",
        r##"(with-temp-buffer
         (insert "first\nsecond\nthird\n")
         (goto-char 2)
         (let* ((window (selected-window))
                (near
                 (list
                  'mouse-3
                  (list window 3 '(2 . 0) 0)))
                (far
                 (list
                  'mouse-3
                  (list window 15 '(2 . 2) 0)))
                calls)
           (cl-letf (((symbol-function 'display-graphic-p)
                      (lambda (&optional _) nil))
                     ((symbol-function 'mouse-set-point)
                      (lambda (event)
                        (push event calls))))
             (list
              (anju-click-and-point-distant-p near)
              (anju-click-and-point-distant-p far)
              (progn
                (anju-adjust-point-for-click near)
                (length calls))
              (progn
                (anju-adjust-point-for-click far)
                (length calls))))))"##,
        expect!["OK (nil t 0 1)"],
    )
}

pub(super) fn context_menu_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        anju_org_context_menu_tracks_heading_item_checkbox_table_link_and_body_workflows(),
        anju_copy_raw_org_link_and_exported_region_drive_real_clipboard_workflows(),
        anju_elisp_context_understands_defuns_ert_tests_lambdas_and_numeric_literals(),
        anju_extract_lambda_replaces_the_call_site_and_builds_an_editable_defun(),
        anju_narrow_context_selects_region_defun_org_markdown_and_widen_actions(),
        anju_region_context_builds_practical_plain_org_markdown_and_read_only_menus(),
        anju_dired_duplicate_file_performs_a_real_deterministic_copy_and_builds_file_menu(),
        anju_compilation_makefile_info_markup_and_wordcount_contexts_are_exact(),
        anju_context_inventory_reconfiguration_is_ordered_idempotent_and_resettable(),
        anju_tty_click_distance_and_point_adjustment_follow_actual_event_positions(),
    ]
}
