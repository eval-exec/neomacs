use expect_test::expect;

use super::ParityBatchCase;

/// The generation, seen from both ends: the palette table each variant is built
/// from, and the same eighteen user-visible faces resolved under all four
/// themes in turn.  Only the nine palette roles differ - the structure, the
/// weights, the slants and the shared accent colours are identical across
/// white, black, gray and cream.
fn every_variant_paints_the_same_faces_from_its_own_palette() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_variant_paints_the_same_faces_from_its_own_palette",
        r##"(progn
  (require 'hl-line)
  (require 'org)
  (list
   (mapcar #'car almost-mono-themes-colors)
   (mapcar (lambda (variant)
             (cons (car variant)
                   (mapcar (lambda (role) (cons (car role) (am-test-copy (cdr role))))
                           (cdr variant))))
           almost-mono-themes-colors)
   (mapcar (lambda (theme)
             (cons theme (am-test-with-theme theme (am-test-face-report))))
           am-test-variants)))"##,
        expect![[
            r##"OK ((white black gray cream) ((white (background . "#ffffff") (foreground . "#000000") (weak . "#888888") (weaker . "#dddddd") (weakest . "#efefef") (highlight . "#fda50f") (warning . "#ff0000") (success . "#00ff00") (string . "#3c5e2b")) (black (background . "#000000") (foreground . "#ffffff") (weak . "#aaaaaa") (weaker . "#666666") (weakest . "#222222") (highlight . "#fda50f") (warning . "#ff0000") (success . "#00ff00") (string . "#a7bca4")) (gray (background . "#2b2b2b") (foreground . "#ffffff") (weak . "#aaaaaa") (weaker . "#666666") (weakest . "#222222") (highlight . "#fda50f") (warning . "#ff0000") (success . "#00ff00") (string . "#a7bca4")) (cream (background . "#f0e5da") (foreground . "#000000") (weak . "#7d7165") (weaker . "#c4baaf") (weakest . "#dbd0c5") (highlight . "#fda50f") (warning . "#ff0000") (success . "#00ff00") (string . "#3c5e2b"))) ((almost-mono-white (default (:background . "#ffffff") (:foreground . "#000000")) (region (:background . "#fda50f") (:foreground . "#000000")) (isearch (:background . "#888888") (:weight . bold)) (lazy-highlight (:background . "#dddddd")) (font-lock-comment-face (:foreground . "#888888") (:slant . italic)) (font-lock-string-face (:foreground . "#3c5e2b")) (font-lock-keyword-face (:weight . bold)) (font-lock-type-face (:slant . italic)) (line-number (:foreground . "#dddddd")) (hl-line (:background . "#efefef")) (mode-line (:background . "#efefef") (:foreground . "#000000") (:box :line-width -1 :color "#dddddd")) (org-todo (:foreground . "#fda50f") (:weight . bold)) (org-done (:foreground . "#00ff00") (:weight . bold)) (show-paren-match (:foreground . "#00ff00") (:weight . bold)) (minibuffer-prompt (:foreground . "#000000") (:weight . bold)) (completions-common-part (:weight . bold) (:underline . t)) (vertical-border (:foreground . "#dddddd"))) (almost-mono-black (default (:background . "#000000") (:foreground . "#ffffff")) (region (:background . "#fda50f") (:foreground . "#ffffff")) (isearch (:background . "#aaaaaa") (:weight . bold)) (lazy-highlight (:background . "#666666")) (font-lock-comment-face (:foreground . "#aaaaaa") (:slant . italic)) (font-lock-string-face (:foreground . "#a7bca4")) (font-lock-keyword-face (:weight . bold)) (font-lock-type-face (:slant . italic)) (line-number (:foreground . "#666666")) (hl-line (:background . "#222222")) (mode-line (:background . "#222222") (:foreground . "#ffffff") (:box :line-width -1 :color "#666666")) (org-todo (:foreground . "#fda50f") (:weight . bold)) (org-done (:foreground . "#00ff00") (:weight . bold)) (show-paren-match (:foreground . "#00ff00") (:weight . bold)) (minibuffer-prompt (:foreground . "#ffffff") (:weight . bold)) (completions-common-part (:weight . bold) (:underline . t)) (vertical-border (:foreground . "#666666"))) (almost-mono-gray (default (:background . "#2b2b2b") (:foreground . "#ffffff")) (region (:background . "#fda50f") (:foreground . "#ffffff")) (isearch (:background . "#aaaaaa") (:weight . bold)) (lazy-highlight (:background . "#666666")) (font-lock-comment-face (:foreground . "#aaaaaa") (:slant . italic)) (font-lock-string-face (:foreground . "#a7bca4")) (font-lock-keyword-face (:weight . bold)) (font-lock-type-face (:slant . italic)) (line-number (:foreground . "#666666")) (hl-line (:background . "#222222")) (mode-line (:background . "#222222") (:foreground . "#ffffff") (:box :line-width -1 :color "#666666")) (org-todo (:foreground . "#fda50f") (:weight . bold)) (org-done (:foreground . "#00ff00") (:weight . bold)) (show-paren-match (:foreground . "#00ff00") (:weight . bold)) (minibuffer-prompt (:foreground . "#ffffff") (:weight . bold)) (completions-common-part (:weight . bold) (:underline . t)) (vertical-border (:foreground . "#666666"))) (almost-mono-cream (default (:background . "#f0e5da") (:foreground . "#000000")) (region (:background . "#fda50f") (:foreground . "#000000")) (isearch (:background . "#7d7165") (:weight . bold)) (lazy-highlight (:background . "#c4baaf")) (font-lock-comment-face (:foreground . "#7d7165") (:slant . italic)) (font-lock-string-face (:foreground . "#3c5e2b")) (font-lock-keyword-face (:weight . bold)) (font-lock-type-face (:slant . italic)) (line-number (:foreground . "#c4baaf")) (hl-line (:background . "#dbd0c5")) (mode-line (:background . "#dbd0c5") (:foreground . "#000000") (:box :line-width -1 :color "#c4baaf")) (org-todo (:foreground . "#fda50f") (:weight . bold)) (org-done (:foreground . "#00ff00") (:weight . bold)) (show-paren-match (:foreground . "#00ff00") (:weight . bold)) (minibuffer-prompt (:foreground . "#000000") (:weight . bold)) (completions-common-part (:weight . bold) (:underline . t)) (vertical-border (:foreground . "#c4baaf")))))"##
        ]],
    )
}

