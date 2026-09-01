use expect_test::expect;

use super::ParityBatchCase;

/// A developer opens a real Elisp file, lets font-lock run, and switches to
/// acme.  Every editor and font-lock face has to move from the terminal
/// defaults to acme's exact Plan 9 palette while the face chosen at each token
/// stays the same.
fn loading_acme_repaints_a_real_elisp_editing_session_with_the_plan_nine_palette() -> ParityBatchCase
{
    ParityBatchCase::value(
        "loading_acme_repaints_a_real_elisp_editing_session_with_the_plan_nine_palette",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "acme-elisp-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source
        (expand-file-name "rotation.el" root))
       (default-directory root)
       (probed-faces
        '(default
          cursor
          fringe
          line-number
          line-number-current-line
          region
          highlight
          link
          link-visited
          isearch
          lazy-highlight
          minibuffer-prompt
          vertical-border
          header-line
          mode-line
          mode-line-inactive
          mode-line-buffer-id
          error
          warning
          success
          font-lock-comment-face
          font-lock-comment-delimiter-face
          font-lock-doc-face
          font-lock-string-face
          font-lock-keyword-face
          font-lock-function-name-face
          font-lock-variable-name-face
          font-lock-builtin-face
          font-lock-constant-face
          font-lock-type-face
          font-lock-warning-face))
       (probed-tokens
        '(";;; rotation.el"
          "Nightly key rotation"
          "Rotate every expired key"
          "defvar"
          "rotation-window-hours"
          "Hours a signing key stays valid"
          "defun"
          "rotation-expired-p"
          "Return non-nil when KEY is past"
          "if"
          "null"
          "error"
          "\"missing key\""
          "message"
          ":rotated"))
       buffer
       before
       after)
  (unwind-protect
      (progn
        (neomacs-acme-test-cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           ";;; rotation.el --- Nightly key rotation\n\n"
           ";; Rotate every expired key before the audit window closes.\n\n"
           "(defvar rotation-window-hours 24\n"
           "  \"Hours a signing key stays valid.\")\n\n"
           "(defun rotation-expired-p (key)\n"
           "  \"Return non-nil when KEY is past its window.\"\n"
           "  (if (null key)\n"
           "      (error \"missing key\")\n"
           "    (message \"checked %s\" key)\n"
           "    :rotated))\n"))
        (setq buffer (find-file-noselect source))
        (switch-to-buffer buffer)
        (emacs-lisp-mode)
        (font-lock-ensure)
        (setq before
              (list
               :themes (copy-sequence custom-enabled-themes)
               :faces (neomacs-acme-test-face-state probed-faces)))
        (load-theme 'acme t)
        (font-lock-flush)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "(message")
        (setq after
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :themes (copy-sequence custom-enabled-themes)
               :point (point)
               :line (line-number-at-pos)
               :modified (buffer-modified-p)
               :content (buffer-substring-no-properties (point-min) (point-max))
               :faces (neomacs-acme-test-face-state probed-faces)
               :tokens (neomacs-acme-test-token-state probed-tokens))))
    (neomacs-acme-test-cleanup root))
  (list :before before :after after))
