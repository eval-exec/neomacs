use expect_test::expect;

use super::{ParityBatchCase, assert_arjen_grey_theme_with_prelude_batch};

fn installed_theme_loads_from_its_registered_path_and_survives_a_reload_cycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "installed_theme_loads_from_its_registered_path_and_survives_a_reload_cycle",
        r##"(let ((before
                    (list
                     (custom-theme-p 'arjen-grey)
                     (custom-theme-enabled-p 'arjen-grey)
                     custom-enabled-themes
                     (face-attribute
                      'default :background nil t)))
                   first
                   second
                   after)
               (unwind-protect
                   (progn
                     (load-theme 'arjen-grey t)
                     (setq first
                           (list
                            custom-enabled-themes
                            (custom-theme-enabled-p
                             'arjen-grey)
                            (face-attribute
                             'default :foreground nil t)
                            (face-attribute
                             'default :background nil t)
                            (face-attribute
                             'mode-line :foreground nil t)
                            (face-attribute
                             'mode-line :background nil t)
                            (length
                             (seq-filter
                              (lambda (directory)
                                (and
                                 (stringp directory)
                                 (string-match-p
                                  "arjen-grey-theme-20170522\\.2047"
                                  directory)))
                              custom-theme-load-path))))
                     (load-theme 'arjen-grey t)
                     (setq second
                           (list
                            custom-enabled-themes
                            (custom-theme-enabled-p
                             'arjen-grey)
                            (face-attribute
                             'default :foreground nil t)
                            (face-attribute
                             'default :background nil t)
                            (face-attribute
                             'cursor :background nil t)
                            (face-attribute
                             'region :background nil t)))
                     (disable-theme 'arjen-grey)
                     (setq after
                           (list
                            custom-enabled-themes
                            (custom-theme-enabled-p
                             'arjen-grey)
                            (face-attribute
                             'default :foreground nil t)
                            (face-attribute
                             'default :background nil t))))
                 (when
                     (custom-theme-enabled-p 'arjen-grey)
                   (disable-theme 'arjen-grey)))
               (list before first second after))"##,
        expect![[
            r##"OK ((nil nil nil "unspecified-bg") (#1=(arjen-grey) #1# "#bdc3ce" "#2a2f38" "#bdc3ce" "#242a34" 1) (#2=(arjen-grey) #2# "#bdc3ce" "#2a2f38" "#e1cb8c" "#3c4449") (nil nil "unspecified-fg" "unspecified-bg"))"##
        ]],
    )
}

fn styles_a_real_elisp_editing_session_with_semantic_and_editor_surfaces() -> ParityBatchCase {
    ParityBatchCase::value(
        "styles_a_real_elisp_editing_session_with_semantic_and_editor_surfaces",
        r##"(unwind-protect
               (progn
                 (load-theme 'arjen-grey t)
                 (with-temp-buffer
                   (emacs-lisp-mode)
                   (insert
                    ";; Publish one validated release artifact.\n"
                    "(defun publish-release (artifact)\n"
                    "  (if (file-exists-p artifact)\n"
                    "      (message \"Publishing %s\" artifact)\n"
                    "    (error \"Missing artifact\")))\n")
                   (font-lock-ensure)
                   (goto-char (point-min))
                   (forward-line 2)
                   (push-mark (line-end-position) t t)
                   (setq mark-active t
                         transient-mark-mode t)
                   (let ((selection
                          (buffer-substring-no-properties
                           (region-beginning)
                           (region-end))))
                     (list
                      (buffer-substring-no-properties
                       (point-min) (point-max))
                      (mapcar
                       (lambda (needle)
                         (goto-char (point-min))
                         (search-forward needle)
                         (let* ((position
                                 (match-beginning 0))
                                (face
                                 (get-text-property
                                  position 'face))
                                (primary
                                 (if (symbolp face)
                                     face
                                   (car-safe face))))
                           (list
                            needle
                            face
                            (and
                             (facep primary)
                             (face-attribute
                              primary
                              :foreground nil t))
                            (and
                             (facep primary)
                             (face-attribute
                              primary
                              :weight nil t)))))
                       '("Publish one" "defun"
                         "publish-release" "if ("
                         "file-exists-p" "message"
                         "\"Publishing %s\"" "error"))
                      (list
                       :default
                       (face-attribute
                        'default :foreground nil t)
                       (face-attribute
                        'default :background nil t)
                       :cursor
                       (face-attribute
                        'cursor :background nil t)
                       :mode-line
                       (face-attribute
                        'mode-line :foreground nil t)
                       (face-attribute
                        'mode-line :background nil t)
                       :region
                       (face-attribute
                        'region :background nil t)
                       :selection selection)))))
             (when
                 (custom-theme-enabled-p 'arjen-grey)
               (disable-theme 'arjen-grey)))"##,
        expect![[
            r##"OK (";; Publish one validated release artifact.\n(defun publish-release (artifact)\n  (if (file-exists-p artifact)\n      (message \"Publishing %s\" artifact)\n    (error \"Missing artifact\")))\n" (("Publish one" font-lock-comment-face "#63747c" unspecified) ("defun" font-lock-keyword-face "#b894b0" unspecified) ("publish-release" font-lock-function-name-face "#909fab" unspecified) ("if (" font-lock-keyword-face "#b894b0" unspecified) ("file-exists-p" nil nil nil) ("message" nil nil nil) ("\"Publishing %s\"" font-lock-string-face "#a8c194" unspecified) ("error" font-lock-warning-face "red" bold)) (:default "#bdc3ce" "#2a2f38" :cursor "#e1cb8c" :mode-line "#bdc3ce" "#242a34" :region "#3c4449" :selection "  (if (file-exists-p artifact)"))"##
        ]],
    )
}

fn filters_and_selects_a_deployment_target_in_a_real_helm_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "filters_and_selects_a_deployment_target_in_a_real_helm_session",
        r##"(progn
               (require 'helm)
               (let (selected rendered)
                 (unwind-protect
                     (progn
                       (load-theme 'arjen-grey t)
                       (fset
                        'arjen-grey-deployment-target
                        (lambda ()
                          (interactive)
                          (helm
                           :sources
                           (helm-build-sync-source
                               "Deployment targets"
                             :candidates
                             '(("production — /srv/app"
                                . (:environment
                                   production
                                   :directory
                                   "/srv/app"))
                               ("staging — /srv/app-staging"
                                . (:environment
                                   staging
                                   :directory
                                   "/srv/app-staging"))
                               ("canary — /srv/app-canary"
                                . (:environment
                                   canary
                                   :directory
                                   "/srv/app-canary")))
                             :action
                             (lambda (candidate)
                               (setq selected
                                     candidate)))
                           :input
                           "stag"
                           :buffer
                           "*helm deployment targets*")))
                       (save-window-excursion
                         (with-temp-buffer
                           (switch-to-buffer
                            (current-buffer))
                           (use-local-map
                            (let ((map
                                   (make-sparse-keymap)))
                              (define-key
                               map
                               (kbd "C-c d")
                               #'arjen-grey-deployment-target)
                              map))
                           (let ((helm-after-update-hook
                                  (list
                                   (lambda ()
                                     (with-current-buffer
                                         (get-buffer
                                          "*helm deployment targets*")
                                       (setq rendered
                                             (list
                                              helm-pattern
                                              (helm-get-selection
                                               nil t)
                                              (face-attribute
                                               'helm-source-header
                                               :foreground nil t)
                                              (face-attribute
                                               'helm-source-header
                                               :background nil t)
                                              (face-attribute
                                               'helm-source-header
                                               :weight nil t)
                                              (face-attribute
                                               'helm-source-header
                                               :box nil t)
                                              (face-attribute
                                               'helm-selection
                                               :background nil t)
                                              (face-attribute
                                               'helm-selection
                                               :underline nil t))))))))
                             (execute-kbd-macro
                              (concat
                               "\C-cd"
                               "\r")))))
                       (list selected rendered))
                   (when
                       (custom-theme-enabled-p
                        'arjen-grey)
                     (disable-theme 'arjen-grey))
                   (when
                       (get-buffer
                        "*helm deployment targets*")
                     (kill-buffer
                      "*helm deployment targets*"))
                   (fmakunbound
                    'arjen-grey-deployment-target))))"##,
        expect![[
            r##"OK ((:environment staging :directory "/srv/app-staging") ("stag" "staging — /srv/app-staging" "#bdc3ce" "#2a2f38" bold (:line-width -1 :style released-button) "#3c4449" nil))"##
        ]],
    )
}

pub(super) fn workflows_arjen_grey_theme_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![installed_theme_loads_from_its_registered_path_and_survives_a_reload_cycle()]
}

pub(super) fn workflows_arjen_grey_theme_batch_cases() -> Vec<ParityBatchCase> {
    vec![styles_a_real_elisp_editing_session_with_semantic_and_editor_surfaces()]
}

pub(super) fn workflows_arjen_grey_theme_with_helm_batch_cases() -> Vec<ParityBatchCase> {
    vec![filters_and_selects_a_deployment_target_in_a_real_helm_session()]
}

fn stacks_over_a_user_theme_and_restores_the_previous_palette_and_faces()
-> (&'static str, ParityBatchCase) {
    let prelude = r##"(progn
                 (defvar hl-paren-colors nil)
                 (custom-declare-theme
                  'arjen-grey-parity-base
                  "Existing user theme.")
                 (custom-theme-set-faces
                  'arjen-grey-parity-base
                  '(default
                     ((t
                       (:foreground "#d8dee9"
                        :background "#1b2028"))))
                  '(font-lock-keyword-face
                     ((t
                       (:foreground
                        "#ffcc66"))))
                  '(region
                     ((t
                       (:background
                        "#3b4252")))))
                 (custom-theme-set-variables
                  'arjen-grey-parity-base
                  '(hl-paren-colors
                    '("#ff79c6"
                      "#8be9fd"))))"##;
    let elisp_form = r##"(let (base arjen restored reapplied)
               (unwind-protect
                   (progn
                     (enable-theme
                      'arjen-grey-parity-base)
                     (setq base
                           (list
                            custom-enabled-themes
                            (face-attribute
                             'default :foreground nil t)
                            (face-attribute
                             'default :background nil t)
                            (face-attribute
                             'font-lock-keyword-face
                             :foreground nil t)
                            (face-attribute
                             'region :background nil t)
                            (copy-tree
                             (default-value
                              'hl-paren-colors))))
                     (enable-theme 'arjen-grey)
                     (setq arjen
                           (list
                            custom-enabled-themes
                            (face-attribute
                             'default :foreground nil t)
                            (face-attribute
                             'default :background nil t)
                            (face-attribute
                             'font-lock-keyword-face
                             :foreground nil t)
                            (face-attribute
                             'region :background nil t)
                            (copy-tree
                             (default-value
                              'hl-paren-colors))))
                     (set-default
                      'hl-paren-colors
                      '("runtime-override"))
                     (disable-theme 'arjen-grey)
                     (setq restored
                           (list
                            custom-enabled-themes
                            (face-attribute
                             'default :foreground nil t)
                            (face-attribute
                             'default :background nil t)
                            (face-attribute
                             'font-lock-keyword-face
                             :foreground nil t)
                            (face-attribute
                             'region :background nil t)
                            (copy-tree
                             (default-value
                              'hl-paren-colors))))
                     (enable-theme 'arjen-grey)
                     (setq reapplied
                           (list
                            custom-enabled-themes
                            (copy-tree
                             (default-value
                              'hl-paren-colors))))
                     (disable-theme 'arjen-grey))
                 (when
                     (custom-theme-enabled-p 'arjen-grey)
                   (disable-theme 'arjen-grey))
                 (when
                     (custom-theme-enabled-p
                      'arjen-grey-parity-base)
                   (disable-theme
                    'arjen-grey-parity-base)))
               (list base arjen restored reapplied
                     custom-enabled-themes))"##;
    let expect = expect![[
        r##"OK ((#1=(arjen-grey-parity-base) "#d8dee9" "#1b2028" "#ffcc66" "#3b4252" ("#ff79c6" "#8be9fd")) ((arjen-grey . #1#) "#bdc3ce" "#2a2f38" "#b894b0" "#3c4449" ("#B9F" "#B8D" "#B7B" "#B69" "#B57" "#B45" "#B33" "#B11")) (#1# "#d8dee9" "#1b2028" "#ffcc66" "#3b4252" ("#ff79c6" "#8be9fd")) ((arjen-grey . #1#) ("#B9F" "#B8D" "#B7B" "#B69" "#B57" "#B45" "#B33" "#B11")) nil)"##
    ]];
    (
        prelude,
        ParityBatchCase::value(
            "stacks_over_a_user_theme_and_restores_the_previous_palette_and_faces",
            elisp_form,
            expect,
        ),
    )
}

#[test]
fn workflows_arjen_grey_theme_with_prelude_batch() {
    let (prelude, case) = stacks_over_a_user_theme_and_restores_the_previous_palette_and_faces();
    assert_arjen_grey_theme_with_prelude_batch(prelude, &[case]);
}
