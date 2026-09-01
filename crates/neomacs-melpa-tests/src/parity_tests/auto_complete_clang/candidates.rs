use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_clang_call_process_unsaved_uses_current_region_and_parses_stubbed_stdout()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_call_process_unsaved_uses_current_region_and_parses_stubbed_stdout",
        r##"(with-temp-buffer
         (insert "int main() { return val; }")
         (let ((ac-clang-executable
                 "/fake/clang")
               (ac-clang-auto-save nil)
               (calls nil))
           (cl-letf
               (((symbol-function
                  'call-process-region)
                 (lambda (start end
                                program
                                delete
                                destination display
                                &rest arguments)
                   (push
                    (list
                     start end
                     (buffer-substring-no-properties
                      start end)
                     program delete
                     (buffer-name destination)
                     display arguments)
                    calls)
                   (with-current-buffer
                       destination
                     (insert
                      "COMPLETION: value : int value\n"
                      "COMPLETION: vector : class vector\n"))
                   0))
                ((symbol-function
                  'call-process)
                 (lambda (&rest _arguments)
                   (error
                    "saved process path used"))))
             (let ((result
                    (ac-clang-call-process
                     "val" "-cc1" "-")))
               (list
                (mapcar
                 #'ac-clang-test-candidate-state
                 result)
                (nreverse calls))))))"##,
        expect![[
            r#"OK ((("value" "int value" nil)) ((1 27 "int main() { return val; }" "/fake/clang" nil "*clang-output*" nil ("-cc1" "-"))))"#
        ]],
    )
}

fn auto_complete_clang_call_process_saved_uses_file_process_without_region() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_call_process_saved_uses_file_process_without_region",
        r##"(with-temp-buffer
         (let ((ac-clang-executable
                 "/fake/clang")
               (ac-clang-auto-save t)
               (calls nil))
           (cl-letf
               (((symbol-function
                  'call-process)
                 (lambda (program infile
                                  destination display
                                  &rest arguments)
                   (push
                    (list
                     program infile
                     (buffer-name destination)
                     display arguments)
                    calls)
                   (with-current-buffer
                       destination
                     (insert
                      "COMPLETION: saved : int saved\n"))
                   0))
                ((symbol-function
                  'call-process-region)
                 (lambda (&rest _arguments)
                   (error
                    "region process path used"))))
             (let ((result
                    (ac-clang-call-process
                     "sav" "-cc1"
                     "saved.c")))
               (list
                (mapcar
                 #'ac-clang-test-candidate-state
                 result)
                (nreverse calls))))))"##,
        expect![[
            r#"OK ((("saved" "int saved" nil)) (("/fake/clang" nil "*clang-output*" nil ("-cc1" "saved.c"))))"#
        ]],
    )
}

fn auto_complete_clang_nonzero_process_still_reports_error_and_returns_useful_completions()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_nonzero_process_still_reports_error_and_returns_useful_completions",
        r##"(with-temp-buffer
         (let ((ac-clang-executable
                 "/fake/clang")
               (ac-clang-auto-save nil)
               (handled nil))
           (cl-letf
               (((symbol-function
                  'call-process-region)
                 (lambda (_start _end
                          _program _delete
                          destination _display
                          &rest _arguments)
                   (with-current-buffer
                       destination
                     (insert
                      "warning before completion\n"
                      "COMPLETION: partial : int partial\n"))
                   3))
                ((symbol-function
                  'ac-clang-handle-error)
                 (lambda (result arguments)
                   (setq handled
                         (list result
                               arguments)))))
             (let ((result
                    (ac-clang-call-process
                     "par" "-cc1"
                     "-bad")))
               (list
                handled
                (mapcar
                 #'ac-clang-test-candidate-state
                 result))))))"##,
        expect![[r#"OK ((3 ("-cc1" "-bad")) (("partial" "int partial" nil)))"#]],
    )
}

fn auto_complete_clang_string_comment_detector_uses_real_c_syntax_state() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_string_comment_detector_uses_real_c_syntax_state",
        r##"(with-temp-buffer
         (c-mode)
         (insert
          "int code = 1;\n"
          "// comment text\n"
          "const char *s = \"literal\";\n"
          "/* block\n"
          "   comment */\n")
         (syntax-propertize
          (point-max))
         (mapcar
          (lambda (needle)
            (goto-char (point-min))
            (search-forward needle)
            (list
             needle
             (ac-in-string/comment)))
          '("code" "comment text"
            "literal" "block")))"##,
        expect![[r#"OK (("code" nil) ("comment text" 15) ("literal" 47) ("block" 58))"#]],
    )
}

