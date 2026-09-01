use expect_test::expect;

use super::ParityBatchCase;

fn editing_a_real_lisp_file_switches_to_anti_zenburn_and_restores_the_baseline_theme()
-> ParityBatchCase {
    ParityBatchCase::value(
        "editing_a_real_lisp_file_switches_to_anti_zenburn_and_restores_the_baseline_theme",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "anti-zenburn-lisp-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source
        (expand-file-name "ledger.el" root))
       (default-directory root)
       buffer
       before
       during
       edited
       restored
       disk)
  (unwind-protect
      (progn
        (neomacs-anti-zenburn-test-cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           ";;; ledger.el --- Settlement workflow\n\n"
           "(defun settle-invoice (invoice)\n"
           "  \"Return the state recorded for INVOICE.\"\n"
           "  ;; Keep the decision visible to reviewers.\n"
           "  (if invoice\n"
           "      (message \"invoice settled\")\n"
           "    :pending))\n"))
        (eval
         '(deftheme
              neomacs-anti-zenburn-baseline
            "Baseline used by the editing workflow."))
        (custom-theme-set-faces
         'neomacs-anti-zenburn-baseline
         '(default
            ((t
              (:foreground "#101820"
               :background "#f7f3e8"))))
         '(region
            ((t
              (:background "#ffe090"))))
         '(mode-line
            ((t
              (:foreground "#f7f3e8"
               :background "#304050"
               :box (:line-width 1 :style released-button)))))
         '(font-lock-keyword-face
            ((t
              (:foreground "#702070"
               :weight normal))))
         '(font-lock-function-name-face
            ((t
              (:foreground "#006050"))))
         '(font-lock-doc-face
            ((t
              (:foreground "#705020"
               :slant italic))))
         '(font-lock-comment-face
            ((t
              (:foreground "#607080"
               :slant italic))))
         '(font-lock-string-face
            ((t
              (:foreground "#905000"))))
         '(font-lock-builtin-face
            ((t
              (:foreground "#204090"
               :weight normal)))))
        (defvar fci-rule-color "unconfigured")
        (custom-theme-set-variables
         'neomacs-anti-zenburn-baseline
         '(fci-rule-color "#d0c8b8"))
        (provide-theme
         'neomacs-anti-zenburn-baseline)
        (enable-theme
         'neomacs-anti-zenburn-baseline)
        (setq buffer (find-file-noselect source))
        (switch-to-buffer buffer)
        (emacs-lisp-mode)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "(if invoice")
        (set-mark (match-beginning 0))
        (search-forward ":pending)")
        (setq transient-mark-mode t)
        (activate-mark)
        (setq before
              (list
               :file
               (file-relative-name buffer-file-name root)
               :mode major-mode
               :content
               (buffer-substring-no-properties
                (point-min)
                (point-max))
               :modified (buffer-modified-p)
               :point (point)
               :mark (mark)
               :mark-active mark-active
               :selected-text
               (buffer-substring-no-properties
                (region-beginning)
                (region-end))
               :themes
               (copy-sequence custom-enabled-themes)
               :faces
               (neomacs-anti-zenburn-test-face-state
                '(default
                  region
                  mode-line
                  font-lock-keyword-face
                  font-lock-function-name-face
                  font-lock-doc-face
                  font-lock-comment-face
                  font-lock-string-face
                  font-lock-builtin-face))
               :fci-rule-color fci-rule-color))
        (load-theme 'anti-zenburn t)
        (font-lock-flush)
        (font-lock-ensure)
        (setq during
              (list
               :themes
               (copy-sequence custom-enabled-themes)
               :point (point)
               :mark (mark)
               :mark-active mark-active
               :tokens
               (neomacs-anti-zenburn-test-token-state
                '("defun"
                  "settle-invoice"
                  "Return the state"
                  "Keep the decision"
                  "if"
                  "message"
                  "\"invoice settled\""
                  ":pending"))
               :faces
               (neomacs-anti-zenburn-test-face-state
                '(default
                  region
                  mode-line
                  font-lock-keyword-face
                  font-lock-function-name-face
                  font-lock-doc-face
                  font-lock-comment-face
                  font-lock-string-face
                  font-lock-builtin-face))
               :fci-rule-color fci-rule-color))
        (goto-char (point-min))
        (search-forward ":pending")
        (replace-match ":paid" t t)
        (save-buffer)
        (font-lock-ensure)
        (setq edited
              (list
               :content
               (buffer-substring-no-properties
                (point-min)
                (point-max))
               :modified (buffer-modified-p)
               :point (point)
               :line (line-number-at-pos)
               :column (current-column)
               :tokens
               (neomacs-anti-zenburn-test-token-state
                '(":paid"))))
        (disable-theme 'anti-zenburn)
        (setq restored
              (list
               :themes
               (copy-sequence custom-enabled-themes)
               :faces
               (neomacs-anti-zenburn-test-face-state
                '(default
                  region
                  mode-line
                  font-lock-keyword-face
                  font-lock-function-name-face
                  font-lock-doc-face
                  font-lock-comment-face
                  font-lock-string-face
                  font-lock-builtin-face))
               :fci-rule-color fci-rule-color))
        (setq disk
              (neomacs-anti-zenburn-test-file-string source)))
    (neomacs-anti-zenburn-test-cleanup root))
  (list
   :before before
   :during during
   :edited edited
   :restored restored
   :disk disk))
