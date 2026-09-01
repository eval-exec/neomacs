use expect_test::expect;

use super::ParityBatchCase;

fn auto_dim_other_buffers_mode_enable_installs_exact_hooks_focus_advice_and_initial_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_mode_enable_installs_exact_hooks_focus_advice_and_initial_state",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-mode-enable*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (set-window-buffer
                   (selected-window)
                   buffer)
                  (let ((auto-dim-other-buffers-affected-faces
                         '((default
                            . auto-dim-other-buffers)))
                        (adob--has-fringes nil))
                    (auto-dim-other-buffers-mode 1)
                    (list
                     auto-dim-other-buffers-mode
                     (adob-test-hook-count
                      'adob--rescan-windows
                      'window-configuration-change-hook)
                     (adob-test-hook-count
                      'adob--buffer-list-update-hook
                      'buffer-list-update-hook)
                     (adob-test-focus-advice-installed-p)
                     (and
                      (advice-member-p
                       #'adob--kill-all-local-variables-advice
                       'kill-all-local-variables)
                      t)
                     (eq
                      adob--last-window
                      (selected-window))
                     (eq
                      adob--last-buffer
                      buffer)
                     (adob-test-window-summary)
                     (adob-test-remap-summary
                      buffer))))
              (auto-dim-other-buffers-mode -1)
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![[
            r#"OK (t 1 1 t t t t ((t " *adob-mode-enable*" nil)) (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))))"#
        ]],
    )
}

fn auto_dim_other_buffers_mode_disable_removes_hooks_advice_state_and_every_owned_remap()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_mode_disable_removes_hooks_advice_state_and_every_owned_remap",
        r##"(save-window-excursion
          (let ((first
                 (generate-new-buffer
                  " *adob-disable-first*"))
                (second
                 (generate-new-buffer
                  " *adob-disable-second*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (let ((other
                         (split-window-below)))
                    (set-window-buffer
                     (selected-window)
                     first)
                    (set-window-buffer
                     other
                     second)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)))
                          (adob--has-fringes nil))
                      (auto-dim-other-buffers-mode 1)
                      (auto-dim-other-buffers-mode -1)
                      (list
                       auto-dim-other-buffers-mode
                       (adob-test-hook-count
                        'adob--rescan-windows
                        'window-configuration-change-hook)
                       (adob-test-hook-count
                        'adob--buffer-list-update-hook
                        'buffer-list-update-hook)
                       (adob-test-focus-advice-installed-p)
                       (advice-member-p
                        #'adob--kill-all-local-variables-advice
                        'kill-all-local-variables)
                       adob--last-buffer
                       adob--last-window
                       (adob-test-remap-summary
                        first)
                       (adob-test-remap-summary
                        second)
                       (adob-test-window-summary)))))
              (auto-dim-other-buffers-mode -1)
              (when (buffer-live-p first)
                (kill-buffer first))
              (when (buffer-live-p second)
                (kill-buffer second)))))"##,
        expect![[
            r#"OK (nil 0 0 nil nil nil nil (nil 0 nil nil) (nil 0 nil nil) ((t " *adob-disable-first*" nil) (nil " *adob-disable-second*" t)))"#
        ]],
    )
}

