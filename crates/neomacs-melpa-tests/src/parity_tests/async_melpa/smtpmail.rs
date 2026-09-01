use expect_test::expect;

use super::ParityBatchCase;

fn current_smtpmail_registry_custom_group_and_hook_metadata_match_gnu_emacs() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_smtpmail_registry_custom_group_and_hook_metadata_match_gnu_emacs",
        r##"
(list
 (get 'smtpmail-async 'group-documentation)
 (get 'smtpmail-async 'custom-group)
 async-smtpmail-before-send-hook
 (documentation-property
  'async-smtpmail-before-send-hook
  'variable-documentation)
 (help-function-arglist 'async-smtpmail-send-it t))
"##,
        expect![[
            r#"OK ("Send e-mail with smtpmail.el asynchronously" nil nil "Hook running in the child emacs in ‘async-smtpmail-send-it’.\nIt is called just before calling ‘smtpmail-send-it’." nil)"#
        ]],
    )
}

fn current_smtpmail_send_captures_complete_message_environment_and_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_smtpmail_send_captures_complete_message_environment_and_completion",
        r##"
(with-temp-buffer
  (insert
   "From: sender@example.test\n"
   "To: Alice <alice@example.test>, bob@example.test\n"
   "Subject: Unicode λ and 日本語\n"
   "X-Fixture: folded\n continuation\n"
   "\n"
   "First line.\nSecond λ line.\n")
  (let (child callback messages)
    (cl-letf (((symbol-function 'async-start)
               (lambda (start finish)
                 (setq child start callback finish)
                 'fixture-mail-process))
              ((symbol-function 'message)
               (lambda (format-string &rest args)
                 (push (apply #'format format-string args) messages))))
      (let ((result (async-smtpmail-send-it)))
        (funcall callback :ignored)
        (let ((printed (prin1-to-string child)))
          (list
           result
           (car child)
           (string-match-p "sender@example.test" printed)
           (string-match-p "Alice <alice@example.test>, bob@example.test" printed)
           (string-match-p "Unicode λ and 日本語" printed)
           (string-match-p "Second λ line" printed)
           (string-match-p "async-smtpmail-before-send-hook" printed)
           (string-match-p "smtpmail-send-it" printed)
           (nreverse messages)))))))
"##,
        expect![[
            r#"OK (fixture-mail-process lambda 65 90 145 213 3501 9021 ("Delivering message to Alice <alice@example.test>, bob@example.test..." "Delivering message to Alice <alice@example.test>, bob@example.test...done"))"#
        ]],
    )
}

fn current_smtpmail_child_recreates_unibyte_buffer_runs_hook_then_sends() -> ParityBatchCase {
    ParityBatchCase::value(
        "current_smtpmail_child_recreates_unibyte_buffer_runs_hook_then_sends",
        r##"
(with-temp-buffer
  (insert
   "From: sender@example.test\n"
   "To: recipient@example.test\n"
   "Subject: fixture\n\n"
   "ASCII wire payload\r\n")
  (let (child callback events fixture-injected)
    (cl-letf (((symbol-function 'async-inject-variables)
               (lambda (&rest _)
                 '(setq fixture-injected '(copied environment))))
              ((symbol-function 'async-start)
               (lambda (start finish)
                 (setq child start callback finish)
                 'fixture-mail-process)))
      (async-smtpmail-send-it))
    (let ((async-smtpmail-before-send-hook
           (list
            (lambda ()
              (push
               (list 'hook
                     (buffer-string)
                     enable-multibyte-characters
                     fixture-injected)
               events)))))
      (cl-letf (((symbol-function 'smtpmail-send-it)
                 (lambda ()
                   (push
                    (list 'send
                          (buffer-string)
                          enable-multibyte-characters
                          fixture-injected)
                    events)
                   :sent)))
        (let ((result (funcall child)))
          (list
           result
           (nreverse events)
           (functionp callback)
           (buffer-name)))))))
"##,
        expect![[
            r#"OK (:sent ((hook "From: sender@example.test\nTo: recipient@example.test\nSubject: fixture\n\nASCII wire payload\15\n" nil nil) (send "From: sender@example.test\nTo: recipient@example.test\nSubject: fixture\n\nASCII wire payload\15\n" nil nil)) t " *temp*")"#
        ]],
    )
}

pub(super) fn smtpmail_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        current_smtpmail_registry_custom_group_and_hook_metadata_match_gnu_emacs(),
        current_smtpmail_send_captures_complete_message_environment_and_completion(),
        current_smtpmail_child_recreates_unibyte_buffer_runs_hook_then_sends(),
    ]
}