"####,
        expect![[
            r##"OK (:before (:file "ledger.el" :mode emacs-lisp-mode :content ";;; ledger.el --- Settlement workflow\n\n(defun settle-invoice (invoice)\n  \"Return the state recorded for INVOICE.\"\n  ;; Keep the decision visible to reviewers.\n  (if invoice\n      (message \"invoice settled\")\n    :pending))\n" :modified nil :point 221 :mark 162 :mark-active t :selected-text "(if invoice\n      (message \"invoice settled\")\n    :pending)" :themes (neomacs-anti-zenburn-baseline) :faces ((:face default :foreground "#101820" :background "#f7f3e8" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :foreground unspecified :background "#ffe090" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :foreground "#f7f3e8" :background "#304050" :weight unspecified :slant unspecified :underline unspecified :box #1=(:line-width 1 :style released-button) :inherit unspecified) (:face font-lock-keyword-face :foreground "#702070" :background unspecified :weight normal :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :foreground "#006050" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :foreground "#705020" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :foreground "#607080" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :foreground "#905000" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :foreground "#204090" :background unspecified :weight normal :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :fci-rule-color "#d0c8b8") :during (:themes (anti-zenburn neomacs-anti-zenburn-baseline) :point 221 :mark 162 :mark-active t :tokens ((:token "defun" :face font-lock-keyword-face) (:token "settle-invoice" :face font-lock-function-name-face) (:token "Return the state" :face font-lock-doc-face) (:token "Keep the decision" :face font-lock-comment-face) (:token "if" :face font-lock-keyword-face) (:token "message" :face nil) (:token "\"invoice settled\"" :face font-lock-string-face) (:token ":pending" :face font-lock-builtin-face)) :faces ((:face default :foreground "#232333" :background "#c0c0c0" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :foreground unspecified :background "#d4d4d4" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :foreground "#704d70" :background "#d4d4d4" :weight unspecified :slant unspecified :underline unspecified :box (:line-width -1 :style released-button) :inherit unspecified) (:face font-lock-keyword-face :foreground "#0f2050" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :foreground "#6c1f1c" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :foreground "#603a60" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :foreground "#806080" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :foreground "#336c6c" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :foreground "#232333" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :fci-rule-color "#c7c7c7") :edited (:content ";;; ledger.el --- Settlement workflow\n\n(defun settle-invoice (invoice)\n  \"Return the state recorded for INVOICE.\"\n  ;; Keep the decision visible to reviewers.\n  (if invoice\n      (message \"invoice settled\")\n    :paid))\n" :modified nil :point 217 :line 8 :column 9 :tokens ((:token ":paid" :face font-lock-builtin-face))) :restored (:themes (neomacs-anti-zenburn-baseline) :faces ((:face default :foreground "#101820" :background "#f7f3e8" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :foreground unspecified :background "#ffe090" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :foreground "#f7f3e8" :background "#304050" :weight unspecified :slant unspecified :underline unspecified :box #1# :inherit unspecified) (:face font-lock-keyword-face :foreground "#702070" :background unspecified :weight normal :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :foreground "#006050" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :foreground "#705020" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :foreground "#607080" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :foreground "#905000" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :foreground "#204090" :background unspecified :weight normal :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :fci-rule-color "#d0c8b8") :disk ";;; ledger.el --- Settlement workflow\n\n(defun settle-invoice (invoice)\n  \"Return the state recorded for INVOICE.\"\n  ;; Keep the decision visible to reviewers.\n  (if invoice\n      (message \"invoice settled\")\n    :paid))\n")"##
        ]],
    )
}

