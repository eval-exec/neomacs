use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_practical_unsaved_candidate_request_uses_prefix_start_and_full_buffer()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_practical_unsaved_candidate_request_uses_prefix_start_and_full_buffer",
        r##"(with-temp-buffer
         (c++-mode)
         (insert
          "#include <vector>\n"
          "int main() {\n"
          "  std::vec\n"
          "}\n")
         (search-backward "vec")
         (goto-char (+ (point) 3))
         (let ((ac-prefix "vec")
               (ac-clang-auto-save nil)
               (ac-clang-flags
                '("-Iproject/include"
                  "-std=c++20"))
               (requests nil))
           (cl-letf
               (((symbol-function
                  'ac-clang-call-process)
                 (lambda (prefix
                          &rest arguments)
                   (push
                    (list
                     prefix arguments
                     (buffer-substring-no-properties
                      (point-min)
                      (point-max)))
                    requests)
                   (list
                    (propertize
                     "vector"
                     'ac-clang-help
                     "class std::vector")))))
             (list
              (mapcar
               #'ac-clang-test-candidate-state
               (ac-clang-candidate))
              (nreverse requests)))))"##,
        expect![[
            r##"OK ((("vector" "class std::vector" nil)) (("vec" ("-cc1" "-fsyntax-only" "-x" "c++" "-Iproject/include" "-std=c++20" "-code-completion-at" "-:3:8" "-") "#include <vector>\nint main() {\n  std::vec\n}\n")))"##
        ]],
    )
}

fn auto_complete_clang_parse_document_action_and_template_pipeline_preserves_overloads()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_parse_document_action_and_template_pipeline_preserves_overloads",
        r##"(with-temp-buffer
         (insert
          "COMPLETION: emplace : iterator emplace(<#const_iterator pos#>, <#value_type value#>)\n"
          "COMPLETION: emplace : iterator emplace(<#const_iterator pos#>, <#size_type count#>, <#value_type value#>)\n")
         (let* ((parsed
                 (ac-clang-parse-output
                  "empl"))
                (candidate (car parsed))
                (documentation
                 (ac-clang-document
                  candidate))
                (ac-last-completion
                 (cons "emplace"
                       candidate))
                (starts 0)
                (messages nil))
           (goto-char (point-max))
           (cl-letf
               (((symbol-function
                  'ac-complete-template)
                 (lambda ()
                   (setq starts
                         (1+ starts))))
                ((symbol-function 'message)
                 (lambda (format-string
                          &rest arguments)
                   (push
                    (apply #'format
                           format-string
                           arguments)
                    messages))))
             (ac-clang-action)
             (list
              (mapcar
               #'ac-clang-test-candidate-state
               parsed)
              documentation
              (mapcar
               #'ac-clang-test-candidate-state
               ac-template-candidates)
              ac-template-start-point
              starts
              messages))))"##,
        expect![[
            r#"OK ((("emplace" "iterator emplace(<#const_iterator pos#>, <#value_type value#>)\niterator emplace(<#const_iterator pos#>, <#size_type count#>, <#value_type value#>)" nil)) "iterator emplace(const_iterator pos, value_type value)\niterator emplace(const_iterator pos, size_type count, value_type value)" (("(const_iterator pos, value_type value)" "" "(<#const_iterator pos#>, <#value_type value#>)") ("(const_iterator pos, size_type count, value_type value)" "" "(<#const_iterator pos#>, <#size_type count#>, <#value_type value#>)")) 192 1 nil)"#
        ]],
    )
}

fn auto_complete_clang_generated_source_commands_invoke_auto_complete_with_exact_source()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_generated_source_commands_invoke_auto_complete_with_exact_source",
        r##"(let ((calls nil))
         (cl-letf
             (((symbol-function
                'auto-complete)
               (lambda (&optional sources)
                 (push sources calls)
                 'completed)))
           (list
            (ac-complete-clang)
            (ac-complete-template)
            (nreverse calls))))"##,
        expect!["OK (completed completed ((ac-source-clang) (ac-source-template)))"],
    )
}

fn auto_complete_clang_real_candidate_subprocess_runs_from_c_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_real_candidate_subprocess_runs_from_c_buffer",
        r##"(let* ((root
                 (expand-file-name
                  "ac-clang-candidate-process"
                  default-directory))
                (script
                 (expand-file-name
                  "fake-clang" root)))
         (unwind-protect
             (progn
               (make-directory root t)
               (ac-clang-test-reset-file
                script
                "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' 'COMPLETION: field : int field' 'COMPLETION: finalize : void finalize(<#int code#>)'\n")
               (set-file-modes script #o755)
               (with-temp-buffer
                 (c-mode)
                 (insert
                  "struct item { int field; };\n"
                  "void use(struct item value) {\n"
                  "  value.fi\n"
                  "}\n")
                 (search-backward "fi")
                 (goto-char (+ (point) 2))
                 (let ((ac-prefix "fi")
                       (ac-clang-executable
                        script)
                       (ac-clang-auto-save nil))
                   (mapcar
                    #'ac-clang-test-candidate-state
                    (ac-clang-candidate)))))
           (delete-directory root t)))"##,
        expect![[
            r#"OK (("finalize" "void finalize(<#int code#>)" nil) ("field" "int field" nil))"#
        ]],
    )
}

