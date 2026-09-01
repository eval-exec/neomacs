use expect_test::expect;

use super::ParityBatchCase;

fn atl_long_lines_minor_mode_initial_metadata_lighter_keymap_and_hook_state_match()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_minor_mode_initial_metadata_lighter_keymap_and_hook_state_match",
        r##"(with-temp-buffer
         (list
          atl-long-lines-mode
          (local-variable-p
           'atl-long-lines-mode)
          (assq
           'atl-long-lines-mode
           minor-mode-alist)
          (assq
           'atl-long-lines-mode
           minor-mode-map-alist)
          (boundp
           'atl-long-lines-mode-hook)
          atl-long-lines-mode-hook
          (atl-long-lines-test-hook-count
           #'atl-long-lines--start-timer
           post-command-hook)
          (local-variable-p
           'post-command-hook)))"##,
        expect![[
            r#"OK (nil nil (atl-long-lines-mode " ATL-LL") nil t (atl-long-lines-mode--set-explicitly) 0 nil)"#
        ]],
    )
}

fn atl_long_lines_enabling_mode_installs_one_buffer_local_post_command_callback_idempotently()
-> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_enabling_mode_installs_one_buffer_local_post_command_callback_idempotently",
        r##"(with-temp-buffer
         (let ((before
                post-command-hook))
           (atl-long-lines-mode 1)
           (let ((first
                  (list
                   atl-long-lines-mode
                   (local-variable-p
                    'atl-long-lines-mode)
                   (local-variable-p
                    'post-command-hook)
                   (atl-long-lines-test-hook-count
                    #'atl-long-lines--start-timer
                    post-command-hook))))
             (atl-long-lines-mode 1)
             (list
              before
              first
              atl-long-lines-mode
              (atl-long-lines-test-hook-count
               #'atl-long-lines--start-timer
               post-command-hook)
              post-command-hook))))"##,
        expect!["OK (nil (t t t 1) t 1 (atl-long-lines--start-timer t))"],
    )
}

fn atl_long_lines_disabling_mode_removes_only_its_local_callback() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_disabling_mode_removes_only_its_local_callback",
        r##"(with-temp-buffer
         (let ((other
                (lambda () :other)))
           (add-hook
            'post-command-hook
            other
            nil
            t)
           (atl-long-lines-mode 1)
           (atl-long-lines-mode -1)
           (list
            atl-long-lines-mode
            (and
             (memq
              other
              post-command-hook)
             t)
            (atl-long-lines-test-hook-count
             #'atl-long-lines--start-timer
             post-command-hook)
            (local-variable-p
             'post-command-hook)
            (length
             (delq
              t
              (copy-sequence
               post-command-hook))))))"##,
        expect!["OK (nil t 0 t 1)"],
    )
}

fn atl_long_lines_mode_hook_sequence_for_repeated_enable_and_disable_matches() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_mode_hook_sequence_for_repeated_enable_and_disable_matches",
        r##"(with-temp-buffer
         (let ((transitions nil))
           (add-hook
            'atl-long-lines-mode-hook
            (lambda ()
              (push
               atl-long-lines-mode
               transitions))
            nil
            t)
           (atl-long-lines-mode 1)
           (atl-long-lines-mode 1)
           (atl-long-lines-mode -1)
           (atl-long-lines-mode -1)
           (list
            (nreverse transitions)
            atl-long-lines-mode
            (atl-long-lines-test-hook-count
             #'atl-long-lines--start-timer
             post-command-hook))))"##,
        expect!["OK ((t t nil nil) nil 0)"],
    )
}

fn atl_long_lines_turn_on_helper_activates_only_the_current_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_turn_on_helper_activates_only_the_current_buffer",
        r##"(let ((first
                (generate-new-buffer
                 " *atl-first*"))
               (second
                (generate-new-buffer
                 " *atl-second*")))
         (unwind-protect
             (progn
               (with-current-buffer first
                 (atl-long-lines--turn-on-atl-long-lines-mode))
               (list
                (buffer-local-value
                 'atl-long-lines-mode
                 first)
                (buffer-local-value
                 'atl-long-lines-mode
                 second)
                (with-current-buffer first
                  (atl-long-lines-test-hook-count
                   #'atl-long-lines--start-timer
                   post-command-hook))
                (with-current-buffer second
                  (atl-long-lines-test-hook-count
                   #'atl-long-lines--start-timer
                   post-command-hook))))
           (kill-buffer first)
           (kill-buffer second)))"##,
        expect!["OK (t nil 1 0)"],
    )
}