fn completing_a_real_org_runbook_keeps_the_saved_document_and_visual_structure_exact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "completing_a_real_org_runbook_keeps_the_saved_document_and_visual_structure_exact",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "anti-zenburn-org-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (runbook
        (expand-file-name "incident-response.org" root))
       (default-directory root)
       buffer
       result)
  (unwind-protect
      (progn
        (neomacs-anti-zenburn-test-cleanup root)
        (make-directory root t)
        (with-temp-file runbook
          (insert
           "#+title: Ledger Incident Response\n"
           "#+author: Operations Team\n\n"
           "* TODO Restore ledger service :operations:\n"
           "SCHEDULED: <2026-08-03 Mon 09:30>\n"
           ":PROPERTIES:\n"
           ":OWNER: Ada\n"
           ":END:\n"
           "- [ ] Confirm the replica is current\n"
           "- [X] Notify the incident channel\n\n"
           "See [[https://status.example.invalid][service status]].\n\n"
           "#+begin_src emacs-lisp\n"
           "(message \"ledger restored\")\n"
           "#+end_src\n"))
        (require 'org)
        (setq buffer (find-file-noselect runbook))
        (switch-to-buffer buffer)
        (org-mode)
        (setq-local org-log-done nil)
        (load-theme 'anti-zenburn t)
        (goto-char (point-min))
        (search-forward "* TODO Restore ledger service")
        (beginning-of-line)
        (org-todo 'done)
        (goto-char (point-min))
        (search-forward "[ ]")
        (goto-char (match-beginning 0))
        (org-toggle-checkbox)
        (save-buffer)
        (font-lock-flush)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "service status")
        (setq result
              (list
               :file
               (file-relative-name buffer-file-name root)
               :mode major-mode
               :mode-name mode-name
               :content
               (buffer-substring-no-properties
                (point-min)
                (point-max))
               :modified (buffer-modified-p)
               :point (point)
               :line (line-number-at-pos)
               :column (current-column)
               :heading
               (substring-no-properties
                (org-get-heading t t t t))
               :owner
               (save-excursion
                 (org-back-to-heading t)
                 (org-entry-get nil "OWNER"))
               :themes
               (copy-sequence custom-enabled-themes)
               :tokens
               (neomacs-anti-zenburn-test-token-state
                '("#+title:"
                  "Ledger Incident Response"
                  "DONE"
                  "Restore ledger service"
                  "2026-08-03"
                  "[X] Confirm"
                  "service status"
                  "#+begin_src"
                  "message"
                  "\"ledger restored\""
                  "#+end_src"))
               :faces
               (neomacs-anti-zenburn-test-face-state
                '(default
                  org-document-info-keyword
                  org-document-title
                  org-done
                  org-level-1
                  org-date
                  org-checkbox
                  org-link
                  org-block-begin-line
                  org-block
                  org-block-end-line))
               :disk
               (neomacs-anti-zenburn-test-file-string runbook))))
    (neomacs-anti-zenburn-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:file "incident-response.org" :mode org-mode :mode-name "Org" :content "#+title: Ledger Incident Response\n#+author: Operations Team\n\n* DONE Restore ledger service                                    :operations:\nSCHEDULED: <2026-08-03 Mon 09:30>\n:PROPERTIES:\n:OWNER: Ada\n:END:\n- [X] Confirm the replica is current\n- [X] Notify the incident channel\n\nSee [[https://status.example.invalid][service status]].\n\n#+begin_src emacs-lisp\n(message \"ledger restored\")\n#+end_src\n" :modified nil :point 329 :line 12 :column 18 :heading "Restore ledger service" :owner "Ada" :themes (anti-zenburn) :tokens ((:token "#+title:" :face org-document-info-keyword) (:token "Ledger Incident Response" :face org-document-title) (:token "DONE" :face (org-done org-level-1)) (:token "Restore ledger service" :face (org-headline-done org-level-1)) (:token "2026-08-03" :face (org-date)) (:token "[X] Confirm" :face (org-checkbox)) (:token "service status" :face org-link) (:token "#+begin_src" :face org-block-begin-line) (:token "message" :face (org-block)) (:token "\"ledger restored\"" :face (font-lock-string-face org-block)) (:token "#+end_src" :face org-block-end-line)) :faces ((:face default :foreground "#232333" :background "#c0c0c0" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face org-document-info-keyword :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit shadow) (:face org-document-title :foreground "#732f2c" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face org-done :foreground "#502750" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face org-level-1 :foreground "#205070" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face org-date :foreground "#732f2c" :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face org-checkbox :foreground "#000010" :background "#a0a0a0" :weight unspecified :slant unspecified :underline unspecified :box (:line-width 1 :style released-button) :inherit unspecified) (:face org-link :foreground "#2f4070" :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face org-block-begin-line :foreground "#806080" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit org-meta-line) (:face org-block :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit shadow) (:face org-block-end-line :foreground "#806080" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit org-block-begin-line)) :disk "#+title: Ledger Incident Response\n#+author: Operations Team\n\n* DONE Restore ledger service                                    :operations:\nSCHEDULED: <2026-08-03 Mon 09:30>\n:PROPERTIES:\n:OWNER: Ada\n:END:\n- [X] Confirm the replica is current\n- [X] Notify the incident channel\n\nSee [[https://status.example.invalid][service status]].\n\n#+begin_src emacs-lisp\n(message \"ledger restored\")\n#+end_src\n")"##
        ]],
    )
}