"####,
        expect![[
            r##"OK (:before (:themes nil :faces ((:face default :defined t :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "white" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "gray" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face line-number :defined t :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :box unspecified :inherit (shadow default)) (:face line-number-current-line :defined t :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :overline nil :box unspecified :inherit line-number) (:face region :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face highlight :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :overline unspecified :box unspecified :inherit underline) (:face link-visited :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :overline unspecified :box unspecified :inherit link) (:face isearch :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face lazy-highlight :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :overline unspecified :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "cyan" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face vertical-border :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit mode-line-inactive) (:face header-line :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :overline unspecified :box unspecified :inherit mode-line) (:face mode-line :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face mode-line-inactive :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit mode-line) (:face mode-line-buffer-id :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-delimiter-face :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :overline unspecified :box unspecified :inherit font-lock-comment-face) (:face font-lock-doc-face :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :overline unspecified :box unspecified :inherit font-lock-string-face) (:face font-lock-string-face :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-variable-name-face :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-constant-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline t :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-type-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline t :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-warning-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit error))) :after (:file "rotation.el" :mode emacs-lisp-mode :themes (acme) :point 313 :line 12 :modified nil :content ";;; rotation.el --- Nightly key rotation\n\n;; Rotate every expired key before the audit window closes.\n\n(defvar rotation-window-hours 24\n  \"Hours a signing key stays valid.\")\n\n(defun rotation-expired-p (key)\n  \"Return non-nil when KEY is past its window.\"\n  (if (null key)\n      (error \"missing key\")\n    (message \"checked %s\" key)\n    :rotated))\n" :faces ((:face default :defined t :foreground "#444444" :background "#FFFFE8" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face cursor :defined t :foreground "#FFFFE8" :background "#444444" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground "#444444" :background "#FFFFE8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face line-number :defined t :foreground "#444444" :background "#EFEFD8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face line-number-current-line :defined t :foreground "#444444" :background "#EFEFD8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#444444" :background "#E8EB98" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face highlight :defined t :foreground "#0066cc" :background unspecified :weight normal :slant unspecified :underline t :overline unspecified :box unspecified :inherit link) (:face link :defined t :foreground "#0066cc" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face link-visited :defined t :foreground "#555599" :background unspecified :weight normal :slant unspecified :underline t :overline unspecified :box unspecified :inherit unspecified) (:face isearch :defined t :foreground "#444444" :background "#A8EFEB" :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face lazy-highlight :defined t :foreground "#444444" :background "#E1FAFF" :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face vertical-border :defined t :foreground "#007777" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face header-line :defined t :foreground "#444444" :background "#E1FAFF" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line :defined t :foreground "#444444" :background "#E1FAFF" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#444444" :background "#E5E5D0" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#880000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#880000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#005500" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#005500" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-delimiter-face :defined t :foreground "#005500" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :defined t :foreground "#888838" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#880000" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#1054AF" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-variable-name-face :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-constant-face :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-type-face :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-warning-face :defined t :foreground "#880000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified)) :tokens ((:token ";;; rotation.el" :face font-lock-comment-delimiter-face :font-lock-face nil :foreground "#005500" :background unspecified :weight unspecified :slant normal) (:token "Nightly key rotation" :face font-lock-comment-face :font-lock-face nil :foreground "#005500" :background unspecified :weight unspecified :slant normal) (:token "Rotate every expired key" :face font-lock-comment-face :font-lock-face nil :foreground "#005500" :background unspecified :weight unspecified :slant normal) (:token "defvar" :face font-lock-keyword-face :font-lock-face nil :foreground "#1054AF" :background unspecified :weight bold :slant unspecified) (:token "rotation-window-hours" :face font-lock-variable-name-face :font-lock-face nil :foreground "#444444" :background unspecified :weight normal :slant unspecified) (:token "Hours a signing key stays valid" :face font-lock-doc-face :font-lock-face nil :foreground "#888838" :background unspecified :weight unspecified :slant normal) (:token "defun" :face font-lock-keyword-face :font-lock-face nil :foreground "#1054AF" :background unspecified :weight bold :slant unspecified) (:token "rotation-expired-p" :face font-lock-function-name-face :font-lock-face nil :foreground "#444444" :background unspecified :weight normal :slant unspecified) (:token "Return non-nil when KEY is past" :face font-lock-doc-face :font-lock-face nil :foreground "#888838" :background unspecified :weight unspecified :slant normal) (:token "if" :face font-lock-keyword-face :font-lock-face nil :foreground "#1054AF" :background unspecified :weight bold :slant unspecified) (:token "null" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil) (:token "error" :face font-lock-warning-face :font-lock-face nil :foreground "#880000" :background unspecified :weight normal :slant unspecified) (:token "\"missing key\"" :face font-lock-string-face :font-lock-face nil :foreground "#880000" :background unspecified :weight unspecified :slant unspecified) (:token "message" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil) (:token ":rotated" :face font-lock-builtin-face :font-lock-face nil :foreground "#444444" :background unspecified :weight normal :slant unspecified))))"##
        ]],
    )
}