fn the_white_theme_paints_a_font_locked_elisp_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_white_theme_paints_a_font_locked_elisp_buffer",
        r##"(am-test-with-theme 'almost-mono-white
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert ";; Configure the reader\n"
            "(defun demo-reader (path)\n"
            "  \"Read PATH and return its contents.\"\n"
            "  (let ((coding-system-for-read 'utf-8))\n"
            "    (message \"reading %s\" path)\n"
            "    t))\n")
    (font-lock-ensure)
    (list (am-test-token-faces
           '(";; Configure the reader" "defun" "demo-reader"
             "\"Read PATH and return its contents.\"" "let"
             "'utf-8" "message" "\"reading %s\"" "path"))
          (am-test-face-report
           '((default :background :foreground)
             (font-lock-comment-face :foreground :slant)
             (font-lock-doc-face :foreground :slant)
             (font-lock-string-face :foreground)
             (font-lock-constant-face :weight :slant)
             (font-lock-function-name-face :weight)
             (font-lock-variable-name-face :foreground :slant)
             (font-lock-warning-face :foreground :underline)))
          (buffer-substring-no-properties (point-min) (point-max)))))"##,
        expect![[
            r##"OK (((";; Configure the reader" font-lock-comment-delimiter-face "#888888" unspecified italic) ("defun" font-lock-keyword-face unspecified bold unspecified) ("demo-reader" font-lock-function-name-face unspecified bold unspecified) ("\"Read PATH and return its contents.\"" font-lock-doc-face "#888888" unspecified italic) ("let" font-lock-keyword-face unspecified bold unspecified) ("'utf-8" nil nil nil nil) ("message" nil nil nil nil) ("\"reading %s\"" font-lock-string-face "#3c5e2b" unspecified unspecified) ("path" nil nil nil nil)) ((default (:background . "#ffffff") (:foreground . "#000000")) (font-lock-comment-face (:foreground . "#888888") (:slant . italic)) (font-lock-doc-face (:foreground . "#888888") (:slant . italic)) (font-lock-string-face (:foreground . "#3c5e2b")) (font-lock-constant-face (:weight . bold) (:slant . italic)) (font-lock-function-name-face (:weight . bold)) (font-lock-variable-name-face (:foreground . "#000000") (:slant . unspecified)) (font-lock-warning-face (:foreground . "#000000") (:underline :color "#ff0000" :style wave))) ";; Configure the reader\n(defun demo-reader (path)\n  \"Read PATH and return its contents.\"\n  (let ((coding-system-for-read 'utf-8))\n    (message \"reading %s\" path)\n    t))\n")"##
        ]],
    )
}

