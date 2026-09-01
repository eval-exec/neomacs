use expect_test::expect;

use super::ParityBatchCase;

fn ascii_table_set_base_normalizes_inputs_and_refreshes_once_per_change() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_set_base_normalizes_inputs_and_refreshes_once_per_change",
        r##"(let ((ascii-table-base 16)
               calls
               results)
         (cl-letf
             (((symbol-function
                'ascii-table--revert-if-active)
               (lambda ()
                 (push ascii-table-base calls)
                 :refreshed)))
           (dolist (value
                    '(2 8 10 16 3 0 -1 nil caret "16"))
             (push
              (list
               value
               (ascii-table--set-base value)
               ascii-table-base)
              results)))
         (list
          (nreverse results)
          (nreverse calls)))"##,
        expect![[
            r#"OK (((2 :refreshed 2) (8 :refreshed 8) (10 :refreshed 10) (16 :refreshed 16) (3 :refreshed 10) (0 :refreshed 10) (-1 :refreshed 10) (nil :refreshed 10) (caret :refreshed 10) ("16" :refreshed 10)) (2 8 10 16 10 10 10 10 10 10))"#
        ]],
    )
}

fn ascii_table_radix_commands_are_interactive_and_delegate_exact_supported_values()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_radix_commands_are_interactive_and_delegate_exact_supported_values",
        r##"(let (calls)
         (cl-letf
             (((symbol-function
                'ascii-table--set-base)
               (lambda (base)
                 (push base calls)
                 (list :selected base))))
           (list
            (call-interactively
             #'ascii-table-base-binary)
            (call-interactively
             #'ascii-table-base-octal)
            (call-interactively
             #'ascii-table-base-decimal)
            (call-interactively
             #'ascii-table-base-hex)
            (nreverse calls))))"##,
        expect!["OK ((:selected 2) (:selected 8) (:selected 10) (:selected 16) (2 8 10 16))"],
    )
}

fn ascii_table_toggle_control_cycles_false_and_all_truthy_states_and_refreshes() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ascii_table_toggle_control_cycles_false_and_all_truthy_states_and_refreshes",
        r##"(let (calls results)
         (cl-letf
             (((symbol-function
                'ascii-table--revert-if-active)
               (lambda ()
                 (push ascii-table-control calls)
                 :refreshed)))
           (dolist (initial
                    '(nil caret t 1 "yes"))
             (let ((ascii-table-control initial))
               (push
                (list
                 initial
                 (ascii-table-toggle-control)
                 ascii-table-control
                 (ascii-table-toggle-control)
                 ascii-table-control)
                results))))
         (list
          (nreverse results)
          (nreverse calls)))"##,
        expect![[
            r#"OK (((nil :refreshed caret :refreshed nil) (caret :refreshed nil :refreshed caret) (t :refreshed nil :refreshed caret) (1 :refreshed nil :refreshed caret) ("yes" :refreshed nil :refreshed caret)) (caret nil nil caret nil caret nil caret nil caret))"#
        ]],
    )
}

fn ascii_table_toggle_escape_uses_boolean_not_semantics_and_refreshes() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_toggle_escape_uses_boolean_not_semantics_and_refreshes",
        r##"(let (calls results)
         (cl-letf
             (((symbol-function
                'ascii-table--revert-if-active)
               (lambda ()
                 (push ascii-table-escape calls)
                 :refreshed)))
           (dolist (initial
                    '(nil t caret 0 "yes"))
             (let ((ascii-table-escape initial))
               (push
                (list
                 initial
                 (ascii-table-toggle-escape)
                 ascii-table-escape
                 (ascii-table-toggle-escape)
                 ascii-table-escape)
                results))))
         (list
          (nreverse results)
          (nreverse calls)))"##,
        expect![[
            r#"OK (((nil :refreshed t :refreshed nil) (t :refreshed nil :refreshed t) (caret :refreshed nil :refreshed t) (0 :refreshed nil :refreshed t) ("yes" :refreshed nil :refreshed t)) (t nil nil t nil t nil t nil t))"#
        ]],
    )
}

