use expect_test::expect;

use super::ParityBatchCase;

fn auto_org_md_on_installs_one_local_after_save_hook_and_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_on_installs_one_local_after_save_hook_and_message",
        r##"(with-temp-buffer
         (let (messages)
           (cl-letf (((symbol-function 'message)
                      (lambda (format-string &rest arguments)
                        (push
                         (apply #'format
                                format-string
                                arguments)
                         messages))))
             (list
              (auto-org-md-on)
              (local-variable-p 'after-save-hook)
              (memq 'auto-org-md-export
                    after-save-hook)
              (auto-org-md-test-hook-count
               'auto-org-md-export
               after-save-hook)
              (nreverse messages)))))"##,
        expect![[r#"OK (#1=("auto-org-md-mode is on.") t (auto-org-md-export t) 1 #1#)"#]],
    )
}

fn auto_org_md_off_removes_local_hook_and_reports_message() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_off_removes_local_hook_and_reports_message",
        r##"(with-temp-buffer
         (let (messages)
           (add-hook
            'after-save-hook
            #'auto-org-md-export
            nil t)
           (cl-letf (((symbol-function 'message)
                      (lambda (format-string &rest arguments)
                        (push
                         (apply #'format
                                format-string
                                arguments)
                         messages))))
             (list
              (auto-org-md-off)
              (local-variable-p 'after-save-hook)
              (memq 'auto-org-md-export
                    after-save-hook)
              (nreverse messages)))))"##,
        expect![[r#"OK (#1=("auto-org-md-mode is off.") nil nil #1#)"#]],
    )
}

fn auto_org_md_on_is_idempotent_for_hook_registration() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_on_is_idempotent_for_hook_registration",
        r##"(with-temp-buffer
         (cl-letf (((symbol-function 'message)
                    (lambda (&rest _arguments) nil)))
           (auto-org-md-on)
           (auto-org-md-on)
           (auto-org-md-on)
           (list
            (auto-org-md-test-hook-count
             'auto-org-md-export
             after-save-hook)
            after-save-hook)))"##,
        expect!["OK (1 (auto-org-md-export t))"],
    )
}

