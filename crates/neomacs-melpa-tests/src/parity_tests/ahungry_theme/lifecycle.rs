use expect_test::expect;

use super::ParityBatchCase;

fn code_palette_overlays_an_existing_editor_theme_and_restores_every_visible_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "code_palette_overlays_an_existing_editor_theme_and_restores_every_visible_state",
        r##"(let ((baseline-theme 'neomacs-ahungry-baseline))
  (deftheme neomacs-ahungry-baseline
    "Deterministic user theme for ahungry lifecycle parity.")
  (custom-theme-set-faces
   baseline-theme
   '(default
      ((t
        (:background "#202020"
         :foreground "#d0d0d0"
         :height 110))))
   '(region
      ((t
        (:background "#405060"
         :foreground "#ffffff"))))
   '(mode-line
      ((t
        (:background "#303030"
         :foreground "#eeeeee"
         :box (:line-width 2 :color "#505050"))))))
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert
     ";;; Release pipeline\n"
     "(defconst release-channel \"stable\")\n"
     "(defun ship-release (candidate)\n"
     "  \"Publish CANDIDATE safely.\"\n"
     "  (when candidate\n"
     "    (message \"ready: %s\" candidate)))\n")
    (font-lock-ensure)
    (let* ((describe
            (lambda (token)
              (goto-char (point-min))
              (search-forward token)
              (let* ((position (- (point) (length token)))
                     (face (get-text-property position 'face))
                     (primary (if (listp face) (car face) face)))
                (list
                 token
                 face
                 (and
                  primary
                  (face-attribute
                   primary :background nil 'default))
                 (and
                  primary
                  (face-attribute
                   primary :foreground nil 'default))
                 (and
                  primary
                  (face-attribute
                   primary :weight nil 'default))
                 (and
                  primary
                  (face-attribute
                   primary :slant nil 'default))))))
           (snapshot
            (lambda ()
              (list
               (copy-sequence custom-enabled-themes)
               (list
                (face-attribute
                 'default :background nil 'default)
                (face-attribute
                 'default :foreground nil 'default)
                (face-attribute
                 'default :height nil 'default))
               (list
                (face-attribute
                 'region :background nil 'default)
                (face-attribute
                 'region :foreground nil 'default))
               (list
                (face-attribute
                 'mode-line :background nil 'default)
                (face-attribute
                 'mode-line :foreground nil 'default)
                (copy-tree
                 (face-attribute
                  'mode-line :box nil 'default)))
               (mapcar
                describe
                '("Release pipeline"
                  "defconst"
                  "release-channel"
                  "\"stable\""
                  "defun"
                  "ship-release"
                  "Publish CANDIDATE"
                  "when"
                  "\"ready: %s\""))))))
      (unwind-protect
          (progn
            (enable-theme baseline-theme)
            (let ((baseline-state (funcall snapshot)))
              (enable-theme 'ahungry)
              (let ((ahungry-state (funcall snapshot)))
                (disable-theme 'ahungry)
                (let ((restored-baseline
                       (funcall snapshot)))
                  (disable-theme baseline-theme)
                  (let ((after (funcall snapshot)))
                    (list
                     baseline-state
                     ahungry-state
                     restored-baseline
                     after
                     (equal
                      baseline-state
                      restored-baseline)))))))
        (dolist
            (theme
             (list 'ahungry baseline-theme))
          (when (memq theme custom-enabled-themes)
            (disable-theme theme)))))))"##,
        expect![[
            r##"OK (((neomacs-ahungry-baseline) ("#202020" "#d0d0d0" 110) ("#405060" "#ffffff") ("#303030" "#eeeeee" (:line-width 2 :color "#505050")) (("Release pipeline" font-lock-comment-face "#202020" "#d0d0d0" bold italic) ("defconst" font-lock-keyword-face "#202020" "#d0d0d0" bold normal) ("release-channel" font-lock-variable-name-face "#202020" "#d0d0d0" bold italic) ("\"stable\"" font-lock-string-face "#202020" "#d0d0d0" normal italic) ("defun" font-lock-keyword-face "#202020" "#d0d0d0" bold normal) ("ship-release" font-lock-function-name-face "#202020" "#d0d0d0" bold normal) ("Publish CANDIDATE" font-lock-doc-face "#202020" "#d0d0d0" normal italic) ("when" font-lock-keyword-face "#202020" "#d0d0d0" bold normal) ("\"ready: %s\"" font-lock-string-face "#202020" "#d0d0d0" normal italic))) ((ahungry neomacs-ahungry-baseline) (unspecified "#ffffff" 130) ("#444444" "#ffffff") ("#77ff00" "#0022aa" (:line-width 1 :color nil :style released-button)) (("Release pipeline" font-lock-comment-face unspecified "#888a85" normal italic) ("defconst" font-lock-keyword-face unspecified "#3cff00" bold normal) ("release-channel" font-lock-variable-name-face unspecified "#0066ff" bold normal) ("\"stable\"" font-lock-string-face unspecified "#ff0077" normal normal) ("defun" font-lock-keyword-face unspecified "#3cff00" bold normal) ("ship-release" font-lock-function-name-face unspecified "#ffee00" bold normal) ("Publish CANDIDATE" font-lock-doc-face unspecified "#777700" bold italic) ("when" font-lock-keyword-face unspecified "#3cff00" bold normal) ("\"ready: %s\"" font-lock-string-face unspecified "#ff0077" normal normal))) ((neomacs-ahungry-baseline) ("#202020" "#d0d0d0" 110) ("#405060" "#ffffff") ("#303030" "#eeeeee" (:line-width 2 :color "#505050")) (("Release pipeline" font-lock-comment-face "#202020" "#d0d0d0" bold italic) ("defconst" font-lock-keyword-face "#202020" "#d0d0d0" bold normal) ("release-channel" font-lock-variable-name-face "#202020" "#d0d0d0" bold italic) ("\"stable\"" font-lock-string-face "#202020" "#d0d0d0" normal italic) ("defun" font-lock-keyword-face "#202020" "#d0d0d0" bold normal) ("ship-release" font-lock-function-name-face "#202020" "#d0d0d0" bold normal) ("Publish CANDIDATE" font-lock-doc-face "#202020" "#d0d0d0" normal italic) ("when" font-lock-keyword-face "#202020" "#d0d0d0" bold normal) ("\"ready: %s\"" font-lock-string-face "#202020" "#d0d0d0" normal italic))) (nil ("unspecified-bg" "unspecified-fg" 1) ("unspecified-bg" "unspecified-fg") ("unspecified-bg" "unspecified-fg" nil) (("Release pipeline" font-lock-comment-face "unspecified-bg" "unspecified-fg" bold italic) ("defconst" font-lock-keyword-face "unspecified-bg" "unspecified-fg" bold normal) ("release-channel" font-lock-variable-name-face "unspecified-bg" "unspecified-fg" bold italic) ("\"stable\"" font-lock-string-face "unspecified-bg" "unspecified-fg" normal italic) ("defun" font-lock-keyword-face "unspecified-bg" "unspecified-fg" bold normal) ("ship-release" font-lock-function-name-face "unspecified-bg" "unspecified-fg" bold normal) ("Publish CANDIDATE" font-lock-doc-face "unspecified-bg" "unspecified-fg" normal italic) ("when" font-lock-keyword-face "unspecified-bg" "unspecified-fg" bold normal) ("\"ready: %s\"" font-lock-string-face "unspecified-bg" "unspecified-fg" normal italic))) t)"##
        ]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![code_palette_overlays_an_existing_editor_theme_and_restores_every_visible_state()]
}