fn ascii_table_revert_if_active_is_noop_without_display_buffer_and_refreshes_named_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_revert_if_active_is_noop_without_display_buffer_and_refreshes_named_buffer",
        r##"(let ((existing
                (get-buffer "*ASCII*"))
               calls
               absent-result
               active-result)
         (when existing
           (kill-buffer existing))
         (setq absent-result
               (ascii-table--revert-if-active))
         (let ((buffer
                (get-buffer-create "*ASCII*")))
           (unwind-protect
               (progn
                 (with-current-buffer buffer
                   (fundamental-mode)
                   (insert "fixture"))
                 (cl-letf
                     (((symbol-function
                        'ascii-table--revert)
                       (lambda (&rest arguments)
                         (push
                          (list
                           (current-buffer)
                           (buffer-name)
                           major-mode
                           (buffer-string)
                           arguments)
                          calls)
                         :reverted)))
                   (setq active-result
                         (ascii-table--revert-if-active)))
                 (list
                  absent-result
                  active-result
                  (mapcar
                   (lambda (entry)
                     (cons
                      (eq buffer (car entry))
                      (cdr entry)))
                   (nreverse calls))
                  (with-current-buffer buffer
                    (buffer-string))))
             (kill-buffer buffer))))"##,
        expect![[r#"OK (nil :reverted ((t "*ASCII*" fundamental-mode "fixture" nil)) "fixture")"#]],
    )
}

fn ascii_table_real_command_sequence_rerenders_same_buffer_across_all_modes() -> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_real_command_sequence_rerenders_same_buffer_across_all_modes",
        r##"(let ((buffer
                (generate-new-buffer
                 " *ascii-table-command-sequence*"))
               (ascii-table-base 16)
               (ascii-table-control nil)
               (ascii-table-escape nil)
               snapshots)
         (unwind-protect
             (with-current-buffer buffer
               (rename-buffer "*ASCII*" t)
               (cl-letf
                   (((symbol-function
                      'ascii-table--width-limit)
                     (lambda () 90)))
                 (ascii-table-mode)
                 (dolist (command
                          '(ascii-table-base-binary
                            ascii-table-base-octal
                            ascii-table-base-decimal
                            ascii-table-base-hex
                            ascii-table-toggle-control
                            ascii-table-toggle-escape
                            ascii-table-toggle-control
                            ascii-table-toggle-escape))
                   (funcall command)
                   (push
                    (list
                     command
                     ascii-table-base
                     ascii-table-control
                     ascii-table-escape
                     (point)
                     (line-number-at-pos
                      (point-max))
                     (secure-hash
                      'sha256
                      (buffer-string))
                     (length
                      (overlays-in
                       (point-min)
                       (point-max))))
                    snapshots)))
               (nreverse snapshots))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK ((ascii-table-base-binary 2 nil nil 1 25 "344a0c231cd072fadb0317786488acf41a4ec96dcad2b865efa5e7cd2f1d0046" 520) (ascii-table-base-octal 8 nil nil 1 19 "e2519d17b65982239fb2c15e4477ac277fbba4bc8bad1eab5e740d24cd9595cb" 776) (ascii-table-base-decimal 10 nil nil 1 19 "caabf05c29a5ef6f9e7401da13a594a7a830e9486ff074d60aceaafae4e423ef" 1032) (ascii-table-base-hex 16 nil nil 1 19 "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25" 1288) (ascii-table-toggle-control 16 caret nil 1 19 "65ca8d45a9597358d9988b42392152b5493b8dc0f2bcb78e09585421ae9ef30d" 1544) (ascii-table-toggle-escape 16 caret t 1 19 "d6d85406d481f6ac0af7fc5f7fdd25a280315873f8ef8d8a2b7a1d5137d0c26a" 1800) (ascii-table-toggle-control 16 nil t 1 19 "0a5ce70f185e38aec3ae97286eb3fd70dc82ef13e59d45e0eaa18b4525f69a5e" 2056) (ascii-table-toggle-escape 16 nil nil 1 19 "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25" 2312))"#
        ]],
    )
}