fn completing_a_real_org_release_checklist_keeps_the_document_and_its_acme_styling_exact()
-> ParityBatchCase {
    ParityBatchCase::value(
        "completing_a_real_org_release_checklist_keeps_the_document_and_its_acme_styling_exact",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "acme-org-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (runbook
        (expand-file-name "release-checklist.org" root))
       (default-directory root)
       buffer
       result)
  (unwind-protect
      (progn
        (neomacs-acme-test-cleanup root)
        (make-directory root t)
        (with-temp-file runbook
          (insert
           "#+title: Plan 9 Release Checklist\n"
           "#+author: Release Team\n\n"
           "* TODO Cut the release branch :release:\n"
           "SCHEDULED: <2026-08-10 Mon 08:00>\n"
           ":PROPERTIES:\n"
           ":OWNER: Rob\n"
           ":END:\n"
           "Run ~make check~ and record =PASS= in the log.\n\n"
           "** Verification matrix\n"
           "| stage   | owner | state |\n"
           "|---------+-------+-------|\n"
           "| build   | Rob   | done  |\n"
           "| package | Ken   | queued|\n\n"
           "#+begin_src emacs-lisp\n"
           "(message \"release cut\")\n"
           "#+end_src\n"))
        (require 'org)
        (load-theme 'acme t)
        (setq buffer (find-file-noselect runbook))
        (switch-to-buffer buffer)
        (org-mode)
        (setq-local org-log-done nil)
        (goto-char (point-min))
        (search-forward "* TODO Cut the release branch")
        (beginning-of-line)
        (org-todo 'done)
        (save-buffer)
        (font-lock-flush)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "Verification matrix")
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :mode-name mode-name
               :themes (copy-sequence custom-enabled-themes)
               :content (buffer-substring-no-properties (point-min) (point-max))
               :modified (buffer-modified-p)
               :point (point)
               :line (line-number-at-pos)
               :heading
               (substring-no-properties (org-get-heading t t t t))
               :owner
               (save-excursion
                 (goto-char (point-min))
                 (search-forward "Cut the release branch")
                 (org-entry-get nil "OWNER"))
               :tokens
               (neomacs-acme-test-token-state
                '("#+title:"
                  "Plan 9 Release Checklist"
                  "DONE"
                  "Cut the release branch"
                  "2026-08-10"
                  ":OWNER:"
                  "make check"
                  "PASS"
                  "Verification matrix"
                  "| stage"
                  "#+begin_src"
                  "message"
                  "\"release cut\""
                  "#+end_src"))
               :faces
               (neomacs-acme-test-face-state
                '(default
                  org-document-title
                  org-document-info-keyword
                  org-meta-line
                  org-level-1
                  org-level-2
                  org-todo
                  org-done
                  org-date
                  org-special-keyword
                  org-table
                  org-code
                  org-verbatim
                  org-block
                  org-block-begin-line
                  org-block-end-line))
               :disk (neomacs-acme-test-file-string runbook))))
    (neomacs-acme-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:file "release-checklist.org" :mode org-mode :mode-name "Org" :themes (acme) :content "#+title: Plan 9 Release Checklist\n#+author: Release Team\n\n* DONE Cut the release branch                                       :release:\nSCHEDULED: <2026-08-10 Mon 08:00>\n:PROPERTIES:\n:OWNER: Rob\n:END:\nRun ~make check~ and record =PASS= in the log.\n\n** Verification matrix\n| stage   | owner | state |\n|---------+-------+-------|\n| build   | Rob   | done  |\n| package | Ken   | queued|\n\n#+begin_src emacs-lisp\n(message \"release cut\")\n#+end_src\n" :modified nil :point 272 :line 11 :heading "Verification matrix" :owner "Rob" :tokens ((:token "#+title:" :face org-document-info-keyword :font-lock-face nil :foreground "#007777" :background unspecified :weight unspecified :slant unspecified) (:token "Plan 9 Release Checklist" :face org-document-title :font-lock-face nil :foreground "#1054AF" :background unspecified :weight bold :slant unspecified) (:token "DONE" :face (org-done org-level-1) :font-lock-face nil :foreground "#005500" :background "#E8FCE8" :weight normal :slant unspecified) (:token "Cut the release branch" :face (org-headline-done org-level-1) :font-lock-face nil :foreground unspecified :background unspecified :weight unspecified :slant unspecified) (:token "2026-08-10" :face (org-date) :font-lock-face nil :foreground "#555599" :background unspecified :weight unspecified :slant unspecified) (:token ":OWNER:" :face org-special-keyword :font-lock-face nil :foreground "#007777" :background unspecified :weight unspecified :slant unspecified) (:token "make check" :face (org-code) :font-lock-face nil :foreground "#880000" :background "#EFEFD8" :weight unspecified :slant unspecified) (:token "PASS" :face (org-verbatim) :font-lock-face nil :foreground "#444444" :background "#EFEFD8" :weight unspecified :slant unspecified) (:token "Verification matrix" :face org-level-2 :font-lock-face nil :foreground "#007777" :background "#E1FAFF" :weight bold :slant unspecified) (:token "| stage" :face org-table :font-lock-face nil :foreground "#555599" :background unspecified :weight unspecified :slant unspecified) (:token "#+begin_src" :face org-block-begin-line :font-lock-face nil :foreground "#B8B09A" :background "#E5E5D0" :weight unspecified :slant italic) (:token "message" :face (org-block) :font-lock-face nil :foreground "#444444" :background "#EFEFD8" :weight unspecified :slant unspecified) (:token "\"release cut\"" :face (font-lock-string-face org-block) :font-lock-face nil :foreground "#880000" :background unspecified :weight unspecified :slant unspecified) (:token "#+end_src" :face org-block-end-line :font-lock-face nil :foreground "#B8B09A" :background "#E5E5D0" :weight unspecified :slant italic)) :faces ((:face default :defined t :foreground "#444444" :background "#FFFFE8" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face org-document-title :defined t :foreground "#1054AF" :background unspecified :weight bold :slant unspecified :underline t :overline unspecified :box unspecified :inherit unspecified) (:face org-document-info-keyword :defined t :foreground "#007777" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face org-meta-line :defined t :foreground "#005500" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face org-level-1 :defined t :foreground "#1054AF" :background "#E1FAFF" :weight bold :slant unspecified :underline unspecified :overline t :box unspecified :inherit unspecified) (:face org-level-2 :defined t :foreground "#007777" :background "#E1FAFF" :weight bold :slant unspecified :underline unspecified :overline t :box unspecified :inherit unspecified) (:face org-todo :defined t :foreground "#888838" :background "#EFEFD8" :weight normal :slant unspecified :underline unspecified :overline unspecified :box (:line-width 1 :style released-button) :inherit unspecified) (:face org-done :defined t :foreground "#005500" :background "#E8FCE8" :weight normal :slant unspecified :underline unspecified :overline unspecified :box (:style released-button) :inherit unspecified) (:face org-date :defined t :foreground "#555599" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face org-special-keyword :defined t :foreground "#007777" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face org-table :defined t :foreground "#555599" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face org-code :defined t :foreground "#880000" :background "#EFEFD8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face org-verbatim :defined t :foreground "#444444" :background "#EFEFD8" :weight unspecified :slant unspecified :underline t :overline unspecified :box unspecified :inherit unspecified) (:face org-block :defined t :foreground "#444444" :background "#EFEFD8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face org-block-begin-line :defined t :foreground "#B8B09A" :background "#E5E5D0" :weight unspecified :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face org-block-end-line :defined t :foreground "#B8B09A" :background "#E5E5D0" :weight unspecified :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified)) :disk "#+title: Plan 9 Release Checklist\n#+author: Release Team\n\n* DONE Cut the release branch                                       :release:\nSCHEDULED: <2026-08-10 Mon 08:00>\n:PROPERTIES:\n:OWNER: Rob\n:END:\nRun ~make check~ and record =PASS= in the log.\n\n** Verification matrix\n| stage   | owner | state |\n|---------+-------+-------|\n| build   | Rob   | done  |\n| package | Ken   | queued|\n\n#+begin_src emacs-lisp\n(message \"release cut\")\n#+end_src\n")"##
        ]],
    )
}