fn switching_variants_repaints_and_disabling_restores_the_baseline() -> ParityBatchCase {
    ParityBatchCase::value(
        "switching_variants_repaints_and_disabling_restores_the_baseline",
        r##"(progn
  (require 'hl-line)
  (let ((baseline (am-test-face-report
                   '((default :background :foreground)
                     (region :background)
                     (font-lock-comment-face :foreground :slant)
                     (line-number :foreground)
                     (hl-line :background)))))
    (load-theme 'almost-mono-white t)
    (let ((white (list (copy-sequence custom-enabled-themes)
                       (am-test-face-report
                        '((default :background :foreground)
                          (font-lock-comment-face :foreground)
                          (hl-line :background))))))
      (load-theme 'almost-mono-black t)
      (let ((black-on-top (list (copy-sequence custom-enabled-themes)
                                (am-test-face-report
                                 '((default :background :foreground)
                                   (font-lock-comment-face :foreground)
                                   (hl-line :background))))))
        (disable-theme 'almost-mono-black)
        (let ((back-to-white (list (copy-sequence custom-enabled-themes)
                                   (am-test-face-report
                                    '((default :background :foreground)
                                      (font-lock-comment-face :foreground)
                                      (hl-line :background))))))
          (disable-theme 'almost-mono-white)
          (let ((restored (am-test-face-report
                           '((default :background :foreground)
                             (region :background)
                             (font-lock-comment-face :foreground :slant)
                             (line-number :foreground)
                             (hl-line :background)))))
            (list baseline white black-on-top back-to-white
                  (copy-sequence custom-enabled-themes)
                  restored
                  (equal baseline restored))))))))"##,
        expect![[
            r##"OK (((default (:background . "unspecified-bg") (:foreground . "unspecified-fg")) (region (:background . unspecified)) (font-lock-comment-face (:foreground . unspecified) (:slant . italic)) (line-number (:foreground . "unspecified-fg")) (hl-line (:background . unspecified))) ((almost-mono-white) ((default (:background . "#ffffff") (:foreground . "#000000")) (font-lock-comment-face (:foreground . "#888888")) (hl-line (:background . "#efefef")))) ((almost-mono-black almost-mono-white) ((default (:background . "#000000") (:foreground . "#ffffff")) (font-lock-comment-face (:foreground . "#aaaaaa")) (hl-line (:background . "#222222")))) ((almost-mono-white) ((default (:background . "#ffffff") (:foreground . "#000000")) (font-lock-comment-face (:foreground . "#888888")) (hl-line (:background . "#efefef")))) nil ((default (:background . "unspecified-bg") (:foreground . "unspecified-fg")) (region (:background . unspecified)) (font-lock-comment-face (:foreground . unspecified) (:slant . italic)) (line-number (:foreground . "unspecified-fg")) (hl-line (:background . unspecified))) t)"##
        ]],
    )
}

fn the_cream_theme_styles_an_org_dashboard() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_cream_theme_styles_an_org_dashboard",
        r##"(progn
  (require 'org)
  (am-test-with-theme 'almost-mono-cream
    (with-temp-buffer
      (org-mode)
      (insert "#+title: Release dashboard\n"
              "* TODO Ship release\n"
              ":PROPERTIES:\n"
              ":Owner: Ada\n"
              ":END:\n"
              "* DONE Archive notes\n"
              "| Item | State |\n"
              "| API  | Ready |\n")
      (font-lock-ensure)
      (list (am-test-token-faces
             '("#+title:" "TODO" "Ship release" ":PROPERTIES:" ":Owner:"
               "DONE" "Archive notes" "| Item | State |"))
            (am-test-face-report
             '((org-todo :foreground :weight)
               (org-done :foreground :weight)
               (org-drawer :foreground)
               (org-special-keyword :foreground :weight)
               (org-property-value :foreground :slant)
               (org-table :foreground)
               (org-document-title :foreground)
               (org-hide :foreground)))))))"##,
        expect![[
            r##"OK ((("#+title:" org-document-info-keyword unspecified unspecified unspecified) ("TODO" (org-todo org-level-1) "#fda50f" bold unspecified) ("Ship release" org-level-1 unspecified bold unspecified) (":PROPERTIES:" org-drawer "#7d7165" unspecified unspecified) (":Owner:" org-special-keyword "#7d7165" bold unspecified) ("DONE" (org-done org-level-1) "#00ff00" bold unspecified) ("Archive notes" (org-headline-done org-level-1) "#000000" bold unspecified) ("| Item | State |" org-table "#7d7165" unspecified unspecified)) ((org-todo (:foreground . "#fda50f") (:weight . bold)) (org-done (:foreground . "#00ff00") (:weight . bold)) (org-drawer (:foreground . "#7d7165")) (org-special-keyword (:foreground . "#7d7165") (:weight . bold)) (org-property-value (:foreground . "#7d7165") (:slant . italic)) (org-table (:foreground . "#7d7165")) (org-document-title (:foreground . "#000000")) (org-hide (:foreground . "#f0e5da"))))"##
        ]],
    )
}

