use expect_test::expect;

use super::ParityBatchCase;

fn native_batch_registers_every_variant_without_faking_color() -> ParityBatchCase {
    ParityBatchCase::value(
        "native_batch_registers_every_variant_without_faking_color",
        r##"(gruvbox-test-run
 (lambda ()
   (let ((baseline
          (gruvbox-test-representative-face-state)))
     (list
      :feature (featurep 'gruvbox)
      :org-compiled gruvbox-test-org-compiled
      :source-load-suffixes load-suffixes
      :source-directory
      (file-name-nondirectory
       (directory-file-name
        (file-name-directory (locate-library "gruvbox"))))
      :display
      (list :graphic (display-graphic-p)
            :cells (display-color-cells)
            :visual-class (display-visual-class)
            :display-type (frame-parameter nil 'display-type)
            :truecolor
            (face-spec-set-match-display
             '((class color) (min-colors 16777215)) nil)
            :color256
            (face-spec-set-match-display
             '((class color) (min-colors 255)) nil))
      :themes-known (and (cl-every #'custom-theme-p gruvbox-test-themes) t)
      :lifecycle
      (mapcar
       (lambda (theme)
         (let ((loaded (load-theme theme t)) enabled faces-unchanged restored)
           (setq enabled
                 (list (copy-sequence custom-enabled-themes)
                       (and (custom-theme-enabled-p theme) t))
                 faces-unchanged
                 (equal baseline
                        (gruvbox-test-representative-face-state)))
           (disable-theme theme)
           (setq restored
                 (equal baseline
                        (gruvbox-test-representative-face-state)))
           (list theme :loaded loaded :enabled enabled
                 :faces-unchanged faces-unchanged :restored restored)))
       '(gruvbox-dark-hard gruvbox-light-soft))))))"##,
        expect![[
            r#"OK (:feature t :org-compiled t :source-load-suffixes (".el") :source-directory "gruvbox-theme-20250117.222" :display (:graphic nil :cells 0 :visual-class static-gray :display-type mono :truecolor nil :color256 nil) :themes-known t :lifecycle ((gruvbox-dark-hard :loaded t :enabled ((gruvbox-dark-hard) t) :faces-unchanged t :restored t) (gruvbox-light-soft :loaded t :enabled ((gruvbox-light-soft) t) :faces-unchanged t :restored t)))"#
        ]],
    )
}

fn dark_and_light_stack_restore_faces_and_theme_variables() -> ParityBatchCase {
    ParityBatchCase::value(
        "dark_and_light_stack_restore_faces_and_theme_variables",
        r##"(gruvbox-test-run
 (lambda ()
   (let* ((baseline
           (list
            :enabled (copy-sequence custom-enabled-themes)
            :ansi (gruvbox-test-variable-state 'ansi-color-names-vector)
            :pdf (gruvbox-test-variable-state 'pdf-view-midnight-colors)
            :faces (gruvbox-test-representative-face-state)
            :mode (frame-parameter nil 'background-mode)))
          dark stacked dark-restored light-only restored second-disable)
     (load-theme 'gruvbox-dark-medium t)
     (setq dark
           (list
            :enabled (copy-sequence custom-enabled-themes)
            :ansi (gruvbox-test-variable-state 'ansi-color-names-vector)
            :pdf (gruvbox-test-variable-state 'pdf-view-midnight-colors)
            :faces-unchanged
            (equal (gruvbox-test-representative-face-state)
                   (plist-get baseline :faces))))
     (load-theme 'gruvbox-light-medium t)
     (setq stacked
           (list
            :enabled (copy-sequence custom-enabled-themes)
            :ansi (gruvbox-test-variable-state 'ansi-color-names-vector)
            :pdf (gruvbox-test-variable-state 'pdf-view-midnight-colors)
            :faces-unchanged
            (equal (gruvbox-test-representative-face-state)
                   (plist-get baseline :faces))))
     (disable-theme 'gruvbox-light-medium)
     (setq dark-restored
           (list
            :enabled (copy-sequence custom-enabled-themes)
            :ansi (gruvbox-test-variable-state 'ansi-color-names-vector)
            :pdf (gruvbox-test-variable-state 'pdf-view-midnight-colors)
            :faces-unchanged
            (equal (gruvbox-test-representative-face-state)
                   (plist-get baseline :faces))))
     (enable-theme 'gruvbox-light-medium)
     (disable-theme 'gruvbox-dark-medium)
     (setq light-only
           (list
            :enabled (copy-sequence custom-enabled-themes)
            :ansi (gruvbox-test-variable-state 'ansi-color-names-vector)
            :pdf (gruvbox-test-variable-state 'pdf-view-midnight-colors)
            :faces-unchanged
            (equal (gruvbox-test-representative-face-state)
                   (plist-get baseline :faces))))
     (disable-theme 'gruvbox-light-medium)
     (setq restored
           (list
            :enabled (copy-sequence custom-enabled-themes)
            :ansi (gruvbox-test-variable-state 'ansi-color-names-vector)
            :pdf (gruvbox-test-variable-state 'pdf-view-midnight-colors)
            :faces-restored
            (equal (gruvbox-test-representative-face-state)
                   (plist-get baseline :faces))))
     (disable-theme 'gruvbox-light-medium)
     (setq second-disable
           (list
            :enabled (copy-sequence custom-enabled-themes)
            :ansi (gruvbox-test-variable-state 'ansi-color-names-vector)
            :pdf (gruvbox-test-variable-state 'pdf-view-midnight-colors)
            :faces-restored
            (equal (gruvbox-test-representative-face-state)
                   (plist-get baseline :faces))))
     (list
      :baseline baseline
      :dark dark
      :stacked stacked
      :dark-restored dark-restored
      :dark-restoration (equal dark dark-restored)
      :light-only light-only
      :light-precedence
      (equal (cddr stacked) (cddr light-only))
      :restored restored
      :baseline-restoration
      (and (equal (plist-get baseline :enabled)
                  (plist-get restored :enabled))
           (equal (plist-get baseline :ansi)
                  (plist-get restored :ansi))
           (equal (plist-get baseline :pdf)
                  (plist-get restored :pdf))
           (plist-get restored :faces-restored))
      :second-disable second-disable
      :second-disable-noop (equal restored second-disable)))))"##,
        expect![[
            r##"OK (:baseline (:enabled nil :ansi (:bound t :value ["black" "red3" "green3" "yellow3" "blue2" "magenta3" "cyan3" "gray90"]) :pdf (:bound t :value ("gruvbox-test-light" . "gruvbox-test-dark")) :faces ((default :direct ((:foreground . "unspecified-fg") (:background . "unspecified-bg")) :resolved ((:foreground . "unspecified-fg") (:background . "unspecified-bg"))) (cursor :direct ((:background . "white")) :resolved ((:background . "white"))) (region :direct ((:foreground . unspecified) (:background . unspecified)) :resolved ((:foreground . "unspecified-fg") (:background . "unspecified-bg"))) (mode-line :direct ((:foreground . unspecified) (:background . unspecified) (:box . unspecified)) :resolved ((:foreground . "unspecified-fg") (:background . "unspecified-bg") (:box))) (font-lock-keyword-face :direct ((:foreground . unspecified) (:weight . bold)) :resolved ((:foreground . "unspecified-fg") (:weight . bold))) (font-lock-string-face :direct ((:foreground . unspecified)) :resolved ((:foreground . "unspecified-fg"))) (org-document-title :direct ((:foreground . unspecified) (:background . unspecified) (:weight . bold)) :resolved ((:foreground . "unspecified-fg") (:background . "unspecified-bg") (:weight . bold))) (org-link :direct ((:foreground . unspecified) (:underline . unspecified)) :resolved ((:foreground . "unspecified-fg") (:underline . t))) (diff-added :direct ((:foreground . unspecified) (:background . unspecified)) :resolved ((:foreground . "unspecified-fg") (:background . "unspecified-bg"))) (diff-removed :direct ((:foreground . unspecified) (:background . unspecified)) :resolved ((:foreground . "unspecified-fg") (:background . "unspecified-bg")))) :mode dark) :dark (:enabled (gruvbox-dark-medium) :ansi (:bound t :value ["#3c3836" "#fb4933" "#b8bb26" "#fabd2f" "#83a598" "#d3869b" "#8ec07c" "#ebdbb2"]) :pdf (:bound t :value ("#fdf4c1" . "#282828")) :faces-unchanged t) :stacked (:enabled (gruvbox-light-medium gruvbox-dark-medium) :ansi (:bound t :value ["#ebdbb2" "#cc241d" "#98971a" "#d79921" "#458588" "#b16286" "#689d6a" "#3c3836"]) :pdf (:bound t :value ("#282828" . "#fbf1c7")) :faces-unchanged t) :dark-restored (:enabled (gruvbox-dark-medium) :ansi (:bound t :value ["#3c3836" "#fb4933" "#b8bb26" "#fabd2f" "#83a598" "#d3869b" "#8ec07c" "#ebdbb2"]) :pdf (:bound t :value ("#fdf4c1" . "#282828")) :faces-unchanged t) :dark-restoration t :light-only (:enabled (gruvbox-light-medium) :ansi (:bound t :value ["#ebdbb2" "#cc241d" "#98971a" "#d79921" "#458588" "#b16286" "#689d6a" "#3c3836"]) :pdf (:bound t :value ("#282828" . "#fbf1c7")) :faces-unchanged t) :light-precedence t :restored (:enabled nil :ansi (:bound t :value ["black" "red3" "green3" "yellow3" "blue2" "magenta3" "cyan3" "gray90"]) :pdf (:bound t :value ("gruvbox-test-light" . "gruvbox-test-dark")) :faces-restored t) :baseline-restoration t :second-disable (:enabled nil :ansi (:bound t :value ["black" "red3" "green3" "yellow3" "blue2" "magenta3" "cyan3" "gray90"]) :pdf (:bound t :value ("gruvbox-test-light" . "gruvbox-test-dark")) :faces-restored t) :second-disable-noop t)"##
        ]],
    )
}

fn missing_theme_failures_preserve_state_and_real_variant_recovers() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_theme_failures_preserve_state_and_real_variant_recovers",
        r##"(gruvbox-test-run
 (lambda ()
   (let* ((before
           (list
            :known (copy-sequence custom-known-themes)
            :enabled (copy-sequence custom-enabled-themes)
            :faces (gruvbox-test-representative-face-state)
            :ansi (gruvbox-test-variable-state 'ansi-color-names-vector)
            :pdf (gruvbox-test-variable-state 'pdf-view-midnight-colors)
            :mode (frame-parameter nil 'background-mode)
            :buffers (buffer-list)
            :processes (process-list)
            :timers (copy-sequence timer-list)))
          (undefined
           (condition-case condition
               (list :unexpected (enable-theme 'gruvbox-absent))
             (error
              (list :signal (car condition)
                    :data (cdr condition)
                    :message (error-message-string condition)))))
          (missing
           (condition-case condition
               (list :unexpected (load-theme 'gruvbox-dark-ultra t))
             (error
              (list :signal (car condition)
                    :data (cdr condition)
                    :message (error-message-string condition)))))
          (after
           (list
            :known (copy-sequence custom-known-themes)
            :enabled (copy-sequence custom-enabled-themes)
            :faces (gruvbox-test-representative-face-state)
            :ansi (gruvbox-test-variable-state 'ansi-color-names-vector)
            :pdf (gruvbox-test-variable-state 'pdf-view-midnight-colors)
            :mode (frame-parameter nil 'background-mode)
            :buffers (buffer-list)
            :processes (process-list)
            :timers (copy-sequence timer-list)))
          (recovery (load-theme 'gruvbox-dark-soft t))
          (recovery-state
           (list :enabled (copy-sequence custom-enabled-themes)
                 :theme (and (custom-theme-enabled-p
                              'gruvbox-dark-soft)
                             t))))
     (disable-theme 'gruvbox-dark-soft)
     (list :undefined undefined
           :missing missing
           :unchanged (equal before after)
           :recovery recovery
           :recovery-state recovery-state
           :post-recovery-enabled (copy-sequence custom-enabled-themes)))))"##,
        expect![[
            r#"OK (:undefined (:signal error :data ("Undefined Custom theme gruvbox-absent") :message "Undefined Custom theme gruvbox-absent") :missing (:signal error :data ("Unable to find theme file for ‘gruvbox-dark-ultra’") :message "Unable to find theme file for ‘gruvbox-dark-ultra’") :unchanged t :recovery t :recovery-state (:enabled (gruvbox-dark-soft) :theme t) :post-recovery-enabled nil)"#
        ]],
    )
}

pub(super) fn public_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        native_batch_registers_every_variant_without_faking_color(),
        dark_and_light_stack_restore_faces_and_theme_variables(),
        missing_theme_failures_preserve_state_and_real_variant_recovers(),
    ]
}