fn auto_dim_other_buffers_repeated_enable_disable_and_toggle_keep_hooks_and_advice_idempotent()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_repeated_enable_disable_and_toggle_keep_hooks_and_advice_idempotent",
        r##"(let (states)
          (dolist
              (argument
               '(1 1 -1 -1 toggle toggle))
            (auto-dim-other-buffers-mode
             argument)
            (push
             (list
              argument
              auto-dim-other-buffers-mode
              (adob-test-hook-count
               'adob--rescan-windows
               'window-configuration-change-hook)
              (adob-test-hook-count
               'adob--buffer-list-update-hook
               'buffer-list-update-hook)
              (adob-test-focus-advice-installed-p)
              (and
               (advice-member-p
                #'adob--kill-all-local-variables-advice
                'kill-all-local-variables)
               t))
             states))
          (auto-dim-other-buffers-mode -1)
          (nreverse states))"##,
        expect![
            "OK ((1 t 1 1 t t) (1 t 1 1 t t) (-1 nil 0 0 nil nil) (-1 nil 0 0 nil nil) (toggle t 1 1 t t) (toggle nil 0 0 nil nil))"
        ],
    )
}

fn auto_dim_other_buffers_each_mode_transition_cancels_existing_focus_timer_before_work()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_each_mode_transition_cancels_existing_focus_timer_before_work",
        r##"(let ((adob--focus-change-timer
                                :enable-timer)
                               events)
          (cl-letf
              (((symbol-function 'cancel-timer)
                (lambda (timer)
                  (push
                   (list :cancel timer)
                   events)))
               ((symbol-function
                 'adob--initialize)
                (lambda ()
                  (push :initialize events)))
               ((symbol-function
                 'adob--remap-cycle-all)
                (lambda (add)
                  (push
                   (list :cycle add)
                   events))))
            (auto-dim-other-buffers-mode 1)
            (setq
             adob--focus-change-timer
             :disable-timer)
            (auto-dim-other-buffers-mode -1)
            (list
             auto-dim-other-buffers-mode
             adob--focus-change-timer
             (nreverse events))))"##,
        expect![
            "OK (nil nil ((:cancel :enable-timer) :initialize (:cancel :disable-timer) (:cycle nil)))"
        ],
    )
}

