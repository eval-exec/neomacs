use expect_test::expect;

use super::ParityBatchCase;

fn repeated_vertical_navigation_moves_the_window_then_recommends_a_larger_jump() -> ParityBatchCase
{
    ParityBatchCase::value(
        "repeated_vertical_navigation_moves_the_window_then_recommends_a_larger_jump",
        r##"(let ((buffer (generate-new-buffer " *annoying-navigation*"))
               (annoying-arrows-too-far-count 2)
               (annoying-arrows--current-count 0)
               (bells 0)
               messages)
         (unwind-protect
             (save-window-excursion
               (switch-to-buffer buffer)
               (insert "alpha\nbeta\ngamma\ndelta\nepsilon\n")
               (goto-char (point-max))
               (set-window-start (selected-window) (point-min))
               (annoying-arrows-mode 1)
               (setq this-command nil
                     last-command nil)
               (message nil)
               (let ((real-message (symbol-function 'message)))
                 (cl-letf (((symbol-function 'beep)
                            (lambda (&optional _arg)
                              (setq bells (1+ bells))))
                           ((symbol-function 'random)
                            (lambda (&optional _limit) 0))
                           ((symbol-function 'message)
                            (lambda (format-string &rest args)
                              (if (null format-string)
                                  (funcall real-message nil)
                                (let ((rendered
                                       (apply #'format
                                              format-string args)))
                                  (push rendered messages)
                                  (apply real-message
                                         format-string args))))))
                   (let ((before
                          (list (line-number-at-pos)
                                (point)
                                (window-point (selected-window))
                                (window-start (selected-window)))))
                     (execute-kbd-macro [up up up up])
                     (let ((repeated-navigation
                            (list (line-number-at-pos)
                                  (point)
                                  (window-point (selected-window))
                                  (window-start (selected-window))
                                  last-command
                                  (car messages)
                                  bells)))
                       (setq messages nil)
                       (message nil)
                       (execute-kbd-macro [right down])
                       (let ((changed-direction
                              (list (line-number-at-pos)
                                    (point)
                                    last-command
                                    (car messages)
                                    bells)))
                         (annoying-arrows-mode -1)
                         (setq messages nil)
                         (message nil)
                         (execute-kbd-macro [down down])
                         (list before
                               repeated-navigation
                               changed-direction
                               (list (line-number-at-pos)
                                     (point)
                                     (window-point (selected-window))
                                     (window-start (selected-window))
                                     last-command
                                     annoying-arrows-mode
                                     (car messages)
                                     bells))))))))
           (when (buffer-live-p buffer)
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK ((6 32 32 1) (2 7 7 1 previous-line #("Annoying! How about using backward-paragraph (M-{) instead?" 46 49 (face help-key-binding font-lock-face help-key-binding)) 1) (3 13 next-line "" 1) (5 25 25 1 next-line nil "" 1))"#
        ]],
    )
}

fn repeated_deletion_warns_while_a_real_typo_correction_keeps_editing_normally() -> ParityBatchCase
{
    ParityBatchCase::value(
        "repeated_deletion_warns_while_a_real_typo_correction_keeps_editing_normally",
        r##"(let ((buffer (generate-new-buffer " *annoying-editing*"))
               (annoying-arrows-too-far-count 0)
               (annoying-arrows--current-count 0)
               (bells 0)
               messages)
         (unwind-protect
             (save-window-excursion
               (switch-to-buffer buffer)
               (insert "release: readxy!!")
               (goto-char (point-max))
               (use-local-map (make-sparse-keymap))
               (local-set-key [backspace]
                              #'backward-delete-char-untabify)
               (annoying-arrows-mode 1)
               (setq this-command nil
                     last-command nil)
               (message nil)
               (let ((real-message (symbol-function 'message)))
                 (cl-letf (((symbol-function 'beep)
                            (lambda (&optional _arg)
                              (setq bells (1+ bells))))
                           ((symbol-function 'random)
                            (lambda (&optional _limit) 0))
                           ((symbol-function 'message)
                            (lambda (format-string &rest args)
                              (if (null format-string)
                                  (funcall real-message nil)
                                (let ((rendered
                                       (apply #'format
                                              format-string args)))
                                  (push rendered messages)
                                  (apply real-message
                                         format-string args))))))
                   (execute-kbd-macro [backspace backspace])
                   (let ((removed-punctuation
                          (list (buffer-string)
                                (point)
                                (current-column)
                                last-command
                                (substring-no-properties
                                 (or (car messages) ""))
                                bells)))
                     (setq annoying-arrows-too-far-count 99
                           messages nil)
                     (message nil)
                     (execute-kbd-macro [left backspace right])
                     (let ((corrected-typo
                            (list (buffer-string)
                                  (point)
                                  (current-column)
                                  last-command
                                  (car messages)
                                  bells)))
                       (annoying-arrows-mode -1)
                       (execute-kbd-macro [left left])
                       (list removed-punctuation
                             corrected-typo
                             (list (buffer-string)
                                   (point)
                                   (current-column)
                                   last-command
                                   annoying-arrows-mode
                                   bells)))))))
           (when (buffer-live-p buffer)
             (kill-buffer buffer))))"##,
        expect![[
            r#"OK (("release: readxy" 16 15 backward-delete-char-untabify "Annoying! How about using backward-kill-word (M-DEL) instead?" 1) ("release: ready" 15 14 right-char "" 1) ("release: ready" 13 12 left-char nil 1))"#
        ]],
    )
}

fn global_mode_and_a_public_suggestion_drive_navigation_across_two_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "global_mode_and_a_public_suggestion_drive_navigation_across_two_buffers",
        r##"(let ((first (generate-new-buffer " *annoying-plan-a*"))
               (second (generate-new-buffer " *annoying-plan-b*"))
               (old-alternatives (get 'next-line 'annoying-arrows--alts))
               (annoying-arrows-too-far-count 1)
               (annoying-arrows--current-count 0)
               (bells 0)
               messages)
         (unwind-protect
             (save-window-excursion
               (dolist (buffer (list first second))
                 (with-current-buffer buffer
                   (insert "todo\nactive\nreview\ndone\n")
                   (goto-char (point-min))))
               (aa-add-suggestion 'next-line 'beginning-of-buffer)
               (global-annoying-arrows-mode 1)
               (switch-to-buffer first)
               (setq this-command nil
                     last-command nil)
               (message nil)
               (let ((real-message (symbol-function 'message)))
                 (cl-letf (((symbol-function 'beep)
                            (lambda (&optional _arg)
                              (setq bells (1+ bells))))
                           ((symbol-function 'random)
                            (lambda (&optional _limit) 0))
                           ((symbol-function 'message)
                            (lambda (format-string &rest args)
                              (if (null format-string)
                                  (funcall real-message nil)
                                (let ((rendered
                                       (apply #'format
                                              format-string args)))
                                  (push rendered messages)
                                  (apply real-message
                                         format-string args))))))
                   (execute-kbd-macro [down down down])
                   (let ((first-navigation
                          (list (buffer-name (window-buffer
                                             (selected-window)))
                                (line-number-at-pos)
                                (point)
                                (window-point (selected-window))
                                last-command
                                (substring-no-properties
                                 (or (car messages) ""))
                                bells)))
                     (switch-to-buffer second)
                     (setq messages nil)
                     (message nil)
                     (execute-kbd-macro [down])
                     (let ((second-navigation
                            (list (buffer-name (window-buffer
                                               (selected-window)))
                                  (line-number-at-pos)
                                  (point)
                                  (window-point (selected-window))
                                  last-command
                                  (substring-no-properties
                                   (or (car messages) ""))
                                  bells)))
                       (global-annoying-arrows-mode -1)
                       (setq messages nil)
                       (message nil)
                       (execute-kbd-macro [down down])
                       (list first-navigation
                             second-navigation
                             (list global-annoying-arrows-mode
                                   (buffer-local-value
                                    'annoying-arrows-mode first)
                                   (buffer-local-value
                                    'annoying-arrows-mode second)
                                   (buffer-name (window-buffer
                                                 (selected-window)))
                                   (line-number-at-pos)
                                   (point)
                                   (window-point (selected-window))
                                   last-command
                                   (car messages)
                                   bells)))))))
           (global-annoying-arrows-mode -1)
           (put 'next-line 'annoying-arrows--alts old-alternatives)
           (when (buffer-live-p first)
             (kill-buffer first))
           (when (buffer-live-p second)
             (kill-buffer second))))"##,
        expect![[
            r#"OK ((" *annoying-plan-a*" 4 20 20 next-line "Annoying! How about using beginning-of-buffer (M-<) instead?" 1) (" *annoying-plan-b*" 2 6 6 next-line "" 1) (nil nil nil " *annoying-plan-b*" 4 20 20 next-line "" 1))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        repeated_vertical_navigation_moves_the_window_then_recommends_a_larger_jump(),
        repeated_deletion_warns_while_a_real_typo_correction_keeps_editing_normally(),
        global_mode_and_a_public_suggestion_drive_navigation_across_two_buffers(),
    ]
}
