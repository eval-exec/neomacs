use expect_test::expect;

use super::ParityBatchCase;

fn searching_deployment_logs_jumps_to_the_matching_text_and_records_the_origin() -> ParityBatchCase
{
    ParityBatchCase::value(
        "searching_deployment_logs_jumps_to_the_matching_text_and_records_the_origin",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (insert "09:00 queued release 184\n"
            "09:02 building release 184\n"
            "production-rollout-completed release=184 at 09:05\n"
            "09:06 health checks passing\n")
    (goto-char (point-min))
    (forward-line 1)
    (move-to-column 6)
    (let ((origin (point)))
      (local-set-key (kbd "C-c l") #'consult-line)
      (execute-kbd-macro
       (vconcat (kbd "C-c l") "production-rollout" (kbd "TAB RET")))
      (let ((history (car consult--line-history)))
        (list
         :origin origin
         :point (point)
         :line (line-number-at-pos)
         :column (current-column)
         :text (neomacs-consult-test-current-line)
         :mark (mark t)
         :history-text (consult--tofu-strip history)
         :history-suffix-properties
         (text-properties-at (1- (length history)) history))))))
"##,
        expect![[
            r##"OK (:origin 32 :point 53 :line 3 :column 0 :text "production-rollout-completed release=184 at 09:05" :mark 32 :history-text "production-rollout-completed release=184 at 09:05" :history-suffix-properties nil)"##
        ]],
    )
}

fn entering_a_line_and_column_moves_to_the_exact_runbook_location() -> ParityBatchCase {
    ParityBatchCase::value(
        "entering_a_line_and_column_moves_to_the_exact_runbook_location",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (insert "title: Release runbook\n"
            "owner: platform\n"
            "region: us-east-1\n"
            "command: deploy --environment production\n"
            "verify: check /health and /ready\n")
    (goto-char (point-min))
    (local-set-key (kbd "C-c g") #'consult-goto-line)
    (execute-kbd-macro (vconcat (kbd "C-c g") "4:9" (kbd "RET")))
    (list
     :point (point)
     :line (line-number-at-pos)
     :column (current-column)
     :text (neomacs-consult-test-current-line)
     :mark (mark t)
     :history (car goto-line-history))))
"##,
        expect![[
            r##"OK (:point 67 :line 4 :column 9 :text "command: deploy --environment production" :mark 1 :history "4:9")"##
        ]],
    )
}

fn choosing_an_imenu_function_jumps_to_its_definition_and_runs_the_jump_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "choosing_an_imenu_function_jumps_to_its_definition_and_runs_the_jump_hook",
        r##"
(with-temp-buffer
  (save-window-excursion
    (switch-to-buffer (current-buffer))
    (emacs-lisp-mode)
    (insert "(defun prepare-release (version)\n"
            "  (message \"Preparing %s\" version))\n\n"
            "(defun deploy-release (environment version)\n"
            "  (list :environment environment :version version))\n\n"
            "(defun verify-release (endpoint)\n"
            "  (string-prefix-p \"https://\" endpoint))\n")
    (goto-char (point-max))
    (let ((events nil))
      (add-hook 'imenu-after-jump-hook
                (lambda ()
                  (push (list :jumped-to (neomacs-consult-test-current-line)
                              :line (line-number-at-pos))
                        events))
                nil t)
      (local-set-key (kbd "C-c i") #'consult-imenu)
      (execute-kbd-macro
       (vconcat (kbd "C-c i")
                "Function" (kbd "C-q SPC") "deploy-release" (kbd "RET")))
      (list
       :point (point)
       :line (line-number-at-pos)
       :column (current-column)
       :text (neomacs-consult-test-current-line)
       :events (nreverse events)
       :mark (mark t)
       :history (car consult-imenu--history)))))
"##,
        expect![[
            r##"OK (:point 71 :line 4 :column 0 :text "(defun deploy-release (environment version)" :events ((:jumped-to "(defun deploy-release (environment version)" :line 4)) :mark 242 :history "Functions deploy-release")"##
        ]],
    )
}

fn selecting_a_named_application_buffer_switches_to_its_live_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "selecting_a_named_application_buffer_switches_to_its_live_state",
        r##"
(let ((dashboard (get-buffer-create "*consult-deployment-dashboard*"))
      (events (get-buffer-create "*consult-release-events*")))
  (unwind-protect
      (save-window-excursion
        (with-current-buffer dashboard
          (erase-buffer)
          (insert "release=184\nstatus=healthy\n")
          (goto-char (point-min)))
        (with-current-buffer events
          (erase-buffer)
          (insert "09:05 rollout completed\n09:06 health checks passing\n")
          (goto-char (point-max)))
        (switch-to-buffer dashboard)
        (let ((consult-buffer-list-function
               (lambda () (list dashboard events)))
              (consult-buffer-sources '(consult-source-buffer)))
          (local-set-key
           (kbd "C-c b")
           (lambda ()
             (interactive)
             (consult-buffer '(consult-source-buffer))))
          (execute-kbd-macro
           (vconcat (kbd "C-c b") "*consult-release-events*" (kbd "TAB RET")))
          (let ((history (car consult--buffer-history)))
            (list
             :buffer (buffer-name)
             :point (point)
             :line (line-number-at-pos)
             :text (buffer-substring-no-properties (point-min) (point-max))
             :history-text (consult--tofu-strip history)
             :history-suffix-properties
             (text-properties-at (1- (length history)) history)))))
    (when (buffer-live-p dashboard) (kill-buffer dashboard))
    (when (buffer-live-p events) (kill-buffer events))))
"##,
        expect![[
            r##"OK (:buffer "*consult-release-events*" :point 53 :line 3 :text "09:05 rollout completed\n09:06 health checks passing\n" :history-text "*consult-release-events*" :history-suffix-properties nil)"##
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        searching_deployment_logs_jumps_to_the_matching_text_and_records_the_origin(),
        entering_a_line_and_column_moves_to_the_exact_runbook_location(),
        choosing_an_imenu_function_jumps_to_its_definition_and_runs_the_jump_hook(),
        selecting_a_named_application_buffer_switches_to_its_live_state(),
    ]
}