fn reviewing_a_real_patch_and_listing_its_matches_uses_acmes_diff_and_match_colours()
-> ParityBatchCase {
    ParityBatchCase::value(
        "reviewing_a_real_patch_and_listing_its_matches_uses_acmes_diff_and_match_colours",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "acme-review-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (patch
        (expand-file-name "rotation-review.patch" root))
       (default-directory root)
       buffer
       review
       occurrences)
  (unwind-protect
      (progn
        (neomacs-acme-test-cleanup root)
        (make-directory root t)
        (with-temp-file patch
          (insert
           "diff --git a/rotation.el b/rotation.el\n"
           "index 1111111..2222222 100644\n"
           "--- a/rotation.el\n"
           "+++ b/rotation.el\n"
           "@@ -12,3 +12,3 @@\n"
           " (validate key)\n"
           "-(rotate key :eager)\n"
           "+(rotate audited-key :verified)\n"
           " ;; auditor signed the rotation window\n"))
        (require 'diff-mode)
        (load-theme 'acme t)
        (setq buffer (find-file-noselect patch))
        (switch-to-buffer buffer)
        (diff-mode)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "@@ -12,3")
        (diff-refine-hunk)
        (goto-char (point-min))
        (search-forward "+(rotate audited-key :verified)")
        (beginning-of-line)
        (set-mark (point))
        (forward-line 1)
        (setq transient-mark-mode t)
        (activate-mark)
        (setq review
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :mode-name mode-name
               :themes (copy-sequence custom-enabled-themes)
               :modified (buffer-modified-p)
               :point (point)
               :mark (mark)
               :mark-active mark-active
               :selected-text
               (buffer-substring-no-properties
                (region-beginning)
                (region-end))
               :tokens
               (neomacs-acme-test-token-state
                '("diff --git"
                  "index 1111111"
                  "--- a/rotation.el"
                  "@@ -12,3"
                  "validate key"
                  "-(rotate"
                  "key :eager"
                  "+(rotate"
                  "audited-key"
                  ":verified"
                  "auditor signed"))
               :faces
               (neomacs-acme-test-face-state
                '(default
                  region
                  diff-header
                  diff-file-header
                  diff-hunk-header
                  diff-context
                  diff-added
                  diff-removed
                  diff-changed
                  diff-indicator-added
                  diff-indicator-removed
                  diff-refine-added
                  diff-refine-removed
                  match
                  isearch
                  lazy-highlight))))
        (occur "rotate")
        (with-current-buffer "*Occur*"
          (setq occurrences
                (list
                 :buffer (buffer-name)
                 :mode major-mode
                 :content
                 (buffer-substring-no-properties (point-min) (point-max))
                 :tokens
                 (neomacs-acme-test-token-state
                  '("rotate key :eager"
                    "rotate audited-key")))))
        (kill-buffer "*Occur*")
        (deactivate-mark))
    (neomacs-acme-test-cleanup root))
  (list :review review :occurrences occurrences))