fn ascii_table_mode_key_bindings_drive_practical_navigation_and_rendering_workflow()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_mode_key_bindings_drive_practical_navigation_and_rendering_workflow",
        r##"(let ((buffer
                (generate-new-buffer
                 " *ascii-table-key-workflow*"))
               (ascii-table-base 16)
               (ascii-table-control nil)
               (ascii-table-escape nil)
               states)
         (unwind-protect
             (with-current-buffer buffer
               (rename-buffer "*ASCII*" t)
               (cl-letf
                   (((symbol-function
                      'ascii-table--width-limit)
                     (lambda () 90)))
                 (ascii-table-mode)
                 (dolist (key
                          '("b" "o" "d" "x"
                            "TAB" "e" "TAB" "e"))
                   (let ((command
                          (key-binding
                           (kbd key)
                           t)))
                     (call-interactively command)
                     (push
                      (list
                       key
                       command
                       ascii-table-base
                       ascii-table-control
                       ascii-table-escape
                       (buffer-substring-no-properties
                        (point-min)
                        (line-end-position))
                       (secure-hash
                        'sha256
                        (buffer-string)))
                      states))))
               (nreverse states))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK (("b" ascii-table-base-binary 2 nil nil "ASCII Table (binary)" "344a0c231cd072fadb0317786488acf41a4ec96dcad2b865efa5e7cd2f1d0046") ("o" ascii-table-base-octal 8 nil nil "ASCII Table (octal)" "e2519d17b65982239fb2c15e4477ac277fbba4bc8bad1eab5e740d24cd9595cb") ("d" ascii-table-base-decimal 10 nil nil "ASCII Table (decimal)" "caabf05c29a5ef6f9e7401da13a594a7a830e9486ff074d60aceaafae4e423ef") ("x" ascii-table-base-hex 16 nil nil "ASCII Table (hex)" "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25") ("TAB" ascii-table-toggle-control 16 caret nil "ASCII Table (hex)" "65ca8d45a9597358d9988b42392152b5493b8dc0f2bcb78e09585421ae9ef30d") ("e" ascii-table-toggle-escape 16 caret t "ASCII Table (hex)" "d6d85406d481f6ac0af7fc5f7fdd25a280315873f8ef8d8a2b7a1d5137d0c26a") ("TAB" ascii-table-toggle-control 16 nil t "ASCII Table (hex)" "0a5ce70f185e38aec3ae97286eb3fd70dc82ef13e59d45e0eaa18b4525f69a5e") ("e" ascii-table-toggle-escape 16 nil nil "ASCII Table (hex)" "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25"))"#
        ]],
    )
}

fn ascii_table_inherited_navigation_and_revert_keys_move_through_real_rendered_table()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_inherited_navigation_and_revert_keys_move_through_real_rendered_table",
        r##"(with-temp-buffer
         (let ((ascii-table-base 16)
               (ascii-table-control nil)
               (ascii-table-escape nil)
               states)
           (cl-letf
               (((symbol-function
                  'ascii-table--width-limit)
                 (lambda () 90)))
             (ascii-table-mode)
             (forward-line 7)
             (push
              (list
               :initial
               (point)
               (line-number-at-pos)
               (buffer-substring-no-properties
                (line-beginning-position)
                (line-end-position)))
              states)
             (dolist (key '(">" "<" "g"))
               (let ((command
                      (key-binding
                       (kbd key)
                       t)))
                 (call-interactively command)
                 (push
                  (list
                   key
                   command
                   (point)
                   (line-number-at-pos)
                   (buffer-substring-no-properties
                    (line-beginning-position)
                    (line-end-position)))
                  states)))
             (nreverse states))))"##,
        expect![[
            r#"OK ((:initial 325 8 "05  ENQ  15  NAK  25  %  35  5  45  E  55  U  65  e  75  u  ") (">" end-of-buffer 996 19 "") ("<" beginning-of-buffer 1 1 "ASCII Table (hex)") ("g" revert-buffer 1 1 "ASCII Table (hex)"))"#
        ]],
    )
}

