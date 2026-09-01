use expect_test::expect;

use super::ParityBatchCase;

fn auto_dim_other_buffers_dim_buffer_sets_distinct_parameters_for_two_windows_showing_same_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_dim_buffer_sets_distinct_parameters_for_two_windows_showing_same_buffer",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-shared*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (set-window-buffer
                   (selected-window)
                   buffer)
                  (let ((other
                         (split-window-below)))
                    (set-window-buffer
                     other
                     buffer)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)))
                          (adob--has-fringes nil))
                      (adob--dim-buffer
                       buffer
                       (selected-window))
                      (let ((except-selected
                             (list
                              (adob-test-window-summary)
                              (adob-test-remap-summary
                               buffer))))
                        (adob--dim-buffer
                         buffer)
                        (list
                         except-selected
                         (adob-test-window-summary)
                         (adob-test-remap-summary
                          buffer))))))
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![[
            r#"OK ((((t " *adob-shared*" nil) (nil " *adob-shared*" t)) (t 1 (default) ((default (#1=(:filtered (:window adob--dim t) auto-dim-other-buffers)))))) ((t " *adob-shared*" nil) (nil " *adob-shared*" t)) (t 1 (default) ((default (#1#)))))"#
        ]],
    )
}

fn auto_dim_other_buffers_never_dim_buffer_removes_existing_remaps_without_mutating_window_parameters()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_never_dim_buffer_removes_existing_remaps_without_mutating_window_parameters",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-never-real*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (set-window-buffer
                   (selected-window)
                   buffer)
                  (let ((other
                         (split-window-below)))
                    (set-window-buffer other buffer)
                    (set-window-parameter
                     (selected-window)
                     'adob--dim
                     :selected-before)
                    (set-window-parameter
                     other
                     'adob--dim
                     :other-before)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)))
                          (auto-dim-other-buffers-never-dim-buffer-functions
                           '(adob-test-never-dim-by-name))
                          (adob-test-never-dim-names nil)
                          (adob--has-fringes nil))
                      (with-current-buffer buffer
                        (adob--remap-add-relative))
                      (setq
                       adob-test-never-dim-names
                       (list
                        (buffer-name buffer)))
                      (list
                       (adob--dim-buffer buffer)
                       (adob-test-window-summary)
                       (adob-test-remap-summary
                        buffer)))))
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![[
            r#"OK (nil ((t " *adob-never-real*" :selected-before) (nil " *adob-never-real*" :other-before)) (nil 0 nil nil))"#
        ]],
    )
}

fn auto_dim_other_buffers_update_switches_real_selected_window_and_buffer_state() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_dim_other_buffers_update_switches_real_selected_window_and_buffer_state",
        r##"(save-window-excursion
          (let ((first
                 (generate-new-buffer
                  " *adob-update-first*"))
                (second
                 (generate-new-buffer
                  " *adob-update-second*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (let* ((first-window
                          (selected-window))
                         (second-window
                          (split-window-below)))
                    (set-window-buffer
                     first-window
                     first)
                    (set-window-buffer
                     second-window
                     second)
                    (select-window first-window)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)))
                          (adob--has-fringes nil)
                          (adob--last-window nil)
                          (adob--last-buffer nil))
                      (adob--update)
                      (let ((first-state
                             (list
                              (adob-test-window-summary)
                              (buffer-name
                               adob--last-buffer)
                              (eq
                               adob--last-window
                               first-window))))
                        (select-window second-window)
                        (adob--update)
                        (list
                         first-state
                         (adob-test-window-summary)
                         (buffer-name
                          adob--last-buffer)
                         (eq
                          adob--last-window
                          second-window)
                         (adob-test-remap-summary
                          first)
                         (adob-test-remap-summary
                          second))))))
              (when (buffer-live-p first)
                (kill-buffer first))
              (when (buffer-live-p second)
                (kill-buffer second)))))"##,
        expect![[
            r#"OK ((((t " *adob-update-first*" nil) (nil " *adob-update-second*" nil)) " *adob-update-first*" t) ((t " *adob-update-second*" nil) (nil " *adob-update-first*" t)) " *adob-update-second*" t (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))) (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))))"#
        ]],
    )
}