"####,
        expect![[
            r##"OK (:review (:file "rotation-review.patch" :mode diff-mode :mode-name "Diff" :themes (acme) :modified nil :point 193 :mark 161 :mark-active t :selected-text "+(rotate audited-key :verified)\n" :tokens ((:token "diff --git" :face diff-header :font-lock-face nil :foreground "#444444" :background unspecified :weight normal :slant unspecified) (:token "index 1111111" :face diff-header :font-lock-face nil :foreground "#444444" :background unspecified :weight normal :slant unspecified) (:token "--- a/rotation.el" :face diff-header :font-lock-face nil :foreground "#444444" :background unspecified :weight normal :slant unspecified) (:token "@@ -12,3" :face diff-hunk-header :font-lock-face nil :foreground "#005500" :background unspecified :weight normal :slant unspecified) (:token "validate key" :face diff-context :font-lock-face nil :foreground "#444444" :background unspecified :weight unspecified :slant unspecified) (:token "-(rotate" :face diff-indicator-removed :font-lock-face nil :foreground "#444444" :background "#F8E8E8" :weight unspecified :slant unspecified) (:token "key :eager" :face diff-removed :font-lock-face nil :foreground "#444444" :background "#F8E8E8" :weight unspecified :slant unspecified) (:token "+(rotate" :face diff-indicator-added :font-lock-face nil :foreground "#444444" :background "#E8FCE8" :weight unspecified :slant unspecified) (:token "audited-key" :face diff-refine-added :font-lock-face nil :foreground "#444444" :background "#E8FCE8" :weight bold :slant unspecified) (:token ":verified" :face diff-added :font-lock-face nil :foreground "#444444" :background "#E8FCE8" :weight unspecified :slant unspecified) (:token "auditor signed" :face diff-context :font-lock-face nil :foreground "#444444" :background unspecified :weight unspecified :slant unspecified)) :faces ((:face default :defined t :foreground "#444444" :background "#FFFFE8" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face region :defined t :foreground "#444444" :background "#E8EB98" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face diff-header :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face diff-file-header :defined t :foreground "#444444" :background "#A8EFEB" :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face diff-hunk-header :defined t :foreground "#005500" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face diff-context :defined t :foreground "#444444" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face diff-added :defined t :foreground "#444444" :background "#E8FCE8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face diff-removed :defined t :foreground "#444444" :background "#F8E8E8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face diff-changed :defined t :foreground "#888838" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face diff-indicator-added :defined t :foreground "#444444" :background "#E8FCE8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit diff-added) (:face diff-indicator-removed :defined t :foreground "#444444" :background "#F8E8E8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit diff-removed) (:face diff-refine-added :defined t :foreground "#444444" :background "#E8FCE8" :weight bold :slant unspecified :underline t :overline unspecified :box unspecified :inherit diff-added) (:face diff-refine-removed :defined t :foreground "#444444" :background "#F8E8E8" :weight bold :slant unspecified :underline t :overline unspecified :box unspecified :inherit diff-removed) (:face match :defined t :foreground "#A8EFEB" :background "#007777" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face isearch :defined t :foreground "#444444" :background "#A8EFEB" :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face lazy-highlight :defined t :foreground "#444444" :background "#E1FAFF" :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified))) :occurrences (:buffer "*Occur*" :mode occur-mode :content "2 matches for \"rotate\" in buffer: rotation-review.patch\n      7:-(rotate key :eager)\n      8:+(rotate audited-key :verified)\n" :tokens ((:token "rotate key :eager" :face (match diff-removed) :font-lock-face nil :foreground "#A8EFEB" :background "#007777" :weight unspecified :slant unspecified) (:token "rotate audited-key" :face (match diff-added) :font-lock-face nil :foreground "#A8EFEB" :background "#007777" :weight unspecified :slant unspecified))))"##
        ]],
    )
}

