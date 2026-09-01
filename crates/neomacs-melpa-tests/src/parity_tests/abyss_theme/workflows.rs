use expect_test::expect;

use super::ParityBatchCase;

/// A reviewer opens a real Elisp file, lets font-lock run, and switches to
/// abyss.  Every font-lock face the buffer actually uses has to move from the
/// terminal defaults to the exact abyss colours, and the face chosen at each
/// token has to stay the same.
fn loading_abyss_repaints_a_real_elisp_editing_session_with_its_documented_palette()
-> ParityBatchCase {
    ParityBatchCase::value(
        "loading_abyss_repaints_a_real_elisp_editing_session_with_its_documented_palette",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "abyss-elisp-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source
        (expand-file-name "settlement.el" root))
       (default-directory root)
       (probed-faces
        '(default
          cursor
          fringe
          region
          mode-line
          mode-line-inactive
          mode-line-buffer-id
          font-lock-comment-delimiter-face
          font-lock-comment-face
          font-lock-doc-face
          font-lock-string-face
          font-lock-keyword-face
          font-lock-function-name-face
          font-lock-variable-name-face
          font-lock-builtin-face
          font-lock-constant-face
          font-lock-warning-face))
       (probed-tokens
        '(";;; settlement.el"
          "Ledger settlement helpers"
          "Keep the audit trail"
          "defvar"
          "settlement-currency"
          "\"EUR\""
          "Currency used when"
          "defun"
          "settlement-settle"
          "Settle INVOICE"
          "if"
          "null"
          "error"
          "\"No invoice supplied\""
          "message"
          ":settled"))
       buffer
       before
       after)
  (unwind-protect
      (progn
        (neomacs-abyss-test-cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           ";;; settlement.el --- Ledger settlement helpers\n\n"
           ";; Keep the audit trail visible to reviewers.\n\n"
           "(defvar settlement-currency \"EUR\"\n"
           "  \"Currency used when settling an invoice.\")\n\n"
           "(defun settlement-settle (invoice)\n"
           "  \"Settle INVOICE and return its new state.\"\n"
           "  (if (null invoice)\n"
           "      (error \"No invoice supplied\")\n"
           "    (message \"settled %s in %s\" invoice settlement-currency)\n"
           "    :settled))\n"))
        (setq buffer (find-file-noselect source))
        (switch-to-buffer buffer)
        (emacs-lisp-mode)
        (font-lock-ensure)
        (setq before
              (list
               :themes (copy-sequence custom-enabled-themes)
               :faces (neomacs-abyss-test-face-state probed-faces)))
        (load-theme 'abyss t)
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
               :faces (neomacs-abyss-test-face-state probed-faces)
               :tokens (neomacs-abyss-test-token-state probed-tokens))))
    (neomacs-abyss-test-cleanup root))
  (list :before before :after after))