fn auto_org_md_hook_is_buffer_local_and_does_not_leak_to_sibling() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_hook_is_buffer_local_and_does_not_leak_to_sibling",
        r##"(let ((first (generate-new-buffer " *auto-org-first*"))
         (second (generate-new-buffer " *auto-org-second*")))
         (unwind-protect
             (progn
               (with-current-buffer first
                 (cl-letf (((symbol-function 'message)
                            (lambda (&rest _arguments) nil)))
                   (auto-org-md-on)))
               (list
                (with-current-buffer first
                  (list
                   (local-variable-p 'after-save-hook)
                   (memq 'auto-org-md-export
                         after-save-hook)))
                (with-current-buffer second
                  (list
                   (local-variable-p 'after-save-hook)
                   (memq 'auto-org-md-export
                         after-save-hook)))
                (memq 'auto-org-md-export
                      (default-value
                       'after-save-hook))))
           (kill-buffer first)
           (kill-buffer second)))"##,
        expect!["OK ((t (auto-org-md-export t)) (nil nil) nil)"],
    )
}

fn auto_org_md_off_preserves_same_function_on_global_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_off_preserves_same_function_on_global_hook",
        r##"(let ((original
                                (default-value
                                 'after-save-hook)))
         (unwind-protect
             (progn
               (add-hook 'after-save-hook
                         #'auto-org-md-export)
               (with-temp-buffer
                 (add-hook 'after-save-hook
                           #'auto-org-md-export
                           nil t)
                 (cl-letf (((symbol-function 'message)
                            (lambda (&rest _arguments) nil)))
                   (auto-org-md-off))
                 (list
                  (memq 'auto-org-md-export
                        after-save-hook)
                  (memq
                   'auto-org-md-export
                   (default-value
                    'after-save-hook)))))
           (set-default 'after-save-hook original)))"##,
        expect!["OK (#1=(auto-org-md-export) #1#)"],
    )
}

fn auto_org_md_mode_first_positive_enable_sets_mode_property_and_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_mode_first_positive_enable_sets_mode_property_and_hook",
        r##"(progn
         (auto-org-md-test-reset-state)
         (with-temp-buffer
           (let (messages)
             (cl-letf (((symbol-function 'message)
                        (lambda (format-string &rest arguments)
                          (push
                           (apply #'format
                                  format-string
                                  arguments)
                           messages))))
               (auto-org-md-mode 1)
               (list
                auto-org-md-mode
                (get 'auto-org-md-mode 'state)
                (memq 'auto-org-md-export
                      after-save-hook)
                (nreverse messages))))))"##,
        expect![[r#"OK (t t (auto-org-md-export t) ("auto-org-md-mode is on."))"#]],
    )
}

fn auto_org_md_mode_repeated_positive_argument_toggles_internal_property_off() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_mode_repeated_positive_argument_toggles_internal_property_off",
        r##"(progn
         (auto-org-md-test-reset-state)
         (with-temp-buffer
           (let (messages)
             (cl-letf (((symbol-function 'message)
                        (lambda (format-string &rest arguments)
                          (push
                           (apply #'format
                                  format-string
                                  arguments)
                           messages))))
               (auto-org-md-mode 1)
               (let ((first
                      (list
                       auto-org-md-mode
                       (get 'auto-org-md-mode 'state)
                       (memq 'auto-org-md-export
                             after-save-hook))))
                 (auto-org-md-mode 1)
                 (list
                  first
                  (list
                   auto-org-md-mode
                   (get 'auto-org-md-mode 'state)
                   (memq 'auto-org-md-export
                         after-save-hook))
                  (nreverse messages)))))))"##,
        expect![[
            r#"OK ((t t (auto-org-md-export t)) (t nil nil) ("auto-org-md-mode is on." "auto-org-md-mode is off."))"#
        ]],
    )
}

fn auto_org_md_mode_negative_argument_can_turn_hook_on_due_to_property_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_mode_negative_argument_can_turn_hook_on_due_to_property_state",
        r##"(progn
         (auto-org-md-test-reset-state)
         (with-temp-buffer
           (cl-letf (((symbol-function 'message)
                      (lambda (&rest _arguments) nil)))
             (auto-org-md-mode -1)
             (list
              auto-org-md-mode
              (get 'auto-org-md-mode 'state)
              (memq 'auto-org-md-export
                    after-save-hook)))))"##,
        expect!["OK (nil t (auto-org-md-export t))"],
    )
}

fn auto_org_md_mode_symbol_property_is_shared_across_buffers() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_mode_symbol_property_is_shared_across_buffers",
        r##"(progn
         (auto-org-md-test-reset-state)
         (let ((first (generate-new-buffer " *auto-org-one*"))
               (second (generate-new-buffer " *auto-org-two*")))
           (unwind-protect
               (cl-letf (((symbol-function 'message)
                          (lambda (&rest _arguments) nil)))
                 (with-current-buffer first
                   (auto-org-md-mode 1))
                 (with-current-buffer second
                   (auto-org-md-mode 1))
                 (list
                  (with-current-buffer first
                    (list
                     auto-org-md-mode
                     (memq 'auto-org-md-export
                           after-save-hook)))
                  (with-current-buffer second
                    (list
                     auto-org-md-mode
                     (memq 'auto-org-md-export
                           after-save-hook)))
                  (get 'auto-org-md-mode 'state)))
             (kill-buffer first)
             (kill-buffer second))))"##,
        expect!["OK ((t (auto-org-md-export t)) (t nil) nil)"],
    )
}

fn auto_org_md_mode_hook_observes_final_variable_property_and_save_hook_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_org_md_mode_hook_observes_final_variable_property_and_save_hook_state",
        r##"(progn
         (auto-org-md-test-reset-state)
         (with-temp-buffer
           (let (observed)
             (add-hook
              'auto-org-md-mode-hook
              (lambda ()
                (push
                 (list
                  auto-org-md-mode
                  (get 'auto-org-md-mode 'state)
                  (not
                   (null
                    (memq 'auto-org-md-export
                          after-save-hook))))
                 observed))
              nil t)
             (cl-letf (((symbol-function 'message)
                        (lambda (&rest _arguments) nil)))
               (auto-org-md-mode 1)
               (auto-org-md-mode -1))
             (nreverse observed))))"##,
        expect!["OK ((t t t) (nil nil nil))"],
    )
}

pub(super) fn lifecycle_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_org_md_on_installs_one_local_after_save_hook_and_message(),
        auto_org_md_off_removes_local_hook_and_reports_message(),
        auto_org_md_on_is_idempotent_for_hook_registration(),
        auto_org_md_hook_is_buffer_local_and_does_not_leak_to_sibling(),
        auto_org_md_off_preserves_same_function_on_global_hook(),
        auto_org_md_mode_first_positive_enable_sets_mode_property_and_hook(),
        auto_org_md_mode_repeated_positive_argument_toggles_internal_property_off(),
        auto_org_md_mode_negative_argument_can_turn_hook_on_due_to_property_state(),
        auto_org_md_mode_symbol_property_is_shared_across_buffers(),
        auto_org_md_mode_hook_observes_final_variable_property_and_save_hook_state(),
    ]
}
