use expect_test::expect;

use super::ParityBatchCase;

fn airline_themes_enable_switch_disable_workflow_applies_and_restores_real_faces() -> ParityBatchCase
{
    ParityBatchCase::value(
        "airline_themes_enable_switch_disable_workflow_applies_and_restores_real_faces",
        r##"(let ((faces
                '(airline-normal-outer
                  airline-normal-center
                  airline-insert-outer
                  airline-visual-outer
                  mode-line
                  mode-line-inactive))
               snapshots)
         (dolist (theme
                  '(airline-light
                    airline-doom-one
                    airline-transparent))
           (dolist (enabled
                    (copy-sequence custom-enabled-themes))
             (disable-theme enabled))
           (let ((loaded (load-theme theme t)))
             (push
              (list
               theme
               loaded
               custom-enabled-themes
               (mapcar
                (lambda (face)
                  (list
                   face
                   (face-attribute
                    face :foreground nil t)
                   (face-attribute
                    face :background nil t)
                   (face-attribute
                    face :box nil t)
                   (face-attribute
                    face :underline nil t)))
                faces))
              snapshots)))
         (dolist (enabled
                  (copy-sequence custom-enabled-themes))
           (disable-theme enabled))
         (list
          (nreverse snapshots)
          custom-enabled-themes
          (mapcar
           (lambda (face)
             (list
              face
              (face-attribute face :foreground nil t)
              (face-attribute face :background nil t)))
           faces)))"##,
        expect![[
            r##"OK (((airline-light t (airline-light) ((airline-normal-outer "#ffffff" "#005fff" unspecified unspecified) (airline-normal-center "#005fff" "#afffff" unspecified unspecified) (airline-insert-outer "#ffffff" "#00875f" unspecified unspecified) (airline-visual-outer "#ffffff" "#ff5f00" unspecified unspecified) (mode-line "#005fff" "#afffff" nil nil) (mode-line-inactive "#666666" "#b2b2b2" nil nil))) (airline-doom-one t (airline-doom-one) ((airline-normal-outer "#1B2229" "#51afef" unspecified unspecified) (airline-normal-center "#bbc2cf" "#21242b" unspecified unspecified) (airline-insert-outer "#1B2229" "#98be65" unspecified unspecified) (airline-visual-outer "#1B2229" "#4db5bd" unspecified unspecified) (mode-line "#bbc2cf" "#21242b" nil nil) (mode-line-inactive "#5B6268" "#23272e" nil nil))) (airline-transparent t (airline-transparent) ((airline-normal-outer "#8d96a1" "NONE" unspecified unspecified) (airline-normal-center "#3f4b59" "NONE" unspecified unspecified) (airline-insert-outer "#1d1f21" "#BBE67E" unspecified unspecified) (airline-visual-outer "#1d1f21" "#F07178" unspecified unspecified) (mode-line "#3f4b59" "NONE" nil nil) (mode-line-inactive "#1d1f21" "NONE" nil nil)))) nil ((airline-normal-outer "#141413" "#aeee00") (airline-normal-center "#8cffba" "#242321") (airline-insert-outer "#141413" "#0a9dff") (airline-visual-outer "#141413" "#ffa724") (mode-line unspecified unspecified) (mode-line-inactive unspecified unspecified)))"##
        ]],
    )
}