"####,
        expect![[
            r##"OK (:before (:themes nil :faces ((:face default :defined t :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "white" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "gray" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line-inactive :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit mode-line) (:face mode-line-buffer-id :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-delimiter-face :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :box unspecified :inherit font-lock-comment-face) (:face font-lock-comment-face :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit font-lock-string-face) (:face font-lock-string-face :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-variable-name-face :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-constant-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline t :box unspecified :inherit unspecified) (:face font-lock-warning-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit error))) :after (:file "settlement.el" :mode emacs-lisp-mode :themes (abyss) :point 326 :line 12 :modified nil :content ";;; settlement.el --- Ledger settlement helpers\n\n;; Keep the audit trail visible to reviewers.\n\n(defvar settlement-currency \"EUR\"\n  \"Currency used when settling an invoice.\")\n\n(defun settlement-settle (invoice)\n  \"Settle INVOICE and return its new state.\"\n  (if (null invoice)\n      (error \"No invoice supplied\")\n    (message \"settled %s in %s\" invoice settlement-currency)\n    :settled))\n" :faces ((:face default :defined t :foreground "#bbe0f0" :background "#050000" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "white" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#0d1000" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#050000" :background "#cc79a7" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#050000" :background "#56b4e9" :weight unspecified :slant unspecified :underline unspecified :box nil :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#cc79a7" :background "#0d1000" :weight unspecified :slant unspecified :underline unspecified :box nil :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#050000" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-delimiter-face :defined t :foreground "#d55e00" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#d55e00" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :defined t :foreground "#e69f00" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#ff00ff" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#f8ec59" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground "#56b4e9" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-variable-name-face :defined t :foreground "#00ff00" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :defined t :foreground "#fcfbe3" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-constant-face :defined t :foreground "#cc79a7" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-warning-face :defined t :foreground "#FF1A00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :tokens ((:token ";;; settlement.el" :face font-lock-comment-delimiter-face :font-lock-face nil :foreground "#d55e00" :background unspecified :weight unspecified :slant italic) (:token "Ledger settlement helpers" :face font-lock-comment-face :font-lock-face nil :foreground "#d55e00" :background unspecified :weight unspecified :slant italic) (:token "Keep the audit trail" :face font-lock-comment-face :font-lock-face nil :foreground "#d55e00" :background unspecified :weight unspecified :slant italic) (:token "defvar" :face font-lock-keyword-face :font-lock-face nil :foreground "#f8ec59" :background unspecified :weight unspecified :slant unspecified) (:token "settlement-currency" :face font-lock-variable-name-face :font-lock-face nil :foreground "#00ff00" :background unspecified :weight unspecified :slant unspecified) (:token "\"EUR\"" :face font-lock-string-face :font-lock-face nil :foreground "#ff00ff" :background unspecified :weight unspecified :slant unspecified) (:token "Currency used when" :face font-lock-doc-face :font-lock-face nil :foreground "#e69f00" :background unspecified :weight unspecified :slant unspecified) (:token "defun" :face font-lock-keyword-face :font-lock-face nil :foreground "#f8ec59" :background unspecified :weight unspecified :slant unspecified) (:token "settlement-settle" :face font-lock-function-name-face :font-lock-face nil :foreground "#56b4e9" :background unspecified :weight unspecified :slant unspecified) (:token "Settle INVOICE" :face font-lock-doc-face :font-lock-face nil :foreground "#e69f00" :background unspecified :weight unspecified :slant unspecified) (:token "if" :face font-lock-keyword-face :font-lock-face nil :foreground "#f8ec59" :background unspecified :weight unspecified :slant unspecified) (:token "null" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil) (:token "error" :face font-lock-warning-face :font-lock-face nil :foreground "#FF1A00" :background unspecified :weight bold :slant unspecified) (:token "\"No invoice supplied\"" :face font-lock-string-face :font-lock-face nil :foreground "#ff00ff" :background unspecified :weight unspecified :slant unspecified) (:token "message" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil) (:token ":settled" :face font-lock-builtin-face :font-lock-face nil :foreground "#fcfbe3" :background unspecified :weight unspecified :slant unspecified))))"##
        ]],
    )
}