fn ascii_table_display_command_creates_selects_and_initializes_exact_named_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_display_command_creates_selects_and_initializes_exact_named_buffer",
        r##"(let ((existing
                (get-buffer "*ASCII*"))
               calls
               result)
         (when existing
           (kill-buffer existing))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'switch-to-buffer-other-window)
                   (lambda (buffer-or-name &rest arguments)
                     (let ((buffer
                            (get-buffer-create
                             buffer-or-name)))
                       (push
                        (list
                         buffer-or-name
                         arguments
                         (buffer-name buffer))
                        calls)
                       (set-buffer buffer)
                       buffer)))
                  ((symbol-function
                    'ascii-table--width-limit)
                   (lambda () 90)))
               (setq result
                     (call-interactively
                      #'ascii-table))
               (list
                result
                (nreverse calls)
                (buffer-name)
                (eq
                 (current-buffer)
                 (get-buffer "*ASCII*"))
                major-mode
                mode-name
                buffer-read-only
                revert-buffer-function
                (point)
                (buffer-substring-no-properties
                 (point-min)
                 (line-end-position))
                (secure-hash
                 'sha256
                 (buffer-string))))
           (when-let ((buffer
                       (get-buffer "*ASCII*")))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK (nil (((:buffer nil) nil "*ASCII*")) "*ASCII*" t ascii-table-mode "ASCII" t ascii-table--revert 1 "ASCII Table (hex)" "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25")"#
        ]],
    )
}

fn ascii_table_display_command_reuses_existing_buffer_and_erases_previous_contents()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_display_command_reuses_existing_buffer_and_erases_previous_contents",
        r##"(let ((buffer
                (get-buffer-create "*ASCII*"))
               calls)
         (unwind-protect
             (progn
               (with-current-buffer buffer
                 (fundamental-mode)
                 (insert "stale contents")
                 (goto-char 6))
               (cl-letf
                   (((symbol-function
                      'switch-to-buffer-other-window)
                     (lambda (buffer-or-name &rest _arguments)
                       (let ((selected
                              (get-buffer-create
                               buffer-or-name)))
                         (push
                          (eq selected buffer)
                          calls)
                         (set-buffer selected)
                         selected)))
                    ((symbol-function
                      'ascii-table--width-limit)
                     (lambda () 90)))
                 (ascii-table)
                 (let ((first-hash
                        (secure-hash
                         'sha256
                         (buffer-string))))
                   (goto-char
                    (point-max))
                   (let ((inhibit-read-only t))
                     (insert "corruption"))
                   (ascii-table)
                   (list
                    (nreverse calls)
                    (eq
                     (current-buffer)
                     buffer)
                    major-mode
                    (point)
                    (string-match-p
                     "stale\\|corruption"
                     (buffer-string))
                    first-hash
                    (secure-hash
                     'sha256
                     (buffer-string))))))
           (kill-buffer buffer)))"##,
        expect![[
            r#"OK ((t t) t ascii-table-mode 1 nil "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25" "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25")"#
        ]],
    )
}