fn auto_dim_other_buffers_update_moves_highlight_between_windows_showing_same_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_update_moves_highlight_between_windows_showing_same_buffer",
        r##"(save-window-excursion
          (let ((buffer
                 (generate-new-buffer
                  " *adob-update-shared*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (let* ((first-window
                          (selected-window))
                         (second-window
                          (split-window-below)))
                    (set-window-buffer
                     first-window
                     buffer)
                    (set-window-buffer
                     second-window
                     buffer)
                    (select-window first-window)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)))
                          (adob--has-fringes nil)
                          (adob--last-window nil)
                          (adob--last-buffer nil))
                      (adob--update)
                      (let ((first-state
                             (adob-test-window-summary)))
                        (select-window second-window)
                        (adob--update)
                        (list
                         first-state
                         (adob-test-window-summary)
                         (eq
                          adob--last-buffer
                          buffer)
                         (eq
                          adob--last-window
                          second-window)
                         (adob-test-remap-summary
                          buffer))))))
              (when (buffer-live-p buffer)
                (kill-buffer buffer)))))"##,
        expect![[
            r#"OK (((t " *adob-update-shared*" nil) (nil " *adob-update-shared*" nil)) ((t " *adob-update-shared*" nil) (nil " *adob-update-shared*" t)) t t (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))))"#
        ]],
    )
}

fn auto_dim_other_buffers_rescan_real_windows_repairs_parameters_and_missing_remaps()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_rescan_real_windows_repairs_parameters_and_missing_remaps",
        r##"(save-window-excursion
          (let ((first
                 (generate-new-buffer
                  " *adob-rescan-first*"))
                (second
                 (generate-new-buffer
                  " *adob-rescan-second*")))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (let* ((first-window
                          (selected-window))
                         (second-window
                          (split-window-below)))
                    (set-window-buffer
                     first-window
                     first)
                    (set-window-buffer
                     second-window
                     second)
                    (select-window first-window)
                    (set-window-parameter
                     first-window
                     'adob--dim
                     t)
                    (set-window-parameter
                     second-window
                     'adob--dim
                     nil)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)))
                          (adob--has-fringes nil))
                      (adob--rescan-windows)
                      (list
                       (adob-test-window-summary)
                       (adob-test-remap-summary
                        first)
                       (adob-test-remap-summary
                        second)))))
              (when (buffer-live-p first)
                (kill-buffer first))
              (when (buffer-live-p second)
                (kill-buffer second)))))"##,
        expect![[
            r#"OK (((t " *adob-rescan-first*" nil) (nil " *adob-rescan-second*" t)) (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))) (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))))"#
        ]],
    )
}

fn auto_dim_other_buffers_buffer_list_hook_routes_selected_and_nonselected_buffers_to_distinct_paths()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_buffer_list_hook_routes_selected_and_nonselected_buffers_to_distinct_paths",
        r##"(save-window-excursion
          (let ((selected-buffer
                 (generate-new-buffer
                  " *adob-hook-selected*"))
                (other-buffer
                 (generate-new-buffer
                  " *adob-hook-other*"))
                events)
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (let ((other-window
                         (split-window-below)))
                    (set-window-buffer
                     (selected-window)
                     selected-buffer)
                    (set-window-buffer
                     other-window
                     other-buffer)
                    (cl-letf
                        (((symbol-function
                           'adob--update)
                          (lambda ()
                            (push :update events)))
                         ((symbol-function
                           'adob--dim-buffer)
                          (lambda (buffer &optional except)
                            (push
                             (list
                              :dim
                              (buffer-name buffer)
                              except)
                             events))))
                      (with-current-buffer
                          selected-buffer
                        (adob--buffer-list-update-hook))
                      (with-current-buffer
                          other-buffer
                        (adob--buffer-list-update-hook))
                      (nreverse events))))
              (when
                  (buffer-live-p selected-buffer)
                (kill-buffer selected-buffer))
              (when
                  (buffer-live-p other-buffer)
                (kill-buffer other-buffer)))))"##,
        expect![[r#"OK (:update (:dim " *adob-hook-other*" nil))"#]],
    )
}