fn auto_complete_clang_failed_real_process_keeps_completion_and_deterministic_diagnostic()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_failed_real_process_keeps_completion_and_deterministic_diagnostic",
        r##"(let* ((root
                 (expand-file-name
                  "ac-clang-failed-process"
                  default-directory))
                (script
                 (expand-file-name
                  "fake-clang" root))
                (messages nil))
         (unwind-protect
             (progn
               (make-directory root t)
               (ac-clang-test-reset-file
                script
                "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' 'warning: recoverable parse issue' 'COMPLETION: retry : int retry'\nexit 4\n")
               (set-file-modes script #o755)
               (with-temp-buffer
                 (insert "ret")
                 (let ((ac-clang-executable
                        script)
                       (ac-clang-auto-save nil))
                   (cl-letf
                       (((symbol-function
                          'current-time-string)
                         (lambda () "NOW"))
                        ((symbol-function
                          'message)
                         (lambda (format-string
                                  &rest arguments)
                           (push
                            (apply #'format
                                   format-string
                                   arguments)
                            messages))))
                     (let ((result
                            (ac-clang-call-process
                             "ret" "-cc1" "-")))
                       (with-current-buffer
                           ac-clang-error-buffer-name
                         (list
                          (mapcar
                           #'ac-clang-test-candidate-state
                           result)
                          (buffer-string)
                          buffer-read-only
                          messages)))))))
           (delete-directory root t)))"##,
        expect![[
            r#"OK ((("retry" "int retry" nil)) "NOW\nclang failed with error 4:\n[ORACLE-SANDBOX]/ac-clang-failed-process/fake-clang -cc1 -\n\nwarning: recoverable parse issue" t nil)"#
        ]],
    )
}

fn auto_complete_clang_auto_save_candidate_writes_real_file_before_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_auto_save_candidate_writes_real_file_before_process",
        r##"(let* ((root
                 (expand-file-name
                  "ac-clang-real-autosave"
                  default-directory))
                (source
                 (expand-file-name
                  "main.c" root))
                (script
                 (expand-file-name
                  "fake-clang" root)))
         (unwind-protect
             (progn
               (make-directory root t)
               (ac-clang-test-reset-file
                source "int old_value;\n")
               (ac-clang-test-reset-file
                script
                "#!/bin/sh\nprintf '%s\\n' 'COMPLETION: new_value : int new_value'\n")
               (set-file-modes script #o755)
               (with-temp-buffer
                 (insert-file-contents source)
                 (setq buffer-file-name
                       source)
                 (goto-char (point-max))
                 (insert "int new_val")
                 (let ((ac-prefix "new_val")
                       (ac-clang-executable
                        script)
                       (ac-clang-auto-save t))
                   (let ((result
                          (ac-clang-candidate)))
                     (list
                      (mapcar
                       #'ac-clang-test-candidate-state
                       result)
                      (buffer-modified-p)
                      (with-temp-buffer
                        (insert-file-contents
                         source)
                        (buffer-string)))))))
           (delete-directory root t)))"##,
        expect![[r#"OK ((("new_value" "int new_value" nil)) nil "int old_value;\nint new_val")"#]],
    )
}

fn auto_complete_clang_interactive_cflags_flow_changes_next_completion_arguments() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_clang_interactive_cflags_flow_changes_next_completion_arguments",
        r##"(with-temp-buffer
         (c-mode)
         (insert "int feat")
         (let ((ac-prefix "feat")
               (ac-clang-auto-save nil)
               (requests nil))
           (cl-letf
               (((symbol-function
                  'read-string)
                 (lambda (&rest _arguments)
                   "-Ivendor -DFEATURE=1"))
                ((symbol-function
                  'ac-clang-call-process)
                 (lambda (prefix
                          &rest arguments)
                   (push
                    (list prefix arguments)
                    requests)
                   '("feature"))))
             (call-interactively
              #'ac-clang-set-cflags)
             (list
              ac-clang-flags
              (ac-clang-candidate)
              (nreverse requests)))))"##,
        expect![[
            r#"OK (("-Ivendor" "-DFEATURE=1") ("feature") (("feat" ("-cc1" "-fsyntax-only" "-x" "c" "-Ivendor" "-DFEATURE=1" "-code-completion-at" "-:1:5" "-"))))"#
        ]],
    )
}

fn auto_complete_clang_member_prefix_and_source_candidate_form_work_together() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_member_prefix_and_source_candidate_form_work_together",
        r##"(with-temp-buffer
         (insert "object->")
         (let ((ac-prefix "object->")
               (prefix-function
                (cdr
                 (assq 'prefix
                       ac-source-clang)))
               (candidate-function
                (cdr
                 (assq 'candidates
                       ac-source-clang))))
           (cl-letf
               (((symbol-function
                  'ac-prefix-symbol)
                 (lambda () nil))
                ((symbol-function
                  'ac-clang-call-process)
                 (lambda (prefix
                          &rest _arguments)
                   (list
                    (propertize
                     (concat prefix "member")
                     'ac-clang-help
                     "int member")))))
             (list
              (funcall prefix-function)
              (mapcar
               #'ac-clang-test-candidate-state
               (funcall
                candidate-function))))))"##,
        expect![[r#"OK (9 (("object->member" "int member" nil)))"#]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_practical_unsaved_candidate_request_uses_prefix_start_and_full_buffer(),
        auto_complete_clang_parse_document_action_and_template_pipeline_preserves_overloads(),
        auto_complete_clang_generated_source_commands_invoke_auto_complete_with_exact_source(),
        auto_complete_clang_real_candidate_subprocess_runs_from_c_buffer(),
        auto_complete_clang_failed_real_process_keeps_completion_and_deterministic_diagnostic(),
        auto_complete_clang_auto_save_candidate_writes_real_file_before_process(),
        auto_complete_clang_interactive_cflags_flow_changes_next_completion_arguments(),
        auto_complete_clang_member_prefix_and_source_candidate_form_work_together(),
    ]
}
