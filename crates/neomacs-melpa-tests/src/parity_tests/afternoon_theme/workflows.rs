use expect_test::expect;

use super::ParityBatchCase;

/// A reviewer opens a real Elisp file, lets font-lock run, and switches to
/// afternoon.  Asserts the resolved appearance of thirty-one faces before and
/// after, and the face and colour at fifteen tokens.
fn loading_afternoon_repaints_a_real_elisp_editing_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_afternoon_repaints_a_real_elisp_editing_session",
        r####"
(let* ((root (file-name-as-directory
              (expand-file-name "afternoon-elisp-session"
                                (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "ledger.el" root))
       (default-directory root)
       (probed-faces
        '(default cursor region fringe highlight hl-line link minibuffer-prompt
          mode-line mode-line-inactive mode-line-buffer-id header-line
          isearch lazy-highlight show-paren-match trailing-whitespace
          error warning success bold italic underline
          font-lock-comment-face font-lock-comment-delimiter-face
          font-lock-doc-face font-lock-string-face font-lock-keyword-face
          font-lock-function-name-face font-lock-variable-name-face
          font-lock-builtin-face font-lock-constant-face font-lock-type-face
          font-lock-warning-face font-lock-preprocessor-face))
       (probed-tokens
        '(";;; ledger.el" "Settlement helpers" "Keep the audit trail"
          "defvar" "ledger-currency" "Currency used when" "defun"
          "ledger-settle" "Settle INVOICE" "if" "null" "error"
          "\"no invoice\"" "message" ":settled"))
       buffer before after)
  (unwind-protect
      (progn
        (aft-test-cleanup root)
        (make-directory root t)
        (require 'hl-line)
        (with-temp-file source
          (insert ";;; ledger.el --- Settlement helpers\n\n"
                  ";; Keep the audit trail visible to reviewers.\n\n"
                  "(defvar ledger-currency \"EUR\"\n"
                  "  \"Currency used when settling an invoice.\")\n\n"
                  "(defun ledger-settle (invoice)\n"
                  "  \"Settle INVOICE and return its new state.\"\n"
                  "  (if (null invoice)\n"
                  "      (error \"no invoice\")\n"
                  "    (message \"settled %s\" invoice)\n"
                  "    :settled))\n"))
        (setq buffer (find-file-noselect source))
        (switch-to-buffer buffer)
        (emacs-lisp-mode)
        (font-lock-ensure)
        (setq before (list :themes (copy-sequence custom-enabled-themes)
                           :faces (aft-test-face-state probed-faces)))
        (load-theme 'afternoon t)
        (font-lock-flush)
        (font-lock-ensure)
        (goto-char (point-min))
        (search-forward "(message")
        (setq after (list :file (file-relative-name buffer-file-name root)
                          :mode major-mode
                          :themes (copy-sequence custom-enabled-themes)
                          :point (point)
                          :modified (buffer-modified-p)
                          :content (buffer-substring-no-properties
                                    (point-min) (point-max))
                          :faces (aft-test-face-state probed-faces)
                          :tokens (aft-test-token-state probed-tokens))))
    (aft-test-cleanup root))
  (list :before before :after after))
"####,
        expect![[
            r##"OK (:before (:themes nil :faces ((:face default :defined t :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "white" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "gray" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face highlight :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face hl-line :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit highlight) (:face link :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit underline) (:face minibuffer-prompt :defined t :foreground "cyan" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line-inactive :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit mode-line) (:face mode-line-buffer-id :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face header-line :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit mode-line) (:face isearch :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face lazy-highlight :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face show-paren-match :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit underline) (:face trailing-whitespace :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face bold :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face underline :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-delimiter-face :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :box unspecified :inherit font-lock-comment-face) (:face font-lock-doc-face :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit font-lock-string-face) (:face font-lock-string-face :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-variable-name-face :defined t :foreground unspecified :background unspecified :weight bold :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-constant-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline t :box unspecified :inherit unspecified) (:face font-lock-type-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline t :box unspecified :inherit unspecified) (:face font-lock-warning-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit error) (:face font-lock-preprocessor-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit font-lock-builtin-face))) :after (:file "ledger.el" :mode emacs-lisp-mode :themes (afternoon) :point 298 :modified nil :content ";;; ledger.el --- Settlement helpers\n\n;; Keep the audit trail visible to reviewers.\n\n(defvar ledger-currency \"EUR\"\n  \"Currency used when settling an invoice.\")\n\n(defun ledger-settle (invoice)\n  \"Settle INVOICE and return its new state.\"\n  (if (null invoice)\n      (error \"no invoice\")\n    (message \"settled %s\" invoice)\n    :settled))\n" :faces ((:face default :defined t :foreground "#eaeaea" :background "#181a26" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face cursor :defined t :foreground unspecified :background "goldenrod" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background "#103050" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face highlight :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face hl-line :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit highlight) (:face link :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "DeepSkyBlue1" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box (:line-width 1 :color "#eaeaea") :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#969896" :background "#14151E" :weight normal :slant unspecified :underline unspecified :box (:line-width 1 :color "#eaeaea") :inherit mode-line) (:face mode-line-buffer-id :defined t :foreground "#c397d8" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face header-line :defined t :foreground "#c397d8" :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit mode-line) (:face isearch :defined t :foreground "#e7c547" :background "#181a26" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face lazy-highlight :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face show-paren-match :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit underline) (:face trailing-whitespace :defined t :foreground "#d54e53" :background unspecified :weight unspecified :slant unspecified :underline nil :box unspecified :inherit unspecified) (:face error :defined t :foreground "#d54e53" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "goldenrod" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "SeaGreen2" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face bold :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face italic :defined t :foreground unspecified :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face underline :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#969896" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-delimiter-face :defined t :foreground "#969896" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-doc-face :defined t :foreground "moccasin" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "burlywood" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "DeepSkyBlue1" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground "goldenrod" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-variable-name-face :defined t :foreground "#e7c547" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-builtin-face :defined t :foreground "LightCoral" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-constant-face :defined t :foreground "DarkOliveGreen3" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-type-face :defined t :foreground "CadetBlue1" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-warning-face :defined t :foreground "#d54e53" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-preprocessor-face :defined t :foreground "gold" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :tokens ((:token ";;; ledger.el" :face font-lock-comment-delimiter-face :foreground "#969896" :weight unspecified :slant unspecified) (:token "Settlement helpers" :face font-lock-comment-face :foreground "#969896" :weight unspecified :slant unspecified) (:token "Keep the audit trail" :face font-lock-comment-face :foreground "#969896" :weight unspecified :slant unspecified) (:token "defvar" :face font-lock-keyword-face :foreground "DeepSkyBlue1" :weight unspecified :slant unspecified) (:token "ledger-currency" :face font-lock-variable-name-face :foreground "#e7c547" :weight unspecified :slant unspecified) (:token "Currency used when" :face font-lock-doc-face :foreground "moccasin" :weight unspecified :slant unspecified) (:token "defun" :face font-lock-keyword-face :foreground "DeepSkyBlue1" :weight unspecified :slant unspecified) (:token "ledger-settle" :face font-lock-function-name-face :foreground "goldenrod" :weight unspecified :slant unspecified) (:token "Settle INVOICE" :face font-lock-doc-face :foreground "moccasin" :weight unspecified :slant unspecified) (:token "if" :face font-lock-keyword-face :foreground "DeepSkyBlue1" :weight unspecified :slant unspecified) (:token "null" :face nil :foreground nil :weight nil :slant nil) (:token "error" :face font-lock-warning-face :foreground "#d54e53" :weight bold :slant unspecified) (:token "\"no invoice\"" :face font-lock-string-face :foreground "burlywood" :weight unspecified :slant unspecified) (:token "message" :face nil :foreground nil :weight nil :slant nil) (:token ":settled" :face font-lock-builtin-face :foreground "LightCoral" :weight unspecified :slant unspecified))))"##
        ]],
    )
}