fn reviewing_a_real_patch_refines_changed_text_highlights_audit_notes_and_keeps_selection_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "reviewing_a_real_patch_refines_changed_text_highlights_audit_notes_and_keeps_selection_state",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "anti-zenburn-review-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (patch
        (expand-file-name "ledger-review.patch" root))
       (default-directory root)
       buffer
       result)
  (unwind-protect
      (progn
        (neomacs-anti-zenburn-test-cleanup root)
        (make-directory root t)
        (with-temp-file patch
          (insert
           "diff --git a/ledger.el b/ledger.el\n"
           "index 0123456..abcdef0 100644\n"
           "--- a/ledger.el\n"
           "+++ b/ledger.el\n"
           "@@ -10,3 +10,3 @@\n"
           " (validate invoice)\n"
           "-(settle invoice :fast)\n"
           "+(settle audited-invoice :reviewed)\n"
           " ;; auditor approved the settlement path\n"))
        (require 'diff-mode)
        (require 'hi-lock)
        (setq buffer (find-file-noselect patch))
        (switch-to-buffer buffer)
        (diff-mode)
        (load-theme 'anti-zenburn t)
        (highlight-regexp "auditor" 'hi-yellow)
        (font-lock-flush)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "@@ -10,3")
        (diff-refine-hunk)
        (goto-char (point-min))
        (search-forward "+(settle audited-invoice :reviewed)")
        (beginning-of-line)
        (set-mark (point))
        (forward-line 1)
        (setq transient-mark-mode t)
        (activate-mark)
        (setq result
              (list
               :file
               (file-relative-name buffer-file-name root)
               :mode major-mode
               :mode-name mode-name
               :content
               (buffer-substring-no-properties
                (point-min)
                (point-max))
               :modified (buffer-modified-p)
               :point (point)
               :mark (mark)
               :mark-active mark-active
               :selected-text
               (buffer-substring-no-properties
                (region-beginning)
                (region-end))
               :themes
               (copy-sequence custom-enabled-themes)
               :tokens
               (neomacs-anti-zenburn-test-token-display-state
                '("diff --git"
                  "index 0123456"
                  "--- a/ledger.el"
                  "+++ b/ledger.el"
                  "@@ -10,3"
                  "validate invoice"
                  "-(settle"
                  "invoice :fast"
                  "+(settle"
                  "audited-invoice"
                  ":reviewed"
                  "auditor"
                  "approved the settlement"))
               :faces
               (neomacs-anti-zenburn-test-face-state
                '(default
                  mode-line
                  region
                  diff-header
                  diff-file-header
                  diff-hunk-header
                  diff-context
                  diff-removed
                  diff-refine-removed
                  diff-added
                  diff-refine-added
                  hi-yellow))
               :disk
               (neomacs-anti-zenburn-test-file-string patch))))
    (neomacs-anti-zenburn-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:file "ledger-review.patch" :mode diff-mode :mode-name "Diff" :content "diff --git a/ledger.el b/ledger.el\nindex 0123456..abcdef0 100644\n--- a/ledger.el\n+++ b/ledger.el\n@@ -10,3 +10,3 @@\n (validate invoice)\n-(settle invoice :fast)\n+(settle audited-invoice :reviewed)\n ;; auditor approved the settlement path\n" :modified nil :point 196 :mark 160 :mark-active t :selected-text "+(settle audited-invoice :reviewed)\n" :themes (anti-zenburn) :tokens ((:token "diff --git" :face diff-header :font-lock-face nil) (:token "index 0123456" :face diff-header :font-lock-face nil) (:token "--- a/ledger.el" :face diff-header :font-lock-face nil) (:token "+++ b/ledger.el" :face diff-header :font-lock-face nil) (:token "@@ -10,3" :face diff-hunk-header :font-lock-face nil) (:token "validate invoice" :face diff-context :font-lock-face nil) (:token "-(settle" :face diff-indicator-removed :font-lock-face nil) (:token "invoice :fast" :face diff-removed :font-lock-face nil) (:token "+(settle" :face diff-indicator-added :font-lock-face nil) (:token "audited-invoice" :face diff-refine-added :font-lock-face nil) (:token ":reviewed" :face diff-added :font-lock-face nil) (:token "auditor" :face hi-yellow :font-lock-face nil) (:token "approved the settlement" :face diff-context :font-lock-face nil)) :faces ((:face default :foreground "#232333" :background "#c0c0c0" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face mode-line :foreground "#704d70" :background "#d4d4d4" :weight unspecified :slant unspecified :underline unspecified :box (:line-width -1 :style released-button) :inherit unspecified) (:face region :foreground unspecified :background "#d4d4d4" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face diff-header :foreground unspecified :background "#a0a0a0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face diff-file-header :foreground "#232333" :background "#a0a0a0" :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face diff-hunk-header :foreground unspecified :background "#a0a0a0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit diff-header) (:face diff-context :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face diff-removed :foreground "#235c5c" :background "#93cccc" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face diff-refine-removed :foreground "#134c4c" :background "#83bcbc" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face diff-added :foreground "#603a60" :background "#d0b0d0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face diff-refine-added :foreground "#502750" :background "#c0a0c0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face hi-yellow :foreground "#d4d4d4" :background "#0f2050" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :disk "diff --git a/ledger.el b/ledger.el\nindex 0123456..abcdef0 100644\n--- a/ledger.el\n+++ b/ledger.el\n@@ -10,3 +10,3 @@\n (validate invoice)\n-(settle invoice :fast)\n+(settle audited-invoice :reviewed)\n ;; auditor approved the settlement path\n")"##
        ]],
    )
}

fn reading_a_real_ansi_build_log_applies_diagnostics_without_overwriting_the_source_file()
-> ParityBatchCase {
    ParityBatchCase::value(
        "reading_a_real_ansi_build_log_applies_diagnostics_without_overwriting_the_source_file",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "anti-zenburn-build-log-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (log
        (expand-file-name "build.log" root))
       (default-directory root)
       (user-palette
        ["user-black"
         "user-red"
         "user-green"
         "user-yellow"
         "user-blue"
         "user-magenta"
         "user-cyan"
         "user-white"])
       buffer
       disk-before
       during
       restored
       disk-after)
  (unwind-protect
      (progn
        (neomacs-anti-zenburn-test-cleanup root)
        (make-directory root t)
        (with-temp-file log
          (insert
           "\e[31msrc/ledger.rs:12:7: error: invoice mismatch\e[0m\n"
           "\e[33msrc/retry.rs:18:3: warning: retry scheduled\e[0m\n"
           "\e[32msrc/check.rs:4:1: note: 42 checks completed\e[0m\n"))
        (require 'ansi-color)
        (require 'compile)
        (setq ansi-color-names-vector
              (copy-sequence user-palette))
        (setq disk-before
              (neomacs-anti-zenburn-test-file-string log))
        (load-theme 'anti-zenburn t)
        (setq buffer (find-file-noselect log))
        (switch-to-buffer buffer)
        (compilation-mode)
        (font-lock-ensure)
        (let ((inhibit-read-only t))
          (ansi-color-apply-on-region
           (point-min)
           (point-max)))
        (goto-char (point-min))
        (search-forward "retry scheduled")
        (setq during
              (list
               :file
               (file-relative-name buffer-file-name root)
               :mode major-mode
               :mode-name mode-name
               :content
               (buffer-substring-no-properties
                (point-min)
                (point-max))
               :modified (buffer-modified-p)
               :point (point)
               :line (line-number-at-pos)
               :column (current-column)
               :themes
               (copy-sequence custom-enabled-themes)
               :ansi-palette
               (copy-sequence ansi-color-names-vector)
               :tokens
               (neomacs-anti-zenburn-test-token-display-state
                '("src/ledger.rs"
                  "12"
                  "7"
                  "error"
                  "invoice mismatch"
                  "src/retry.rs"
                  "18"
                  "3"
                  "warning"
                  "retry scheduled"
                  "src/check.rs"
                  "4"
                  "1"
                  "note"
                  "42 checks completed"))
               :faces
               (neomacs-anti-zenburn-test-face-state
                '(default
                  mode-line
                  compilation-line-number
                  compilation-error
                  compilation-warning
                  compilation-info
                  compilation-mode-line-exit
                  compilation-mode-line-fail))))
        (disable-theme 'anti-zenburn)
        (setq restored
              (list
               :themes
               (copy-sequence custom-enabled-themes)
               :ansi-palette
               (copy-sequence ansi-color-names-vector)))
        (setq disk-after
              (neomacs-anti-zenburn-test-file-string log)))
    (neomacs-anti-zenburn-test-cleanup root))
  (list
   :disk-before disk-before
   :during during
   :restored restored
   :disk-after disk-after))
"####,
        expect![[
            r##"OK (:disk-before "\33[31msrc/ledger.rs:12:7: error: invoice mismatch\33[0m\n\33[33msrc/retry.rs:18:3: warning: retry scheduled\33[0m\n\33[32msrc/check.rs:4:1: note: 42 checks completed\33[0m\n" :during (:file "build.log" :mode compilation-mode :mode-name "Compilation" :content "src/ledger.rs:12:7: error: invoice mismatch\nsrc/retry.rs:18:3: warning: retry scheduled\nsrc/check.rs:4:1: note: 42 checks completed\n" :modified t :point 88 :line 2 :column 43 :themes (anti-zenburn) :ansi-palette ["#c0c0c0" "#336c6c" "#806080" "#0f2050" "#732f2c" "#23733c" "#6c1f1c" "#232333"] :tokens ((:token "src/ledger.rs" :face (:foreground "red3") :font-lock-face (compilation-error underline)) (:token "12" :face (:foreground "red3") :font-lock-face (compilation-line-number underline)) (:token "7" :face (:foreground "red3") :font-lock-face (compilation-column-number underline)) (:token "error" :face (:foreground "red3") :font-lock-face (underline)) (:token "invoice mismatch" :face (:foreground "red3") :font-lock-face nil) (:token "src/retry.rs" :face (:foreground "yellow3") :font-lock-face (compilation-warning underline)) (:token "18" :face (:foreground "yellow3") :font-lock-face (compilation-line-number underline)) (:token "3" :face (:foreground "yellow3") :font-lock-face (compilation-column-number underline)) (:token "warning" :face (:foreground "yellow3") :font-lock-face (underline)) (:token "retry scheduled" :face (:foreground "yellow3") :font-lock-face nil) (:token "src/check.rs" :face (:foreground "green3") :font-lock-face (compilation-info underline)) (:token "4" :face (:foreground "green3") :font-lock-face (compilation-line-number underline)) (:token "1" :face (:foreground "red3") :font-lock-face (compilation-line-number underline)) (:token "note" :face (:foreground "green3") :font-lock-face (underline)) (:token "42 checks completed" :face (:foreground "green3") :font-lock-face nil)) :faces ((:face default :foreground "#232333" :background "#c0c0c0" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face mode-line :foreground "#704d70" :background "#d4d4d4" :weight unspecified :slant unspecified :underline unspecified :box (:line-width -1 :style released-button) :inherit unspecified) (:face compilation-line-number :foreground "#0f2050" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face compilation-error :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit error) (:face compilation-warning :foreground "#205070" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit warning) (:face compilation-info :foreground "#401440" :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face compilation-mode-line-exit :foreground "#603a60" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face compilation-mode-line-fail :foreground "#336c6c" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified))) :restored (:themes nil :ansi-palette ["user-black" "user-red" "user-green" "user-yellow" "user-blue" "user-magenta" "user-cyan" "user-white"]) :disk-after "\33[31msrc/ledger.rs:12:7: error: invoice mismatch\33[0m\n\33[33msrc/retry.rs:18:3: warning: retry scheduled\33[0m\n\33[32msrc/check.rs:4:1: note: 42 checks completed\33[0m\n")"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        editing_a_real_lisp_file_switches_to_anti_zenburn_and_restores_the_baseline_theme(),
        completing_a_real_org_runbook_keeps_the_saved_document_and_visual_structure_exact(),
        reviewing_a_real_patch_refines_changed_text_highlights_audit_notes_and_keeps_selection_state(),
        reading_a_real_ansi_build_log_applies_diagnostics_without_overwriting_the_source_file(),
    ]
}