fn auto_dim_other_buffers_minibuffer_switch_option_controls_update_while_real_minibuffer_is_identified()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_minibuffer_switch_option_controls_update_while_real_minibuffer_is_identified",
        r##"(let ((real-minibuffer
                                (window-minibuffer-p
                                 (minibuffer-window)))
                               events)
          (cl-letf
              (((symbol-function
                 'selected-window)
                (lambda ()
                  :fixture-minibuffer-window))
               ((symbol-function
                 'window-minibuffer-p)
                (lambda (&optional window)
                  (eq
                   (or
                    window
                    :fixture-minibuffer-window)
                   :fixture-minibuffer-window)))
               ((symbol-function
                 'window-buffer)
                (lambda (_window)
                  (current-buffer)))
               ((symbol-function
                 'adob--remap-faces)
                (lambda (&rest arguments)
                  (push
                   (list :remap arguments)
                   events))))
            (let ((auto-dim-other-buffers-dim-on-switch-to-minibuffer
                   nil)
                  (adob--last-window
                   :previous-window)
                  (adob--last-buffer
                   (current-buffer)))
              (push
               (list
                :disabled
                (adob--update)
                adob--last-window)
               events))
            (let ((auto-dim-other-buffers-dim-on-switch-to-minibuffer
                   t)
                  (adob--last-window
                   :fixture-minibuffer-window)
                  (adob--last-buffer
                   (current-buffer)))
              (push
               (list
                :enabled
                (adob--update)
                adob--last-window)
               events))
            (list
             real-minibuffer
             (nreverse events))))"##,
        expect![
            "OK (t ((:disabled nil :previous-window) (:enabled nil :fixture-minibuffer-window)))"
        ],
    )
}

fn auto_dim_other_buffers_initialize_records_selected_pair_and_dims_each_supplied_live_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_initialize_records_selected_pair_and_dims_each_supplied_live_buffer",
        r##"(let ((first
                                (generate-new-buffer
                                 " *adob-init-first*"))
                               (second
                                (generate-new-buffer
                                 " *adob-init-second*"))
                               events)
          (unwind-protect
              (cl-letf
                  (((symbol-function 'buffer-list)
                    (lambda ()
                      (list first second)))
                   ((symbol-function
                     'adob--dim-buffer)
                    (lambda (buffer &optional except)
                      (push
                       (list
                        (buffer-name buffer)
                        (eq
                         except
                         (selected-window)))
                       events))))
                (adob--initialize)
                (list
                 (eq
                  adob--last-window
                  (selected-window))
                 (eq
                  adob--last-buffer
                  (window-buffer
                   (selected-window)))
                 (nreverse events)))
            (when (buffer-live-p first)
              (kill-buffer first))
            (when (buffer-live-p second)
              (kill-buffer second))))"##,
        expect![[r#"OK (t t ((" *adob-init-first*" t) (" *adob-init-second*" t)))"#]],
    )
}

fn auto_dim_other_buffers_base_and_indirect_buffers_keep_independent_remap_cookies_in_real_windows()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_dim_other_buffers_base_and_indirect_buffers_keep_independent_remap_cookies_in_real_windows",
        r##"(save-window-excursion
          (let* ((base
                  (generate-new-buffer
                   " *adob-base*"))
                 (indirect
                  (make-indirect-buffer
                   base
                   " *adob-indirect*"
                   t)))
            (unwind-protect
                (progn
                  (delete-other-windows)
                  (let ((other
                         (split-window-below)))
                    (set-window-buffer
                     (selected-window)
                     base)
                    (set-window-buffer
                     other
                     indirect)
                    (let ((auto-dim-other-buffers-affected-faces
                           '((default
                              . auto-dim-other-buffers)))
                          (adob--has-fringes nil))
                      (adob--dim-buffer
                       base
                       (selected-window))
                      (adob--dim-buffer
                       indirect)
                      (list
                       (buffer-base-buffer
                        indirect)
                       (adob-test-window-summary)
                       (adob-test-remap-summary
                        base)
                       (adob-test-remap-summary
                        indirect)))))
              (when (buffer-live-p indirect)
                (kill-buffer indirect))
              (when (buffer-live-p base)
                (kill-buffer base)))))"##,
        expect![[
            r#"OK ((:buffer nil) ((t " *adob-base*" nil) (nil " *adob-indirect*" t)) (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))) (t 1 (default) ((default ((:filtered (:window adob--dim t) auto-dim-other-buffers))))))"#
        ]],
    )
}

pub(super) fn windows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_dim_other_buffers_dim_buffer_sets_distinct_parameters_for_two_windows_showing_same_buffer(),
        auto_dim_other_buffers_never_dim_buffer_removes_existing_remaps_without_mutating_window_parameters(),
        auto_dim_other_buffers_update_switches_real_selected_window_and_buffer_state(),
        auto_dim_other_buffers_update_moves_highlight_between_windows_showing_same_buffer(),
        auto_dim_other_buffers_rescan_real_windows_repairs_parameters_and_missing_remaps(),
        auto_dim_other_buffers_buffer_list_hook_routes_selected_and_nonselected_buffers_to_distinct_paths(),
        auto_dim_other_buffers_minibuffer_switch_option_controls_update_while_real_minibuffer_is_identified(),
        auto_dim_other_buffers_initialize_records_selected_pair_and_dims_each_supplied_live_buffer(),
        auto_dim_other_buffers_base_and_indirect_buffers_keep_independent_remap_cookies_in_real_windows(),
    ]
}