fn atl_long_lines_minor_mode_state_and_hooks_are_independent_across_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "atl_long_lines_minor_mode_state_and_hooks_are_independent_across_buffers",
        r##"(let ((first
                (generate-new-buffer
                 " *atl-one*"))
               (second
                (generate-new-buffer
                 " *atl-two*")))
         (unwind-protect
             (progn
               (with-current-buffer first
                 (atl-long-lines-mode 1))
               (with-current-buffer second
                 (atl-long-lines-mode 1)
                 (atl-long-lines-mode -1))
               (list
                (buffer-local-value
                 'atl-long-lines-mode
                 first)
                (buffer-local-value
                 'atl-long-lines-mode
                 second)
                (with-current-buffer first
                  (atl-long-lines-test-hook-count
                   #'atl-long-lines--start-timer
                   post-command-hook))
                (with-current-buffer second
                  (atl-long-lines-test-hook-count
                   #'atl-long-lines--start-timer
                   post-command-hook))))
           (kill-buffer first)
           (kill-buffer second)))"##,
        expect!["OK (t nil 1 0)"],
    )
}

fn global_atl_long_lines_mode_updates_existing_ordinary_buffers_and_cleans_up() -> ParityBatchCase {
    ParityBatchCase::value(
        "global_atl_long_lines_mode_updates_existing_ordinary_buffers_and_cleans_up",
        r##"(let ((first
                (generate-new-buffer
                 "atl-global-one"))
               (second
                (generate-new-buffer
                 "atl-global-two")))
         (unwind-protect
             (progn
               (with-current-buffer first
                 (fundamental-mode))
               (with-current-buffer second
                 (text-mode))
               (global-atl-long-lines-mode 1)
               (let ((enabled
                      (list
                       global-atl-long-lines-mode
                       (buffer-local-value
                        'atl-long-lines-mode
                        first)
                       (buffer-local-value
                        'atl-long-lines-mode
                        second))))
                 (global-atl-long-lines-mode -1)
                 (list
                  enabled
                  global-atl-long-lines-mode
                  (buffer-local-value
                   'atl-long-lines-mode
                   first)
                  (buffer-local-value
                   'atl-long-lines-mode
                   second)
                  (with-current-buffer first
                    (atl-long-lines-test-hook-count
                     #'atl-long-lines--start-timer
                     post-command-hook))
                  (with-current-buffer second
                    (atl-long-lines-test-hook-count
                     #'atl-long-lines--start-timer
                     post-command-hook)))))
           (global-atl-long-lines-mode -1)
           (kill-buffer first)
           (kill-buffer second)))"##,
        expect!["OK ((t t t) nil nil nil 0 0)"],
    )
}

fn global_atl_long_lines_mode_activates_buffers_created_after_global_enable() -> ParityBatchCase {
    ParityBatchCase::value(
        "global_atl_long_lines_mode_activates_buffers_created_after_global_enable",
        r##"(let (created)
         (unwind-protect
             (progn
               (global-atl-long-lines-mode 1)
               (setq
                created
                (generate-new-buffer
                 "atl-global-future"))
               (with-current-buffer created
                 (fundamental-mode))
               (list
                global-atl-long-lines-mode
                (buffer-local-value
                 'atl-long-lines-mode
                 created)
                (with-current-buffer created
                  (atl-long-lines-test-hook-count
                   #'atl-long-lines--start-timer
                   post-command-hook))))
           (global-atl-long-lines-mode -1)
           (when
               (buffer-live-p created)
             (kill-buffer created))))"##,
        expect!["OK (t t 1)"],
    )
}

pub(super) fn modes_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        atl_long_lines_minor_mode_initial_metadata_lighter_keymap_and_hook_state_match(),
        atl_long_lines_enabling_mode_installs_one_buffer_local_post_command_callback_idempotently(),
        atl_long_lines_disabling_mode_removes_only_its_local_callback(),
        atl_long_lines_mode_hook_sequence_for_repeated_enable_and_disable_matches(),
        atl_long_lines_turn_on_helper_activates_only_the_current_buffer(),
        atl_long_lines_minor_mode_state_and_hooks_are_independent_across_buffers(),
        global_atl_long_lines_mode_updates_existing_ordinary_buffers_and_cleans_up(),
        global_atl_long_lines_mode_activates_buffers_created_after_global_enable(),
    ]
}
