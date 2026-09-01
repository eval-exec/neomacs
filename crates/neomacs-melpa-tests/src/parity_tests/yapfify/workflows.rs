use expect_test::expect;

use super::ParityBatchCase;

fn call_bin_builds_line_range_and_returns_exit_code() -> ParityBatchCase {
    ParityBatchCase::value(
        "call_bin_builds_line_range_and_returns_exit_code",
        r####"
(let ((calls nil)
      (yapfify-executable "yapf"))
  (cl-letf (((symbol-function 'call-process-region)
             (lambda (start end program &rest args)
               (push (list :start start :end end :program program :args args)
                     calls)
               0)))
    (with-temp-buffer
      (insert "def f():\n  return 1\n")
      (let ((out (get-buffer-create " *yapfify-out*")))
        (unwind-protect
            (let ((code (yapfify-call-bin (current-buffer) out 1 2)))
              (list :code code
                    :calls (nreverse calls)
                    :exec yapfify-executable))
          (kill-buffer out))))))
"####,
        expect![[
            r#"OK (:code 0 :calls ((:start 1 :end 21 :program "yapf" :args (nil (:buffer nil) nil "-l" "1-2"))) :exec "yapf")"#
        ]],
    )
}

fn region_success_replaces_buffer_from_tmp() -> ParityBatchCase {
    ParityBatchCase::value(
        "region_success_replaces_buffer_from_tmp",
        r####"
(cl-letf (((symbol-function 'yapfify-call-bin)
           (lambda (_in out _s _e)
             (with-current-buffer out
               (erase-buffer)
               (insert "def f():\n    return 1\n"))
             0)))
  (with-temp-buffer
    (insert "def f():\n  return 1\n")
    (yapfify-region (point-min) (point-max))
    (list :text (buffer-string))))
"####,
        expect![[r#"OK (:text "def f():\n    return 1\n")"#]],
    )
}

fn region_error_signals_and_keeps_source() -> ParityBatchCase {
    ParityBatchCase::value(
        "region_error_signals_and_keeps_source",
        r####"
(cl-letf (((symbol-function 'yapfify-call-bin)
           (lambda (_in out _s _e)
             (with-current-buffer out (erase-buffer) (insert "boom"))
             1)))
  (with-temp-buffer
    (insert "broken(\n")
    (list :err
          (condition-case err
              (progn (yapfify-region (point-min) (point-max)) :ok)
            (error (error-message-string err)))
          :kept (buffer-string))))
"####,
        expect![[r#"OK (:err "Yapf failed, see *yapfify* buffer for details" :kept "broken(\n")"#]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        call_bin_builds_line_range_and_returns_exit_code(),
        region_success_replaces_buffer_from_tmp(),
        region_error_signals_and_keeps_source(),
    ]
}
