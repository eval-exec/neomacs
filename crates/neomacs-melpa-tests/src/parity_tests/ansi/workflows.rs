use expect_test::expect;

use super::ParityBatchCase;

fn runtime_selected_effects_render_a_complete_ci_summary_with_exact_formatting() -> ParityBatchCase
{
    ParityBatchCase::value(
        "runtime_selected_effects_render_a_complete_ci_summary_with_exact_formatting",
        r##"(let ((jobs
       '((compile success 42 42 3.14)
         (lint warning 18 20 0.87)
         (integration failure 127 130 12.50))))
  (mapconcat
   (lambda (job)
     (let* ((name (nth 0 job))
            (state (nth 1 job))
            (done (nth 2 job))
            (total (nth 3 job))
            (seconds (nth 4 job))
            (effect
             (cond
              ((eq state 'success) 'green)
              ((eq state 'warning) 'yellow)
              (t 'bright-white)))
            (status
             (cond
              ((eq state 'success) "PASS")
              ((eq state 'warning) "WARN")
              (t "FAIL")))
            (rendered-status
             (if (eq state 'failure)
                 (ansi-on-red
                  (ansi-apply effect " %-4s " status))
               (ansi-apply effect "%-6s" status))))
       (concat
        (ansi-bold "%-12s" name)
        " "
        rendered-status
        " "
        (ansi-cyan "%3d/%-3d" done total)
        (ansi-dark " %6.2fs" seconds))))
   jobs
   "\n"))"##,
        expect![[
            r#"OK "\33[1mcompile     \33[0m \33[32mPASS  \33[0m \33[36m 42/42 \33[0m\33[2m   3.14s\33[0m\n\33[1mlint        \33[0m \33[33mWARN  \33[0m \33[36m 18/20 \33[0m\33[2m   0.87s\33[0m\n\33[1mintegration \33[0m \33[41m\33[97m FAIL \33[0m\33[0m \33[36m127/130\33[0m\33[2m  12.50s\33[0m""#
        ]],
    )
}

fn documented_direct_dsl_and_runtime_interfaces_render_the_same_nested_alert() -> ParityBatchCase {
    ParityBatchCase::value(
        "documented_direct_dsl_and_runtime_interfaces_render_the_same_nested_alert",
        r##"(let* ((direct
        (ansi-bold
         (ansi-on-red
          (ansi-bright-white " DEPLOY BLOCKED "))))
       (dsl
        (with-ansi
         (bold
          (on-red
           (bright-white " DEPLOY BLOCKED ")))))
       (runtime
        (ansi-apply
         'bold
         (ansi-apply
          'on-red
          (ansi-apply 'bright-white " DEPLOY BLOCKED ")))))
  (list
   direct
   dsl
   runtime
   (equal direct dsl)
   (equal dsl runtime)
   (string-to-list dsl)))"##,
        expect![[
            r#"OK ("\33[1m\33[41m\33[97m DEPLOY BLOCKED \33[0m\33[0m\33[0m" "\33[1m\33[41m\33[97m DEPLOY BLOCKED \33[0m\33[0m\33[0m" "\33[1m\33[41m\33[97m DEPLOY BLOCKED \33[0m\33[0m\33[0m" t t (27 91 49 109 27 91 52 49 109 27 91 57 55 109 32 68 69 80 76 79 89 32 66 76 79 67 75 69 68 32 27 91 48 109 27 91 48 109 27 91 48 109))"#
        ]],
    )
}

fn terminal_progress_redraws_emit_an_exact_incremental_csi_transcript() -> ParityBatchCase {
    ParityBatchCase::value(
        "terminal_progress_redraws_emit_an_exact_incremental_csi_transcript",
        r##"(with-output-to-string
  (with-ansi-princ
   (column 1)
   (kill 2)
   (bold "compile ")
   (cyan "%3d%%" 0)
   "\n")
  (with-ansi-princ
   (previous-line 1)
   (column 1)
   (kill 2)
   (bold "compile ")
   (yellow "%3d%%" 50)
   "\n")
  (with-ansi-princ
   (previous-line 1)
   (column 1)
   (kill 2)
   (bold "compile ")
   (green "%3d%%" 100)
   " "
   (green "✓")
   "\n"))"##,
        expect![[
            r#"OK "\33[1G\33[2K\33[1mcompile \33[0m\33[36m  0%\33[0m\n\33[1F\33[1G\33[2K\33[1mcompile \33[0m\33[33m 50%\33[0m\n\33[1F\33[1G\33[2K\33[1mcompile \33[0m\33[32m100%\33[0m \33[32m✓\33[0m\n""#
        ]],
    )
}

fn with_ansi_princ_writes_one_exact_multiline_release_report_to_a_real_buffer() -> ParityBatchCase {
    ParityBatchCase::value(
        "with_ansi_princ_writes_one_exact_multiline_release_report_to_a_real_buffer",
        r##"(let ((output (generate-new-buffer " *ansi-release-report*")))
  (unwind-protect
      (let ((standard-output output))
        (with-ansi-princ
         (bold (bright-white "neomacs 0.14.0"))
         "\n"
         (green "  ✓ %-18s %4d tests" "core" 2048)
         "\n"
         (green "  ✓ %-18s %4d tests" "oracle" 512)
         "\n"
         (yellow "  ! %-18s %s" "docs" "2 warnings")
         "\n")
        (with-current-buffer output
          (list
           (buffer-string)
           (buffer-size)
           (line-number-at-pos (point-max)))))
    (kill-buffer output)))"##,
        expect![[
            r#"OK ("\33[1m\33[97mneomacs 0.14.0\33[0m\33[0m\n\33[32m  ✓ core               2048 tests\33[0m\n\33[32m  ✓ oracle              512 tests\33[0m\n\33[33m  ! docs               2 warnings\33[0m\n" 161 5)"#
        ]],
    )
}

fn dumb_terminal_inhibition_keeps_the_report_text_and_restores_colored_output() -> ParityBatchCase {
    ParityBatchCase::value(
        "dumb_terminal_inhibition_keeps_the_report_text_and_restores_colored_output",
        r##"(let ((ansi-inhibit-ansi nil))
  (let* ((render
          (lambda (inhibit)
            (let ((ansi-inhibit-ansi inhibit))
              (with-ansi
               (column 1)
               (kill 2)
               (bold (red "ERROR"))
               ": "
               (on-yellow
                (black "%s at %d%%%%" "/var/lib/neomacs" 99))
               "\n"
               (italic "free space: %.1f GiB" 0.2)
               "\n"))))
         (colored-before (funcall render nil))
         (plain (funcall render t))
         (colored-after (funcall render nil)))
    (list
     colored-before
     plain
     colored-after
     (equal colored-before colored-after)
     ansi-inhibit-ansi
     (string-match-p (regexp-quote "\e[") plain))))"##,
        expect![[
            r#"OK ("\33[1G\33[2K\33[1m\33[31mERROR\33[0m\33[0m: \33[43m\33[30m/var/lib/neomacs at 99%\33[0m\33[0m\n\33[3mfree space: 0.2 GiB\33[0m\n" "ERROR: /var/lib/neomacs at 99%\nfree space: 0.2 GiB\n" "\33[1G\33[2K\33[1m\33[31mERROR\33[0m\33[0m: \33[43m\33[30m/var/lib/neomacs at 99%\33[0m\33[0m\n\33[3mfree space: 0.2 GiB\33[0m\n" t nil nil)"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        runtime_selected_effects_render_a_complete_ci_summary_with_exact_formatting(),
        documented_direct_dsl_and_runtime_interfaces_render_the_same_nested_alert(),
        terminal_progress_redraws_emit_an_exact_incremental_csi_transcript(),
        with_ansi_princ_writes_one_exact_multiline_release_report_to_a_real_buffer(),
        dumb_terminal_inhibition_keeps_the_report_text_and_restores_colored_output(),
    ]
}
