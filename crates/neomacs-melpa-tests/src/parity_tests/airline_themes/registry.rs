use expect_test::expect;

use super::ParityBatchCase;

fn airline_themes_registers_the_complete_public_surface_and_custom_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_registers_the_complete_public_surface_and_custom_contract",
        r##"(let ((functions
                '(airline-themes-set-eshell-prompt
                  airline-themes-mode-line-format
                  airline-themes-set-modeline
                  airline-themes-set-deftheme
                  airline--git-branch-from-head-file
                  airline-curr-dir-git-branch-string
                  airline-get-vc
                  airline-shorten-directory
                  airline-generate-theme-file
                  airline-generate-themes
                  airline-preview-themes))
               (variables
                '(airline-shortened-directory-length
                  airline-hide-state-on-inactive-buffers
                  airline-hide-eyebrowse-on-inactive-buffers
                  airline-hide-vc-branch-on-inactive-buffers
                  airline-eshell-colors
                  airline-helm-colors
                  airline-cursor-colors
                  airline-display-directory
                  airline-utf-glyph-separator-left
                  airline-utf-glyph-separator-right
                  airline-utf-glyph-subseparator-left
                  airline-utf-glyph-subseparator-right
                  airline-utf-glyph-branch
                  airline-utf-glyph-readonly
                  airline-utf-glyph-linenumber))
               (faces
                '(airline-normal-outer airline-normal-inner
                  airline-normal-center airline-insert-outer
                  airline-insert-inner airline-insert-center
                  airline-visual-outer airline-visual-inner
                  airline-visual-center airline-replace-outer
                  airline-replace-inner airline-replace-center
                  airline-emacs-outer airline-emacs-inner
                  airline-emacs-center airline-inactive3)))
         (list
          (featurep 'airline-themes)
          (featurep 'powerline)
          (mapcar
           (lambda (function)
             (list function
                   (help-function-arglist function t)
                   (commandp function)))
           functions)
          (mapcar
           (lambda (variable)
             (list variable
                   (default-value variable)
                   (get variable 'custom-type)
                   (get variable 'custom-group)))
           variables)
          (mapcar
           (lambda (face)
             (list face
                   (facep face)
                   (face-spec-reset-face face)
                   (get face 'face-documentation)
                   (get face 'customized-face)))
           faces)))"##,
        expect![[
            r#"OK (t t ((airline-themes-set-eshell-prompt nil nil) (airline-themes-mode-line-format nil nil) (airline-themes-set-modeline nil t) (airline-themes-set-deftheme (theme-name) nil) (airline--git-branch-from-head-file (filename) nil) (airline-curr-dir-git-branch-string (pwd) nil) (airline-get-vc nil t) (airline-shorten-directory (dir max-length) nil) (airline-generate-theme-file (theme-name json) nil) (airline-generate-themes nil t) (airline-preview-themes nil t)) ((airline-shortened-directory-length 30 (integer) nil) (airline-hide-state-on-inactive-buffers nil (choice (const :tag "Hidden" t) (const :tag "Shown" nil)) nil) (airline-hide-eyebrowse-on-inactive-buffers nil (choice (const :tag "Hidden" t) (const :tag "Shown" nil)) nil) (airline-hide-vc-branch-on-inactive-buffers nil (choice (const :tag "Hidden" t) (const :tag "Shown" nil)) nil) (airline-eshell-colors t (choice (const :tag "Enabled" t) (const :tag "Disabled" nil)) nil) (airline-helm-colors t (choice (const :tag "Enabled" t) (const :tag "Disabled" nil)) nil) (airline-cursor-colors t (choice (const :tag "Enabled" t) (const :tag "Disabled" nil)) nil) (airline-display-directory nil (choice (const :tag "Full" airline-directory-full) (const :tag "Shortened" airline-directory-shortened) (const :tag "Disabled" nil)) nil) (airline-utf-glyph-separator-left 57520 (choice (const :tag "Space: #x20" 32) (const :tag "Box Drawing Bar: │ #x2502" 9474) (const :tag "Box Drawing Forward: Slash ╲ #x2572" 9586) (const :tag "Box Drawing Back Slash: ╱ #x2571" 9585) (const :tag "Block Element Solid Block: █ #x2588" 9608) (const :tag "Block Element 75% Block: ▓ #x2593" 9619) (const :tag "Block Element 50% Block: ▒ #x2592" 9618) (const :tag "Block Element 25% Block: ░ #x2591" 9617) (const :tag "powerline:  #xe0b0" 57520) (const :tag "vim-powerline: ⮀ #x2b80" 11136)) nil) (airline-utf-glyph-separator-right 57522 (choice (const :tag "Space: #x20" 32) (const :tag "Box Drawing Bar: │ #x2502" 9474) (const :tag "Box Drawing Forward Slash: ╲ #x2572" 9586) (const :tag "Box Drawing Back Slash: ╱ #x2571" 9585) (const :tag "Block Element Solid Block: █ #x2588" 9608) (const :tag "Block Element 75% Block: ▓ #x2593" 9619) (const :tag "Block Element 50% Block: ▒ #x2592" 9618) (const :tag "Block Element 25% Block: ░ #x2591" 9617) (const :tag "powerline:  #xe0b2" 57522) (const :tag "vim-powerline: ⮂ #x2b82" 11138)) nil) (airline-utf-glyph-subseparator-left 57521 (choice (const :tag "Space: #x20" 32) (const :tag "Box Drawing Bar: │ #x2502" 9474) (const :tag "Box Drawing Forward Slash: ╲ #x2572" 9586) (const :tag "Box Drawing Back Slash: ╱ #x2571" 9585) (const :tag "Block Element Solid Block: █ #x2588" 9608) (const :tag "Block Element 75% Block: ▓ #x2593" 9619) (const :tag "Block Element 50% Block: ▒ #x2592" 9618) (const :tag "Block Element 25% Block: ░ #x2591" 9617) (const :tag "powerline:  #xe0b1" 57521) (const :tag "vim-powerline ⮁ #x2b81" 11137)) nil) (airline-utf-glyph-subseparator-right 57523 (choice (const :tag "Space: #x20" 32) (const :tag "Box Drawing Bar: │ #x2502" 9474) (const :tag "Box Drawing Forward Slash: ╲ #x2572" 9586) (const :tag "Box Drawing Back Slash: ╱ #x2571" 9585) (const :tag "Block Element Solid Block: █ #x2588" 9608) (const :tag "Block Element 75% Block: ▓ #x2593" 9619) (const :tag "Block Element 50% Block: ▒ #x2592" 9618) (const :tag "Block Element 25% Block: ░ #x2591" 9617) (const :tag "powerline:  #xe0b3" 57523) (const :tag "vim-powerline: ⮃ #x2b83" 11139)) nil) (airline-utf-glyph-branch 57504 (choice (const :tag "option key symbol: ⌥ #x2325" 8997) (const :tag "runic letter fehu: ᚠ #x16a0" 5792) (const :tag "powerline:  #xe0a0" 57504) (const :tag "vim-powerline: ⭠ #x2b60" 11104)) nil) (airline-utf-glyph-readonly 57506 (choice (const :tag "powerline:  #xe0a2" 57506) (const :tag "vim-powerline: ⭤ #x2b64" 11108)) nil) (airline-utf-glyph-linenumber 9552 (choice (const :tag "Box Drawing two horizontal lines: ═ #x2550" 9552) (const :tag "Three horizontal lines: ☰ #x2630" 9776) (const :tag "powerline ln:  #xe0a1" 57505) (const :tag "vim-powerline ln: ⭡ #x2b61" 11105)) nil)) ((airline-normal-outer [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Normal Outer Face" nil) (airline-normal-inner [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Normal Inner Face" nil) (airline-normal-center [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Normal Center Face" nil) (airline-insert-outer [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Insert Outer Face" nil) (airline-insert-inner [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Insert Inner Face" nil) (airline-insert-center [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Insert Center Face" nil) (airline-visual-outer [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Visual Outer Face" nil) (airline-visual-inner [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Visual Inner Face" nil) (airline-visual-center [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Visual Center Face" nil) (airline-replace-outer [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Replace Outer Face" nil) (airline-replace-inner [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Replace Inner Face" nil) (airline-replace-center [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Replace Center Face" nil) (airline-emacs-outer [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Emacs Outer Face" nil) (airline-emacs-inner [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Emacs Inner Face" nil) (airline-emacs-center [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Emacs Center Face" nil) (airline-inactive3 [face :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface :ignore-defface unspecified :ignore-defface unspecified unspecified :ignore-defface] nil "Airline Inactive Center Face" nil)))"#
        ]],
    )
    .fresh_process()
}

fn airline_themes_installed_theme_manifest_is_complete_and_content_addressed_in_chunks()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_installed_theme_manifest_is_complete_and_content_addressed_in_chunks",
        r##"(let* ((directory
                 (file-name-directory
                  (getenv "NEOMACS_PACKAGE_SOURCE")))
                (files
                 (sort
                  (directory-files directory t
                                   "\\`airline-.*-theme\\.el\\'")
                  #'string-lessp))
                (remaining files)
                chunks)
         (while remaining
           (let* ((chunk (seq-take remaining 40))
                  (names
                   (mapcar #'file-name-nondirectory chunk))
                  (hashes
                   (mapcar
                    (lambda (file)
                      (with-temp-buffer
                        (insert-file-contents-literally file)
                        (secure-hash 'sha256 (current-buffer))))
                    chunk)))
             (push
              (list
               (car names)
               (car (last names))
               (length names)
               (secure-hash 'sha256
                            (prin1-to-string hashes)))
              chunks)
             (setq remaining (nthcdr (length chunk) remaining))))
         (list
          (length files)
          (length
           (delete-dups
            (mapcar #'file-name-nondirectory files)))
          (nreverse chunks)))"##,
        expect![[
            r#"OK (245 245 (("airline-alduin-theme.el" "airline-base16_atelierlakeside-theme.el" 40 "139c92f75f6b5b4001a4f60cae0639b5b07025ec7f84f722e9ecc6dd3b2d8c54") ("airline-base16_atelierseaside-theme.el" "airline-base16_flat-theme.el" 40 "8dec9c518e6257b4dfbfb34f2448d6de5ea35576cfecbacd682a7cbba41bb703") ("airline-base16_framer-theme.el" "airline-base16_material_darker-theme.el" 40 "7fced424f5b2c27b8b5b41977d2965cebe4bfd8a662be7d17287d562d9fb9b3c") ("airline-base16_material_lighter-theme.el" "airline-base16_tomorrow_night_eighties-theme.el" 40 "af0137ee95d40b9e4c1a41723911c83feae1b5cb4a418f1007fcc14f50644c6e") ("airline-base16_tube-theme.el" "airline-laederon-theme.el" 40 "0bafb6b19d91f3f6218855b8327cf8f7f1858d0ca1a4227659bf947a18b2e014") ("airline-lessnoise-theme.el" "airline-ubaryd-theme.el" 40 "44fc56ba84349a86cca71b5573b8ec0667b14e90236926239f13d0df5ff8b51c") ("airline-understated-theme.el" "airline-zenburn-theme.el" 5 "2b2c6d4972a81ad6fdc532ef1cb9e442761b9f9ff4da04fc753a2ace06de4100")))"#
        ]],
    )
}

fn airline_themes_every_installed_theme_loads_and_registers_real_face_settings() -> ParityBatchCase
{
    ParityBatchCase::value(
        "airline_themes_every_installed_theme_loads_and_registers_real_face_settings",
        r##"(let* ((directory
                 (file-name-directory
                  (getenv "NEOMACS_PACKAGE_SOURCE")))
                (files
                 (sort
                  (directory-files directory nil
                                   "\\`airline-.*-theme\\.el\\'")
                  #'string-lessp))
                (remaining files)
                chunks
                failures)
         (unless (facep 'rainbow-delimiters-depth-9-face)
           (make-empty-face
            'rainbow-delimiters-depth-9-face))
         (set-face-attribute
          'rainbow-delimiters-depth-9-face nil
          :foreground "#778899"
          :background "#101820")
         (dolist (file files)
           (let* ((name
                   (string-remove-suffix
                    "-theme.el" file))
                  (theme (intern name)))
             (condition-case error-data
                 (progn
                   (load-theme theme t t)
                   (unless
                       (and (not
                             (null
                              (custom-theme-p theme)))
                            (eq (get theme 'theme-feature)
                                (intern (concat name "-theme")))
                            (= 31
                               (seq-count
                                (lambda (setting)
                                  (eq (car setting) 'theme-face))
                                (get theme 'theme-settings))))
                     (push
                     (list file
                            (not
                             (null
                              (custom-theme-p theme)))
                            (get theme 'theme-feature)
                            (length
                             (get theme 'theme-settings)))
                      failures)))
               (error
                (push
                 (list file
                       (car error-data)
                       (error-message-string error-data))
                 failures)))))
         (setq remaining
               (sort
                (seq-filter
                 (lambda (theme)
                   (string-prefix-p "airline-" (symbol-name theme)))
                 (copy-sequence custom-known-themes))
                (lambda (left right)
                  (string-lessp
                   (symbol-name left)
                   (symbol-name right)))))
         (while remaining
           (let* ((chunk (seq-take remaining 40))
                  (records
                   (mapcar
                    (lambda (theme)
                      (list
                       theme
                       (length (get theme 'theme-settings))
                       (secure-hash
                        'sha256
                        (let ((print-circle nil))
                          (prin1-to-string
                           (get theme 'theme-settings))))))
                    chunk)))
             (push
              (list
               (caar records)
               (car (car (last records)))
               (length records)
               (secure-hash 'sha256
                            (prin1-to-string records)))
              chunks)
             (setq remaining (nthcdr (length chunk) remaining))))
         (list
          (length files)
          (length
           (seq-filter
            (lambda (theme)
              (string-prefix-p "airline-" (symbol-name theme)))
            custom-known-themes))
          (nreverse failures)
          (nreverse chunks)))"##,
        expect![[
            r#"OK (245 245 nil ((airline-alduin airline-base16_atelierlakeside 40 "ab20bdbeeae2619dff2c387373eace274981a00a50825285320bfe0ecf26cc8d") (airline-base16_atelierseaside airline-base16_flat 40 "834db76b372cc8aa3d008abd887e275db7f10de643e9f0a058fc20920102cfd6") (airline-base16_framer airline-base16_material_darker 40 "3f80b09677ae8c7a50de61301b91648d0d1fc5added62e0e48c7fc1b23eea745") (airline-base16_material_lighter airline-base16_tomorrow_night_eighties 40 "70069f313b1c522433e2986c7b14175cf0eb06694479d9c0a853e66e3e04982e") (airline-base16_tube airline-laederon 40 "1feb2c22d70788e414403d6477d7b8e536d3effcddcac7102f9ce99a323b8ce5") (airline-lessnoise airline-ubaryd 40 "03a4ef0e550552313a39eb23391bacb94e04f682ee233ad5014dda7f05fa967e") (airline-understated airline-zenburn 5 "12a5957611d2ef5fe8dc24541559c05e31c52a0e6aa3b2ea89f73495e92c2391")))"#
        ]],
    )
    .fresh_process()
}

fn airline_themes_runtime_has_no_hidden_asset_dependency_and_locates_every_entrypoint()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_runtime_has_no_hidden_asset_dependency_and_locates_every_entrypoint",
        r##"(let* ((library (locate-library "airline-themes"))
               (directory (file-name-directory library))
               (representative
                '("airline-light-theme.el"
                  "airline-dark-theme.el"
                  "airline-doom-one-theme.el"
                  "airline-transparent-theme.el"
                  "airline-base16-gui-dark-theme.el"
                  "airline-base16-shell-dark-theme.el"))
               ;; Mask the elpa root rather than this package's own
               ;; directory, because `powerline' below is a *sibling*
               ;; install and only the root is common to both.  Spelling
               ;; the root out pinned the harness's acquisition layout, so
               ;; this expectation broke when the cache moved from
               ;; package-cache/ to the revision-pinned
               ;; source-install-cache/.  The package directory names and
               ;; their versions are the part that carries meaning and are
               ;; kept.
               (elpa
                (directory-file-name
                 (file-name-directory
                  (directory-file-name directory))))
               (mask
                (lambda (value)
                  (if (stringp value)
                      (replace-regexp-in-string
                       (regexp-quote elpa)
                       "[ELPA]"
                       value t t)
                    value))))
         (list
          (funcall mask library)
          (file-name-nondirectory library)
          (mapcar
           (lambda (file)
             (let ((path (expand-file-name file directory)))
               (list file
                     (file-readable-p path)
                     (file-attribute-size
                      (file-attributes path)))))
           representative)
          (directory-files directory nil
                           "\\.\\(png\\|gif\\|jpg\\|svg\\)\\'")
          (funcall mask (locate-library "powerline"))
          (featurep 's)
          (featurep 'json)))"##,
        expect![[
            r#"OK ("[ELPA]/airline-themes-20250502.1915/airline-themes.el" "airline-themes.el" (("airline-light-theme.el" t 2073) ("airline-dark-theme.el" t 2062) ("airline-doom-one-theme.el" t 2130) ("airline-transparent-theme.el" t 2052) ("airline-base16-gui-dark-theme.el" t 2948) ("airline-base16-shell-dark-theme.el" t 2357)) nil "[ELPA]/powerline-20221110.1956/powerline.el" nil t)"#
        ]],
    )
}

fn airline_themes_autoloads_register_the_installed_directory_without_loading_runtime()
-> ParityBatchCase {
    ParityBatchCase::value(
        "airline_themes_autoloads_register_the_installed_directory_without_loading_runtime",
        r##"(let* ((source (getenv "NEOMACS_PACKAGE_SOURCE"))
               (directory
                (file-name-as-directory
                 (file-name-directory source)))
               (theme-files
                (sort
                 (directory-files directory nil
                                  "\\`airline-.*-theme\\.el\\'")
                 #'string-lessp))
               ;; Mask the installed package's own directory.  Spelling it
               ;; out pinned the harness's acquisition layout, so this
               ;; expectation broke when the cache moved from
               ;; package-cache/ to the revision-pinned
               ;; source-install-cache/ -- a harness change wearing the
               ;; shape of a package regression.  What the assertions are
               ;; about is that the installed directory is on
               ;; `custom-theme-load-path', and first on it.
               (mask
                (lambda (value)
                  (if (stringp value)
                      (replace-regexp-in-string
                       (regexp-quote directory)
                       "[PACKAGE]/"
                       value t t)
                    value))))
         (list
          (featurep 'airline-themes)
          (custom-theme-p 'airline-doom-one)
          (mapcar mask (member directory custom-theme-load-path))
          (funcall mask (car custom-theme-load-path))
          (length theme-files)
          (car theme-files)
          (car (last theme-files))
          (file-name-nondirectory source)
          (file-readable-p
           (expand-file-name "airline-themes.el" directory))
          (file-readable-p
           (expand-file-name "airline-themes-pkg.el" directory))))"##,
        expect![[
            r#"OK (nil nil ("[PACKAGE]/" custom-theme-directory t) "[PACKAGE]/" 245 "airline-alduin-theme.el" "airline-zenburn-theme.el" "airline-themes-autoloads.el" t t)"#
        ]],
    )
}

pub(super) fn registry_airline_themes_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        airline_themes_registers_the_complete_public_surface_and_custom_contract(),
        airline_themes_installed_theme_manifest_is_complete_and_content_addressed_in_chunks(),
        airline_themes_every_installed_theme_loads_and_registers_real_face_settings(),
        airline_themes_runtime_has_no_hidden_asset_dependency_and_locates_every_entrypoint(),
    ]
}

pub(super) fn registry_airline_themes_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![airline_themes_autoloads_register_the_installed_directory_without_loading_runtime()]
}