fn ascii_table_independent_buffers_share_global_preferences_but_keep_mode_state_local()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ascii_table_independent_buffers_share_global_preferences_but_keep_mode_state_local",
        r##"(let ((first
                (generate-new-buffer
                 " *ascii-table-first*"))
               (second
                (generate-new-buffer
                 " *ascii-table-second*"))
               (ascii-table-base 16)
               (ascii-table-control nil)
               (ascii-table-escape nil))
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'ascii-table--width-limit)
                   (lambda () 90)))
               (with-current-buffer first
                 (ascii-table-mode))
               (with-current-buffer second
                 (ascii-table-mode))
               (with-current-buffer first
                 (ascii-table-base-decimal))
               (with-current-buffer second
                 (ascii-table-toggle-control))
               (list
                ascii-table-base
                ascii-table-control
                ascii-table-escape
                (with-current-buffer first
                  (list
                   major-mode
                   (local-variable-p
                    'ascii-table-base)
                   (local-variable-p
                    'ascii-table-control)
                   (secure-hash
                    'sha256
                    (buffer-string))))
                (with-current-buffer second
                  (list
                   major-mode
                   (local-variable-p
                    'ascii-table-base)
                   (local-variable-p
                    'ascii-table-control)
                   (secure-hash
                    'sha256
                    (buffer-string))))))
           (kill-buffer first)
           (kill-buffer second)))"##,
        expect![[
            r#"OK (10 caret nil (ascii-table-mode nil nil "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25") (ascii-table-mode nil nil "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25"))"#
        ]],
    )
}

fn ascii_table_autoloaded_public_command_loads_feature_then_initializes_display() -> ParityBatchCase
{
    ParityBatchCase::value(
        "ascii_table_autoloaded_public_command_loads_feature_then_initializes_display",
        r##"(let ((before
                (list
                 (featurep 'ascii-table)
                 (autoloadp
                  (symbol-function
                   'ascii-table))))
               calls)
         (unwind-protect
             (cl-letf
                 (((symbol-function
                    'switch-to-buffer-other-window)
                   (lambda (buffer-or-name &rest _arguments)
                     (push buffer-or-name calls)
                     (set-buffer
                      (get-buffer-create
                       buffer-or-name))))
                  ((symbol-function
                    'window-width)
                   (lambda (&optional _window)
                     90)))
               (call-interactively #'ascii-table)
               (list
                before
                (featurep 'ascii-table)
                (autoloadp
                 (symbol-function
                  'ascii-table))
                (nreverse calls)
                (buffer-name)
                major-mode
                (secure-hash
                 'sha256
                 (buffer-string))))
           (when-let ((buffer
                       (get-buffer "*ASCII*")))
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((nil t) t nil ((:buffer nil)) "*ASCII*" ascii-table-mode "a7b33d4144b327ae7701b2011d98b0cb84434a0643791e317aa4510639d3ac25")"#
        ]],
    )
}

pub(super) fn commands_ascii_table_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ascii_table_set_base_normalizes_inputs_and_refreshes_once_per_change(),
        ascii_table_radix_commands_are_interactive_and_delegate_exact_supported_values(),
        ascii_table_toggle_control_cycles_false_and_all_truthy_states_and_refreshes(),
        ascii_table_toggle_escape_uses_boolean_not_semantics_and_refreshes(),
        ascii_table_revert_if_active_is_noop_without_display_buffer_and_refreshes_named_buffer(),
        ascii_table_real_command_sequence_rerenders_same_buffer_across_all_modes(),
        ascii_table_mode_key_bindings_drive_practical_navigation_and_rendering_workflow(),
        ascii_table_inherited_navigation_and_revert_keys_move_through_real_rendered_table(),
        ascii_table_display_command_creates_selects_and_initializes_exact_named_buffer(),
        ascii_table_display_command_reuses_existing_buffer_and_erases_previous_contents(),
        ascii_table_independent_buffers_share_global_preferences_but_keep_mode_state_local(),
    ]
}

pub(super) fn commands_ascii_table_autoload_batch_cases() -> Vec<ParityBatchCase> {
    vec![ascii_table_autoloaded_public_command_loads_feature_then_initializes_display()]
}