fn the_black_foreground_option_rewrites_every_derived_face_on_the_next_load_theme()
-> ParityBatchCase {
    ParityBatchCase::value(
        "the_black_foreground_option_rewrites_every_derived_face_on_the_next_load_theme",
        r####"
(let* ((fg-derived
        '(default
          cursor
          fringe
          line-number
          region
          isearch
          minibuffer-prompt
          mode-line
          mode-line-inactive
          mode-line-buffer-id
          menu
          font-lock-builtin-face
          font-lock-function-name-face
          font-lock-variable-name-face
          font-lock-type-face
          font-lock-constant-face))
       (palette-only
        '(link
          vertical-border
          font-lock-keyword-face
          font-lock-string-face
          font-lock-comment-face
          font-lock-doc-face
          error
          warning
          success))
       option
       default-fg
       black-fg
       restored)
  (unwind-protect
      (progn
        (setq option
              (list
               :customizable (and (custom-variable-p 'acme-theme-black-fg) t)
               :type (get 'acme-theme-black-fg 'custom-type)
               :standard-value
               (eval (car (get 'acme-theme-black-fg 'standard-value)) t)
               :group (get 'acme-theme-black-fg 'custom-group)
               :member-of-acme-theme-group
               (and (assq 'acme-theme-black-fg
                          (get 'acme-theme 'custom-group))
                    t)
               :value acme-theme-black-fg))
        (load-theme 'acme t)
        (setq default-fg
              (list
               :option acme-theme-black-fg
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'acme 'theme-settings))
               :fg-derived (neomacs-acme-test-face-state fg-derived)
               :palette-only (neomacs-acme-test-face-state palette-only)))
        (customize-set-variable 'acme-theme-black-fg t)
        (load-theme 'acme t)
        (setq black-fg
              (list
               :option acme-theme-black-fg
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'acme 'theme-settings))
               :fg-derived (neomacs-acme-test-face-state fg-derived)
               :palette-only (neomacs-acme-test-face-state palette-only)))
        (customize-set-variable 'acme-theme-black-fg nil)
        (load-theme 'acme t)
        (setq restored
              (list
               :option acme-theme-black-fg
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'acme 'theme-settings))
               :fg-derived (neomacs-acme-test-face-state fg-derived)
               :palette-only (neomacs-acme-test-face-state palette-only))))
    (when (custom-theme-enabled-p 'acme)
      (disable-theme 'acme))
    (setq acme-theme-black-fg nil))
  (list
   :option option
   :default-fg default-fg
   :black-fg black-fg
   :restored restored
   :option-changed-the-foregrounds
   (not (equal (plist-get default-fg :fg-derived)
               (plist-get black-fg :fg-derived)))
   :option-left-the-palette-alone
   (equal (plist-get default-fg :palette-only)
          (plist-get black-fg :palette-only))
   :round-trip-restored-the-foregrounds
   (equal (plist-get default-fg :fg-derived)
          (plist-get restored :fg-derived))))
"####,
        expect![[
            r##"OK (:option (:customizable t :type boolean :standard-value nil :group nil :member-of-acme-theme-group t :value nil) :default-fg (:option nil :themes (acme) :settings 314 :fg-derived ((:face default :defined t :foreground "#444444" :background "#FFFFE8" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face cursor :defined t :foreground "#FFFFE8" :background "#444444" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground "#444444" :background "#FFFFE8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face line-number :defined t :foreground "#444444" :background "#EFEFD8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#444444" :background "#E8EB98" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face isearch :defined t :foreground "#444444" :background "#A8EFEB" :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#444444" :background "#E1FAFF" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#444444" :background "#E5E5D0" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face menu :defined t :foreground "#FFFFE8" :background "#444444" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-variable-name-face :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-type-face :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-constant-face :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified)) :palette-only ((:face link :defined t :foreground "#0066cc" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face vertical-border :defined t :foreground "#007777" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#1054AF" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#880000" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#005500" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :defined t :foreground "#888838" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#880000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#880000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#005500" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified))) :black-fg (:option t :themes (acme) :settings 314 :fg-derived ((:face default :defined t :foreground "#000000" :background "#FFFFE8" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face cursor :defined t :foreground "#FFFFE8" :background "#000000" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground "#000000" :background "#FFFFE8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face line-number :defined t :foreground "#000000" :background "#EFEFD8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#000000" :background "#E8EB98" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face isearch :defined t :foreground "#000000" :background "#A8EFEB" :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#000000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#000000" :background "#E1FAFF" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#000000" :background "#E5E5D0" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#000000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face menu :defined t :foreground "#FFFFE8" :background "#000000" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :defined t :foreground "#000000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground "#000000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-variable-name-face :defined t :foreground "#000000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-type-face :defined t :foreground "#000000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-constant-face :defined t :foreground "#000000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified)) :palette-only ((:face link :defined t :foreground "#0066cc" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face vertical-border :defined t :foreground "#007777" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#1054AF" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#880000" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#005500" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :defined t :foreground "#888838" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#880000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#880000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#005500" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified))) :restored (:option nil :themes (acme) :settings 314 :fg-derived ((:face default :defined t :foreground "#444444" :background "#FFFFE8" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face cursor :defined t :foreground "#FFFFE8" :background "#444444" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground "#444444" :background "#FFFFE8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face line-number :defined t :foreground "#444444" :background "#EFEFD8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#444444" :background "#E8EB98" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face isearch :defined t :foreground "#444444" :background "#A8EFEB" :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#444444" :background "#E1FAFF" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#444444" :background "#E5E5D0" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face menu :defined t :foreground "#FFFFE8" :background "#444444" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-variable-name-face :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-type-face :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-constant-face :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified)) :palette-only ((:face link :defined t :foreground "#0066cc" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face vertical-border :defined t :foreground "#007777" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#1054AF" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#880000" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#005500" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :defined t :foreground "#888838" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#880000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#880000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#005500" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified))) :option-changed-the-foregrounds t :option-left-the-palette-alone t :round-trip-restored-the-foregrounds t)"##
        ]],
    )
}

fn stacking_acme_over_a_dark_user_theme_and_disabling_it_restores_the_previous_appearance()
-> ParityBatchCase {
    ParityBatchCase::value(
        "stacking_acme_over_a_dark_user_theme_and_disabling_it_restores_the_previous_appearance",
        r####"
(let* ((probed-faces
        '(default
          cursor
          region
          fringe
          hl-line
          link
          minibuffer-prompt
          mode-line
          mode-line-inactive
          mode-line-buffer-id
          header-line
          show-paren-match
          font-lock-keyword-face
          font-lock-string-face
          font-lock-comment-face
          error
          warning
          success
          bold
          italic
          shadow))
       baseline
       stacked
       restored)
  (unwind-protect
      (progn
        (require 'hl-line)
        (eval
         '(deftheme neomacs-acme-baseline
            "Dark theme the developer already uses at night."))
        (custom-theme-set-faces
         'neomacs-acme-baseline
         '(default ((t (:foreground "#d8d8d0" :background "#12151a"))))
         '(cursor ((t (:background "#f0a000"))))
         '(region ((t (:foreground "#12151a" :background "#4a6fa5"))))
         '(fringe ((t (:background "#1a1e24"))))
         '(hl-line ((t (:background "#1f242c"))))
         '(link ((t (:foreground "#7fb3ff" :underline t))))
         '(minibuffer-prompt ((t (:foreground "#f0a000" :weight bold))))
         '(mode-line
           ((t (:foreground "#12151a" :background "#8fa6c4"
                :box (:line-width 2 :style released-button)))))
         '(mode-line-inactive ((t (:foreground "#6a7280" :background "#1a1e24"))))
         '(mode-line-buffer-id ((t (:foreground "#12151a" :weight normal))))
         '(header-line ((t (:foreground "#d8d8d0" :background "#252a33"))))
         '(show-paren-match ((t (:background "#3a5f3a" :weight bold))))
         '(font-lock-keyword-face ((t (:foreground "#c98fff" :weight normal))))
         '(font-lock-string-face ((t (:foreground "#8fd08f"))))
         '(font-lock-comment-face ((t (:foreground "#6a7280" :slant italic))))
         '(error ((t (:foreground "#ff6b6b" :weight bold))))
         '(warning ((t (:foreground "#ffc060" :weight bold))))
         '(success ((t (:foreground "#7fd07f" :weight bold))))
         '(bold ((t (:weight bold))))
         '(italic ((t (:slant italic))))
         '(shadow ((t (:foreground "#5a6270")))))
        (provide-theme 'neomacs-acme-baseline)
        (enable-theme 'neomacs-acme-baseline)
        (setq baseline
              (list
               :themes (copy-sequence custom-enabled-themes)
               :acme-enabled (and (custom-theme-enabled-p 'acme) t)
               :faces (neomacs-acme-test-face-state probed-faces)))
        (load-theme 'acme t)
        (setq stacked
              (list
               :themes (copy-sequence custom-enabled-themes)
               :acme-enabled (and (custom-theme-enabled-p 'acme) t)
               :baseline-enabled
               (and (custom-theme-enabled-p 'neomacs-acme-baseline) t)
               :faces (neomacs-acme-test-face-state probed-faces)))
        (disable-theme 'acme)
        (setq restored
              (list
               :themes (copy-sequence custom-enabled-themes)
               :acme-enabled (and (custom-theme-enabled-p 'acme) t)
               :acme-still-known (and (custom-theme-p 'acme) t)
               :faces (neomacs-acme-test-face-state probed-faces))))
    (dolist (theme '(acme neomacs-acme-baseline))
      (when (custom-theme-enabled-p theme)
        (disable-theme theme))))
  (list
   :baseline baseline
   :stacked stacked
   :restored restored
   :restored-matches-baseline
   (equal (plist-get baseline :faces) (plist-get restored :faces))
   :acme-changed-the-appearance
   (not (equal (plist-get baseline :faces) (plist-get stacked :faces)))))
"####,
        expect![[
            r##"OK (:baseline (:themes (neomacs-acme-baseline) :acme-enabled nil :faces ((:face default :defined t :foreground "#d8d8d0" :background "#12151a" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "#f0a000" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#12151a" :background "#4a6fa5" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#1a1e24" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face hl-line :defined t :foreground unspecified :background "#1f242c" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground "#7fb3ff" :background unspecified :weight unspecified :slant unspecified :underline t :overline unspecified :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#f0a000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#12151a" :background "#8fa6c4" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box (:line-width 2 :style released-button) :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#6a7280" :background "#1a1e24" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#12151a" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face header-line :defined t :foreground "#d8d8d0" :background "#252a33" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face show-paren-match :defined t :foreground unspecified :background "#3a5f3a" :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#c98fff" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#8fd08f" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#6a7280" :background unspecified :weight unspecified :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#ff6b6b" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#ffc060" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#7fd07f" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face bold :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face shadow :defined t :foreground "#5a6270" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified))) :stacked (:themes (acme neomacs-acme-baseline) :acme-enabled t :baseline-enabled t :faces ((:face default :defined t :foreground "#444444" :background "#FFFFE8" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face cursor :defined t :foreground "#FFFFE8" :background "#444444" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#444444" :background "#E8EB98" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground "#444444" :background "#FFFFE8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face hl-line :defined t :foreground unspecified :background "#EFEFD8" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground "#0066cc" :background unspecified :weight normal :slant unspecified :underline t :overline unspecified :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#444444" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#444444" :background "#E1FAFF" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#444444" :background "#E5E5D0" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#444444" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face header-line :defined t :foreground "#444444" :background "#E1FAFF" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box 1 :inherit unspecified) (:face show-paren-match :defined t :foreground "#444444" :background "#A8EFEB" :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#1054AF" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#880000" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#005500" :background unspecified :weight unspecified :slant normal :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#880000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#880000" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#005500" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face bold :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face shadow :defined t :foreground "#5a6270" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified))) :restored (:themes (neomacs-acme-baseline) :acme-enabled nil :acme-still-known t :faces ((:face default :defined t :foreground "#d8d8d0" :background "#12151a" :weight normal :slant normal :underline nil :overline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "#f0a000" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#12151a" :background "#4a6fa5" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#1a1e24" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face hl-line :defined t :foreground unspecified :background "#1f242c" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground "#7fb3ff" :background unspecified :weight unspecified :slant unspecified :underline t :overline unspecified :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#f0a000" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#12151a" :background "#8fa6c4" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box (:line-width 2 :style released-button) :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#6a7280" :background "#1a1e24" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#12151a" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face header-line :defined t :foreground "#d8d8d0" :background "#252a33" :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face show-paren-match :defined t :foreground unspecified :background "#3a5f3a" :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#c98fff" :background unspecified :weight normal :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#8fd08f" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#6a7280" :background unspecified :weight unspecified :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#ff6b6b" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#ffc060" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#7fd07f" :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face bold :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :overline unspecified :box unspecified :inherit unspecified) (:face shadow :defined t :foreground "#5a6270" :background unspecified :weight unspecified :slant unspecified :underline unspecified :overline unspecified :box unspecified :inherit unspecified))) :restored-matches-baseline t :acme-changed-the-appearance t)"##
        ]],
    )
}

fn acme_registers_its_installed_directory_and_reloading_the_theme_changes_nothing()
-> ParityBatchCase {
    ParityBatchCase::value(
        "acme_registers_its_installed_directory_and_reloading_the_theme_changes_nothing",
        r####"
(let* ((installed-directories
        (lambda ()
          (length
           (seq-filter
            (lambda (directory)
              (and (stringp directory)
                   (string-match-p "acme-theme-20210430\\.302" directory)))
            custom-theme-load-path))))
       before
       first
       second
       after)
  (unwind-protect
      (progn
        (setq before
              (list
               :known (and (memq 'acme custom-known-themes) t)
               :registered (and (custom-theme-p 'acme) t)
               :enabled (and (custom-theme-enabled-p 'acme) t)
               :name-valid (custom-theme-name-valid-p 'acme)
               :themes (copy-sequence custom-enabled-themes)
               :documentation (get 'acme 'theme-documentation)
               :feature (get 'acme 'theme-feature)
               :provided (featurep 'acme-theme)
               :setting-kinds
               (delete-dups (mapcar #'car (get 'acme 'theme-settings)))
               :settings (length (get 'acme 'theme-settings))
               :load-path-entries (funcall installed-directories)
               :default-foreground (face-attribute 'default :foreground nil t)
               :default-background (face-attribute 'default :background nil t)))
        (load-theme 'acme t)
        (setq first
              (list
               :enabled (and (custom-theme-enabled-p 'acme) t)
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'acme 'theme-settings))
               :load-path-entries (funcall installed-directories)
               :default-foreground (face-attribute 'default :foreground nil t)
               :default-background (face-attribute 'default :background nil t)
               :region-background (face-attribute 'region :background nil t)
               :mode-line-background (face-attribute 'mode-line :background nil t)
               :keyword-foreground
               (face-attribute 'font-lock-keyword-face :foreground nil t)))
        (load-theme 'acme t)
        (setq second
              (list
               :enabled (and (custom-theme-enabled-p 'acme) t)
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'acme 'theme-settings))
               :load-path-entries (funcall installed-directories)
               :default-foreground (face-attribute 'default :foreground nil t)
               :default-background (face-attribute 'default :background nil t)
               :region-background (face-attribute 'region :background nil t)
               :mode-line-background (face-attribute 'mode-line :background nil t)
               :keyword-foreground
               (face-attribute 'font-lock-keyword-face :foreground nil t)))
        (disable-theme 'acme)
        (setq after
              (list
               :known (and (memq 'acme custom-known-themes) t)
               :registered (and (custom-theme-p 'acme) t)
               :enabled (and (custom-theme-enabled-p 'acme) t)
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'acme 'theme-settings))
               :load-path-entries (funcall installed-directories)
               :default-foreground (face-attribute 'default :foreground nil t)
               :default-background (face-attribute 'default :background nil t)
               :region-background (face-attribute 'region :background nil t)
               :mode-line-background (face-attribute 'mode-line :background nil t)
               :keyword-foreground
               (face-attribute 'font-lock-keyword-face :foreground nil t))))
    (when (custom-theme-enabled-p 'acme)
      (disable-theme 'acme)))
  (list
   :before before
   :first first
   :second second
   :reload-is-idempotent (equal first second)
   :after after))
"####,
        expect![[
            r##"OK (:before (:known t :registered t :enabled nil :name-valid t :themes nil :documentation "A color theme based on Acme & Sam" :feature acme-theme :provided t :setting-kinds (theme-face) :settings 314 :load-path-entries 1 :default-foreground "unspecified-fg" :default-background "unspecified-bg") :first (:enabled t :themes (acme) :settings 314 :load-path-entries 1 :default-foreground "#444444" :default-background "#FFFFE8" :region-background "#E8EB98" :mode-line-background "#E1FAFF" :keyword-foreground "#1054AF") :second (:enabled t :themes (acme) :settings 314 :load-path-entries 1 :default-foreground "#444444" :default-background "#FFFFE8" :region-background "#E8EB98" :mode-line-background "#E1FAFF" :keyword-foreground "#1054AF") :reload-is-idempotent t :after (:known t :registered t :enabled nil :themes nil :settings 314 :load-path-entries 1 :default-foreground "unspecified-fg" :default-background "unspecified-bg" :region-background unspecified :mode-line-background unspecified :keyword-foreground unspecified))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_acme_repaints_a_real_elisp_editing_session_with_the_plan_nine_palette(),
        completing_a_real_org_release_checklist_keeps_the_document_and_its_acme_styling_exact(),
        reviewing_a_real_patch_and_listing_its_matches_uses_acmes_diff_and_match_colours(),
        the_black_foreground_option_rewrites_every_derived_face_on_the_next_load_theme(),
        stacking_acme_over_a_dark_user_theme_and_disabling_it_restores_the_previous_appearance(),
        acme_registers_its_installed_directory_and_reloading_the_theme_changes_nothing(),
    ]
}
