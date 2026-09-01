use expect_test::expect;

use super::ParityBatchCase;

fn built_in_levels_are_ordered_constants() -> ParityBatchCase {
    ParityBatchCase::value(
        "built_in_levels_are_ordered_constants",
        r####"
(list :error logito:error-level
      :info logito:info-level
      :verbose logito:verbose-level
      :debug logito:debug-level
      :ordered
      (and (< logito:error-level logito:info-level)
           (< logito:info-level logito:verbose-level)
           (< logito:verbose-level logito:debug-level)
           t))
"####,
        expect!["OK (:error 0 :info 5 :verbose 10 :debug 15 :ordered t)"],
    )
}

fn should_log_respects_level_threshold() -> ParityBatchCase {
    ParityBatchCase::value(
        "should_log_respects_level_threshold",
        r####"
(let ((log (logito-object :level logito:info-level)))
  (list :error-ok (and (logito-should-log log logito:error-level) t)
        :info-ok (and (logito-should-log log logito:info-level) t)
        :debug-no (logito-should-log log logito:debug-level)
        :nil-log (logito-log nil logito:error-level 'tag "ignored %s" "x")))
"####,
        expect!["OK (:error-ok t :info-ok t :debug-no nil :nil-log nil)"],
    )
}

fn buffer_logger_appends_tagged_lines() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_logger_appends_tagged_lines",
        r####"
(let* ((buf-name (generate-new-buffer-name " *logito-parity*"))
       (log (logito-buffer-object :level logito:debug-level :buffer buf-name)))
  (unwind-protect
      (progn
        (logito:info log "hello %s" "world")
        (logito:error log "boom %d" 7)
        (with-current-buffer buf-name
          (list :text (string-trim (buffer-string))
                :has-info (and (string-match-p "\\[info\\] hello world" (buffer-string)) t)
                :has-error (and (string-match-p "\\[error\\] boom 7" (buffer-string)) t)
                :line-pairs
                (length (split-string (string-trim (buffer-string)) "\n\n" t)))))
    (when (get-buffer buf-name)
      (kill-buffer buf-name))))
"####,
        expect![[
            r#"OK (:text "[info] hello world\n\n[error] boom 7" :has-info t :has-error t :line-pairs 2)"#
        ]],
    )
}

fn buffer_logger_skips_when_buffer_or_level_blocks() -> ParityBatchCase {
    ParityBatchCase::value(
        "buffer_logger_skips_when_buffer_or_level_blocks",
        r####"
(let* ((buf-name (generate-new-buffer-name " *logito-skip*"))
       (no-buf (logito-buffer-object :level logito:debug-level :buffer nil))
       (quiet (logito-buffer-object :level logito:error-level :buffer buf-name)))
  (unwind-protect
      (progn
        (logito:info no-buf "nope")
        (let ((after-no-buf (and (null (get-buffer buf-name)) t)))
          (logito:info quiet "too quiet")
          (let ((after-quiet
                 (if (get-buffer buf-name)
                     (with-current-buffer buf-name
                       (string-trim (buffer-string)))
                   "")))
            (logito:error quiet "loud enough")
            (list :after-no-buf-empty after-no-buf
                  :after-quiet-empty (and (string-empty-p after-quiet) t)
                  :after-error
                  (with-current-buffer buf-name
                    (string-trim (buffer-string)))))))
    (when (get-buffer buf-name)
      (kill-buffer buf-name))))
"####,
        expect![[
            r#"OK (:after-no-buf-empty t :after-quiet-empty t :after-error "[error] loud enough")"#
        ]],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        built_in_levels_are_ordered_constants(),
        should_log_respects_level_threshold(),
        buffer_logger_appends_tagged_lines(),
        buffer_logger_skips_when_buffer_or_level_blocks(),
    ]
}
