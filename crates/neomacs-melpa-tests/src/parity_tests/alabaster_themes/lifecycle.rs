use expect_test::expect;

use super::ParityBatchCase;

fn load_helper_disables_competing_theme_runs_hook_and_enables_exact_target() -> ParityBatchCase {
    ParityBatchCase::value(
        "load_helper_disables_competing_theme_runs_hook_and_enables_exact_target",
        r##"
(progn
  (mapc #'disable-theme custom-enabled-themes)
  (deftheme alabaster-test-competing
    "Deterministic competing theme.")
  (custom-theme-set-faces
   'alabaster-test-competing
   '(default ((t (:foreground "#101010"
                  :background "#fefefe")))))
  (provide-theme 'alabaster-test-competing)
  (enable-theme 'alabaster-test-competing)
  (defvar alabaster-themes-test-events nil)
  (setq alabaster-themes-test-events nil)
  (let ((alabaster-themes-post-load-hook
         (list
          (lambda ()
            (push
             (copy-sequence custom-enabled-themes)
             alabaster-themes-test-events)))))
    (unwind-protect
        (let ((result
               (alabaster-themes-load-theme
                'alabaster-themes-dark)))
          (list
           result
           custom-enabled-themes
           (custom-theme-enabled-p
            'alabaster-test-competing)
           (nreverse alabaster-themes-test-events)
           (face-attribute
            'default :foreground nil 'default)
           (face-attribute
            'default :background nil 'default)))
      (mapc #'disable-theme custom-enabled-themes)
      (makunbound 'alabaster-themes-test-events))))
"##,
        expect![[
            r#"OK (alabaster-themes-dark (alabaster-themes-dark) nil ((alabaster-themes-dark)) "unspecified-fg" "unspecified-bg")"#
        ]],
    )
}

fn switching_through_all_variants_updates_enabled_theme_and_resolved_core_palette()
-> ParityBatchCase {
    ParityBatchCase::value(
        "switching_through_all_variants_updates_enabled_theme_and_resolved_core_palette",
        r##"
(progn
  (mapc #'disable-theme custom-enabled-themes)
  (defvar alabaster-themes-test-events nil)
  (setq alabaster-themes-test-events nil)
  (let ((alabaster-themes-post-load-hook
         (list
          (lambda ()
            (push
             (car custom-enabled-themes)
             alabaster-themes-test-events)))))
    (unwind-protect
        (list
         (mapcar
          (lambda (theme)
            (let ((result
                   (alabaster-themes-load-theme theme)))
              (list
               theme result
               (copy-sequence custom-enabled-themes)
               (face-attribute
                'default :foreground nil 'default)
               (face-attribute
                'default :background nil 'default)
               (face-attribute
                'font-lock-comment-face
                :foreground nil 'default)
               (face-attribute
                'font-lock-comment-face
                :background nil 'default)
               (face-attribute
                'font-lock-string-face
                :foreground nil 'default)
               (face-attribute
                'font-lock-string-face
                :background nil 'default))))
          alabaster-themes-collection)
         (nreverse alabaster-themes-test-events))
      (mapc #'disable-theme custom-enabled-themes)
      (makunbound 'alabaster-themes-test-events))))
"##,
        expect![[
            r#"OK (((alabaster-themes-light alabaster-themes-light (alabaster-themes-light) "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg") (alabaster-themes-light-bg alabaster-themes-light-bg (alabaster-themes-light-bg) "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg") (alabaster-themes-light-mono alabaster-themes-light-mono (alabaster-themes-light-mono) "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg") (alabaster-themes-dark alabaster-themes-dark (alabaster-themes-dark) "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg") (alabaster-themes-dark-mono alabaster-themes-dark-mono (alabaster-themes-dark-mono) "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg" "unspecified-fg" "unspecified-bg")) (alabaster-themes-light alabaster-themes-light-bg alabaster-themes-light-mono alabaster-themes-dark alabaster-themes-dark-mono))"#
        ]],
    )
}

fn repeated_load_is_idempotent_for_registry_enabled_state_and_face_specs() -> ParityBatchCase {
    ParityBatchCase::value(
        "repeated_load_is_idempotent_for_registry_enabled_state_and_face_specs",
        r##"
(progn
  (mapc #'disable-theme custom-enabled-themes)
  (defvar alabaster-themes-test-hook-count nil)
  (setq alabaster-themes-test-hook-count 0)
  (let ((alabaster-themes-post-load-hook
         (list
          (lambda ()
            (setq alabaster-themes-test-hook-count
                  (1+ alabaster-themes-test-hook-count))))))
    (unwind-protect
        (let (states)
          (dotimes (_ 3)
            (alabaster-themes-load-theme
             'alabaster-themes-light)
            (let ((settings
                   (get 'alabaster-themes-light
                        'theme-settings)))
              (push
               (list
                (copy-sequence custom-enabled-themes)
                (length settings)
                (secure-hash
                 'sha256
                 (prin1-to-string
                  (mapcar
                   (lambda (setting)
                     (secure-hash
                      'sha256
                      (prin1-to-string setting)))
                   settings))))
               states)))
          (list
           (nreverse states)
           alabaster-themes-test-hook-count))
      (mapc #'disable-theme custom-enabled-themes)
      (makunbound 'alabaster-themes-test-hook-count))))
"##,
        expect![[
            r#"OK ((((alabaster-themes-light) 501 "2fb6e2032be43146001b10a3ecda9d71ea8e8317bbac6e5da7fa6c33b118c063") ((alabaster-themes-light) 501 "2fb6e2032be43146001b10a3ecda9d71ea8e8317bbac6e5da7fa6c33b118c063") ((alabaster-themes-light) 501 "2fb6e2032be43146001b10a3ecda9d71ea8e8317bbac6e5da7fa6c33b118c063")) 3)"#
        ]],
    )
}

fn enable_disable_reenable_cycle_restores_and_reapplies_theme_without_rereading_file()
-> ParityBatchCase {
    ParityBatchCase::value(
        "enable_disable_reenable_cycle_restores_and_reapplies_theme_without_rereading_file",
        r##"
(progn
  (mapc #'disable-theme custom-enabled-themes)
  (load-theme 'alabaster-themes-light t t)
  (let ((snapshot
         (lambda ()
           (list
            (copy-sequence custom-enabled-themes)
            (custom-theme-enabled-p
             'alabaster-themes-light)
            (face-attribute
             'default :foreground nil 'default)
            (face-attribute
             'default :background nil 'default)
            (face-attribute
             'font-lock-function-name-face
             :foreground nil 'default)))))
    (unwind-protect
        (progn
          (enable-theme 'alabaster-themes-light)
          (let ((enabled (funcall snapshot)))
            (disable-theme 'alabaster-themes-light)
            (let ((disabled (funcall snapshot)))
              (enable-theme 'alabaster-themes-light)
              (list
               enabled
               disabled
               (funcall snapshot)
               (custom-theme-p
                'alabaster-themes-light)))))
      (mapc #'disable-theme custom-enabled-themes))))
"##,
        expect![[
            r#"OK (((alabaster-themes-light) (alabaster-themes-light) "unspecified-fg" "unspecified-bg" "unspecified-fg") (nil nil "unspecified-fg" "unspecified-bg" "unspecified-fg") ((alabaster-themes-light) (alabaster-themes-light) "unspecified-fg" "unspecified-bg" "unspecified-fg") (alabaster-themes-light user changed))"#
        ]],
    )
}

fn reloading_after_palette_override_changes_faces_and_reset_restores_original_specs()
-> ParityBatchCase {
    ParityBatchCase::value(
        "reloading_after_palette_override_changes_faces_and_reset_restores_original_specs",
        r##"
(progn
  (load-theme 'alabaster-themes-light t t)
  (let ((alabaster-themes-common-palette-overrides nil)
        (alabaster-themes-light-palette-overrides nil))
    (let ((capture
           (lambda ()
             (mapcar
              (lambda (face)
                (let ((entry
                       (seq-find
                        (lambda (setting)
                          (eq (nth 1 setting) face))
                        (get 'alabaster-themes-light
                             'theme-settings))))
                  (list face (copy-tree (nth 3 entry)))))
              '(default font-lock-string-face
                font-lock-function-name-face
                font-lock-comment-face)))))
      (load-theme 'alabaster-themes-light t t)
      (let ((original (funcall capture)))
        (setq
         alabaster-themes-common-palette-overrides
         '((green "#00aa00")
           (blue "#0000aa"))
         alabaster-themes-light-palette-overrides
         '((blue "#1234ff")
           (comment "#ee1111")))
        (load-theme 'alabaster-themes-light t t)
        (let ((overridden (funcall capture)))
          (setq
           alabaster-themes-common-palette-overrides nil
           alabaster-themes-light-palette-overrides nil)
          (load-theme 'alabaster-themes-light t t)
          (list
           original overridden (funcall capture)
           (equal original (funcall capture))))))))
"##,
        expect![[
            r##"OK (((default ((((class color) (min-colors 256)) :background "#F7F7F7" :foreground "#000000"))) (font-lock-string-face ((((class color) (min-colors 256)) :foreground "#448C27"))) (font-lock-function-name-face ((((class color) (min-colors 256)) :foreground "#325CC0"))) (font-lock-comment-face ((((class color) (min-colors 256)) :foreground "#AA3731")))) ((default ((((class color) (min-colors 256)) :background "#F7F7F7" :foreground "#000000"))) (font-lock-string-face ((((class color) (min-colors 256)) :foreground "#00aa00"))) (font-lock-function-name-face ((((class color) (min-colors 256)) :foreground "#1234ff"))) (font-lock-comment-face ((((class color) (min-colors 256)) :foreground "#ee1111")))) ((default ((((class color) (min-colors 256)) :background "#F7F7F7" :foreground "#000000"))) (font-lock-string-face ((((class color) (min-colors 256)) :foreground "#448C27"))) (font-lock-function-name-face ((((class color) (min-colors 256)) :foreground "#325CC0"))) (font-lock-comment-face ((((class color) (min-colors 256)) :foreground "#AA3731")))) t)"##
        ]],
    )
}

fn invalid_theme_failure_happens_after_existing_themes_are_deliberately_disabled() -> ParityBatchCase
{
    ParityBatchCase::value(
        "invalid_theme_failure_happens_after_existing_themes_are_deliberately_disabled",
        r##"
(progn
  (mapc #'disable-theme custom-enabled-themes)
  (load-theme 'alabaster-themes-light t)
  (let ((before
         (copy-sequence custom-enabled-themes))
        outcome)
    (condition-case error-data
        (setq outcome
              (alabaster-themes-load-theme
               'alabaster-themes-does-not-exist))
      (error
       (setq outcome
             (list
              (car error-data)
              (cadr error-data)))))
    (list
     before outcome
     custom-enabled-themes
     (custom-theme-enabled-p
      'alabaster-themes-light))))
"##,
        expect![[
            r#"OK ((alabaster-themes-light) (error "Unable to find theme file for ‘alabaster-themes-does-not-exist’") nil nil)"#
        ]],
    )
}

fn direct_select_ignores_variant_and_delegates_to_full_load_hook_workflow() -> ParityBatchCase {
    ParityBatchCase::value(
        "direct_select_ignores_variant_and_delegates_to_full_load_hook_workflow",
        r##"
(progn
  (mapc #'disable-theme custom-enabled-themes)
  (defvar alabaster-themes-test-events nil)
  (setq alabaster-themes-test-events nil)
  (let ((alabaster-themes-post-load-hook
         (list
          (lambda ()
            (push
             (copy-sequence custom-enabled-themes)
             alabaster-themes-test-events)))))
    (unwind-protect
        (list
         (alabaster-themes-select
          'alabaster-themes-dark-mono
          'light)
         custom-enabled-themes
         (nreverse alabaster-themes-test-events))
      (mapc #'disable-theme custom-enabled-themes)
      (makunbound 'alabaster-themes-test-events))))
"##,
        expect![
            "OK (alabaster-themes-dark-mono (alabaster-themes-dark-mono) ((alabaster-themes-dark-mono)))"
        ],
    )
}

fn selection_prompt_exercises_all_light_dark_and_interactive_subset_paths() -> ParityBatchCase {
    ParityBatchCase::value(
        "selection_prompt_exercises_all_light_dark_and_interactive_subset_paths",
        r##"
(let (events)
  (cl-letf
      (((symbol-function 'completing-read)
        (lambda (prompt collection &optional predicate
                        require-match initial-input
                        history default inherit-input-method)
          (let ((candidates
                 (all-completions
                  "" collection predicate)))
            (push
             (list prompt candidates require-match
                   initial-input history default
                   inherit-input-method)
             events)
            (car candidates))))
       ((symbol-function 'read-multiple-choice)
        (lambda (&rest arguments)
          (push
           (cons 'choice arguments)
           events)
          '(?d "dark" "Load a dark theme"))))
    (list
     (alabaster-themes--select-prompt "All: ")
     (alabaster-themes--select-prompt "Light: " 'light)
     (alabaster-themes--select-prompt "Dark: " 'dark)
     (alabaster-themes--select-prompt "Choose: " t)
     (nreverse events))))
"##,
        expect![[
            r#"OK (alabaster-themes-light alabaster-themes-light alabaster-themes-dark alabaster-themes-dark (("All: " ("alabaster-themes-light" "alabaster-themes-light-bg" "alabaster-themes-light-mono" "alabaster-themes-dark" "alabaster-themes-dark-mono") t nil alabaster-themes--select-theme-history nil nil) ("Light: " ("alabaster-themes-light" "alabaster-themes-light-bg" "alabaster-themes-light-mono") t nil alabaster-themes--select-theme-history nil nil) ("Dark: " ("alabaster-themes-dark" "alabaster-themes-dark-mono") t nil alabaster-themes--select-theme-history nil nil) (choice "Variant" ((100 "dark" "Load a dark theme") (108 "light" "Load a light theme")) "Limit to the dark or light subset of the Alabaster themes collection.") ("Choose: " ("alabaster-themes-dark" "alabaster-themes-dark-mono") t nil alabaster-themes--select-theme-history nil nil)))"#
        ]],
    )
}

fn real_palette_preview_buffer_initializes_tabulated_mode_and_reuses_named_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "real_palette_preview_buffer_initializes_tabulated_mode_and_reuses_named_buffer",
        r##"
(progn
  (load-theme 'alabaster-themes-light t t)
  (let (shown)
    (cl-letf
        (((symbol-function 'pop-to-buffer)
          (lambda (buffer-or-name &rest _)
            (setq shown
                  (get-buffer buffer-or-name))
            shown)))
      (unwind-protect
          (let* ((first
                  (alabaster-themes-list-colors
                   'alabaster-themes-light))
                 (first-buffer shown)
                 (second
                  (alabaster-themes-list-colors
                   'alabaster-themes-light))
                 (second-buffer shown))
            (list
             first second
             (eq first-buffer second-buffer)
             (buffer-name first-buffer)
             (with-current-buffer first-buffer
               (list
                major-mode
                mode-name
                (append tabulated-list-format nil)
                (length tabulated-list-entries)
                (buffer-size)
                (buffer-substring-no-properties
                 (point-min)
                 (min (point-max) 120))))))
        (when (buffer-live-p shown)
          (kill-buffer shown))))))
"##,
        expect![[
            r#"OK ((:buffer nil) (:buffer nil) t "*alabaster-themes-light-list-all*" (alabaster-themes-preview-mode "Alabaster palette" (("Mapping?" 10 t) ("Symbol name" 30 t) ("As foreground" 30 t) ("As background" 0 t)) 108 11232 "           bg-main                        #F7F7F7                        #F7F7F7                       \n           fg-m"))"#
        ]],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        load_helper_disables_competing_theme_runs_hook_and_enables_exact_target(),
        switching_through_all_variants_updates_enabled_theme_and_resolved_core_palette(),
        repeated_load_is_idempotent_for_registry_enabled_state_and_face_specs(),
        enable_disable_reenable_cycle_restores_and_reapplies_theme_without_rereading_file(),
        reloading_after_palette_override_changes_faces_and_reset_restores_original_specs(),
        invalid_theme_failure_happens_after_existing_themes_are_deliberately_disabled(),
        direct_select_ignores_variant_and_delegates_to_full_load_hook_workflow(),
        selection_prompt_exercises_all_light_dark_and_interactive_subset_paths(),
        real_palette_preview_buffer_initializes_tabulated_mode_and_reuses_named_buffer(),
    ]
}