fn a_whitespace_review_session_flags_tabs_and_long_lines_and_clears_them_again() -> ParityBatchCase
{
    ParityBatchCase::value(
        "a_whitespace_review_session_flags_tabs_and_long_lines_and_clears_them_again",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "abyss-whitespace-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source
        (expand-file-name "deploy.conf" root))
       (default-directory root)
       buffer
       result)
  (unwind-protect
      (progn
        (neomacs-abyss-test-cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           "# deploy.conf -- production settings\n"
           "name\tledger-service\n"
           "replicas\t3   \n"
           "notes\tThis annotation is deliberately longer than the review column so it is flagged.\n"))
        (require 'whitespace)
        (setq buffer (find-file-noselect source))
        (switch-to-buffer buffer)
        (text-mode)
        (load-theme 'abyss t)
        (setq-local whitespace-style '(face tabs trailing lines))
        (setq-local whitespace-line-column 40)
        ;; `whitespace-mode' refuses to turn on while `noninteractive' is set,
        ;; so unset that environment flag for the duration of the command.
        (let ((noninteractive nil))
          (whitespace-mode 1))
        (font-lock-flush)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "notes")
        (beginning-of-line)
        (set-mark (point))
        (end-of-line)
        (setq transient-mark-mode t)
        (activate-mark)
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :whitespace-mode whitespace-mode
               :content (buffer-substring-no-properties (point-min) (point-max))
               :modified (buffer-modified-p)
               :point (point)
               :mark (mark)
               :mark-active mark-active
               :selected-text
               (buffer-substring-no-properties (region-beginning) (region-end))
               :themes (copy-sequence custom-enabled-themes)
               :tokens
               (neomacs-abyss-test-token-state
                '("# deploy.conf"
                  "\tledger-service"
                  "\t3"
                  "   \n"
                  "so it is flagged"))
               :faces
               (neomacs-abyss-test-face-state
                '(default
                  region
                  whitespace-tab
                  whitespace-line
                  whitespace-trailing
                  whitespace-space
                  trailing-whitespace
                  highlight
                  secondary-selection))
               :tab-hidden-by-background
               (equal (face-attribute 'whitespace-tab :background nil t)
                      (face-attribute 'default :background nil t))))
        (deactivate-mark)
        (let ((noninteractive nil))
          (whitespace-mode -1))
        (font-lock-ensure)
        (setq result
              (append
               result
               (list
                :after-whitespace-mode whitespace-mode
                :after-tokens
                (neomacs-abyss-test-token-state
                 '("\tledger-service"
                   "so it is flagged"))))))
    (neomacs-abyss-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:file "deploy.conf" :mode text-mode :whitespace-mode t :content "# deploy.conf -- production settings\nname\11ledger-service\nreplicas\0113   \nnotes\11This annotation is deliberately longer than the review column so it is flagged.\n" :modified nil :point 157 :mark 72 :mark-active t :selected-text "notes\11This annotation is deliberately longer than the review column so it is flagged." :themes (abyss) :tokens ((:token "# deploy.conf" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil) (:token "\11ledger-service" :face whitespace-tab :font-lock-face nil :foreground unspecified :background "#050000" :weight unspecified :slant unspecified) (:token "\0113" :face whitespace-tab :font-lock-face nil :foreground unspecified :background "#050000" :weight unspecified :slant unspecified) (:token "   \n" :face whitespace-trailing :font-lock-face nil :foreground unspecified :background unspecified :weight bold :slant unspecified) (:token "so it is flagged" :face (whitespace-line) :font-lock-face nil :foreground "#ffffff" :background "#dd5542" :weight unspecified :slant unspecified)) :faces ((:face default :defined t :foreground "#bbe0f0" :background "#050000" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :defined t :foreground "#050000" :background "#cc79a7" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face whitespace-tab :defined t :foreground unspecified :background "#050000" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face whitespace-line :defined t :foreground "#ffffff" :background "#dd5542" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face whitespace-trailing :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline t :box unspecified :inherit unspecified) (:face whitespace-space :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face trailing-whitespace :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face highlight :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face secondary-selection :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :tab-hidden-by-background t :after-whitespace-mode nil :after-tokens ((:token "\11ledger-service" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil) (:token "so it is flagged" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil)))"##
        ]],
    )
}