fn auto_complete_clang_candidate_skips_process_inside_comment_and_string() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_candidate_skips_process_inside_comment_and_string",
        r##"(with-temp-buffer
         (c-mode)
         (insert
          "int value;\n"
          "// val\n"
          "const char *s = \"val\";\n")
         (syntax-propertize
          (point-max))
         (let ((calls nil)
               (ac-prefix "val"))
           (cl-letf
               (((symbol-function
                  'ac-clang-call-process)
                 (lambda (&rest arguments)
                   (push arguments calls)
                   '("value"))))
             (goto-char (point-min))
             (search-forward "value")
             (let ((code
                    (ac-clang-candidate)))
               (search-forward "val")
               (let ((comment
                      (ac-clang-candidate)))
                 (search-forward "\"val")
                 (forward-char 2)
                 (let ((string
                        (ac-clang-candidate)))
                   (list
                    code comment string
                    (nreverse calls))))))))"##,
        expect![[
            r#"OK (#1=("value") nil #1# (("val" "-cc1" "-fsyntax-only" "-x" "c" "-code-completion-at" "-:1:7" "-") ("val" "-cc1" "-fsyntax-only" "-x" "c" "-code-completion-at" "-:3:20" "-")))"#
        ]],
    )
}

fn auto_complete_clang_candidate_auto_save_saves_modified_buffer_before_invocation()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_candidate_auto_save_saves_modified_buffer_before_invocation",
        r##"(with-temp-buffer
         (insert "int val")
         (setq buffer-file-name
               (expand-file-name
                "autosave.c"
                default-directory))
         (set-buffer-modified-p t)
         (let ((ac-prefix "val")
               (ac-clang-auto-save t)
               (saves 0)
               (calls nil))
           (cl-letf
               (((symbol-function
                  'basic-save-buffer)
                 (lambda ()
                   (setq saves (1+ saves))
                   (set-buffer-modified-p
                    nil)
                   'saved))
                ((symbol-function
                  'ac-clang-call-process)
                 (lambda (prefix
                          &rest arguments)
                   (setq calls
                         (list
                          prefix arguments
                          (buffer-modified-p)))
                   '("value"))))
             (list
              (ac-clang-candidate)
              saves calls
              (buffer-modified-p)))))"##,
        expect![[
            r#"OK (("value") 1 ("val" ("-cc1" "-fsyntax-only" "-code-completion-at" "[ORACLE-SANDBOX]/autosave.c:1:5" "[ORACLE-SANDBOX]/autosave.c") nil) nil)"#
        ]],
    )
}

fn auto_complete_clang_candidate_widens_narrowing_and_builds_completion_at_prefix_start()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_candidate_widens_narrowing_and_builds_completion_at_prefix_start",
        r##"(with-temp-buffer
         (insert
          "header\nint alpha;\nint beta;\nfooter")
         (goto-char (point-min))
         (forward-line 1)
         (let ((start (point)))
           (forward-line 2)
           (narrow-to-region
            start (point)))
         (goto-char (point-min))
         (search-forward "beta")
         (let ((ac-prefix "beta")
               (ac-clang-auto-save nil)
               (calls nil))
           (cl-letf
               (((symbol-function
                  'ac-clang-call-process)
                 (lambda (prefix
                          &rest arguments)
                   (setq calls
                         (list
                          prefix arguments
                          (point-min)
                          (point-max)))
                   '("beta"))))
             (list
              (ac-clang-candidate)
              calls
              (buffer-narrowed-p)))))"##,
        expect![[
            r#"OK (("beta") ("beta" ("-cc1" "-fsyntax-only" "-x" "c++" "-code-completion-at" "-:3:5" "-") 1 35) t)"#
        ]],
    )
}

fn auto_complete_clang_prefix_prefers_symbol_then_member_access_operators() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_prefix_prefers_symbol_then_member_access_operators",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (car case))
             (goto-char (point-max))
             (cl-letf
                 (((symbol-function
                    'ac-prefix-symbol)
                   (lambda ()
                     (cadr case))))
               (ac-clang-prefix))))
         '(("identifier" 3)
           ("object." nil)
           ("pointer->" nil)
           ("Type::" nil)
           ("greater>" nil)
           ("colon:" nil)
           ("" nil)))"##,
        expect!["OK (3 8 10 7 nil nil nil)"],
    )
}

fn auto_complete_clang_real_subprocess_consumes_unsaved_buffer_and_returns_candidates()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_clang_real_subprocess_consumes_unsaved_buffer_and_returns_candidates",
        r##"(let* ((root
                 (expand-file-name
                  "ac-clang-real-process"
                  default-directory))
                (script
                 (expand-file-name
                  "fake-clang" root)))
         (unwind-protect
             (progn
               (make-directory root t)
               (ac-clang-test-reset-file
                script
                "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' 'COMPLETION: value : int value' 'COMPLETION: validate : bool validate(<#int input#>)'\n")
               (set-file-modes script #o755)
               (with-temp-buffer
                 (insert
                  "int main(void) {\n  return val;\n}\n")
                 (let ((ac-clang-executable
                        script)
                       (ac-clang-auto-save
                        nil))
                   (mapcar
                    #'ac-clang-test-candidate-state
                    (ac-clang-call-process
                     "val" "-cc1"
                     "-fsyntax-only" "-")))))
           (delete-directory root t)))"##,
        expect![[
            r#"OK (("validate" "bool validate(<#int input#>)" nil) ("value" "int value" nil))"#
        ]],
    )
}

pub(super) fn candidates_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_clang_call_process_unsaved_uses_current_region_and_parses_stubbed_stdout(),
        auto_complete_clang_call_process_saved_uses_file_process_without_region(),
        auto_complete_clang_nonzero_process_still_reports_error_and_returns_useful_completions(),
        auto_complete_clang_string_comment_detector_uses_real_c_syntax_state(),
        auto_complete_clang_candidate_skips_process_inside_comment_and_string(),
        auto_complete_clang_candidate_auto_save_saves_modified_buffer_before_invocation(),
        auto_complete_clang_candidate_widens_narrowing_and_builds_completion_at_prefix_start(),
        auto_complete_clang_prefix_prefers_symbol_then_member_access_operators(),
        auto_complete_clang_real_subprocess_consumes_unsaved_buffer_and_returns_candidates(),
    ]
}
