use expect_test::expect;

use super::ParityBatchCase;

fn real_batch_display_rejects_the_color_gate_without_faking_a_frame() -> ParityBatchCase {
    ParityBatchCase::value(
        "real_batch_display_rejects_the_color_gate_without_faking_a_frame",
        r##"(neomacs-leuven-test-isolated
  (let ((baseline
         (list
          (neomacs-leuven-test-face
           'default '(:foreground :background))
          (neomacs-leuven-test-face
           'font-lock-keyword-face '(:foreground :weight))))
        (light-loaded nil)
        (dark-loaded nil)
        (light-state nil)
        (dark-state nil)
        (after-light nil)
        (after-dark nil))
    (setq light-loaded (load-theme 'leuven t)
          light-state
          (list
           :enabled (copy-sequence custom-enabled-themes)
           :faces
           (list
            (neomacs-leuven-test-face
             'default '(:foreground :background))
            (neomacs-leuven-test-face
             'font-lock-keyword-face '(:foreground :weight)))))
    (disable-theme 'leuven)
    (setq after-light
          (list
           (neomacs-leuven-test-face
            'default '(:foreground :background))
           (neomacs-leuven-test-face
            'font-lock-keyword-face '(:foreground :weight)))
          dark-loaded (load-theme 'leuven-dark t)
          dark-state
          (list
           :enabled (copy-sequence custom-enabled-themes)
           :faces
           (list
            (neomacs-leuven-test-face
             'default '(:foreground :background))
            (neomacs-leuven-test-face
             'font-lock-keyword-face '(:foreground :weight)))))
    (disable-theme 'leuven-dark)
    (setq after-dark
          (list
           (neomacs-leuven-test-face
            'default '(:foreground :background))
           (neomacs-leuven-test-face
            'font-lock-keyword-face '(:foreground :weight))))
      (list
       :display
       (list :graphic (display-graphic-p)
             :color-cells (display-color-cells)
             :visual-class (display-visual-class)
             :gate
             (face-spec-set-match-display
              '((class color) (min-colors 89)) nil))
       :loaded (list light-loaded dark-loaded)
       :known
       (list (and (custom-theme-p 'leuven) t)
             (and (custom-theme-p 'leuven-dark) t))
       :source-directory
       (let ((file
              (locate-file
               "leuven-theme.el"
               (cl-remove-if-not #'stringp custom-theme-load-path))))
         (and file
              (file-name-nondirectory
               (directory-file-name (file-name-directory file)))))
       :baseline baseline
       :light light-state
       :dark dark-state
       :light-restored (equal baseline after-light)
       :dark-restored (equal baseline after-dark))))"##,
        expect![[
            r#"OK (:display (:graphic nil :color-cells 0 :visual-class static-gray :gate nil) :loaded (t t) :known (t t) :source-directory "leuven-theme-20260213.1052" :baseline ((default :direct ((:foreground . "unspecified-fg") (:background . "unspecified-bg")) :resolved ((:foreground . "unspecified-fg") (:background . "unspecified-bg"))) (font-lock-keyword-face :direct ((:foreground . unspecified) (:weight . bold)) :resolved ((:foreground . "unspecified-fg") (:weight . bold)))) :light (:enabled (leuven) :faces ((default :direct ((:foreground . "unspecified-fg") (:background . "unspecified-bg")) :resolved ((:foreground . "unspecified-fg") (:background . "unspecified-bg"))) (font-lock-keyword-face :direct ((:foreground . unspecified) (:weight . bold)) :resolved ((:foreground . "unspecified-fg") (:weight . bold))))) :dark (:enabled (leuven-dark) :faces ((default :direct ((:foreground . "unspecified-fg") (:background . "unspecified-bg")) :resolved ((:foreground . "unspecified-fg") (:background . "unspecified-bg"))) (font-lock-keyword-face :direct ((:foreground . unspecified) (:weight . bold)) :resolved ((:foreground . "unspecified-fg") (:weight . bold))))) :light-restored t :dark-restored t)"#
        ]],
    )
}

fn every_unconditional_face_applies_for_both_variants_and_restores() -> ParityBatchCase {
    ParityBatchCase::value(
        "every_unconditional_face_applies_for_both_variants_and_restores",
        r##"(neomacs-leuven-test-isolated
  (let* ((baseline-habits
          (neomacs-leuven-test-face-list
           neomacs-leuven-test-unconditional-faces))
         (baseline-tab-line
          (neomacs-leuven-test-face
           'tab-line '(:foreground :background))))
    (load-theme 'leuven t)
    (let ((light-habits
           (neomacs-leuven-test-face-list
            neomacs-leuven-test-unconditional-faces))
          (light-tab-line
           (neomacs-leuven-test-face
            'tab-line '(:foreground :background))))
      (disable-theme 'leuven)
      (let ((after-light
             (list
              (neomacs-leuven-test-face-list
               neomacs-leuven-test-unconditional-faces)
              (neomacs-leuven-test-face
               'tab-line '(:foreground :background)))))
        (load-theme 'leuven-dark t)
        (let ((dark-habits
               (neomacs-leuven-test-face-list
                neomacs-leuven-test-unconditional-faces))
              (dark-tab-line
               (neomacs-leuven-test-face
                'tab-line '(:foreground :background))))
          (disable-theme 'leuven-dark)
          (let ((after-dark
                 (list
                  (neomacs-leuven-test-face-list
                   neomacs-leuven-test-unconditional-faces)
                  (neomacs-leuven-test-face
                   'tab-line '(:foreground :background)))))
            (list
             :baseline-habits baseline-habits
             :baseline-tab-line baseline-tab-line
             :light-habits light-habits
             :light-tab-line light-tab-line
             :after-light after-light
             :dark-habits dark-habits
             :dark-tab-line dark-tab-line
             :after-dark after-dark
             :light-restored
             (equal after-light
                    (list baseline-habits baseline-tab-line))
             :dark-restored
             (equal after-dark
                    (list baseline-habits baseline-tab-line)))))))))"##,
        expect![[
            r##"OK (:baseline-habits ((org-habit-clear-face :direct ((:foreground . unspecified) (:background . "blue")) :resolved ((:foreground . "unspecified-fg") (:background . "blue"))) (org-habit-clear-future-face :direct ((:foreground . unspecified) (:background . "midnightblue")) :resolved ((:foreground . "unspecified-fg") (:background . "midnightblue"))) (org-habit-ready-face :direct ((:foreground . unspecified) (:background . "forestgreen")) :resolved ((:foreground . "unspecified-fg") (:background . "forestgreen"))) (org-habit-ready-future-face :direct ((:foreground . unspecified) (:background . "darkgreen")) :resolved ((:foreground . "unspecified-fg") (:background . "darkgreen"))) (org-habit-alert-face :direct ((:foreground . unspecified) (:background . "gold")) :resolved ((:foreground . "unspecified-fg") (:background . "gold"))) (org-habit-alert-future-face :direct ((:foreground . unspecified) (:background . "darkgoldenrod")) :resolved ((:foreground . "unspecified-fg") (:background . "darkgoldenrod"))) (org-habit-overdue-face :direct ((:foreground . unspecified) (:background . "firebrick")) :resolved ((:foreground . "unspecified-fg") (:background . "firebrick"))) (org-habit-overdue-future-face :direct ((:foreground . unspecified) (:background . "darkred")) :resolved ((:foreground . "unspecified-fg") (:background . "darkred")))) :baseline-tab-line (tab-line :direct ((:foreground . unspecified) (:background . "grey")) :resolved ((:foreground . "unspecified-fg") (:background . "grey"))) :light-habits ((org-habit-clear-face :direct ((:foreground . unspecified) (:background . "#5C888B")) :resolved ((:foreground . "unspecified-fg") (:background . "#5C888B"))) (org-habit-clear-future-face :direct ((:foreground . unspecified) (:background . "#4C7073")) :resolved ((:foreground . "unspecified-fg") (:background . "#4C7073"))) (org-habit-ready-face :direct ((:foreground . unspecified) (:background . "#7F9F7F")) :resolved ((:foreground . "unspecified-fg") (:background . "#7F9F7F"))) (org-habit-ready-future-face :direct ((:foreground . unspecified) (:background . "#5F7F5F")) :resolved ((:foreground . "unspecified-fg") (:background . "#5F7F5F"))) (org-habit-alert-face :direct ((:foreground . "#3F3F3F") (:background . "#E0CF9F")) :resolved ((:foreground . "#3F3F3F") (:background . "#E0CF9F"))) (org-habit-alert-future-face :direct ((:foreground . "#3F3F3F") (:background . "#D0BF8F")) :resolved ((:foreground . "#3F3F3F") (:background . "#D0BF8F"))) (org-habit-overdue-face :direct ((:foreground . unspecified) (:background . "#9C6363")) :resolved ((:foreground . "unspecified-fg") (:background . "#9C6363"))) (org-habit-overdue-future-face :direct ((:foreground . unspecified) (:background . "#8C5353")) :resolved ((:foreground . "unspecified-fg") (:background . "#8C5353")))) :light-tab-line (tab-line :direct ((:foreground . "#5D6B99") (:background . "#5D6B99")) :resolved ((:foreground . "#5D6B99") (:background . "#5D6B99"))) :after-light (((org-habit-clear-face :direct ((:foreground . unspecified) (:background . "blue")) :resolved ((:foreground . "unspecified-fg") (:background . "blue"))) (org-habit-clear-future-face :direct ((:foreground . unspecified) (:background . "midnightblue")) :resolved ((:foreground . "unspecified-fg") (:background . "midnightblue"))) (org-habit-ready-face :direct ((:foreground . unspecified) (:background . "forestgreen")) :resolved ((:foreground . "unspecified-fg") (:background . "forestgreen"))) (org-habit-ready-future-face :direct ((:foreground . unspecified) (:background . "darkgreen")) :resolved ((:foreground . "unspecified-fg") (:background . "darkgreen"))) (org-habit-alert-face :direct ((:foreground . unspecified) (:background . "gold")) :resolved ((:foreground . "unspecified-fg") (:background . "gold"))) (org-habit-alert-future-face :direct ((:foreground . unspecified) (:background . "darkgoldenrod")) :resolved ((:foreground . "unspecified-fg") (:background . "darkgoldenrod"))) (org-habit-overdue-face :direct ((:foreground . unspecified) (:background . "firebrick")) :resolved ((:foreground . "unspecified-fg") (:background . "firebrick"))) (org-habit-overdue-future-face :direct ((:foreground . unspecified) (:background . "darkred")) :resolved ((:foreground . "unspecified-fg") (:background . "darkred")))) (tab-line :direct ((:foreground . unspecified) (:background . "grey")) :resolved ((:foreground . "unspecified-fg") (:background . "grey")))) :dark-habits ((org-habit-clear-face :direct ((:foreground . unspecified) (:background . "#5C888B")) :resolved ((:foreground . "unspecified-fg") (:background . "#5C888B"))) (org-habit-clear-future-face :direct ((:foreground . unspecified) (:background . "#4C7073")) :resolved ((:foreground . "unspecified-fg") (:background . "#4C7073"))) (org-habit-ready-face :direct ((:foreground . unspecified) (:background . "#7F9F7F")) :resolved ((:foreground . "unspecified-fg") (:background . "#7F9F7F"))) (org-habit-ready-future-face :direct ((:foreground . unspecified) (:background . "#5F7F5F")) :resolved ((:foreground . "unspecified-fg") (:background . "#5F7F5F"))) (org-habit-alert-face :direct ((:foreground . "#3F3F3F") (:background . "#E0CF9F")) :resolved ((:foreground . "#3F3F3F") (:background . "#E0CF9F"))) (org-habit-alert-future-face :direct ((:foreground . "#3F3F3F") (:background . "#D0BF8F")) :resolved ((:foreground . "#3F3F3F") (:background . "#D0BF8F"))) (org-habit-overdue-face :direct ((:foreground . unspecified) (:background . "#9C6363")) :resolved ((:foreground . "unspecified-fg") (:background . "#9C6363"))) (org-habit-overdue-future-face :direct ((:foreground . unspecified) (:background . "#8C5353")) :resolved ((:foreground . "unspecified-fg") (:background . "#8C5353")))) :dark-tab-line (tab-line :direct ((:foreground . unspecified) (:background . "grey")) :resolved ((:foreground . "unspecified-fg") (:background . "grey"))) :after-dark (((org-habit-clear-face :direct ((:foreground . unspecified) (:background . "blue")) :resolved ((:foreground . "unspecified-fg") (:background . "blue"))) (org-habit-clear-future-face :direct ((:foreground . unspecified) (:background . "midnightblue")) :resolved ((:foreground . "unspecified-fg") (:background . "midnightblue"))) (org-habit-ready-face :direct ((:foreground . unspecified) (:background . "forestgreen")) :resolved ((:foreground . "unspecified-fg") (:background . "forestgreen"))) (org-habit-ready-future-face :direct ((:foreground . unspecified) (:background . "darkgreen")) :resolved ((:foreground . "unspecified-fg") (:background . "darkgreen"))) (org-habit-alert-face :direct ((:foreground . unspecified) (:background . "gold")) :resolved ((:foreground . "unspecified-fg") (:background . "gold"))) (org-habit-alert-future-face :direct ((:foreground . unspecified) (:background . "darkgoldenrod")) :resolved ((:foreground . "unspecified-fg") (:background . "darkgoldenrod"))) (org-habit-overdue-face :direct ((:foreground . unspecified) (:background . "firebrick")) :resolved ((:foreground . "unspecified-fg") (:background . "firebrick"))) (org-habit-overdue-future-face :direct ((:foreground . unspecified) (:background . "darkred")) :resolved ((:foreground . "unspecified-fg") (:background . "darkred")))) (tab-line :direct ((:foreground . unspecified) (:background . "grey")) :resolved ((:foreground . "unspecified-fg") (:background . "grey")))) :light-restored t :dark-restored t)"##
        ]],
    )
}

fn light_and_dark_stack_through_public_lifecycle_and_variables_restore() -> ParityBatchCase {
    ParityBatchCase::value(
        "light_and_dark_stack_through_public_lifecycle_and_variables_restore",
        r##"(neomacs-leuven-test-isolated
  (let ((events nil)
        (baseline-faces (neomacs-leuven-test-copy ansi-color-faces-vector))
        (baseline-names (neomacs-leuven-test-copy ansi-color-names-vector))
        (baseline-hl-sexp
         (neomacs-leuven-test-variable-state 'hl-sexp-background-color))
        (light-hl-sexp nil))
    (let ((enable-theme-functions
           (list (lambda (theme) (push (list 'enabled theme) events))))
          (disable-theme-functions
           (list (lambda (theme) (push (list 'disabled theme) events)))))
      (load-theme 'leuven t)
      (setq light-hl-sexp
            (neomacs-leuven-test-variable-state
             'hl-sexp-background-color))
      (load-theme 'leuven-dark t)
      (let ((stacked
             (list
              (copy-sequence custom-enabled-themes)
              (neomacs-leuven-test-variable-state
               'hl-sexp-background-color)
              (neomacs-leuven-test-copy ansi-color-faces-vector)
              (neomacs-leuven-test-copy ansi-color-names-vector))))
        (disable-theme 'leuven-dark)
        (let ((light-restored
               (list
                (copy-sequence custom-enabled-themes)
                (neomacs-leuven-test-variable-state
                 'hl-sexp-background-color)
                (equal ansi-color-faces-vector baseline-faces)
                (equal ansi-color-names-vector baseline-names))))
          (enable-theme 'leuven-dark)
          (disable-theme 'leuven)
          (let ((dark-only
                 (list
                  (copy-sequence custom-enabled-themes)
                  (neomacs-leuven-test-variable-state
                   'hl-sexp-background-color)
                  (neomacs-leuven-test-copy ansi-color-faces-vector)
                  (neomacs-leuven-test-copy ansi-color-names-vector))))
            (disable-theme 'leuven-dark)
            (list :baseline-hl-sexp baseline-hl-sexp
                  :light-hl-sexp light-hl-sexp
                  :stacked stacked
                  :light-restored light-restored
                  :dark-only dark-only
                  :baseline-restored
                  (list
                        (equal
                         (neomacs-leuven-test-variable-state
                          'hl-sexp-background-color)
                         baseline-hl-sexp)
                        (equal ansi-color-faces-vector baseline-faces)
                        (equal ansi-color-names-vector baseline-names))
                  :events (nreverse events))))))))"##,
        expect![[
            r##"OK (:baseline-hl-sexp (:bound t :value "neomacs-leuven-baseline") :light-hl-sexp (:bound t :value "#efebe9") :stacked ((leuven-dark leuven) (:bound t :value "#33323e") [default default default italic underline success warning error] ["#ffffff" "#37ffff" "#e074e3" "#3732ff" "#ffff0b" "#37ff3c" "#ff400b" "#848088"]) :light-restored ((leuven) (:bound t :value "#efebe9") t t) :dark-only ((leuven-dark) (:bound t :value "#33323e") [default default default italic underline success warning error] ["#ffffff" "#37ffff" "#e074e3" "#3732ff" "#ffff0b" "#37ff3c" "#ff400b" "#848088"]) :baseline-restored (t t t) :events ((enabled user) (enabled leuven) (enabled user) (enabled leuven-dark) (disabled leuven-dark) (enabled user) (enabled leuven-dark) (disabled leuven) (disabled leuven-dark)))"##
        ]],
    )
}

fn missing_and_invalid_theme_requests_fail_without_mutating_the_editor() -> ParityBatchCase {
    ParityBatchCase::value(
        "missing_and_invalid_theme_requests_fail_without_mutating_the_editor",
        r##"(neomacs-leuven-test-isolated
  (let ((before-enabled (copy-sequence custom-enabled-themes))
        (before-known (copy-sequence custom-known-themes)))
    (list
     (condition-case error-data
         (list :unexpected (load-theme 'leuven-parity-missing t))
       (error (list :signal (car error-data) (cdr error-data))))
     (condition-case error-data
         (list :unexpected (load-theme "leuven" t))
       (error (list :signal (car error-data) (cdr error-data))))
     (condition-case error-data
         (list :unexpected (enable-theme 'leuven-parity-undefined))
       (error (list :signal (car error-data) (cdr error-data))))
     :enabled-unchanged (equal before-enabled custom-enabled-themes)
     :known-unchanged (equal before-known custom-known-themes)
     :leuven-enabled (and (custom-theme-enabled-p 'leuven) t)
     :dark-enabled (and (custom-theme-enabled-p 'leuven-dark) t))))"##,
        expect![[
            r#"OK ((:signal error ("Unable to find theme file for ‘leuven-parity-missing’")) (:signal wrong-type-argument (symbolp "leuven")) (:signal error ("Undefined Custom theme leuven-parity-undefined")) :enabled-unchanged t :known-unchanged t :leuven-enabled nil :dark-enabled nil)"#
        ]],
    )
}

pub(super) fn public_workflow_cases() -> Vec<ParityBatchCase> {
    vec![
        real_batch_display_rejects_the_color_gate_without_faking_a_frame(),
        every_unconditional_face_applies_for_both_variants_and_restores(),
        light_and_dark_stack_through_public_lifecycle_and_variables_restore(),
        missing_and_invalid_theme_requests_fail_without_mutating_the_editor(),
    ]
}