fn airline_themes_reload_is_idempotent_for_settings_modeline_and_enabled_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "airline_themes_reload_is_idempotent_for_settings_modeline_and_enabled_state",
        r##"(let (snapshots first-settings)
         (dotimes (iteration 3)
           (when (custom-theme-p 'airline-doom-one)
             (put 'airline-doom-one 'theme-settings nil)
             (setq custom-known-themes
                   (delq 'airline-doom-one
                         custom-known-themes)))
           (load-theme 'airline-doom-one t t)
           (unless first-settings
             (setq first-settings
                   (copy-tree
                    (get 'airline-doom-one
                         'theme-settings))))
           (push
            (list
             iteration
             (length
              (get 'airline-doom-one 'theme-settings))
             (equal
              first-settings
              (get 'airline-doom-one 'theme-settings))
             (secure-hash
              'sha256
              (prin1-to-string
               (default-value 'mode-line-format)))
             (seq-count
              (lambda (theme)
                (eq theme 'airline-doom-one))
              custom-known-themes)
             (custom-theme-enabled-p
              'airline-doom-one))
            snapshots))
         (load-theme 'airline-doom-one t)
         (let ((first-enabled
                (copy-sequence custom-enabled-themes)))
           (load-theme 'airline-doom-one t)
           (list
            (nreverse snapshots)
            first-enabled
            custom-enabled-themes
            (seq-count
             (lambda (theme)
               (eq theme 'airline-doom-one))
             custom-enabled-themes))))"##,
        expect![[
            r#"OK (((0 31 t "59bc99f40dbf40670e9af38f4b8c39468ce24909f30ff9dd5f7bfcac517c10a4" 1 nil) (1 31 t "59bc99f40dbf40670e9af38f4b8c39468ce24909f30ff9dd5f7bfcac517c10a4" 1 nil) (2 31 t "59bc99f40dbf40670e9af38f4b8c39468ce24909f30ff9dd5f7bfcac517c10a4" 1 nil)) (airline-doom-one) (airline-doom-one) 1)"#
        ]],
    )
}

fn airline_themes_cursor_customization_drives_all_evil_state_cursor_shapes() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_cursor_customization_drives_all_evil_state_cursor_shapes",
        r##"(let ((symbols
                '(evil-emacs-state-cursor
                  evil-normal-state-cursor
                  evil-insert-state-cursor
                  evil-replace-state-cursor
                  evil-visual-state-cursor)))
         (dolist (symbol symbols)
           (set symbol (intern
                        (format "before-%s" symbol))))
         (setq airline-cursor-colors t)
         (load-theme 'airline-doom-one t t)
         (let ((enabled
                (mapcar
                 (lambda (symbol)
                   (list symbol (symbol-value symbol)))
                 symbols)))
           (put 'airline-doom-one 'theme-settings nil)
           (setq custom-known-themes
                 (delq 'airline-doom-one
                       custom-known-themes)
                 airline-cursor-colors nil)
           (dolist (symbol symbols)
             (set symbol (intern
                          (format "sentinel-%s" symbol))))
           (load-theme 'airline-doom-one t t)
           (list
            enabled
            (mapcar
             (lambda (symbol)
               (list symbol (symbol-value symbol)))
             symbols))))"##,
        expect![[
            r##"OK (((evil-emacs-state-cursor "#a9a1e1") (evil-normal-state-cursor "#51afef") (evil-insert-state-cursor (bar "#98be65")) (evil-replace-state-cursor "#ff6c6b") (evil-visual-state-cursor "#4db5bd")) ((evil-emacs-state-cursor sentinel-evil-emacs-state-cursor) (evil-normal-state-cursor sentinel-evil-normal-state-cursor) (evil-insert-state-cursor sentinel-evil-insert-state-cursor) (evil-replace-state-cursor sentinel-evil-replace-state-cursor) (evil-visual-state-cursor sentinel-evil-visual-state-cursor)))"##
        ]],
    )
}

fn airline_themes_eshell_customization_preserves_or_installs_a_real_prompt_function()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_eshell_customization_preserves_or_installs_a_real_prompt_function",
        r##"(let ((sentinel (lambda () "sentinel prompt")))
         (setq eshell-prompt-function sentinel
               eshell-prompt-regexp "sentinel-regexp"
               eshell-highlight-prompt nil
               airline-eshell-colors nil)
         (load-theme 'airline-light t t)
         (let ((disabled
                (list
                 (eq eshell-prompt-function sentinel)
                 (funcall eshell-prompt-function)
                 eshell-prompt-regexp
                 eshell-highlight-prompt)))
           (put 'airline-light 'theme-settings nil)
           (setq custom-known-themes
                 (delq 'airline-light custom-known-themes)
                 airline-eshell-colors t)
           (load-theme 'airline-light t t)
           (list
            disabled
            (functionp eshell-prompt-function)
            (eq eshell-prompt-function sentinel)
            eshell-prompt-regexp
            eshell-highlight-prompt)))"##,
        expect![[r#"OK ((t "sentinel prompt" "sentinel-regexp" nil) t nil "^ [^#$]* [#$] " t)"#]],
    )
}

fn airline_themes_set_modeline_replaces_global_and_local_formats_consistently() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_set_modeline_replaces_global_and_local_formats_consistently",
        r##"(let ((original-default
                (copy-tree
                 (default-value 'mode-line-format)))
               first-buffer second-buffer)
         (with-temp-buffer
           (setq-local mode-line-format
                       '("LOCAL-OVERRIDE"))
           (airline-themes-set-modeline)
           (setq first-buffer
                 (list
                  (local-variable-p 'mode-line-format)
                  mode-line-format
                  (default-value 'mode-line-format))))
         (with-temp-buffer
           (setq second-buffer
                 (list
                  (local-variable-p 'mode-line-format)
                  mode-line-format
                  (default-value 'mode-line-format))))
         (list
          first-buffer
          second-buffer
          (equal original-default
                 (default-value 'mode-line-format))
          (length
           (default-value 'mode-line-format))
          (secure-hash
           'sha256
           (prin1-to-string
            (default-value 'mode-line-format)))))"##,
        expect![[
            r#"OK ((nil #1=("%e" (:eval (let* ((current-window-width (window-width)) (active (powerline-selected-window-active)) (separator-left (intern (format "powerline-%s-%s" (powerline-current-separator) (car powerline-default-separator-dir)))) (separator-right (intern (format "powerline-%s-%s" (powerline-current-separator) (cdr powerline-default-separator-dir)))) (mode-line-face (if active 'mode-line 'mode-line-inactive)) (evil-mode-active (featurep 'evil)) (visual-block (if evil-mode-active (and (evil-visual-state-p) (eq evil-visual-selection 'block)) nil)) (visual-line (if evil-mode-active (and (evil-visual-state-p) (eq evil-visual-selection 'line)) nil)) (current-evil-state-string (if evil-mode-active (upcase (concat (symbol-name evil-state) (cond (visual-block "-BLOCK") (visual-line "-LINE")))) nil)) (current-evil-state-string (if (and current-evil-state-string (< current-window-width 80)) (substring current-evil-state-string 0 1) current-evil-state-string)) (outer-face (if active (if evil-mode-active (cond ((eq evil-state (intern "normal")) 'airline-normal-outer) ((eq evil-state (intern "insert")) 'airline-insert-outer) ((eq evil-state (intern "visual")) 'airline-visual-outer) ((eq evil-state (intern "replace")) 'airline-replace-outer) ((eq evil-state (intern "emacs")) 'airline-emacs-outer) (t 'airline-normal-outer)) 'airline-normal-outer) 'powerline-inactive1)) (inner-face (if active (if evil-mode-active (cond ((eq evil-state (intern "normal")) 'airline-normal-inner) ((eq evil-state (intern "insert")) 'airline-insert-inner) ((eq evil-state (intern "visual")) 'airline-visual-inner) ((eq evil-state (intern "replace")) 'airline-replace-inner) ((eq evil-state (intern "emacs")) 'airline-emacs-inner) (t 'airline-normal-inner)) 'airline-normal-inner) 'powerline-inactive2)) (center-face (if active (if evil-mode-active (cond ((eq evil-state (intern "normal")) 'airline-normal-center) ((eq evil-state (intern "insert")) 'airline-insert-center) ((eq evil-state (intern "visual")) 'airline-visual-center) ((eq evil-state (intern "replace")) 'airline-replace-center) ((eq evil-state (intern "emacs")) 'airline-emacs-center) (t 'airline-normal-center)) 'airline-normal-center) 'airline-inactive3)) (lhs-mode (when (or (not airline-hide-state-on-inactive-buffers) (and airline-hide-state-on-inactive-buffers active)) (if evil-mode-active (list (powerline-raw (concat " " current-evil-state-string " ") outer-face) (funcall separator-left outer-face inner-face) (powerline-raw "%*" inner-face 'l)) (list (powerline-raw "%*" outer-face 'l) (powerline-raw " " outer-face) (funcall separator-left outer-face inner-face))))) (lhs-rest (list (if (and (or (not airline-hide-eyebrowse-on-inactive-buffers) (and airline-hide-eyebrowse-on-inactive-buffers active)) (featurep 'eyebrowse)) (powerline-raw (concat " " (eyebrowse-mode-line-indicator)) inner-face 'r)) (if (and (or (not airline-hide-vc-branch-on-inactive-buffers) (and airline-hide-vc-branch-on-inactive-buffers active)) buffer-file-name vc-mode) (powerline-raw (airline-get-vc) inner-face)) (powerline-raw " " inner-face) (funcall separator-left inner-face center-face) (cond ((and buffer-file-name (eq airline-display-directory 'airline-directory-shortened)) (powerline-raw (airline-shorten-directory default-directory airline-shortened-directory-length) center-face 'l)) ((and buffer-file-name (eq airline-display-directory 'airline-directory-full)) (powerline-raw default-directory center-face 'l)) (t (powerline-raw " " center-face))) (powerline-raw "%b" center-face) (when (and (boundp 'which-func-mode) which-func-mode) (powerline-raw which-func-format center-face 'l)) (when (boundp 'erc-modified-channels-object) (powerline-raw erc-modified-channels-object center-face 'l)))) (lhs (append lhs-mode lhs-rest)) (rhs (list (powerline-raw global-mode-string center-face 'r) (powerline-minor-modes center-face 'l) (powerline-raw (char-to-string airline-utf-glyph-subseparator-right) center-face 'l) (powerline-major-mode center-face 'l) (powerline-process center-face) (powerline-raw " " center-face) (funcall separator-right center-face inner-face) (powerline-raw (format " %s " buffer-file-coding-system) inner-face) (funcall separator-right inner-face outer-face) (powerline-raw "%3p" outer-face 'l) (powerline-raw (char-to-string airline-utf-glyph-linenumber) outer-face 'l) (powerline-raw (format "%%l/%d" (count-lines (point-min) (point-max))) outer-face 'l) (powerline-raw "ln :" outer-face 'l) (powerline-raw "%3c " outer-face 'l)))) (concat (powerline-render lhs) (powerline-fill center-face (powerline-width rhs)) (powerline-render rhs))))) #1#) (nil #1# #1#) nil 2 "59bc99f40dbf40670e9af38f4b8c39468ce24909f30ff9dd5f7bfcac517c10a4")"#
        ]],
    )
    .fresh_process()
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        airline_themes_enable_switch_disable_workflow_applies_and_restores_real_faces(),
        airline_themes_reload_is_idempotent_for_settings_modeline_and_enabled_state(),
        airline_themes_cursor_customization_drives_all_evil_state_cursor_shapes(),
        airline_themes_eshell_customization_preserves_or_installs_a_real_prompt_function(),
        airline_themes_set_modeline_replaces_global_and_local_formats_consistently(),
    ]
}