fn the_palette_follows_the_terminals_colour_count_at_load_time() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_palette_follows_the_terminals_colour_count_at_load_time",
        r####"
(let ((palette-faces '(default hl-line fringe region cursor
                       font-lock-keyword-face font-lock-string-face
                       font-lock-function-name-face font-lock-comment-face))
      (names '(fci-rule-color ansi-color-names-vector))
      truecolor two-fifty-six back)
  (require 'hl-line)
  (unwind-protect
      (progn
        (setq aft-test-color-cells 16777216)
        (load-theme 'afternoon t)
        (setq truecolor (list :cells (display-color-cells)
                              :faces (aft-test-face-state palette-faces)
                              :vars (aft-test-variables names)))
        (disable-theme 'afternoon)
        (setq aft-test-color-cells 256)
        (load-theme 'afternoon t)
        (setq two-fifty-six (list :cells (display-color-cells)
                                  :faces (aft-test-face-state palette-faces)
                                  :vars (aft-test-variables names)))
        (disable-theme 'afternoon)
        (setq aft-test-color-cells 16777216)
        (load-theme 'afternoon t)
        (setq back (list :cells (display-color-cells)
                         :faces (aft-test-face-state palette-faces)
                         :vars (aft-test-variables names))))
    (when (custom-theme-enabled-p 'afternoon) (disable-theme 'afternoon))
    (setq aft-test-color-cells 16777216))
  (list :truecolor truecolor
        :two-fifty-six two-fifty-six
        :back-to-truecolor back
        :branch-changed-the-palette
        (not (equal (plist-get truecolor :faces) (plist-get two-fifty-six :faces)))
        :branch-is-reversible
        (equal (plist-get truecolor :faces) (plist-get back :faces))))
"####,
        expect![[
            r##"OK (:truecolor (:cells 16777216 :faces ((:face default :defined t :foreground "#eaeaea" :background "#181a26" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face hl-line :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit highlight) (:face fringe :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background "#103050" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face cursor :defined t :foreground unspecified :background "goldenrod" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "DeepSkyBlue1" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "burlywood" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground "goldenrod" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#969896" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :vars ((fci-rule-color "#14151E") (ansi-color-names-vector ["#eaeaea" "#d54e53" "DarkOliveGreen3" "#e7c547" "DeepSkyBlue1" "#c397d8" "#70c0b1" "#181a26"]))) :two-fifty-six (:cells 256 :faces ((:face default :defined t :foreground "#eaeaea" :background "#1c1c1c" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face hl-line :defined t :foreground unspecified :background "#121212" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit highlight) (:face fringe :defined t :foreground unspecified :background "#121212" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background "#103050" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face cursor :defined t :foreground unspecified :background "goldenrod" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "DeepSkyBlue1" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "burlywood" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground "goldenrod" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#969896" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :vars ((fci-rule-color "#121212") (ansi-color-names-vector ["#eaeaea" "#d54e53" "DarkOliveGreen3" "#e7c547" "DeepSkyBlue1" "#c397d8" "#70c0b1" "#1c1c1c"]))) :back-to-truecolor (:cells 16777216 :faces ((:face default :defined t :foreground "#eaeaea" :background "#181a26" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face hl-line :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit highlight) (:face fringe :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face region :defined t :foreground unspecified :background "#103050" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face cursor :defined t :foreground unspecified :background "goldenrod" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "DeepSkyBlue1" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "burlywood" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-function-name-face :defined t :foreground "goldenrod" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#969896" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified)) :vars ((fci-rule-color "#14151E") (ansi-color-names-vector ["#eaeaea" "#d54e53" "DarkOliveGreen3" "#e7c547" "DeepSkyBlue1" "#c397d8" "#70c0b1" "#181a26"]))) :branch-changed-the-palette t :branch-is-reversible t)"##
        ]],
    )
}

fn the_theme_sets_six_variables_and_disable_theme_gives_them_all_back() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_theme_sets_six_variables_and_disable_theme_gives_them_all_back",
        r####"
(let ((names '(fci-rule-color vc-annotate-color-map vc-annotate-very-old-color
               vc-annotate-background ansi-color-names-vector
               ansi-color-faces-vector))
      (faces '(default region mode-line font-lock-keyword-face error))
      before during after)
  (unwind-protect
      (progn
        (setq before (list :themes (copy-sequence custom-enabled-themes)
                           :settings (length (get 'afternoon 'theme-settings))
                           :kinds (delete-dups
                                   (mapcar #'car (get 'afternoon 'theme-settings)))
                           :vars (aft-test-variables names)
                           :faces (aft-test-face-state faces)))
        (load-theme 'afternoon t)
        (setq during (list :themes (copy-sequence custom-enabled-themes)
                           :vars (aft-test-variables names)
                           :faces (aft-test-face-state faces)))
        (disable-theme 'afternoon)
        (setq after (list :themes (copy-sequence custom-enabled-themes)
                          :known (and (custom-theme-p 'afternoon) t)
                          :vars (aft-test-variables names)
                          :faces (aft-test-face-state faces))))
    (when (custom-theme-enabled-p 'afternoon) (disable-theme 'afternoon)))
  (list :before before :during during :after after
        :variables-changed
        (not (equal (plist-get before :vars) (plist-get during :vars)))
        :variables-restored
        (equal (plist-get before :vars) (plist-get after :vars))
        :faces-restored
        (equal (plist-get before :faces) (plist-get after :faces))))
"####,
        expect![[
            r##"OK (:before (:themes nil :settings 417 :kinds (theme-value theme-face) :vars ((fci-rule-color "unconfigured") (vc-annotate-color-map ((20 . "#FF3F3F") (40 . "#FF6C3F") (60 . "#FF993F") (80 . "#FFC63F") (100 . "#FFF33F") (120 . "#DDFF3F") (140 . "#B0FF3F") (160 . "#83FF3F") (180 . "#56FF3F") (200 . "#3FFF56") (220 . "#3FFF83") (240 . "#3FFFB0") (260 . "#3FFFDD") (280 . "#3FF3FF") (300 . "#3FC6FF") (320 . "#3F99FF") (340 . "#3F6CFF") (360 . "#3F3FFF"))) (vc-annotate-very-old-color "#3F3FFF") (vc-annotate-background nil) (ansi-color-names-vector #1=["black" "red3" "green3" "yellow3" "blue2" "magenta3" "cyan3" "gray90"]) (ansi-color-faces-vector #2=[default bold default italic underline success warning error])) :faces ((:face default :defined t :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified))) :during (:themes (afternoon) :vars ((fci-rule-color "#14151E") (vc-annotate-color-map ((20 . "#d54e53") (40 . "goldenrod") (60 . "#e7c547") (80 . "DarkOliveGreen3") (100 . "#70c0b1") (120 . "DeepSkyBlue1") (140 . "#c397d8") (160 . "#d54e53") (180 . "goldenrod") (200 . "#e7c547") (220 . "DarkOliveGreen3") (240 . "#70c0b1") (260 . "DeepSkyBlue1") (280 . "#c397d8") (300 . "#d54e53") (320 . "goldenrod") (340 . "#e7c547") (360 . "DarkOliveGreen3"))) (vc-annotate-very-old-color nil) (vc-annotate-background nil) (ansi-color-names-vector ["#eaeaea" "#d54e53" "DarkOliveGreen3" "#e7c547" "DeepSkyBlue1" "#c397d8" "#70c0b1" "#181a26"]) (ansi-color-faces-vector [default bold shadow italic underline bold bold-italic bold])) :faces ((:face default :defined t :foreground "#eaeaea" :background "#181a26" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :defined t :foreground unspecified :background "#103050" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box (:line-width 1 :color "#eaeaea") :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "DeepSkyBlue1" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#d54e53" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified))) :after (:themes nil :known t :vars ((fci-rule-color "unconfigured") (vc-annotate-color-map ((20 . "#FF3F3F") (40 . "#FF6C3F") (60 . "#FF993F") (80 . "#FFC63F") (100 . "#FFF33F") (120 . "#DDFF3F") (140 . "#B0FF3F") (160 . "#83FF3F") (180 . "#56FF3F") (200 . "#3FFF56") (220 . "#3FFF83") (240 . "#3FFFB0") (260 . "#3FFFDD") (280 . "#3FF3FF") (300 . "#3FC6FF") (320 . "#3F99FF") (340 . "#3F6CFF") (360 . "#3F3FFF"))) (vc-annotate-very-old-color "#3F3FFF") (vc-annotate-background nil) (ansi-color-names-vector #1#) (ansi-color-faces-vector #2#)) :faces ((:face default :defined t :foreground "unspecified-fg" :background "unspecified-bg" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground unspecified :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified))) :variables-changed t :variables-restored t :faces-restored t)"##
        ]],
    )
    .fresh_process()
}

fn stacking_afternoon_over_a_light_user_theme_and_taking_it_off_again() -> ParityBatchCase {
    ParityBatchCase::value(
        "stacking_afternoon_over_a_light_user_theme_and_taking_it_off_again",
        r####"
(let ((probed '(default region cursor fringe link minibuffer-prompt mode-line
                mode-line-inactive font-lock-keyword-face font-lock-string-face
                font-lock-comment-face error warning success shadow tooltip))
      (installed (lambda ()
                   (length (seq-filter
                            (lambda (d) (and (stringp d)
                                             (string-match-p
                                              "afternoon-theme-20140104\\.1859" d)))
                            custom-theme-load-path))))
      baseline stacked restored reloaded)
  (unwind-protect
      (progn
        (eval '(deftheme neomacs-afternoon-baseline
                 "Light theme the reviewer already uses during the day."))
        (custom-theme-set-faces
         'neomacs-afternoon-baseline
         '(default ((t (:foreground "#1a1c20" :background "#fdf6e3"))))
         '(region ((t (:background "#ffe6a7"))))
         '(cursor ((t (:background "#2b6cb0"))))
         '(fringe ((t (:background "#f3ecd8"))))
         '(link ((t (:foreground "#1f5f9e" :underline t))))
         '(minibuffer-prompt ((t (:foreground "#8a4b00" :weight bold))))
         '(mode-line ((t (:foreground "#fdf6e3" :background "#3a4250"))))
         '(mode-line-inactive ((t (:foreground "#6a7280" :background "#eee6d0"))))
         '(font-lock-keyword-face ((t (:foreground "#7b2d8b"))))
         '(font-lock-string-face ((t (:foreground "#8a4b00"))))
         '(font-lock-comment-face ((t (:foreground "#5a6570" :slant italic))))
         '(error ((t (:foreground "#a01010" :weight bold))))
         '(warning ((t (:foreground "#8a5a00" :weight bold))))
         '(success ((t (:foreground "#1f6b3a" :weight bold))))
         '(shadow ((t (:foreground "#8a8f98"))))
         '(tooltip ((t (:background "#fffbe8")))))
        (provide-theme 'neomacs-afternoon-baseline)
        (enable-theme 'neomacs-afternoon-baseline)
        (setq baseline (list :themes (copy-sequence custom-enabled-themes)
                             :faces (aft-test-face-state probed)))
        (load-theme 'afternoon t)
        (setq stacked (list :themes (copy-sequence custom-enabled-themes)
                            :settings (length (get 'afternoon 'theme-settings))
                            :load-path-entries (funcall installed)
                            :faces (aft-test-face-state probed)))
        (load-theme 'afternoon t)
        (setq reloaded (list :themes (copy-sequence custom-enabled-themes)
                             :settings (length (get 'afternoon 'theme-settings))
                             :load-path-entries (funcall installed)
                             :faces (aft-test-face-state probed)))
        (disable-theme 'afternoon)
        (setq restored (list :themes (copy-sequence custom-enabled-themes)
                             :known (and (custom-theme-p 'afternoon) t)
                             :faces (aft-test-face-state probed))))
    (dolist (theme '(afternoon neomacs-afternoon-baseline))
      (when (custom-theme-enabled-p theme) (disable-theme theme))))
  (list :baseline baseline :stacked stacked :restored restored
        :reload-changed-nothing (equal stacked reloaded)
        :restored-matches-baseline
        (equal (plist-get baseline :faces) (plist-get restored :faces))
        :afternoon-changed-the-appearance
        (not (equal (plist-get baseline :faces) (plist-get stacked :faces)))))
"####,
        expect![[
            r##"OK (:baseline (:themes (neomacs-afternoon-baseline) :faces ((:face default :defined t :foreground "#1a1c20" :background "#fdf6e3" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :defined t :foreground unspecified :background "#ffe6a7" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face cursor :defined t :foreground unspecified :background "#2b6cb0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#f3ecd8" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground "#1f5f9e" :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#8a4b00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#fdf6e3" :background "#3a4250" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#6a7280" :background "#eee6d0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#7b2d8b" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#8a4b00" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#5a6570" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#a01010" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#8a5a00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#1f6b3a" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face shadow :defined t :foreground "#8a8f98" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face tooltip :defined t :foreground unspecified :background "#fffbe8" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified))) :stacked (:themes (afternoon neomacs-afternoon-baseline) :settings 417 :load-path-entries 1 :faces ((:face default :defined t :foreground "#eaeaea" :background "#181a26" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :defined t :foreground unspecified :background "#103050" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face cursor :defined t :foreground unspecified :background "goldenrod" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground unspecified :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "DeepSkyBlue1" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground unspecified :background "#14151E" :weight unspecified :slant unspecified :underline unspecified :box (:line-width 1 :color "#eaeaea") :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#969896" :background "#14151E" :weight normal :slant unspecified :underline unspecified :box (:line-width 1 :color "#eaeaea") :inherit mode-line) (:face font-lock-keyword-face :defined t :foreground "DeepSkyBlue1" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "burlywood" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#969896" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#d54e53" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "goldenrod" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "SeaGreen2" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face shadow :defined t :foreground "#969896" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face tooltip :defined t :foreground unspecified :background "#fffbe8" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified))) :restored (:themes (neomacs-afternoon-baseline) :known t :faces ((:face default :defined t :foreground "#1a1c20" :background "#fdf6e3" :weight normal :slant normal :underline nil :box nil :inherit nil) (:face region :defined t :foreground unspecified :background "#ffe6a7" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face cursor :defined t :foreground unspecified :background "#2b6cb0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face fringe :defined t :foreground unspecified :background "#f3ecd8" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face link :defined t :foreground "#1f5f9e" :background unspecified :weight unspecified :slant unspecified :underline t :box unspecified :inherit unspecified) (:face minibuffer-prompt :defined t :foreground "#8a4b00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line :defined t :foreground "#fdf6e3" :background "#3a4250" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face mode-line-inactive :defined t :foreground "#6a7280" :background "#eee6d0" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-keyword-face :defined t :foreground "#7b2d8b" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-string-face :defined t :foreground "#8a4b00" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face font-lock-comment-face :defined t :foreground "#5a6570" :background unspecified :weight unspecified :slant italic :underline unspecified :box unspecified :inherit unspecified) (:face error :defined t :foreground "#a01010" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face warning :defined t :foreground "#8a5a00" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face success :defined t :foreground "#1f6b3a" :background unspecified :weight bold :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face shadow :defined t :foreground "#8a8f98" :background unspecified :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified) (:face tooltip :defined t :foreground unspecified :background "#fffbe8" :weight unspecified :slant unspecified :underline unspecified :box unspecified :inherit unspecified))) :reload-changed-nothing t :restored-matches-baseline t :afternoon-changed-the-appearance t)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_afternoon_repaints_a_real_elisp_editing_session(),
        the_palette_follows_the_terminals_colour_count_at_load_time(),
        the_theme_sets_six_variables_and_disable_theme_gives_them_all_back(),
        stacking_afternoon_over_a_light_user_theme_and_taking_it_off_again(),
    ]
}
