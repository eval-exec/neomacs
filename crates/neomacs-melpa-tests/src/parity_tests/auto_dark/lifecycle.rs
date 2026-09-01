use expect_test::expect;

use super::ParityBatchCase;

fn auto_dark_mode_enable_with_configured_method_checks_theme_then_registers_listener()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_mode_enable_with_configured_method_checks_theme_then_registers_listener",
        r##"(let ((auto-dark-mode nil)
                               (auto-dark-detection-method
                                'manual)
                               events)
          (cl-letf
              (((symbol-function
                 'auto-dark--determine-detection-method)
                (lambda ()
                  (push :determine events)
                  'unexpected))
               ((symbol-function
                 'auto-dark--check-and-set-dark-mode)
                (lambda ()
                  (push
                   (list
                    :check
                    auto-dark-mode
                    auto-dark-detection-method)
                   events)
                  :checked))
               ((symbol-function
                 'auto-dark--register-change-listener)
                (lambda ()
                  (push
                   (list
                    :register
                    auto-dark-mode
                    auto-dark-detection-method)
                   events)
                  :registered)))
            (list
             (auto-dark-mode 1)
             auto-dark-mode
             auto-dark-detection-method
             (nreverse events))))"##,
        expect!["OK (t t manual ((:check t manual) (:register t manual)))"],
    )
}

fn auto_dark_mode_enable_without_method_persists_detection_before_check_and_registration()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_mode_enable_without_method_persists_detection_before_check_and_registration",
        r##"(let ((auto-dark-mode nil)
                               (auto-dark-detection-method nil)
                               events)
          (cl-letf
              (((symbol-function
                 'auto-dark--determine-detection-method)
                (lambda ()
                  (push
                   (list
                    :determine
                    auto-dark-detection-method)
                   events)
                  'dbus))
               ((symbol-function
                 'auto-dark--check-and-set-dark-mode)
                (lambda ()
                  (push
                   (list
                    :check
                    auto-dark-detection-method)
                   events)))
               ((symbol-function
                 'auto-dark--register-change-listener)
                (lambda ()
                  (push
                   (list
                    :register
                    auto-dark-detection-method)
                   events))))
            (list
             (auto-dark-mode 1)
             auto-dark-mode
             auto-dark-detection-method
             (nreverse events))))"##,
        expect!["OK (t t dbus ((:determine nil) (:check dbus) (:register dbus)))"],
    )
}

fn auto_dark_mode_disable_only_unregisters_listener_and_preserves_theme_and_detection_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_mode_disable_only_unregisters_listener_and_preserves_theme_and_detection_state",
        r##"(let ((auto-dark-mode t)
                               (auto-dark-detection-method
                                'manual)
                               (auto-dark--last-dark-mode-state
                                'dark)
                               (auto-dark-themes
                                '((wombat)
                                  (leuven)))
                               (custom-enabled-themes
                                '(wombat))
                               events)
          (cl-letf
              (((symbol-function
                 'auto-dark--check-and-set-dark-mode)
                (lambda ()
                  (push :unexpected-check
                        events)))
               ((symbol-function
                 'auto-dark--register-change-listener)
                (lambda ()
                  (push :unexpected-register
                        events)))
               ((symbol-function
                 'auto-dark--unregister-change-listener)
                (lambda ()
                  (push
                   (list
                    :unregister
                    auto-dark-mode)
                   events)
                  :unregistered)))
            (list
             (auto-dark-mode -1)
             auto-dark-mode
             auto-dark-detection-method
             auto-dark--last-dark-mode-state
             auto-dark-themes
             custom-enabled-themes
             (nreverse events))))"##,
        expect!["OK (nil nil manual dark ((wombat) (leuven)) (wombat) ((:unregister nil)))"],
    )
}

fn auto_dark_global_mode_numeric_toggle_hook_and_repeated_enable_semantics_match() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dark_global_mode_numeric_toggle_hook_and_repeated_enable_semantics_match",
        r##"(let* ((auto-dark-mode nil)
                                events
                                (auto-dark-mode-hook
                                 (list
                                  (lambda ()
                                    (push
                                     (list
                                      :mode-hook
                                      auto-dark-mode)
                                     events)))))
          (cl-letf
              (((symbol-function
                 'auto-dark--check-and-set-dark-mode)
                (lambda ()
                  (push :check events)))
               ((symbol-function
                 'auto-dark--register-change-listener)
                (lambda ()
                  (push :register events)))
               ((symbol-function
                 'auto-dark--unregister-change-listener)
                (lambda ()
                  (push :unregister events))))
            (let ((auto-dark-detection-method
                   'manual))
              (list
               (auto-dark-mode 1)
               auto-dark-mode
               (auto-dark-mode 1)
               auto-dark-mode
               (auto-dark-mode 'toggle)
               auto-dark-mode
               (auto-dark-mode nil)
               auto-dark-mode
               (auto-dark-mode -7)
               auto-dark-mode
               (nreverse events)))))"##,
        expect![
            "OK (t t t t nil nil t t nil nil (:check :register (:mode-hook t) :check :register (:mode-hook t) :unregister (:mode-hook nil) :check :register (:mode-hook t) :unregister (:mode-hook nil)))"
        ],
    )
}

fn auto_dark_mode_enable_failure_leaves_mode_variable_on_and_skips_registration() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dark_mode_enable_failure_leaves_mode_variable_on_and_skips_registration",
        r##"(let ((auto-dark-mode nil)
                               (auto-dark-detection-method
                                'manual)
                               events)
          (cl-letf
              (((symbol-function
                 'auto-dark--check-and-set-dark-mode)
                (lambda ()
                  (push :check events)
                  (error
                   "fixture theme failure")))
               ((symbol-function
                 'auto-dark--register-change-listener)
                (lambda ()
                  (push :register events))))
            (list
             (auto-dark-test-error-data
              (lambda ()
                (auto-dark-mode 1)))
             auto-dark-mode
             auto-dark-detection-method
             (nreverse events))))"##,
        expect![[r#"OK ((:error error ("fixture theme failure")) t manual (:check))"#]],
    )
}

fn auto_dark_set_theme_hook_failure_occurs_after_state_frame_and_theme_changes() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dark_set_theme_hook_failure_occurs_after_state_frame_and_theme_changes",
        r##"(let ((auto-dark-themes
                                '((dark-theme)
                                  (light-theme)))
                               (auto-dark--last-dark-mode-state
                                'unknown)
                               events
                               (auto-dark-dark-mode-hook
                                (list
                                 (lambda ()
                                   (push :hook events)
                                   (error
                                    "fixture hook failure")))))
          (cl-letf
              (((symbol-function
                 'auto-dark--update-frame-backgrounds)
                (lambda (appearance)
                  (push
                   (list :frames appearance)
                   events)))
               ((symbol-function
                 'auto-dark--enable-themes)
                (lambda (themes)
                  (push
                   (list :themes themes)
                   events))))
            (list
             (auto-dark-test-error-data
              (lambda ()
                (auto-dark--set-theme
                 'dark)))
             auto-dark--last-dark-mode-state
             (nreverse events))))"##,
        expect![
            "OK ((:error void-variable (events)) dark ((:frames dark) (:themes (dark-theme))))"
        ],
    )
}

fn auto_dark_autoload_customize_before_enable_runs_real_initial_light_theme_workflow()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_autoload_customize_before_enable_runs_real_initial_light_theme_workflow",
        r##"(let ((custom-safe-themes t)
                               (frame-background-mode
                                'light))
          (unwind-protect
              (progn
                (mapc
                 #'disable-theme
                 '(tango-dark tango))
                (setq custom-enabled-themes
                      nil)
                (custom-set-variables
                 '(auto-dark-detection-method
                   'manual)
                 '(auto-dark-themes
                   '((tango-dark)
                     (tango))))
                (let ((before
                       (list
                        (featurep 'auto-dark)
                        (boundp
                         'auto-dark-themes)
                        (boundp
                         'auto-dark-detection-method)
                        custom-enabled-themes)))
                  (auto-dark-mode 1)
                  (let ((after
                         (list
                          (featurep 'auto-dark)
                          auto-dark-mode
                          auto-dark-detection-method
                          auto-dark-themes
                          custom-enabled-themes
                          auto-dark--last-dark-mode-state
                          frame-background-mode
                          (timerp
                           auto-dark--timer))))
                    (auto-dark-mode -1)
                    (list
                     before
                     after
                     auto-dark-mode
                     (timerp
                      auto-dark--timer)))))
            (when
                (boundp 'auto-dark--timer)
              (auto-dark-stop-timer))
            (mapc
             #'disable-theme
             '(tango-dark tango))))"##,
        expect![
            "OK ((nil nil nil nil) (t t manual ((tango-dark) (tango)) (tango-dark) dark dark t) nil t)"
        ],
    )
}

pub(super) fn lifecycle_auto_dark_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dark_mode_enable_with_configured_method_checks_theme_then_registers_listener(),
        auto_dark_mode_enable_without_method_persists_detection_before_check_and_registration(),
        auto_dark_mode_disable_only_unregisters_listener_and_preserves_theme_and_detection_state(),
        auto_dark_global_mode_numeric_toggle_hook_and_repeated_enable_semantics_match(),
        auto_dark_mode_enable_failure_leaves_mode_variable_on_and_skips_registration(),
        auto_dark_set_theme_hook_failure_occurs_after_state_frame_and_theme_changes(),
    ]
}

pub(super) fn lifecycle_auto_dark_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![auto_dark_autoload_customize_before_enable_runs_real_initial_light_theme_workflow()]
}
