use expect_test::expect;

use super::ParityBatchCase;

fn atom_one_dark_theme_three_variable_settings_preserve_exact_forms_and_order() -> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_three_variable_settings_preserve_exact_forms_and_order",
        r##"(mapcar
         (lambda (setting)
           (list
            (cadr setting)
            (nth 3 setting)
            (length setting)
            (car setting)
            (caddr setting)))
         (atom-one-dark-test-value-settings))"##,
        expect![[
            r##"OK ((fci-rule-color "#3E4451" 4 theme-value atom-one-dark) (tetris-x-colors [[229 192 123] [97 175 239] [209 154 102] [224 108 117] [152 195 121] [198 120 221] [86 182 194]] 4 theme-value atom-one-dark) (ansi-color-names-vector ["#282C34" "#E06C75" "#98C379" "#E5C07B" "#61AFEF" "#C678DD" "#56B6C2" "#ABB2BF"] 4 theme-value atom-one-dark))"##
        ]],
    )
    .fresh_process()
}

fn atom_one_dark_theme_real_emacs_lisp_font_lock_tokens_resolve_to_theme_palette() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_real_emacs_lisp_font_lock_tokens_resolve_to_theme_palette",
        r##"(unwind-protect
         (progn
           (enable-theme 'atom-one-dark)
           (with-temp-buffer
             (insert
              "(defun greet (name)\n  \"Return greeting for NAME.\"\n  (message \"Hello, %s\" name))\n;; trailing comment\n")
             (emacs-lisp-mode)
             (font-lock-ensure)
             (list
              (mapcar
               (lambda (token)
                 (goto-char
                  (point-min))
                 (search-forward token)
                 (list
                  token
                  (get-text-property
                   (match-beginning 0)
                   'face)))
               '("defun"
                 "greet"
                 "name"
                 "\"Return greeting"
                 "message"
                 "\"Hello, %s\""
                 "trailing comment"))
              (mapcar
               (lambda (face)
                 (cons
                  face
                  (atom-one-dark-test-face-attributes
                   face
                   '(:foreground
                     :background
                     :weight
                     :slant
                     :inherit))))
               '(font-lock-keyword-face
                 font-lock-function-name-face
                 font-lock-variable-name-face
                 font-lock-doc-face
                 font-lock-builtin-face
                 font-lock-string-face
                 font-lock-comment-face)))))
       (when
           (custom-theme-enabled-p
            'atom-one-dark)
         (disable-theme 'atom-one-dark)))"##,
        expect![[
            r##"OK ((("defun" font-lock-keyword-face) ("greet" font-lock-function-name-face) ("name" nil) ("\"Return greeting" font-lock-doc-face) ("message" nil) ("\"Hello, %s\"" font-lock-string-face) ("trailing comment" font-lock-comment-face)) ((font-lock-keyword-face (:foreground "#C678DD" "#C678DD") (:background unspecified unspecified) (:weight normal normal) (:slant unspecified unspecified) (:inherit unspecified unspecified)) (font-lock-function-name-face (:foreground "#61AFEF" "#61AFEF") (:background unspecified unspecified) (:weight unspecified unspecified) (:slant unspecified unspecified) (:inherit unspecified unspecified)) (font-lock-variable-name-face (:foreground "#E06C75" "#E06C75") (:background unspecified unspecified) (:weight unspecified unspecified) (:slant unspecified unspecified) (:inherit unspecified unspecified)) (font-lock-doc-face (:foreground unspecified "#98C379") (:background unspecified unspecified) (:weight unspecified unspecified) (:slant unspecified unspecified) (:inherit #1=(font-lock-string-face) #1#)) (font-lock-builtin-face (:foreground "#56B6C2" "#56B6C2") (:background unspecified unspecified) (:weight unspecified unspecified) (:slant unspecified unspecified) (:inherit unspecified unspecified)) (font-lock-string-face (:foreground "#98C379" "#98C379") (:background unspecified unspecified) (:weight unspecified unspecified) (:slant unspecified unspecified) (:inherit unspecified unspecified)) (font-lock-comment-face (:foreground "#5C6370" "#5C6370") (:background unspecified unspecified) (:weight unspecified unspecified) (:slant italic italic) (:inherit unspecified unspecified))))"##
        ]],
    )
}

fn atom_one_dark_theme_ansi_color_variable_drives_real_escape_sequence_rendering() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_ansi_color_variable_drives_real_escape_sequence_rendering",
        r##"(progn
         (require 'ansi-color)
         (unwind-protect
             (progn
               (enable-theme 'atom-one-dark)
               (let* ((rendered
                       (ansi-color-apply
                        "\e[31mRED\e[0m plain \e[32mGREEN\e[0m"))
                      (red-position
                       (string-match "RED" rendered))
                      (green-position
                       (string-match "GREEN" rendered)))
                 (list
                  (default-value
                   'ansi-color-names-vector)
                  rendered
                  (get-text-property
                   red-position
                   'face
                   rendered)
                  (get-text-property
                   green-position
                   'face
                   rendered)
                  (substring-no-properties
                   rendered))))
           (when
               (custom-theme-enabled-p
                'atom-one-dark)
             (disable-theme 'atom-one-dark))))"##,
        expect![[
            r##"OK (["#282C34" "#E06C75" "#98C379" "#E5C07B" "#61AFEF" "#C678DD" "#56B6C2" "#ABB2BF"] #("RED plain GREEN" 0 3 (font-lock-face (:foreground "red3")) 10 15 (font-lock-face (:foreground "green3"))) nil nil "RED plain GREEN")"##
        ]],
    )
}

fn atom_one_dark_theme_real_compilation_buffer_uses_line_column_success_and_error_faces()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atom_one_dark_theme_real_compilation_buffer_uses_line_column_success_and_error_faces",
        r##"(progn
         (require 'compile)
         (unwind-protect
             (progn
               (enable-theme 'atom-one-dark)
               (with-temp-buffer
                 (insert
                  "src/main.rs:12:7: error: broken\nsrc/lib.rs:3:2: warning: check\nfinished\n")
                 (compilation-mode)
                 (font-lock-ensure)
                 (list
                  (mapcar
                   (lambda (token)
                     (goto-char
                      (point-min))
                     (search-forward token)
                     (list
                      token
                      (get-text-property
                       (match-beginning 0)
                       'face)))
                   '("src/main.rs"
                     "12"
                     "7"
                     "error"
                     "warning"
                     "finished"))
                  (mapcar
                   (lambda (face)
                     (cons
                      face
                      (atom-one-dark-test-face-attributes
                       face
                       '(:foreground
                         :background
                         :weight
                         :inherit))))
                   '(compilation-line-number
                     compilation-column-number
                     compilation-error
                     compilation-warning
                     compilation-info
                     compilation-mode-line-exit
                     compilation-mode-line-fail)))))
           (when
               (custom-theme-enabled-p
                'atom-one-dark)
             (disable-theme 'atom-one-dark))))"##,
        expect![[
            r##"OK ((("src/main.rs" font-lock-function-name-face) ("12" nil) ("7" nil) ("error" nil) ("warning" nil) ("finished" nil)) ((compilation-line-number (:foreground "#828997" "#828997") (:background unspecified unspecified) (:weight unspecified unspecified) (:inherit unspecified unspecified)) (compilation-column-number (:foreground "#828997" "#828997") (:background unspecified unspecified) (:weight unspecified unspecified) (:inherit unspecified unspecified)) (compilation-error (:foreground unspecified "#E06C75") (:background unspecified unspecified) (:weight unspecified bold) (:inherit error error)) (compilation-warning (:foreground unspecified "#E5C07B") (:background unspecified unspecified) (:weight unspecified unspecified) (:inherit warning warning)) (compilation-info (:foreground unspecified "#98C379") (:background unspecified unspecified) (:weight unspecified unspecified) (:inherit success success)) (compilation-mode-line-exit (:foreground unspecified "#98C379") (:background unspecified unspecified) (:weight bold bold) (:inherit compilation-info compilation-info)) (compilation-mode-line-fail (:foreground unspecified "#E06C75") (:background unspecified unspecified) (:weight bold bold) (:inherit compilation-error compilation-error))))"##
        ]],
    )
}

fn atom_one_dark_theme_real_org_buffer_and_org_faces_use_practical_theme_values() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_real_org_buffer_and_org_faces_use_practical_theme_values",
        r##"(progn
         (require 'org)
         (unwind-protect
             (progn
               (enable-theme 'atom-one-dark)
               (with-temp-buffer
                 (insert
                  "* TODO Ship parity\nScheduled: <2026-07-27 Mon>\nSee [[https://example.test][example]].\n")
                 (org-mode)
                 (font-lock-ensure)
                 (list
                  (mapcar
                   (lambda (token)
                     (goto-char
                      (point-min))
                     (search-forward token)
                     (list
                      token
                      (get-text-property
                       (match-beginning 0)
                       'face)))
                   '("TODO"
                     "Ship parity"
                     "Scheduled:"
                     "<2026-07-27 Mon>"
                     "example"))
                  (mapcar
                   (lambda (face)
                     (cons
                      face
                      (atom-one-dark-test-face-attributes
                       face
                       '(:foreground
                         :background
                         :weight
                         :underline
                         :inherit))))
                   '(org-level-1
                     org-todo
                     org-date
                     org-document-title
                     org-document-info
                     org-footnote
                     link)))))
           (when
               (custom-theme-enabled-p
                'atom-one-dark)
             (disable-theme 'atom-one-dark))))"##,
        expect![[
            r##"OK ((("TODO" (org-todo org-level-1)) ("Ship parity" org-level-1) ("Scheduled:" nil) ("<2026-07-27 Mon>" (org-date)) ("example" org-link)) ((org-level-1 (:foreground unspecified "#61AFEF") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit outline-1 outline-1)) (org-todo (:foreground unspecified unspecified) (:background unspecified unspecified) (:weight bold bold) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (org-date (:foreground "#56B6C2" "#56B6C2") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (org-document-title (:foreground unspecified unspecified) (:background unspecified unspecified) (:weight bold bold) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (org-document-info (:foreground "#828997" "#828997") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (org-footnote (:foreground "#56B6C2" "#56B6C2") (:background unspecified unspecified) (:weight unspecified unspecified) (:underline unspecified unspecified) (:inherit unspecified unspecified)) (link (:foreground "#61AFEF" "#61AFEF") (:background unspecified unspecified) (:weight bold bold) (:underline t t) (:inherit unspecified unspecified))))"##
        ]],
    )
}

fn atom_one_dark_theme_mode_line_tab_bar_and_tab_line_rendering_attributes_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "atom_one_dark_theme_mode_line_tab_bar_and_tab_line_rendering_attributes_match",
        r##"(progn
         (dolist
             (face
              '(mode-line
                mode-line-inactive
                mode-line-buffer-id
                tab-bar
                tab-bar-tab
                tab-bar-tab-inactive
                tab-line
                tab-line-tab
                tab-line-tab-current
                tab-line-tab-inactive
                tab-line-highlight))
           (unless
               (facep face)
             (face-spec-set
              face
              '((t
                 (:foreground "fixture-fg"
                  :background "fixture-bg")))
              'face-defface-spec)))
         (unwind-protect
         (progn
           (enable-theme 'atom-one-dark)
           (mapcar
            (lambda (face)
              (cons
               face
               (atom-one-dark-test-face-attributes
                face
                '(:foreground
                  :background
                  :weight
                  :slant
                  :underline
                  :box
                  :inherit))))
            '(mode-line
              mode-line-inactive
              mode-line-buffer-id
              tab-bar
              tab-bar-tab
              tab-bar-tab-inactive
              tab-line
              tab-line-tab
              tab-line-tab-current
              tab-line-tab-inactive
              tab-line-highlight)))
       (when
           (custom-theme-enabled-p
            'atom-one-dark)
         (disable-theme 'atom-one-dark))))"##,
        expect![[
            r##"OK ((mode-line (:foreground "#9DA5B4" "#9DA5B4") (:background "#21252B" "#21252B") (:weight unspecified unspecified) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box #1=(:color "#181A1F" :line-width 1) #1#) (:inherit unspecified unspecified)) (mode-line-inactive (:foreground "#5C6370" "#5C6370") (:background "#181A1F" "#181A1F") (:weight unspecified unspecified) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box #2=(:color "#181A1F" :line-width 1) #2#) (:inherit unspecified unspecified)) (mode-line-buffer-id (:foreground unspecified unspecified) (:background unspecified unspecified) (:weight bold bold) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box unspecified unspecified) (:inherit unspecified unspecified)) (tab-bar (:foreground unspecified unspecified) (:background "#2C323C" "#2C323C") (:weight unspecified unspecified) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box unspecified unspecified) (:inherit unspecified unspecified)) (tab-bar-tab (:foreground "#C678DD" "#C678DD") (:background "#282C34" "#282C34") (:weight unspecified unspecified) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box unspecified unspecified) (:inherit unspecified unspecified)) (tab-bar-tab-inactive (:foreground "#ABB2BF" "#ABB2BF") (:background "#2C323C" "#2C323C") (:weight unspecified unspecified) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box unspecified unspecified) (:inherit unspecified unspecified)) (tab-line (:foreground "#ABB2BF" "#ABB2BF") (:background "#21252B" "#21252B") (:weight unspecified unspecified) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box #3=(:line-width 1 :color "#21252B") #3#) (:inherit unspecified unspecified)) (tab-line-tab (:foreground "#ABB2BF" "#ABB2BF") (:background "#21252B" "#21252B") (:weight unspecified unspecified) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box #4=(:line-width 1 :color "#21252B") #4#) (:inherit unspecified unspecified)) (tab-line-tab-current (:foreground "#ABB2BF" "#ABB2BF") (:background "#282C34" "#282C34") (:weight bold bold) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box #5=(:line-width 1 :color "#282C34") #5#) (:inherit unspecified unspecified)) (tab-line-tab-inactive (:foreground "#ABB2BF" "#ABB2BF") (:background "#21252B" "#21252B") (:weight unspecified unspecified) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box #6=(:line-width 1 :color "#21252B") #6#) (:inherit unspecified unspecified)) (tab-line-highlight (:foreground "#ABB2BF" "#ABB2BF") (:background "#282C34" "#282C34") (:weight unspecified unspecified) (:slant unspecified unspecified) (:underline unspecified unspecified) (:box #7=(:line-width 1 :color "#282C34") #7#) (:inherit unspecified unspecified)))"##
        ]],
    )
}

pub(super) fn practical_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atom_one_dark_theme_three_variable_settings_preserve_exact_forms_and_order(),
        atom_one_dark_theme_real_emacs_lisp_font_lock_tokens_resolve_to_theme_palette(),
        atom_one_dark_theme_ansi_color_variable_drives_real_escape_sequence_rendering(),
        atom_one_dark_theme_real_compilation_buffer_uses_line_column_success_and_error_faces(),
        atom_one_dark_theme_real_org_buffer_and_org_faces_use_practical_theme_values(),
        atom_one_dark_theme_mode_line_tab_bar_and_tab_line_rendering_attributes_match(),
    ]
}