fn the_gray_theme_styles_a_unified_diff() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_gray_theme_styles_a_unified_diff",
        r##"(progn
  (require 'diff-mode)
  (am-test-with-theme 'almost-mono-gray
    (with-temp-buffer
      (diff-mode)
      (insert "--- a/config.el\n"
              "+++ b/config.el\n"
              "@@ -1,4 +1,4 @@\n"
              " (setq inhibit-startup-screen t)\n"
              "-(setq make-backup-files t)\n"
              "+(setq make-backup-files nil)\n"
              " (global-display-line-numbers-mode)\n")
      (font-lock-ensure)
      (list (am-test-token-faces
             '("--- a/config.el" "+++ b/config.el" "@@ -1,4 +1,4 @@"
               "-(setq make-backup-files t)" "+(setq make-backup-files nil)"))
            (am-test-face-report
             '((default :background :foreground)
               (line-number :foreground)
               (region :background)
               (isearch :background :weight)
               (lazy-highlight :background)
               (vertical-border :foreground)))))))"##,
        expect![[
            r##"OK ((("--- a/config.el" diff-header unspecified bold unspecified) ("+++ b/config.el" diff-header unspecified bold unspecified) ("@@ -1,4 +1,4 @@" diff-hunk-header unspecified bold unspecified) ("-(setq make-backup-files t)" diff-indicator-removed unspecified unspecified unspecified) ("+(setq make-backup-files nil)" diff-indicator-added unspecified unspecified unspecified)) ((default (:background . "#2b2b2b") (:foreground . "#ffffff")) (line-number (:foreground . "#666666")) (region (:background . "#fda50f")) (isearch (:background . "#aaaaaa") (:weight . bold)) (lazy-highlight (:background . "#666666")) (vertical-border (:foreground . "#666666"))))"##
        ]],
    )
}

fn installing_the_package_offers_all_four_variants_by_name() -> ParityBatchCase {
    ParityBatchCase::value(
        "installing_the_package_offers_all_four_variants_by_name",
        r##"(list
 (let ((directory (file-name-directory (locate-library "almost-mono-themes"))))
   (list (and (member (file-name-as-directory directory) custom-theme-load-path) t)
         (sort (mapcar #'am-test-copy
                       (mapcar #'file-name-nondirectory
                               (directory-files directory t "almost-mono.*\\.el\\'")))
               #'string<)))
 (sort (cl-remove-if-not (lambda (theme)
                           (string-prefix-p "almost-mono" (symbol-name theme)))
                         (custom-available-themes))
       (lambda (a b) (string< (symbol-name a) (symbol-name b))))
 (mapcar (lambda (theme)
           (list theme
                 (and (custom-theme-p theme) t)
                 (get theme 'theme-immediate)
                 (am-test-with-theme theme
                   (list (and (custom-theme-p theme) t)
                         (get theme 'theme-immediate)
                         (length (get theme 'theme-settings))
                         (am-test-copy (face-attribute 'default :background nil t))))))
         am-test-variants)
 (condition-case error (progn (load-theme 'almost-mono-purple t) :loaded)
   (error (list (car error) (am-test-copy (cadr error))))))"##,
        expect![[
            r##"OK ((t ("almost-mono-black-theme.el" "almost-mono-cream-theme.el" "almost-mono-gray-theme.el" "almost-mono-themes-autoloads.el" "almost-mono-themes-pkg.el" "almost-mono-themes.el" "almost-mono-white-theme.el")) (almost-mono-black almost-mono-cream almost-mono-gray almost-mono-white) ((almost-mono-white nil nil (t t 73 "#ffffff")) (almost-mono-black nil nil (t t 73 "#000000")) (almost-mono-gray nil nil (t t 73 "#2b2b2b")) (almost-mono-cream nil nil (t t 73 "#f0e5da"))) (error "Unable to find theme file for ‘almost-mono-purple’"))"##
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        every_variant_paints_the_same_faces_from_its_own_palette(),
        the_white_theme_paints_a_font_locked_elisp_buffer(),
        switching_variants_repaints_and_disabling_restores_the_baseline(),
        the_cream_theme_styles_an_org_dashboard(),
        the_gray_theme_styles_a_unified_diff(),
        installing_the_package_offers_all_four_variants_by_name(),
    ]
}