fn reading_a_real_build_log_paints_compilation_diagnostics_with_the_abyss_status_palette()
-> ParityBatchCase {
    ParityBatchCase::value(
        "reading_a_real_build_log_paints_compilation_diagnostics_with_the_abyss_status_palette",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "abyss-build-log-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (log
        (expand-file-name "build.log" root))
       (default-directory root)
       buffer
       result)
  (unwind-protect
      (progn
        (neomacs-abyss-test-cleanup root)
        (make-directory root t)
        (with-temp-file log
          (insert
           "make -C ledger check\n"
           "src/settle.el:12:7: error: invoice mismatch\n"
           "src/retry.el:18:3: warning: retry budget exhausted\n"
           "src/audit.el:4:1: note: 42 assertions completed\n\n"
           "Compilation exited abnormally with code 2\n"))
        (require 'compile)
        (load-theme 'abyss t)
        (setq buffer (find-file-noselect log))
        (switch-to-buffer buffer)
        (compilation-mode)
        (font-lock-ensure)
        (goto-char (point-min))
        (compilation-next-error 2)
        (setq result
              (list
               :file (file-relative-name buffer-file-name root)
               :mode major-mode
               :mode-name mode-name
               :themes (copy-sequence custom-enabled-themes)
               :point (point)
               :line (line-number-at-pos)
               :column (current-column)
               :current-line
               (buffer-substring-no-properties
                (line-beginning-position)
                (line-end-position))
               :modified (buffer-modified-p)
               :tokens
               (neomacs-abyss-test-token-state
                '("src/settle.el"
                  "12"
                  "7"
                  "error:"
                  "invoice mismatch"
                  "src/retry.el"
                  "18"
                  "warning:"
                  "src/audit.el"
                  "4"
                  "note:"
                  "42 assertions"
                  "exited abnormally"
                  "code 2"))
               :faces
               (neomacs-abyss-test-face-state
                '(default
                  error
                  warning
                  success
                  compilation-error
                  compilation-warning
                  compilation-info
                  compilation-line-number
                  compilation-column-number
                  compilation-mode-line-exit
                  compilation-mode-line-fail
                  compilation-mode-line-run
                  mode-line
                  mode-line-inactive
                  fringe))
               :warning-foreground-equals-default-background
               (equal (face-attribute 'warning :foreground nil t)
                      (face-attribute 'default :background nil t))
               :error-foreground-differs-from-default-background
               (not (equal (face-attribute 'error :foreground nil t)
                           (face-attribute 'default :background nil t))))))
    (neomacs-abyss-test-cleanup root))
  result)
"####,
        expect![[
            r##"OK (:file "build.log" :mode compilation-mode :mode-name "Compilation" :themes (abyss) :point 66 :line 3 :column 0 :current-line "src/retry.el:18:3: warning: retry budget exhausted" :modified nil :tokens ((:token "src/settle.el" :face font-lock-function-name-face :font-lock-face (compilation-error underline) :foreground "#56b4e9" :background unspecified :weight unspecified :slant unspecified) (:token "12" :face nil :font-lock-face (compilation-line-number underline) :foreground "#f8ec59" :background unspecified :weight unspecified :slant unspecified) (:token "7" :face nil :font-lock-face (compilation-column-number underline) :foreground "#e69f00" :background unspecified :weight unspecified :slant unspecified) (:token "error:" :face nil :font-lock-face (underline) :foreground unspecified :background unspecified :weight unspecified :slant unspecified) (:token "invoice mismatch" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil) (:token "src/retry.el" :face font-lock-function-name-face :font-lock-face (compilation-warning underline) :foreground "#56b4e9" :background unspecified :weight unspecified :slant unspecified) (:token "18" :face nil :font-lock-face (compilation-line-number underline) :foreground "#f8ec59" :background unspecified :weight unspecified :slant unspecified) (:token "warning:" :face nil :font-lock-face (underline) :foreground unspecified :background unspecified :weight unspecified :slant unspecified) (:token "src/audit.el" :face font-lock-function-name-face :font-lock-face (compilation-info underline) :foreground "#56b4e9" :background unspecified :weight unspecified :slant unspecified) (:token "4" :face nil :font-lock-face (compilation-line-number underline) :foreground "#f8ec59" :background unspecified :weight unspecified :slant unspecified) (:token "note:" :face nil :font-lock-face (underline) :foreground unspecified :background unspecified :weight unspecified :slant unspecified) (:token "42 assertions" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil) (:token "exited abnormally" :face compilation-error :font-lock-face nil :foreground "#FF1A00" :background unspecified :weight bold :slant unspecified) (:token "code 2" :face nil :font-lock-face nil :foreground nil :background nil :weight nil :slant nil)) :faces ((:face default :defined t :foreground "#bbe0f0" :background "#050000" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face error :defined t :foreground "#FF1A00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#050000" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#009e73" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face compilation-error :defined t :foreground "#FF1A00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face compilation-warning :defined t :foreground "#050000" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face compilation-info :defined t :foreground "#009e73" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face compilation-line-number :defined t :foreground "#f8ec59" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit font-lock-keyword-face) (:face compilation-column-number :defined t :foreground "#e69f00" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit font-lock-doc-face) (:face compilation-mode-line-exit :defined t :foreground "#009e73" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face compilation-mode-line-fail :defined t :foreground "#FF1A00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face compilation-mode-line-run :defined t :foreground "#050000" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#050000" :background "#56b4e9" :weight unspecified :slant unspecified :underline unspecified :box nil :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#cc79a7" :background "#0d1000" :weight unspecified :slant unspecified :underline unspecified :box nil :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#0d1000" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :warning-foreground-equals-default-background t :error-foreground-differs-from-default-background t)"##
        ]],
    )
}

fn switching_from_a_light_theme_to_abyss_and_back_restores_the_previous_appearance()
-> ParityBatchCase {
    ParityBatchCase::value(
        "switching_from_a_light_theme_to_abyss_and_back_restores_the_previous_appearance",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "abyss-theme-switch-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (probed-faces
        '(default
          cursor
          region
          fringe
          link
          minibuffer-prompt
          mode-line
          mode-line-inactive
          mode-line-buffer-id
          font-lock-keyword-face
          font-lock-string-face
          font-lock-comment-face
          error
          warning
          success
          bold
          bold-italic
          italic
          underline))
       baseline
       stacked
       restored)
  (unwind-protect
      (progn
        (neomacs-abyss-test-cleanup root)
        (make-directory root t)
        (eval
         '(deftheme neomacs-abyss-baseline
            "Light theme the reviewer already uses during the day."))
        (custom-theme-set-faces
         'neomacs-abyss-baseline
         '(default ((t (:foreground "#1a1c20" :background "#fdf6e3"))))
         '(cursor ((t (:background "#2b6cb0"))))
         '(region ((t (:foreground "#1a1c20" :background "#ffe6a7"))))
         '(fringe ((t (:background "#f3ecd8"))))
         '(link ((t (:foreground "#1f5f9e" :underline t))))
         '(minibuffer-prompt ((t (:foreground "#8a4b00" :weight bold))))
         '(mode-line
           ((t (:foreground "#fdf6e3" :background "#3a4250"
                :box (:line-width 1 :style released-button)))))
         '(mode-line-inactive ((t (:foreground "#6a7280" :background "#eee6d0"))))
         '(mode-line-buffer-id ((t (:foreground "#fdf6e3" :weight normal))))
         '(font-lock-keyword-face ((t (:foreground "#7b2d8b" :weight normal))))
         '(font-lock-string-face ((t (:foreground "#8a4b00"))))
         '(font-lock-comment-face ((t (:foreground "#5a6570" :slant italic))))
         '(error ((t (:foreground "#a01010" :weight bold))))
         '(warning ((t (:foreground "#8a5a00" :weight bold))))
         '(success ((t (:foreground "#1f6b3a" :weight bold))))
         '(italic ((t (:slant italic))))
         '(underline ((t (:underline t)))))
        (provide-theme 'neomacs-abyss-baseline)
        (enable-theme 'neomacs-abyss-baseline)
        (setq baseline
              (list
               :themes (copy-sequence custom-enabled-themes)
               :abyss-enabled (and (custom-theme-enabled-p 'abyss) t)
               :faces (neomacs-abyss-test-face-state probed-faces)))
        (load-theme 'abyss t)
        (setq stacked
              (list
               :themes (copy-sequence custom-enabled-themes)
               :abyss-enabled (and (custom-theme-enabled-p 'abyss) t)
               :baseline-enabled
               (and (custom-theme-enabled-p 'neomacs-abyss-baseline) t)
               :faces (neomacs-abyss-test-face-state probed-faces)))
        (disable-theme 'abyss)
        (setq restored
              (list
               :themes (copy-sequence custom-enabled-themes)
               :abyss-enabled (and (custom-theme-enabled-p 'abyss) t)
               :abyss-still-known (and (custom-theme-p 'abyss) t)
               :faces (neomacs-abyss-test-face-state probed-faces))))
    (neomacs-abyss-test-cleanup root))
  (list
   :baseline baseline
   :stacked stacked
   :restored restored
   :restored-matches-baseline
   (equal (plist-get baseline :faces) (plist-get restored :faces))
   :abyss-changed-the-appearance
   (not (equal (plist-get baseline :faces) (plist-get stacked :faces)))))
"####,
        expect![[
            r##"OK (:baseline (:themes (neomacs-abyss-baseline) :abyss-enabled nil :faces ((:face default :defined t :foreground "#1a1c20" :background "#fdf6e3" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "#2b6cb0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#1a1c20" :background "#ffe6a7" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#f3ecd8" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground "#1f5f9e" :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#8a4b00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#fdf6e3" :background "#3a4250" :weight unspecified :slant unspecified :underline unspecified :box (:line-width 1 :style released-button) :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#6a7280" :background "#eee6d0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#fdf6e3" :background unspecified :weight normal :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#7b2d8b" :background unspecified :weight normal :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#8a4b00" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#5a6570" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#a01010" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#8a5a00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#1f6b3a" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face bold :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face bold-italic :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face underline :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified))) :stacked (:themes (abyss neomacs-abyss-baseline) :abyss-enabled t :baseline-enabled t :faces ((:face default :defined t :foreground "#bbe0f0" :background "#050000" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "#2b6cb0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#050000" :background "#cc79a7" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#0d1000" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground "#1f5f9e" :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#8a4b00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#050000" :background "#56b4e9" :weight unspecified :slant unspecified :underline unspecified :box nil :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#cc79a7" :background "#0d1000" :weight unspecified :slant unspecified :underline unspecified :box nil :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#050000" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#f8ec59" :background unspecified :weight normal :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#ff00ff" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#d55e00" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#FF1A00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#050000" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#009e73" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face bold :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face bold-italic :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face underline :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified))) :restored (:themes (neomacs-abyss-baseline) :abyss-enabled nil :abyss-still-known t :faces ((:face default :defined t :foreground "#1a1c20" :background "#fdf6e3" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "#2b6cb0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground "#1a1c20" :background "#ffe6a7" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#f3ecd8" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground "#1f5f9e" :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#8a4b00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#fdf6e3" :background "#3a4250" :weight unspecified :slant unspecified :underline unspecified :box (:line-width 1 :style released-button) :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#6a7280" :background "#eee6d0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line-buffer-id :defined t :foreground "#fdf6e3" :background unspecified :weight normal :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#7b2d8b" :background unspecified :weight normal :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#8a4b00" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#5a6570" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#a01010" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#8a5a00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#1f6b3a" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face bold :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face bold-italic :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face underline :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified))) :restored-matches-baseline t :abyss-changed-the-appearance t)"##
        ]],
    )
}

fn the_abyss_theme_command_enables_the_installed_theme_and_reloading_it_changes_nothing()
-> ParityBatchCase {
    ParityBatchCase::value(
        "the_abyss_theme_command_enables_the_installed_theme_and_reloading_it_changes_nothing",
        r####"
(let* ((installed-directories
        (lambda ()
          (length
           (seq-filter
            (lambda (directory)
              (and (stringp directory)
                   (string-match-p "abyss-theme-20260125\\.1959" directory)))
            custom-theme-load-path))))
       before
       first
       second
       after)
  (unwind-protect
      (progn
        (setq before
              (list
               :known (and (memq 'abyss custom-known-themes) t)
               :registered (and (custom-theme-p 'abyss) t)
               :enabled (and (custom-theme-enabled-p 'abyss) t)
               :themes (copy-sequence custom-enabled-themes)
               :documentation (get 'abyss 'theme-documentation)
               :setting-kinds
               (delete-dups (mapcar #'car (get 'abyss 'theme-settings)))
               :settings (length (get 'abyss 'theme-settings))
               :command (and (commandp 'abyss-theme) t)
               :load-path-entries (funcall installed-directories)
               :default-foreground (face-attribute 'default :foreground nil t)
               :default-background (face-attribute 'default :background nil t)))
        (call-interactively 'abyss-theme)
        (setq first
              (list
               :enabled (and (custom-theme-enabled-p 'abyss) t)
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'abyss 'theme-settings))
               :default-foreground (face-attribute 'default :foreground nil t)
               :default-background (face-attribute 'default :background nil t)
               :mode-line-background (face-attribute 'mode-line :background nil t)
               :region-background (face-attribute 'region :background nil t)
               :keyword-foreground
               (face-attribute 'font-lock-keyword-face :foreground nil t)))
        (abyss-theme)
        (setq second
              (list
               :enabled (and (custom-theme-enabled-p 'abyss) t)
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'abyss 'theme-settings))
               :default-foreground (face-attribute 'default :foreground nil t)
               :default-background (face-attribute 'default :background nil t)
               :mode-line-background (face-attribute 'mode-line :background nil t)
               :region-background (face-attribute 'region :background nil t)
               :keyword-foreground
               (face-attribute 'font-lock-keyword-face :foreground nil t)))
        (disable-theme 'abyss)
        (setq after
              (list
               :known (and (memq 'abyss custom-known-themes) t)
               :registered (and (custom-theme-p 'abyss) t)
               :enabled (and (custom-theme-enabled-p 'abyss) t)
               :themes (copy-sequence custom-enabled-themes)
               :settings (length (get 'abyss 'theme-settings))
               :default-foreground (face-attribute 'default :foreground nil t)
               :default-background (face-attribute 'default :background nil t)
               :mode-line-background (face-attribute 'mode-line :background nil t)
               :region-background (face-attribute 'region :background nil t)
               :keyword-foreground
               (face-attribute 'font-lock-keyword-face :foreground nil t))))
    (when (custom-theme-enabled-p 'abyss)
      (disable-theme 'abyss)))
  (list
   :before before
   :first first
   :second second
   :reload-is-idempotent (equal first second)
   :after after))
"####,
        expect![[
            r##"OK (:before (:known t :registered t :enabled nil :themes nil :documentation "Dark background and contrasting colours." :setting-kinds (theme-face) :settings 52 :command t :load-path-entries 1 :default-foreground "unspecified-fg" :default-background "unspecified-bg") :first (:enabled t :themes (abyss) :settings 52 :default-foreground "#bbe0f0" :default-background "#050000" :mode-line-background "#56b4e9" :region-background "#cc79a7" :keyword-foreground "#f8ec59") :second (:enabled t :themes (abyss) :settings 52 :default-foreground "#bbe0f0" :default-background "#050000" :mode-line-background "#56b4e9" :region-background "#cc79a7" :keyword-foreground "#f8ec59") :reload-is-idempotent t :after (:known t :registered t :enabled nil :themes nil :settings 52 :default-foreground "unspecified-fg" :default-background "unspecified-bg" :mode-line-background unspecified :region-background unspecified :keyword-foreground unspecified))"##
        ]],
    )
}

fn packages_loaded_after_abyss_adopt_its_faces_and_recover_their_own_when_it_is_disabled()
-> ParityBatchCase {
    ParityBatchCase::value(
        "packages_loaded_after_abyss_adopt_its_faces_and_recover_their_own_when_it_is_disabled",
        r####"
(let* ((root
        (file-name-as-directory
         (expand-file-name
          "abyss-late-package-session"
          (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "checkout.el" root))
       (default-directory root)
       (package-faces
        '(flycheck-error
          flycheck-fringe-warning
          envrc-mode-line-on-face
          magit-item-highlight
          flycheck-error-list-highlight))
       buffer
       before
       after
       rendered
       without-theme)
  (unwind-protect
      (progn
        (neomacs-abyss-test-cleanup root)
        (make-directory root t)
        (with-temp-file source
          (insert
           "(defun checkout-total (basket)\n"
           "  (apply #'+ (mapcar #'cdr basket)))\n"))
        (load-theme 'abyss t)
        (setq before
              (list
               :themes (copy-sequence custom-enabled-themes)
               :faces (neomacs-abyss-test-face-state package-faces)))
        ;; The packages abyss styles are not dependencies of the theme; loading
        ;; them later is the ordinary startup order, so declare exactly the
        ;; faces they declare.
        (defface flycheck-error
          '((t (:underline (:style wave :color "Red1"))))
          "Flycheck face for errors.")
        (defface flycheck-fringe-warning
          '((t (:inherit warning)))
          "Flycheck face for the warning fringe indicator.")
        (defface envrc-mode-line-on-face
          '((t (:inherit success)))
          "Envrc face for an active direnv environment.")
        (defface magit-item-highlight
          '((t (:background "grey85")))
          "Magit face for the highlighted item.")
        (defface flycheck-error-list-highlight
          '((t (:inherit highlight)))
          "Flycheck face for the selected error-list row.")
        (setq after
              (list
               :faces (neomacs-abyss-test-face-state package-faces)))
        (setq buffer (find-file-noselect source))
        (switch-to-buffer buffer)
        (emacs-lisp-mode)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "basket")
        (overlay-put
         (make-overlay (match-beginning 0) (match-end 0))
         'face 'flycheck-error)
        (goto-char (point-min))
        (search-forward "mapcar")
        (overlay-put
         (make-overlay (match-beginning 0) (match-end 0))
         'face 'magit-item-highlight)
        (setq rendered
              (list
               :tokens
               (neomacs-abyss-test-token-state
                '("basket" "mapcar" "defun"))))
        (disable-theme 'abyss)
        (setq without-theme
              (list
               :themes (copy-sequence custom-enabled-themes)
               :faces (neomacs-abyss-test-face-state package-faces))))
    (neomacs-abyss-test-cleanup root))
  (list
   :before before
   :after after
   :rendered rendered
   :without-theme without-theme))
"####,
        expect![[
            r##"OK (:before (:themes (abyss) :faces ((:face flycheck-error :defined nil) (:face flycheck-fringe-warning :defined nil) (:face envrc-mode-line-on-face :defined nil) (:face magit-item-highlight :defined nil) (:face flycheck-error-list-highlight :defined nil))) :after (:faces ((:face flycheck-error :defined t :foreground "#FF1A00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face flycheck-fringe-warning :defined t :foreground "#e69f00" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face envrc-mode-line-on-face :defined t :foreground "#009e73" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit nil) (:face magit-item-highlight :defined t :foreground "#050000" :background "#cc79a7" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit region) (:face flycheck-error-list-highlight :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit highlight))) :rendered (:tokens ((:token "basket" :face flycheck-error :font-lock-face nil :foreground "#FF1A00" :background unspecified :weight bold :slant unspecified) (:token "mapcar" :face magit-item-highlight :font-lock-face nil :foreground "#050000" :background "#cc79a7" :weight unspecified :slant unspecified) (:token "defun" :face font-lock-keyword-face :font-lock-face nil :foreground "#f8ec59" :background unspecified :weight unspecified :slant unspecified))) :without-theme (:themes nil :faces ((:face flycheck-error :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline (:style wave :color "Red1") :box unspecified :inherit unspecified) (:face flycheck-fringe-warning :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit warning) (:face envrc-mode-line-on-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit success) (:face magit-item-highlight :defined t :foreground unspecified :background "grey85" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face flycheck-error-list-highlight :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit highlight))))"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_abyss_repaints_a_real_elisp_editing_session_with_its_documented_palette(),
        a_whitespace_review_session_flags_tabs_and_long_lines_and_clears_them_again(),
        reading_a_real_build_log_paints_compilation_diagnostics_with_the_abyss_status_palette(),
        switching_from_a_light_theme_to_abyss_and_back_restores_the_previous_appearance(),
        the_abyss_theme_command_enables_the_installed_theme_and_reloading_it_changes_nothing(),
        packages_loaded_after_abyss_adopt_its_faces_and_recover_their_own_when_it_is_disabled(),
    ]
}
