use expect_test::expect;

use super::ParityBatchCase;

fn flymake_output_parser_maps_all_severities_lines_and_messages_to_exact_regions() -> ParityBatchCase
{
    ParityBatchCase::value(
        "flymake_output_parser_maps_all_severities_lines_and_messages_to_exact_regions",
        r##"(with-temp-buffer
  (insert
   "first line\n"
   "second line has an invalid reference\n"
   "third line has a missing include\n"
   "fourth line uses old syntax\n")
  (let ((diagnostics
         (asciidoc--flymake-parse-output
          (concat
           "noise before diagnostics\n"
           "asciidoctor: WARNING: <stdin>: line 2: invalid reference\n"
           "asciidoctor: ERROR: <stdin>: Line 3: missing include\n"
           "asciidoctor: DEPRECATED: <stdin>: line 4: old syntax\n"
           "asciidoctor: ERROR: other.adoc: line 1: external file\n"
           "asciidoctor: WARNING: <stdin>: line 99: outside buffer\n")
          (current-buffer)
          1)))
    (mapcar
     (lambda (diagnostic)
       (list
        (flymake-diagnostic-type diagnostic)
        (flymake-diagnostic-beg diagnostic)
        (flymake-diagnostic-end diagnostic)
        (flymake-diagnostic-text diagnostic)
        (buffer-substring-no-properties
         (flymake-diagnostic-beg diagnostic)
         (flymake-diagnostic-end diagnostic))))
     diagnostics)))"##,
        expect![[
            r#"OK ((:warning 12 48 "invalid reference" "second line has an invalid reference") (:error 49 81 "missing include" "third line has a missing include") (:note 82 109 "old syntax" "fourth line uses old syntax") (:warning 82 109 "outside buffer" "fourth line uses old syntax"))"#
        ]],
    )
}

fn fatal_asciidoctor_failure_becomes_one_buffer_error_only_when_no_line_diagnostic_exists()
-> ParityBatchCase {
    ParityBatchCase::value(
        "fatal_asciidoctor_failure_becomes_one_buffer_error_only_when_no_line_diagnostic_exists",
        r##"(with-temp-buffer
  (insert "= Document\n\nbody\n")
  (cl-labels
      ((summaries
        (output status)
        (mapcar
         (lambda (diagnostic)
           (list
            (flymake-diagnostic-type
             diagnostic)
            (flymake-diagnostic-beg
             diagnostic)
            (flymake-diagnostic-end
             diagnostic)
            (flymake-diagnostic-text
             diagnostic)))
         (asciidoc--flymake-parse-output
          output (current-buffer) status))))
    (list
     (summaries
      "asciidoctor: FAILED: missing converter for backend\n"
      1)
     (summaries
      "asciidoctor: WARNING: <stdin>: line 2: warning wins\nasciidoctor: FAILED: later fatal\n"
      1)
     (summaries
      "asciidoctor: FAILED: ignored on success\n"
      0)
     (summaries "all good\n" 0))))"##,
        expect![[
            r#"OK (((:error 1 11 "asciidoctor: FAILED: missing converter for backend")) ((:warning 12 13 "warning wins")) nil nil)"#
        ]],
    )
}

fn public_flymake_backend_runs_unsaved_buffer_through_a_deterministic_real_process()
-> ParityBatchCase {
    ParityBatchCase::value(
        "public_flymake_backend_runs_unsaved_buffer_through_a_deterministic_real_process",
        r##"(with-temp-buffer
  (insert
   "= Process Contract\n\n"
   "include::missing.adoc[]\n"
   "old syntax\n")
  (asciidoc-mode)
  (let ((asciidoc-asciidoctor-command
         "/bin/sh")
        (asciidoc-asciidoctor-extra-args
         '("-c"
           "cat >/dev/null; printf 'asciidoctor: ERROR: <stdin>: line 3: deterministic include\\nasciidoctor: DEPRECATED: <stdin>: Line 4: deterministic syntax\\n' >&2; exit 1"
           "asciidoc-mode-test"))
        result
        callback-args
        done
        command)
    (asciidoc-flymake
     (lambda (diagnostics &rest arguments)
       (setq
        result diagnostics
        callback-args arguments
        done t)))
    (setq command
          (process-command
           asciidoc--flymake-proc))
    (let ((attempt 0))
      (while (and (not done)
                  (< attempt 500))
        (accept-process-output nil 0.01)
        (setq attempt (1+ attempt))))
    (list
     done
     command
     callback-args
     (mapcar
      (lambda (diagnostic)
        (list
         (flymake-diagnostic-type
          diagnostic)
         (flymake-diagnostic-beg
          diagnostic)
         (flymake-diagnostic-end
          diagnostic)
         (flymake-diagnostic-text
          diagnostic)))
      result)
     (and asciidoc--flymake-proc
          (process-status
           asciidoc--flymake-proc)))))"##,
        expect![[
            r#"OK (t ("/bin/sh" "-c" "cat >/dev/null; printf 'asciidoctor: ERROR: <stdin>: line 3: deterministic include\\nasciidoctor: DEPRECATED: <stdin>: Line 4: deterministic syntax\\n' >&2; exit 1" "asciidoc-mode-test" "-B" "[ORACLE-SANDBOX]/" "-o" "/dev/null" "-") nil ((:error 21 44 "deterministic include") (:note 45 55 "deterministic syntax")) exit)"#
        ]],
    )
}

fn public_flymake_backend_rejects_missing_executable_before_mutating_process_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "public_flymake_backend_rejects_missing_executable_before_mutating_process_state",
        r##"(with-temp-buffer
  (insert "= Missing Tool\n")
  (asciidoc-mode)
  (let ((asciidoc-asciidoctor-command
         "asciidoc-mode-no-such-executable")
        (asciidoc--flymake-proc nil)
        reports)
    (condition-case error
        (asciidoc-flymake
         (lambda (diagnostics &rest _)
           (push diagnostics reports)))
      (error
       (list
        (car error)
        (cdr error)
        asciidoc--flymake-proc
        reports
        (memq #'asciidoc-flymake
              flymake-diagnostic-functions))))))"##,
        expect![[
            r#"OK (error ("Cannot find the Asciidoctor executable \"asciidoc-mode-no-such-executable\"") nil nil (asciidoc-flymake t))"#
        ]],
    )
}

pub(super) fn diagnostics_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        flymake_output_parser_maps_all_severities_lines_and_messages_to_exact_regions(),
        fatal_asciidoctor_failure_becomes_one_buffer_error_only_when_no_line_diagnostic_exists(),
        public_flymake_backend_runs_unsaved_buffer_through_a_deterministic_real_process(),
        public_flymake_backend_rejects_missing_executable_before_mutating_process_state(),
    ]
}
