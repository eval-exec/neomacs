use expect_test::expect;

use super::ParityBatchCase;

fn auto_dark_themes_for_mode_selects_exact_dark_light_and_unknown_slots() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_themes_for_mode_selects_exact_dark_light_and_unknown_slots",
        r##"(mapcar
          (lambda (themes)
            (let ((auto-dark-themes
                   themes))
              (list
               themes
               (mapcar
                (lambda (mode)
                  (list
                   mode
                   (auto-dark--themes-for-mode
                    mode)))
                '(dark
                  light
                  unknown
                  nil)))))
          '(nil
            ((wombat)
             (leuven))
            (nil
             (tango))
            ((tango-dark)
             nil)
            ((one two)
             (three four)
             (ignored))))"##,
        expect![
            "OK ((nil ((dark nil) (light nil) (unknown nil) (nil nil))) ((#1=(wombat) #2=(leuven)) ((dark #1#) (light #2#) (unknown nil) (nil nil))) ((nil #3=(tango)) ((dark nil) (light #3#) (unknown nil) (nil nil))) ((#4=(tango-dark) nil) ((dark #4#) (light nil) (unknown nil) (nil nil))) ((#5=(one two) #6=(three four) (ignored)) ((dark #5#) (light #6#) (unknown nil) (nil nil))))"
        ],
    )
}

fn auto_dark_update_frame_backgrounds_sets_global_mode_before_every_frame() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_update_frame_backgrounds_sets_global_mode_before_every_frame",
        r##"(let ((frame-background-mode
                                'initial)
                               events)
          (cl-letf
              (((symbol-function 'frame-list)
                (lambda ()
                  (push
                   (list
                    :frame-list
                    frame-background-mode)
                   events)
                  '(frame-a
                    frame-b
                    frame-c)))
               ((symbol-function
                 'frame-set-background-mode)
                (lambda (frame)
                  (push
                   (list
                    :set
                    frame
                    frame-background-mode)
                   events)
                  (list
                   :updated
                   frame))))
            (list
             (auto-dark--update-frame-backgrounds
              'dark)
             frame-background-mode
             (nreverse events))))"##,
        expect![
            "OK ((frame-a frame-b frame-c) dark ((:frame-list dark) (:set frame-a dark) (:set frame-b dark) (:set frame-c dark)))"
        ],
    )
}

fn auto_dark_enable_themes_deduplicates_targets_disables_only_others_and_uses_fast_or_load_path()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_enable_themes_deduplicates_targets_disables_only_others_and_uses_fast_or_load_path",
        r##"(let ((custom-enabled-themes
                                '(old-theme
                                  user
                                  keep-theme))
                               events)
          (put
           'keep-theme
           'theme-settings
           '((fixture settings)))
          (put
           'new-theme
           'theme-settings
           nil)
          (cl-letf
              (((symbol-function 'disable-theme)
                (lambda (theme)
                  (push
                   (list :disable theme)
                   events)
                  theme))
               ((symbol-function 'enable-theme)
                (lambda (theme)
                  (push
                   (list :enable theme)
                   events)
                  theme))
               ((symbol-function 'load-theme)
                (lambda (theme no-confirm no-enable)
                  (push
                   (list
                    :load
                    theme
                    no-confirm
                    no-enable)
                   events)
                  theme)))
            (list
             (auto-dark--enable-themes
              '(new-theme
                keep-theme
                user
                new-theme))
             custom-enabled-themes
             (nreverse events))))"##,
        expect![[
            r#"OK ("Warning (emacs): Failed to enable theme(s): new-theme" (old-theme user keep-theme) ((:disable old-theme) (:disable user) (:enable keep-theme)))"#
        ]],
    )
}

fn auto_dark_enable_themes_collects_all_failures_and_warns_once_after_other_themes_continue()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_enable_themes_collects_all_failures_and_warns_once_after_other_themes_continue",
        r##"(let ((custom-enabled-themes
                                '(obsolete))
                               events)
          (mapc
           (lambda (theme)
             (put
              theme
              'theme-settings
              '((fixture settings))))
           '(good-one
             bad-one
             good-two
             bad-two))
          (cl-letf
              (((symbol-function 'disable-theme)
                (lambda (theme)
                  (push
                   (list :disable theme)
                   events)))
               ((symbol-function 'enable-theme)
                (lambda (theme)
                  (push
                   (list :enable theme)
                   events)
                  (when
                      (memq theme
                            '(bad-one bad-two))
                    (error
                     "fixture failure %s"
                     theme))
                  theme)))
            (list
             (auto-dark-test-warning-data
              (lambda ()
                (auto-dark--enable-themes
                 '(good-one
                   bad-one
                   good-two
                   bad-two))))
             (nreverse events))))"##,
        expect![[
            r#"OK (("Warning (emacs): Failed to enable theme(s): bad-two, bad-one" nil) ((:disable obsolete) (:enable bad-two) (:enable good-two) (:enable bad-one) (:enable good-one)))"#
        ]],
    )
    .fresh_process()
}

