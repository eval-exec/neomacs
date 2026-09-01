use expect_test::expect;

use super::ParityBatchCase;

fn captures_and_reopens_the_live_document_in_documented_link_formats() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "zathura-documented-links"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (pdf (expand-file-name "Incident review Δ.pdf" sandbox))
       (proc-a "org.pwmt.zathura.PID-20")
       (proc-b "org.pwmt.zathura.PID-21")
       (zathura-session-proc nil)
       annotations selections properties prompts launches selected result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (with-temp-file pdf
          (insert "%PDF documented links\n"))
        (with-temp-buffer
          (org-mode)
          (cl-letf (((symbol-function 'dbus-list-names)
                     (lambda (&rest _)
                       (list "org.freedesktop.DBus" proc-a proc-b)))
                    ((symbol-function 'completing-read)
                     (lambda (prompt collection &rest _)
                       (when (null selections)
                         (let* ((metadata
                                 (funcall collection "" nil 'metadata))
                                (annotator
                                 (cdr
                                  (assq
                                   'annotation-function
                                   (cdr metadata)))))
                           (setq annotations
                                 (mapcar
                                  (lambda (candidate)
                                    (list
                                     candidate
                                     (substring-no-properties
                                      (funcall annotator candidate))))
                                  (list proc-a proc-b)))))
                       (push prompt selections)
                       proc-b))
                    ((symbol-function 'dbus-get-property)
                     (lambda (bus service path interface property)
                       (push
                        (list bus service path interface property)
                        properties)
                       (pcase property
                         ("pagenumber" 11)
                         ("filename" pdf))))
                    ((symbol-function 'read-string)
                     (lambda (prompt &rest _)
                       (push prompt prompts)
                       "Incident evidence Δ"))
                    ((symbol-function 'call-process)
                     (lambda (&rest args)
                       (push args launches)
                       0)))
            (setq selected (call-interactively #'zathura-select-proc))
            (call-interactively #'zathura-insert-org-elisp-link)
            (insert "\n")
            (call-interactively #'zathura-insert-hy-link)
            (let ((contents
                   (buffer-substring-no-properties
                    (point-min) (point-max))))
              (goto-char (point-min))
              (let ((org-confirm-elisp-link-function nil))
                (call-interactively #'org-open-at-point))
              (let* ((tree (org-element-parse-buffer))
                     (link
                      (car (org-element-map tree 'link #'identity))))
                (setq result
                      (list
                       :contents contents
                       :org-link
                       (list
                        (org-element-property :type link)
                        (org-element-property :path link)
                        (org-element-property :raw-link link)
                       (substring-no-properties
                         (org-element-interpret-data
                          (org-element-contents link))))
                       :selected selected
                       :session zathura-session-proc
                       :annotations annotations
                       :selections (nreverse selections)
                       :properties (nreverse properties)
                       :prompts (nreverse prompts)
                       :launches (nreverse launches)
                       :point (point))))
              result))))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r##"OK (:contents "[[elisp:(zathura \"[ORACLE-SANDBOX]/zathura-documented-links/Incident review Δ.pdf\" 11)][Incident evidence Δ]]\n<zathura \"[ORACLE-SANDBOX]/zathura-documented-links/Incident review Δ.pdf\" 11>" :org-link ("elisp" "(zathura \"[ORACLE-SANDBOX]/zathura-documented-links/Incident review Δ.pdf\" 11)" "elisp:(zathura \"[ORACLE-SANDBOX]/zathura-documented-links/Incident review Δ.pdf\" 11)" "Incident evidence Δ") :selected "org.pwmt.zathura.PID-21" :session "org.pwmt.zathura.PID-21" :annotations (("org.pwmt.zathura.PID-20" "   p.12 [ORACLE-SANDBOX]/zathura-documented-links/Incident review Δ.pdf") ("org.pwmt.zathura.PID-21" "   p.12 [ORACLE-SANDBOX]/zathura-documented-links/Incident review Δ.pdf")) :selections ("Select process: " "Select process: " "Select process: ") :properties ((:session "org.pwmt.zathura.PID-20" "/org/pwmt/zathura" "org.pwmt.zathura" "pagenumber") (:session "org.pwmt.zathura.PID-20" "/org/pwmt/zathura" "org.pwmt.zathura" "filename") (:session "org.pwmt.zathura.PID-21" "/org/pwmt/zathura" "org.pwmt.zathura" "pagenumber") (:session "org.pwmt.zathura.PID-21" "/org/pwmt/zathura" "org.pwmt.zathura" "filename") (:session "org.pwmt.zathura.PID-21" "/org/pwmt/zathura" "org.pwmt.zathura" "pagenumber") (:session "org.pwmt.zathura.PID-21" "/org/pwmt/zathura" "org.pwmt.zathura" "filename") (:session "org.pwmt.zathura.PID-21" "/org/pwmt/zathura" "org.pwmt.zathura" "pagenumber") (:session "org.pwmt.zathura.PID-21" "/org/pwmt/zathura" "org.pwmt.zathura" "filename")) :prompts ("Description: ") :launches (("zathura" nil 0 nil "-P" "11" "[ORACLE-SANDBOX]/zathura-documented-links/Incident review Δ.pdf")) :point 1)"##
    ]];
    ParityBatchCase::value(
        "captures_and_reopens_the_live_document_in_documented_link_formats",
        elisp_form,
        expect,
    )
}

fn inserts_a_relative_org_link_from_the_live_document() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "zathura-insert-link"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (notes (expand-file-name "notes" sandbox))
       (reports (expand-file-name "reports" sandbox))
       (note (expand-file-name "review.org" notes))
       (pdf (expand-file-name "Q3 capacity Δ.pdf" reports))
       (proc "org.pwmt.zathura.PID-41")
       (zathura-session-proc proc)
       (zathura-link-file-path-type 'relative)
       (zathura-mode nil)
       (org-file-apps (copy-tree org-file-apps))
       (org-link-parameters (copy-tree org-link-parameters))
       buffer dbus-calls prompts result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory notes t)
        (make-directory reports t)
        (with-temp-file note
          (insert "* Capacity review\n\n"))
        (with-temp-file pdf
          (insert "%PDF-1.7 parity fixture\n"))
        (setq buffer (find-file-noselect note))
        (with-current-buffer buffer
          (org-mode)
          (goto-char (point-max))
          (zathura-mode 1)
          (cl-letf (((symbol-function 'dbus-list-names)
                     (lambda (&rest args)
                       (push (cons :list-names args) dbus-calls)
                       (list "org.freedesktop.DBus" proc)))
                    ((symbol-function 'dbus-get-property)
                     (lambda (bus service path interface property)
                       (push
                        (list :get bus service path interface property)
                        dbus-calls)
                       (pcase property
                         ("filename" pdf)
                         ("pagenumber" 6))))
                    ((symbol-function 'read-string)
                     (lambda (prompt &rest _)
                       (push prompt prompts)
                       (if (= (length prompts) 1)
                           "Capacity Δ review"
                         "Absolute capacity source"))))
            (call-interactively #'zathura-insert-org-link)
            (insert "\n")
            (setq zathura-link-file-path-type 'absolute)
            (call-interactively #'zathura-insert-org-link))
          (zathura-mode -1)
          (let ((tree (org-element-parse-buffer)))
            (setq result
                  (list
                   :file buffer-file-name
                   :mode major-mode
                   :contents
                   (buffer-substring-no-properties
                    (point-min) (point-max))
                   :point-at-end (= (point) (point-max))
                   :links
                   (org-element-map
                       tree 'link
                     (lambda (link)
                       (list
                        (org-element-property :type link)
                        (org-element-property :path link)
                        (org-element-property :raw-link link)
                        (substring-no-properties
                         (org-element-interpret-data
                          (org-element-contents link))))))
                   :calls (nreverse dbus-calls)
                   :prompts (nreverse prompts)
                   :modified (buffer-modified-p)))
            result)))
    (when zathura-mode
      (zathura-mode -1))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (set-buffer-modified-p nil))
      (kill-buffer buffer))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r##"OK (:file "[ORACLE-SANDBOX]/zathura-insert-link/notes/review.org" :mode org-mode :contents "* Capacity review\n\n[[pdf:../reports/Q3 capacity Δ.pdf::7][Capacity Δ review]]\n[[pdf:[ORACLE-SANDBOX]/zathura-insert-link/reports/Q3 capacity Δ.pdf::7][Absolute capacity source]]" :point-at-end t :links (("pdf" "../reports/Q3 capacity Δ.pdf::7" "pdf:../reports/Q3 capacity Δ.pdf::7" "Capacity Δ review") ("pdf" "[ORACLE-SANDBOX]/zathura-insert-link/reports/Q3 capacity Δ.pdf::7" "pdf:[ORACLE-SANDBOX]/zathura-insert-link/reports/Q3 capacity Δ.pdf::7" "Absolute capacity source")) :calls ((:list-names :session) (:get :session "org.pwmt.zathura.PID-41" "/org/pwmt/zathura" "org.pwmt.zathura" "filename") (:get :session "org.pwmt.zathura.PID-41" "/org/pwmt/zathura" "org.pwmt.zathura" "pagenumber") (:list-names :session) (:get :session "org.pwmt.zathura.PID-41" "/org/pwmt/zathura" "org.pwmt.zathura" "filename") (:get :session "org.pwmt.zathura.PID-41" "/org/pwmt/zathura" "org.pwmt.zathura" "pagenumber")) :prompts ("Description: " "Description: ") :modified t)"##
    ]];
    ParityBatchCase::value(
        "inserts_a_relative_org_link_from_the_live_document",
        elisp_form,
        expect,
    )
}

fn views_and_jumps_between_existing_and_new_documents() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "zathura-view-jump"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (current (expand-file-name "current.pdf" sandbox))
       (next (expand-file-name "new plan Δ.pdf" sandbox))
       (default-directory (file-name-as-directory sandbox))
       (proc "org.pwmt.zathura.PID-52")
       (zathura-session-proc proc)
       (zathura-mode nil)
       (org-file-apps (copy-tree org-file-apps))
       (org-link-parameters (copy-tree org-link-parameters))
       (current-file current)
       calls focus list-count result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (with-temp-file current
          (insert "%PDF current\n"))
        (with-temp-file next
          (insert "%PDF next\n"))
        (cl-letf (((symbol-function 'dbus-list-names)
                   (lambda (&rest _)
                     (setq list-count (1+ (or list-count 0)))
                     (list "org.freedesktop.DBus" proc)))
                  ((symbol-function 'dbus-get-property)
                   (lambda (_bus _service _path _interface property)
                     (pcase property
                       ("filename" current-file))))
                  ((symbol-function 'dbus-call-method)
                   (lambda (&rest args)
                     (push args calls)
                     (when (string= (nth 4 args) "OpenDocument")
                       (setq current-file (nth 5 args)))
                     :dbus-ok))
                  ((symbol-function 'selected-frame)
                   (lambda () 'frame-primary))
                  ((symbol-function 'select-frame-set-input-focus)
                   (lambda (frame &rest _)
                     (when (symbolp frame)
                       (push frame focus))
                     :focused)))
          (zathura-view-file current 5)
          (zathura-jump-file next 3)
          (zathura-view-link "new plan Δ.pdf::9" nil)
          (zathura-mode 1)
          (with-temp-buffer
            (org-mode)
            (insert "[[pdf:current.pdf::6][Return to current plan]]")
            (goto-char (point-min))
            (search-forward "Return to current")
            (call-interactively #'zathura-jump-link-at-point))
          (setq result
                (list
                 :calls (nreverse calls)
                 :focus (nreverse focus)
                 :list-count list-count
                 :current-file current-file
                 :session zathura-session-proc))
          result))
    (when zathura-mode
      (zathura-mode -1))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r##"OK (:calls ((:session "org.pwmt.zathura.PID-52" "/org/pwmt/zathura" "org.pwmt.zathura" "GotoPage" :uint32 4) (:session "org.pwmt.zathura.PID-52" "/org/pwmt/zathura" "org.pwmt.zathura" "OpenDocument" "[ORACLE-SANDBOX]/zathura-view-jump/new plan Δ.pdf" "" :int32 2) (:session "org.pwmt.zathura.PID-52" "/org/pwmt/zathura" "org.pwmt.zathura" "GotoPage" :uint32 8) (:session "org.pwmt.zathura.PID-52" "/org/pwmt/zathura" "org.pwmt.zathura" "OpenDocument" "[ORACLE-SANDBOX]/zathura-view-jump/current.pdf" "" :int32 5)) :focus (frame-primary frame-primary) :list-count 4 :current-file "[ORACLE-SANDBOX]/zathura-view-jump/current.pdf" :session "org.pwmt.zathura.PID-52")"##
    ]];
    ParityBatchCase::value(
        "views_and_jumps_between_existing_and_new_documents",
        elisp_form,
        expect,
    )
}

fn renders_folds_and_navigates_a_document_outline() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((proc "org.pwmt.zathura.PID-63")
       (zathura-session-proc proc)
       (zathura-outline-page-column 24)
       (zathura-outline-indent 3)
       (zathura-outline-numbered t)
       (document-info
        "{\"index\":[{\"title\":\"Overview\",\"page\":1,\"sub-index\":[{\"title\":\"Capacity Δ\",\"page\":3},{\"title\":\"Risks\",\"page\":5}]},{\"title\":\"Appendix\",\"page\":9}]}")
       outline-buffer displayed-buffer calls focus-count
       unnumbered-contents unnumbered-rows result)
  (unwind-protect
      (cl-letf (((symbol-function 'dbus-list-names)
                 (lambda (&rest _)
                   (list "org.freedesktop.DBus" proc)))
                ((symbol-function 'dbus-get-property)
                 (lambda (_bus _service _path _interface property)
                   (pcase property
                     ("documentinfo" document-info))))
                ((symbol-function 'dbus-call-method)
                 (lambda (&rest args)
                   (push args calls)
                   :dbus-ok))
                ((symbol-function 'pop-to-buffer)
                 (lambda (buffer-or-name &rest _)
                   (setq displayed-buffer (get-buffer buffer-or-name))))
                ((symbol-function 'select-frame-set-input-focus)
                 (lambda (&rest _)
                   (setq focus-count (1+ (or focus-count 0)))
                   :focused)))
        (zathura-show-outline)
        (setq outline-buffer (get-buffer "*zathura-outline*"))
        (with-current-buffer outline-buffer
            (let ((contents
                   (buffer-substring-no-properties
                    (point-min) (point-max)))
                  (rows (neomacs-melpa-zathura--outline-rows)))
              (goto-char (point-min))
              (outline-hide-subtree)
              (forward-line 1)
              (let ((child-hidden
                     (not (null (get-char-property (point) 'invisible)))))
                (outline-show-all)
                (let ((child-visible
                       (null (get-char-property (point) 'invisible))))
                  (goto-char (point-min))
                  (forward-line 1)
                  (call-interactively (key-binding (kbd "RET")))
                  (goto-char (point-min))
                  (forward-line 3)
                  (call-interactively #'zathura-outline-jump)
                  (setq result
                        (list
                         :displayed (buffer-name displayed-buffer)
                         :mode major-mode
                         :mode-name (format "%s" mode-name)
                         :contents contents
                         :rows rows
                         :read-only buffer-read-only
                         :truncate truncate-lines
                         :keys
                         (list
                          (key-binding (kbd "RET"))
                          (key-binding (kbd "q")))
                         :folded child-hidden
                         :shown child-visible
                         :calls (nreverse calls)
                         :focus-count focus-count
                         :point-line (line-number-at-pos)))))))
        (setq zathura-outline-numbered nil)
        (zathura-show-outline)
        (with-current-buffer outline-buffer
          (setq unnumbered-contents
                (buffer-substring-no-properties
                 (point-min) (point-max))
                unnumbered-rows
                (neomacs-melpa-zathura--outline-rows)))
        (setq result
              (append
               result
               (list
                :unnumbered-contents unnumbered-contents
                :unnumbered-rows unnumbered-rows)))
        result)
    (when (buffer-live-p outline-buffer)
      (kill-buffer outline-buffer))))
"####;
    let expect = expect![[
        r##"OK (:displayed "*zathura-outline*" :mode zathura-outline-mode :mode-name "Zathura-Outline" :contents "1 Overview\11\0111\n   1.1 Capacity Δ\0113\n   1.2 Risks\11\0115\n2 Appendix\11\119\n" :rows (("1 Overview\11\0111" 1 1) ("   1.1 Capacity Δ\0113" 3 2) ("   1.2 Risks\11\0115" 5 2) ("2 Appendix\11\119" 9 1)) :read-only t :truncate t :keys (zathura-outline-view bury-buffer) :folded t :shown t :calls ((:session "org.pwmt.zathura.PID-63" "/org/pwmt/zathura" "org.pwmt.zathura" "GotoPage" :uint32 2) (:session "org.pwmt.zathura.PID-63" "/org/pwmt/zathura" "org.pwmt.zathura" "GotoPage" :uint32 8)) :focus-count 1 :point-line 4 :unnumbered-contents "Overview\11\0111\n   Capacity Δ\11\0113\n   Risks\11\0115\nAppendix\11\119\n" :unnumbered-rows (("Overview\11\0111" 1 1) ("   Capacity Δ\11\0113" 3 2) ("   Risks\11\0115" 5 2) ("Appendix\11\119" 9 1)))"##
    ]];
    ParityBatchCase::value(
        "renders_folds_and_navigates_a_document_outline",
        elisp_form,
        expect,
    )
}

fn inserts_the_document_outline_as_real_org_headings_and_links() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "zathura-insert-outline"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (notes (expand-file-name "notes" sandbox))
       (reports (expand-file-name "reports" sandbox))
       (pdf (expand-file-name "Architecture Δ.pdf" reports))
       (default-directory (file-name-as-directory notes))
       (proc "org.pwmt.zathura.PID-74")
       (zathura-session-proc proc)
       (zathura-link-file-path-type 'relative)
       (document-info
        "{\"index\":[{\"title\":\"Architecture\",\"page\":2,\"sub-index\":[{\"title\":\"Data flow\",\"page\":7},{\"title\":\"Open questions\"}]},{\"title\":\"Operations\",\"page\":11}]}")
       (zathura-mode nil)
       (org-file-apps (copy-tree org-file-apps))
       (org-link-parameters (copy-tree org-link-parameters))
       result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory notes t)
        (make-directory reports t)
        (with-temp-file pdf
          (insert "%PDF outline\n"))
        (with-temp-buffer
          (org-mode)
          (zathura-mode 1)
          (cl-letf (((symbol-function 'dbus-list-names)
                     (lambda (&rest _)
                       (list "org.freedesktop.DBus" proc)))
                    ((symbol-function 'dbus-get-property)
                     (lambda (_bus _service _path _interface property)
                       (pcase property
                         ("filename" pdf)
                         ("documentinfo" document-info)))))
            (call-interactively #'zathura-insert-outline))
          (let ((tree (org-element-parse-buffer)))
            (setq result
                  (list
                   :contents
                   (buffer-substring-no-properties
                    (point-min) (point-max))
                   :headings
                   (org-element-map
                       tree 'headline
                     (lambda (headline)
                       (list
                        (org-element-property :level headline)
                        (org-element-property :raw-value headline))))
                   :links
                   (org-element-map
                       tree 'link
                     (lambda (link)
                       (list
                        (org-element-property :type link)
                        (org-element-property :path link)
                        (org-element-property :raw-link link)
                        (substring-no-properties
                         (org-element-interpret-data
                          (org-element-contents link))))))
                   :point (point)
                   :modified (buffer-modified-p)))
            (zathura-mode -1)
            result)))
    (when zathura-mode
      (zathura-mode -1))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r##"OK (:contents "* Architecture\n[[pdf:../reports/Architecture Δ.pdf::2][Architecture]]\n\n** Data flow\n[[pdf:../reports/Architecture Δ.pdf::7][Data flow]]\n\n** Open questions\n* Operations\n[[pdf:../reports/Architecture Δ.pdf::11][Operations]]\n\n" :headings ((1 "Architecture") (2 "Data flow") (2 "Open questions") (1 "Operations")) :links (("pdf" "../reports/Architecture Δ.pdf::2" "pdf:../reports/Architecture Δ.pdf::2" "Architecture") ("pdf" "../reports/Architecture Δ.pdf::7" "pdf:../reports/Architecture Δ.pdf::7" "Data flow") ("pdf" "../reports/Architecture Δ.pdf::11" "pdf:../reports/Architecture Δ.pdf::11" "Operations")) :point 224 :modified t)"##
    ]];
    ParityBatchCase::value(
        "inserts_the_document_outline_as_real_org_headings_and_links",
        elisp_form,
        expect,
    )
}

fn routes_pdf_visits_through_the_global_mode_but_visits_text_normally() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "zathura-global-mode"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (pdf (expand-file-name "Guide Δ.PDF" sandbox))
       (text-file (expand-file-name "notes.txt" sandbox))
       (proc "org.pwmt.zathura.PID-85")
       (zathura-session-proc proc)
       (zathura-mode nil)
       (org-file-apps (copy-tree org-file-apps))
       (org-link-parameters (copy-tree org-link-parameters))
       (current-file "")
       calls focus-count text-buffer result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (with-temp-file pdf
          (insert "%PDF routed\n"))
        (with-temp-file text-file
          (insert "ordinary text remains inside Emacs\n"))
        (cl-letf (((symbol-function 'dbus-list-names)
                   (lambda (&rest _)
                     (list "org.freedesktop.DBus" proc)))
                  ((symbol-function 'dbus-get-property)
                   (lambda (_bus _service _path _interface property)
                     (pcase property
                       ("filename" current-file))))
                  ((symbol-function 'dbus-call-method)
                   (lambda (&rest args)
                     (push args calls)
                     (when (string= (nth 4 args) "OpenDocument")
                       (setq current-file (nth 5 args)))
                     :dbus-ok))
                  ((symbol-function 'select-frame-set-input-focus)
                   (lambda (&rest _)
                     (setq focus-count (1+ (or focus-count 0)))
                     :focused)))
          (zathura-mode 1)
          (let ((enabled
                 (list
                  zathura-mode
                  (not
                   (null
                    (advice-member-p
                     #'zathura--find-file-advice 'find-file)))
                  (cdr (assoc "\\.pdf\\'" org-file-apps))
                  (org-link-get-parameter "pdf" :follow))))
            (save-window-excursion
              (save-current-buffer
                (find-file pdf)
                (find-file text-file)
                (setq text-buffer (current-buffer))))
            (let ((during
                   (list
                    :pdf-buffer (and (get-file-buffer pdf) t)
                    :text-file
                    (and (buffer-live-p text-buffer)
                         (with-current-buffer text-buffer buffer-file-name))
                    :text
                    (neomacs-melpa-zathura--buffer-string text-buffer)
                    :calls (nreverse calls)
                    :focus-count focus-count)))
              (zathura-mode -1)
              (setq result
                    (list
                     :enabled enabled
                     :during during
                     :disabled
                     (list
                      zathura-mode
                      (not
                       (null
                        (advice-member-p
                         #'zathura--find-file-advice 'find-file)))
                      (cdr (assoc "\\.pdf\\'" org-file-apps))
                      (org-link-get-parameter "pdf" :follow))))
              result))))
    (when zathura-mode
      (zathura-mode -1))
    (when (buffer-live-p text-buffer)
      (kill-buffer text-buffer))
    (let ((pdf-buffer (get-file-buffer pdf)))
      (when (buffer-live-p pdf-buffer)
        (kill-buffer pdf-buffer)))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r##"OK (:enabled (t t zathura-view-file zathura-view-link) :during (:pdf-buffer nil :text-file "[ORACLE-SANDBOX]/zathura-global-mode/notes.txt" :text "ordinary text remains inside Emacs\n" :calls ((:session "org.pwmt.zathura.PID-85" "/org/pwmt/zathura" "org.pwmt.zathura" "OpenDocument" "[ORACLE-SANDBOX]/zathura-global-mode/Guide Δ.PDF" "" :int32 0)) :focus-count 1) :disabled (nil nil default zathura-view-link))"##
    ]];
    ParityBatchCase::value(
        "routes_pdf_visits_through_the_global_mode_but_visits_text_normally",
        elisp_form,
        expect,
    )
}

fn starts_a_new_viewer_session_when_none_is_running() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "zathura-start-session"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (pdf (expand-file-name "Launch plan.pdf" sandbox))
       (proc "org.pwmt.zathura.PID-96")
       (zathura-session-proc nil)
       list-count starts sleeps calls focus-count result)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (with-temp-file pdf
          (insert "%PDF launch\n"))
        (cl-letf (((symbol-function 'dbus-list-names)
                   (lambda (&rest _)
                     (setq list-count (1+ (or list-count 0)))
                     (if (< list-count 3)
                         (list "org.freedesktop.DBus")
                       (list "org.freedesktop.DBus" proc))))
                  ((symbol-function 'start-process)
                   (lambda (&rest args)
                     (push args starts)
                     'fake-zathura-process))
                  ((symbol-function 'sleep-for)
                   (lambda (&rest args)
                     (push args sleeps)))
                  ((symbol-function 'dbus-get-property)
                   (lambda (_bus _service _path _interface property)
                     (pcase property
                       ("filename" pdf))))
                  ((symbol-function 'dbus-call-method)
                   (lambda (&rest args)
                     (push args calls)
                     :dbus-ok))
                  ((symbol-function 'select-frame-set-input-focus)
                   (lambda (&rest _)
                     (setq focus-count (1+ (or focus-count 0)))
                     :focused)))
          (zathura-view-file pdf 4)
          (setq result
                (list
                 :session zathura-session-proc
                 :list-count list-count
                 :starts (nreverse starts)
                 :sleeps (nreverse sleeps)
                 :calls (nreverse calls)
                 :focus-count focus-count))
          result))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r##"OK (:session "org.pwmt.zathura.PID-96" :list-count 3 :starts (("zathura" nil "zathura" "[ORACLE-SANDBOX]/zathura-start-session/Launch plan.pdf")) :sleeps ((0.1)) :calls ((:session "org.pwmt.zathura.PID-96" "/org/pwmt/zathura" "org.pwmt.zathura" "GotoPage" :uint32 3)) :focus-count 1)"##
    ]];
    ParityBatchCase::value(
        "starts_a_new_viewer_session_when_none_is_running",
        elisp_form,
        expect,
    )
}

fn reports_a_failed_viewer_start_after_the_bounded_polling_window() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((sandbox
        (expand-file-name
         "zathura-start-timeout"
         (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
       (pdf (expand-file-name "Unavailable viewer.pdf" sandbox))
       (zathura-session-proc nil)
       list-count starts sleeps outcome)
  (unwind-protect
      (progn
        (when (file-directory-p sandbox)
          (delete-directory sandbox t))
        (make-directory sandbox t)
        (with-temp-file pdf
          (insert "%PDF start timeout\n"))
        (cl-letf (((symbol-function 'dbus-list-names)
                   (lambda (&rest _)
                     (setq list-count (1+ (or list-count 0)))
                     (list "org.freedesktop.DBus")))
                  ((symbol-function 'start-process)
                   (lambda (&rest args)
                     (push args starts)
                     'fake-zathura-process))
                  ((symbol-function 'sleep-for)
                   (lambda (&rest args)
                     (push args sleeps))))
          (setq outcome
                (neomacs-melpa-zathura--signal
                 (lambda () (zathura-view-file pdf 6))))
          (list
           :outcome outcome
           :session zathura-session-proc
           :list-count list-count
           :starts (nreverse starts)
           :sleep-count (length sleeps)
           :sleeps (nreverse sleeps))))
    (when (file-directory-p sandbox)
      (delete-directory sandbox t))))
"####;
    let expect = expect![[
        r##"OK (:outcome (:signal error ("Failed to start zathura D-Bus process")) :session nil :list-count 22 :starts (("zathura" nil "zathura" "[ORACLE-SANDBOX]/zathura-start-timeout/Unavailable viewer.pdf")) :sleep-count 20 :sleeps ((0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1) (0.1)))"##
    ]];
    ParityBatchCase::value(
        "reports_a_failed_viewer_start_after_the_bounded_polling_window",
        elisp_form,
        expect,
    )
}

fn reports_actionable_errors_for_missing_sessions_documents_and_links() -> ParityBatchCase {
    let elisp_form = r####"
(let* ((proc "org.pwmt.zathura.PID-107")
       (zathura-session-proc nil)
       no-session no-document no-page no-link no-running)
  (cl-letf (((symbol-function 'dbus-list-names)
             (lambda (&rest _) nil)))
    (setq no-session
          (neomacs-melpa-zathura--signal
           (lambda () (call-interactively #'zathura-select-proc))))
    (setq no-running
          (neomacs-melpa-zathura--signal
           (lambda () (zathura-get-link-details)))))
  (let ((zathura-session-proc proc))
    (cl-letf (((symbol-function 'dbus-list-names)
               (lambda (&rest _) (list proc)))
              ((symbol-function 'dbus-get-property)
               (lambda (_bus _service _path _interface property)
                 (pcase property
                   ("filename" "")
                   ("pagenumber" 0))))
              ((symbol-function 'read-string)
               (lambda (&rest _) "Unused description")))
      (with-temp-buffer
        (setq no-document
              (neomacs-melpa-zathura--signal
               (lambda ()
                 (call-interactively #'zathura-insert-org-link)))))))
  (with-temp-buffer
    (insert "Unlinked outline row\n")
    (zathura-outline-mode)
    (goto-char (point-min))
    (setq no-page
          (neomacs-melpa-zathura--signal
           (lambda () (call-interactively #'zathura-outline-view)))))
  (with-temp-buffer
    (org-mode)
    (insert "[[https://example.test][ordinary web link]]")
    (goto-char (point-min))
    (search-forward "ordinary")
    (setq no-link
          (neomacs-melpa-zathura--signal
           (lambda ()
             (call-interactively #'zathura-jump-link-at-point)))))
  (list
   :select-without-process no-session
   :details-without-process no-running
   :insert-without-document no-document
   :outline-row-without-page no-page
   :non-pdf-link no-link))
"####;
    let expect = expect![[
        r##"OK (:select-without-process (:signal user-error ("No zathura process is running")) :details-without-process (:signal error ("Zathura is not running")) :insert-without-document (:signal user-error ("No document open in zathura")) :outline-row-without-page (:signal user-error ("No page on this line")) :non-pdf-link (:signal user-error ("No pdf link at point")))"##
    ]];
    ParityBatchCase::value(
        "reports_actionable_errors_for_missing_sessions_documents_and_links",
        elisp_form,
        expect,
    )
}

pub(super) fn practical_workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        captures_and_reopens_the_live_document_in_documented_link_formats(),
        inserts_a_relative_org_link_from_the_live_document(),
        views_and_jumps_between_existing_and_new_documents(),
        renders_folds_and_navigates_a_document_outline(),
        inserts_the_document_outline_as_real_org_headings_and_links(),
        routes_pdf_visits_through_the_global_mode_but_visits_text_normally(),
        starts_a_new_viewer_session_when_none_is_running(),
        reports_a_failed_viewer_start_after_the_bounded_polling_window(),
        reports_actionable_errors_for_missing_sessions_documents_and_links(),
    ]
}