fn auto_dim_other_buffers_never_dim_customize_setter_sets_default_and_reinitializes_only_when_enabled()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_never_dim_customize_setter_sets_default_and_reinitializes_only_when_enabled",
        r##"(let ((setter
                                (get
                                 'auto-dim-other-buffers-never-dim-buffer-functions
                                 'custom-set))
                               events)
          (cl-letf
              (((symbol-function
                 'adob--initialize)
                (lambda ()
                  (push
                   (list
                    :initialize
                    (current-buffer))
                   events)
                  :initialized)))
            (let ((auto-dim-other-buffers-mode
                   nil))
              (push
               (list
                :disabled
                (funcall
                 setter
                 'auto-dim-other-buffers-never-dim-buffer-functions
                 '(ignore))
                (default-value
                 'auto-dim-other-buffers-never-dim-buffer-functions))
               events))
            (let ((auto-dim-other-buffers-mode
                   t))
              (push
               (list
                :enabled
                (funcall
                 setter
                 'auto-dim-other-buffers-never-dim-buffer-functions
                 '(always))
                (default-value
                 'auto-dim-other-buffers-never-dim-buffer-functions))
               events))
            (nreverse events)))"##,
        expect![[
            r#"OK ((:disabled #1=(ignore) #1#) (:initialize (:buffer "*scratch*")) (:enabled #2=(always) #2#))"#
        ]],
    )
}

fn auto_dim_other_buffers_affected_faces_customize_setter_rebuilds_only_when_enabled()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_affected_faces_customize_setter_rebuilds_only_when_enabled",
        r##"(let ((setter
                                (get
                                 'auto-dim-other-buffers-affected-faces
                                 'custom-set))
                               events)
          (cl-letf
              (((symbol-function
                 'adob--remap-cycle-all)
                (lambda (add)
                  (push
                   (list :cycle add)
                   events)
                  :cycled)))
            (let ((auto-dim-other-buffers-mode
                   nil))
              (push
               (list
                :disabled
                (funcall
                 setter
                 'auto-dim-other-buffers-affected-faces
                 '((default
                    . auto-dim-other-buffers)))
                (default-value
                 'auto-dim-other-buffers-affected-faces))
               events))
            (let ((auto-dim-other-buffers-mode
                   t))
              (push
               (list
                :enabled
                (funcall
                 setter
                 'auto-dim-other-buffers-affected-faces
                 '((default
                    . (nil . bold))))
                (default-value
                 'auto-dim-other-buffers-affected-faces))
               events))
            (nreverse events)))"##,
        expect![
            "OK ((:disabled nil ((default . auto-dim-other-buffers))) (:cycle t) (:enabled :cycled ((default nil . bold))))"
        ],
    )
}

fn auto_dim_other_buffers_mode_enable_failure_leaves_mode_and_installed_hooks_before_initialization()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_mode_enable_failure_leaves_mode_and_installed_hooks_before_initialization",
        r##"(let ((auto-dim-other-buffers-mode
                                nil))
          (cl-letf
              (((symbol-function
                 'adob--initialize)
                (lambda ()
                  (error
                   "fixture initialization failed"))))
            (let ((result
                   (adob-test-error-data
                    (lambda ()
                      (auto-dim-other-buffers-mode
                       1)))))
              (prog1
                  (list
                   result
                   auto-dim-other-buffers-mode
                   (adob-test-hook-count
                    'adob--rescan-windows
                    'window-configuration-change-hook)
                   (adob-test-hook-count
                    'adob--buffer-list-update-hook
                    'buffer-list-update-hook)
                   (adob-test-focus-advice-installed-p)
                   (and
                    (advice-member-p
                     #'adob--kill-all-local-variables-advice
                     'kill-all-local-variables)
                    t))
                (auto-dim-other-buffers-mode
                 -1)))))"##,
        expect![[r#"OK ((:error error ("fixture initialization failed")) t 1 1 t t)"#]],
    )
}

fn auto_dim_other_buffers_autoload_mode_command_loads_source_and_runs_real_enable_disable_lifecycle()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_autoload_mode_command_loads_source_and_runs_real_enable_disable_lifecycle",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-autoload*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (set-window-buffer
                   (selected-window)
                   buffer)
                  (let ((before
                         (list
                          (featurep
                           'auto-dim-other-buffers)
                          (autoloadp
                           (symbol-function
                            'auto-dim-other-buffers-mode)))))
                    (auto-dim-other-buffers-mode 1)
                    (let ((enabled
                           (list
                            (featurep
                             'auto-dim-other-buffers)
                            (autoloadp
                             (symbol-function
                              'auto-dim-other-buffers-mode))
                            auto-dim-other-buffers-mode
                            (adob-test-window-summary)
                            (adob-test-remap-summary
                             buffer))))
                      (auto-dim-other-buffers-mode -1)
                      (list
                       before
                       enabled
                       auto-dim-other-buffers-mode
                       (adob-test-remap-summary
                        buffer)))))
              (when (fboundp
                     'auto-dim-other-buffers-mode)
                (auto-dim-other-buffers-mode
                 -1))
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((nil t) (t nil t ((t " *adob-autoload*" nil)) (t 4 (default fringe org-block org-hide) ((org-hide ((:filtered (:window adob--dim t) auto-dim-other-buffers-hide))) (org-block ((:filtered (:window adob--dim t) auto-dim-other-buffers))) (fringe ((:filtered (:window adob--dim t) auto-dim-other-buffers))) (default ((:filtered (:window adob--dim t) auto-dim-other-buffers)))))) nil (nil 0 nil nil))"#
        ]],
    )
}

pub(super) fn lifecycle_auto_dim_other_buffers_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dim_other_buffers_mode_enable_installs_exact_hooks_focus_advice_and_initial_state(),
        auto_dim_other_buffers_mode_disable_removes_hooks_advice_state_and_every_owned_remap(),
        auto_dim_other_buffers_repeated_enable_disable_and_toggle_keep_hooks_and_advice_idempotent(),
        auto_dim_other_buffers_each_mode_transition_cancels_existing_focus_timer_before_work(),
        auto_dim_other_buffers_never_dim_customize_setter_sets_default_and_reinitializes_only_when_enabled(),
        auto_dim_other_buffers_affected_faces_customize_setter_rebuilds_only_when_enabled(),
        auto_dim_other_buffers_mode_enable_failure_leaves_mode_and_installed_hooks_before_initialization(),
    ]
}

pub(super) fn lifecycle_auto_dim_other_buffers_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dim_other_buffers_autoload_mode_command_loads_source_and_runs_real_enable_disable_lifecycle(),
    ]
}