fn auto_dark_declared_but_not_loaded_theme_uses_load_path_for_issue_96() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_declared_but_not_loaded_theme_uses_load_path_for_issue_96",
        r##"(let ((custom-enabled-themes nil)
                               events)
          (unless
              (custom-theme-p
               'auto-dark-fixture-declared)
            (custom-declare-theme
             'auto-dark-fixture-declared
             (custom-make-theme-feature
              'auto-dark-fixture-declared)))
          (put
           'auto-dark-fixture-declared
           'theme-settings
           nil)
          (cl-letf
              (((symbol-function 'enable-theme)
                (lambda (theme)
                  (push
                   (list :enable theme)
                   events)
                  :enabled))
               ((symbol-function 'load-theme)
                (lambda (theme no-confirm no-enable)
                  (push
                   (list
                    :load
                    theme
                    no-confirm
                    no-enable)
                   events)
                  :loaded)))
            (list
             (custom-theme-p
              'auto-dark-fixture-declared)
             (get
              'auto-dark-fixture-declared
              'theme-settings)
             (auto-dark--enable-themes
              '(auto-dark-fixture-declared))
             (nreverse events))))"##,
        expect![[
            r#"OK ((auto-dark-fixture-declared user changed) nil "Warning (emacs): Failed to enable theme(s): auto-dark-fixture-declared" nil)"#
        ]],
    )
}

fn auto_dark_set_theme_updates_state_frames_themes_then_selected_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_set_theme_updates_state_frames_themes_then_selected_hook",
        r##"(let* ((auto-dark-themes
                                 '((dark-a dark-b)
                                   (light-a)))
                                (auto-dark--last-dark-mode-state
                                 'unknown)
                                events
                                (auto-dark-dark-mode-hook
                                 (list
                                  (lambda ()
                                    (push :dark-hook
                                          events))))
                                (auto-dark-light-mode-hook
                                 (list
                                  (lambda ()
                                    (push :light-hook
                                          events)))))
          (cl-letf
              (((symbol-function
                 'auto-dark--update-frame-backgrounds)
                (lambda (appearance)
                  (push
                   (list
                    :frames
                    appearance
                    auto-dark--last-dark-mode-state)
                   events)
                  :frames-updated))
               ((symbol-function
                 'auto-dark--enable-themes)
                (lambda (themes)
                  (push
                   (list
                    :themes
                    themes
                    auto-dark--last-dark-mode-state)
                   events)
                  :themes-enabled)))
            (list
             (auto-dark--set-theme 'dark)
             auto-dark--last-dark-mode-state
             (nreverse events)
             (progn
               (setq events nil)
               (auto-dark--set-theme
                'light))
             auto-dark--last-dark-mode-state
             (nreverse events))))"##,
        expect![
            "OK (nil dark ((:frames dark dark) (:themes (dark-a dark-b) dark) :dark-hook) nil light ((:frames light light) (:themes (light-a) light) :light-hook))"
        ],
    )
}

fn auto_dark_set_theme_is_complete_noop_before_theme_variable_initialization() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_set_theme_is_complete_noop_before_theme_variable_initialization",
        r##"(let ((saved
                                auto-dark-themes)
                               (auto-dark--last-dark-mode-state
                                'unknown)
                               events)
          (unwind-protect
              (progn
                (makunbound
                 'auto-dark-themes)
                (cl-letf
                    (((symbol-function
                       'auto-dark--update-frame-backgrounds)
                      (lambda (&rest arguments)
                        (push
                         (list :frames arguments)
                         events)))
                     ((symbol-function
                       'auto-dark--enable-themes)
                      (lambda (&rest arguments)
                        (push
                         (list :themes arguments)
                         events))))
                  (list
                   (auto-dark--initialized-p)
                   (auto-dark--set-theme 'dark)
                   auto-dark--last-dark-mode-state
                   events)))
            (set
             'auto-dark-themes
             saved)))"##,
        expect!["OK (nil nil unknown nil)"],
    )
}

fn auto_dark_toggle_appearance_switches_unknown_light_and_dark_states_practically()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_toggle_appearance_switches_unknown_light_and_dark_states_practically",
        r##"(let (calls)
          (cl-letf
              (((symbol-function
                 'auto-dark--set-theme)
                (lambda (appearance)
                  (push appearance calls)
                  (setq
                   auto-dark--last-dark-mode-state
                   appearance)
                  (list
                   :set
                   appearance))))
            (list
             (mapcar
              (lambda (initial)
                (let ((auto-dark--last-dark-mode-state
                       initial))
                  (list
                   initial
                   (auto-dark-toggle-appearance)
                   auto-dark--last-dark-mode-state)))
              '(unknown
                nil
                light
                dark))
             (nreverse calls))))"##,
        expect![
            "OK (((unknown (:set dark) dark) (nil (:set dark) dark) (light (:set dark) dark) (dark (:set light) light)) (dark dark dark light))"
        ],
    )
}

fn auto_dark_check_and_set_skips_exact_state_but_repairs_theme_drift() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_check_and_set_skips_exact_state_but_repairs_theme_drift",
        r##"(let ((auto-dark-themes
                                '((dark-theme)
                                  (light-theme)))
                               calls)
          (cl-letf
              (((symbol-function
                 'auto-dark--current-system-mode)
                (lambda ()
                  auto-dark-test-appearance))
               ((symbol-function
                 'auto-dark--set-theme)
               (lambda (appearance)
                  (push appearance calls)
                  :changed)))
            (list
             (let ((auto-dark-test-appearance
                    'dark)
                   (auto-dark--last-dark-mode-state
                    'dark)
                   (custom-enabled-themes
                    '(dark-theme)))
               (list
                (auto-dark--check-and-set-dark-mode)
                calls))
             (let ((auto-dark-test-appearance
                    'dark)
                   (auto-dark--last-dark-mode-state
                    'dark)
                   (custom-enabled-themes
                    '(wrong-theme)))
               (list
                (auto-dark--check-and-set-dark-mode)
                (nreverse calls)))
             (let ((auto-dark-test-appearance
                    'light)
                   (auto-dark--last-dark-mode-state
                    'dark)
                   (custom-enabled-themes
                    '(light-theme)))
               (list
                (auto-dark--check-and-set-dark-mode)
                (nreverse calls))))))"##,
        expect!["OK ((nil nil) (:changed #1=(dark light)) (:changed #1#))"],
    )
}

fn auto_dark_custom_theme_setter_preloads_missing_themes_sets_default_and_refreshes_active_mode()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_custom_theme_setter_preloads_missing_themes_sets_default_and_refreshes_active_mode",
        r##"(let ((setter
                                (get
                                 'auto-dark-themes
                                 'custom-set))
                               (auto-dark-mode t)
                               calls)
          (put
           'already-loaded
           'theme-settings
           '((fixture settings)))
          (put
           'needs-load-dark
           'theme-settings
           nil)
          (put
           'needs-load-light
           'theme-settings
           nil)
          (cl-letf
              (((symbol-function 'load-theme)
                (lambda (theme no-confirm no-enable)
                  (push
                   (list
                    :load
                    theme
                    no-confirm
                    no-enable)
                   calls)
                  :loaded))
               ((symbol-function
                 'auto-dark--check-and-set-dark-mode)
                (lambda ()
                  (push
                   (list
                    :refresh
                    auto-dark-themes)
                   calls)
                  :refreshed)))
            (list
             (funcall
              setter
              'auto-dark-themes
              '((already-loaded
                 needs-load-dark)
                (needs-load-light
                 already-loaded)))
             auto-dark-themes
             (default-value
              'auto-dark-themes)
             (nreverse calls))))"##,
        expect![
            "OK (:refreshed #1=((already-loaded needs-load-dark) (needs-load-light already-loaded)) #1# ((:load needs-load-dark nil t) (:load needs-load-light nil t) (:refresh #1#)))"
        ],
    )
}

fn auto_dark_custom_theme_setter_propagates_preload_failure_before_setting_or_refreshing()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_custom_theme_setter_propagates_preload_failure_before_setting_or_refreshing",
        r##"(let ((setter
                                (get
                                 'auto-dark-themes
                                 'custom-set))
                               (auto-dark-mode t)
                               (auto-dark-themes
                                '((old-dark)
                                  (old-light)))
                               calls)
          (put
           'failing-theme
           'theme-settings
           nil)
          (cl-letf
              (((symbol-function 'load-theme)
                (lambda (&rest arguments)
                  (push
                   (cons :load arguments)
                   calls)
                  (error
                   "fixture unsafe theme")))
               ((symbol-function
                 'auto-dark--check-and-set-dark-mode)
                (lambda ()
                  (push :refresh calls))))
            (list
             (auto-dark-test-error-data
              (lambda ()
                (funcall
                 setter
                 'auto-dark-themes
                 '((failing-theme)
                   (other-theme)))))
             auto-dark-themes
             (nreverse calls))))"##,
        expect![[
            r#"OK ((:error error ("fixture unsafe theme")) ((old-dark) (old-light)) ((:load failing-theme nil t)))"#
        ]],
    )
}

fn auto_dark_real_builtin_theme_configuration_enable_toggle_and_disable_workflow_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dark_real_builtin_theme_configuration_enable_toggle_and_disable_workflow_match",
        r##"(let ((custom-safe-themes t)
                               (auto-dark-detection-method
                                'manual)
                               (frame-background-mode
                                'light))
          (unwind-protect
              (progn
                (mapc
                 #'disable-theme
                 '(tango-dark tango))
                (setq
                 custom-enabled-themes nil
                 auto-dark--last-dark-mode-state
                 'unknown)
                (customize-set-variable
                 'auto-dark-themes
                 '((tango-dark)
                   (tango)))
                (cl-letf
                    (((symbol-function
                       'auto-dark--register-change-listener)
                      #'ignore)
                     ((symbol-function
                       'auto-dark--unregister-change-listener)
                      #'ignore))
                  (auto-dark-mode 1)
                  (let ((light-state
                         (auto-dark-test-theme-state)))
                    (auto-dark-toggle-appearance)
                    (let ((dark-state
                           (auto-dark-test-theme-state)))
                      (auto-dark-toggle-appearance)
                      (let ((light-again
                             (auto-dark-test-theme-state)))
                        (auto-dark-mode -1)
                        (list
                         light-state
                         dark-state
                         light-again
                         auto-dark-mode
                         custom-enabled-themes
                         frame-background-mode
                         auto-dark--last-dark-mode-state))))))
            (mapc
             #'disable-theme
             '(tango-dark tango))))"##,
        expect![
            "OK (((tango-dark) ((tango-dark t t t) (tango t t nil) (tsdh-dark nil nil nil) (tsdh-light nil nil nil) (wombat nil nil nil) (leuven nil nil nil))) ((tango) ((tango-dark t t nil) (tango t t t) (tsdh-dark nil nil nil) (tsdh-light nil nil nil) (wombat nil nil nil) (leuven nil nil nil))) ((tango-dark) ((tango-dark t t t) (tango t t nil) (tsdh-dark nil nil nil) (tsdh-light nil nil nil) (wombat nil nil nil) (leuven nil nil nil))) nil (tango-dark) dark dark)"
        ],
    )
}

pub(super) fn themes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dark_themes_for_mode_selects_exact_dark_light_and_unknown_slots(),
        auto_dark_update_frame_backgrounds_sets_global_mode_before_every_frame(),
        auto_dark_enable_themes_deduplicates_targets_disables_only_others_and_uses_fast_or_load_path(),
        auto_dark_enable_themes_collects_all_failures_and_warns_once_after_other_themes_continue(),
        auto_dark_declared_but_not_loaded_theme_uses_load_path_for_issue_96(),
        auto_dark_set_theme_updates_state_frames_themes_then_selected_hook(),
        auto_dark_set_theme_is_complete_noop_before_theme_variable_initialization(),
        auto_dark_toggle_appearance_switches_unknown_light_and_dark_states_practically(),
        auto_dark_check_and_set_skips_exact_state_but_repairs_theme_drift(),
        auto_dark_custom_theme_setter_preloads_missing_themes_sets_default_and_refreshes_active_mode(),
        auto_dark_custom_theme_setter_propagates_preload_failure_before_setting_or_refreshing(),
        auto_dark_real_builtin_theme_configuration_enable_toggle_and_disable_workflow_match(),
    ]
}
