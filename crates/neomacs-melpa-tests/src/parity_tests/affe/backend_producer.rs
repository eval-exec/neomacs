use expect_test::expect;

use super::ParityBatchCase;

fn affe_backend_producer_filter_accumulates_fragments_lines_totals_and_tail_links()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_producer_filter_accumulates_fragments_lines_totals_and_tail_links",
        r##"(let* ((affe-backend--producer-head
                      (list nil))
                     (affe-backend--producer-tail
                      affe-backend--producer-head)
                     (affe-backend--producer-total 0)
                     (affe-backend--producer-rest "")
                     (affe-backend--restrict-regexp
                      nil))
               (affe-backend--producer-filter
                nil "alpha")
               (let ((fragment-state
                      (list
                       (cdr
                        affe-backend--producer-head)
                       affe-backend--producer-total
                       affe-backend--producer-rest
                       (eq affe-backend--producer-tail
                           affe-backend--producer-head))))
                 (affe-backend--producer-filter
                  nil
                  " beta\ncharlie\r\ndelta\n")
                 (list
                  fragment-state
                  (cdr affe-backend--producer-head)
                  affe-backend--producer-total
                  affe-backend--producer-rest
                  (eq affe-backend--producer-tail
                      (last
                       affe-backend--producer-head)))))"##,
        expect![[r#"OK ((nil 0 "alpha" t) ("alpha beta" "charlie" "delta") 3 "" t)"#]],
    )
}

fn affe_backend_producer_filter_restricts_match_region_and_retains_prefix_suffix_properties()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_producer_filter_restricts_match_region_and_retains_prefix_suffix_properties",
        r##"(let* ((affe-backend--producer-head
                      (list nil))
                     (affe-backend--producer-tail
                      affe-backend--producer-head)
                     (affe-backend--producer-total 0)
                     (affe-backend--producer-rest "")
                     (affe-backend--restrict-regexp
                      "\\`[^:]+:[0-9]+:\\(.*\\)\\'"))
               (affe-backend--producer-filter
                nil
                "src/a.el:10:needle\nplain line\nlib/b.el:2:tail\nrest")
               (list
                (mapcar
                 (lambda (line)
                   (list
                    (substring-no-properties line)
                    (get-text-property
                     0 'affe--prefix line)
                    (get-text-property
                     0 'affe--suffix line)
                    (text-properties-at 0 line)))
                 (cdr
                  affe-backend--producer-head))
                affe-backend--producer-total
                affe-backend--producer-rest))"##,
        expect![[
            r#"OK ((("needle" "src/a.el:10:" "" (affe--suffix "" affe--prefix "src/a.el:10:")) ("plain line" nil nil nil) ("tail" "lib/b.el:2:" "" (affe--suffix "" affe--prefix "lib/b.el:2:"))) 3 "rest")"#
        ]],
    )
}

fn affe_backend_producer_sentinel_logs_stderr_marks_done_and_appends_final_fragment_once()
-> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_producer_sentinel_logs_stderr_marks_done_and_appends_final_fragment_once",
        r##"(let* ((affe-backend--client 'client)
                     (affe-backend--producer-head
                      (list nil "ready"))
                     (affe-backend--producer-tail
                      (last
                       affe-backend--producer-head))
                     (affe-backend--producer-total 1)
                     (affe-backend--producer-rest
                      "tail")
                     (affe-backend--producer-done nil)
                     writes)
               (with-current-buffer
                   (get-buffer-create
                    "*producer stderr*")
                 (erase-buffer)
                 (insert "warning\n"))
               (cl-letf
                   (((symbol-function
                      'process-send-string)
                     (lambda (process string)
                       (push
                        (list process string)
                        writes))))
                 (affe-backend--producer-sentinel
                  nil "finished\n")
                 (setq
                  affe-backend--producer-rest
                  "")
                 (affe-backend--producer-sentinel
                  nil "closed\n")
                 (list
                  affe-backend--producer-done
                  affe-backend--producer-total
                  (cdr
                   affe-backend--producer-head)
                  (eq
                   affe-backend--producer-tail
                   (last
                    affe-backend--producer-head))
                  (nreverse writes))))"##,
        expect![[
            r#"OK (t 2 ("ready" "tail") t ((client "(log \"Sentinel: finished\\n\\n\")\n") (client "(log \"Stderr:\\nwarning\\n\\n\")\n") (client "(log \"Sentinel: closed\\n\\n\")\n") (client "(log \"Stderr:\\nwarning\\n\\n\")\n")))"#
        ]],
    )
}

fn affe_backend_producer_start_logs_and_builds_exact_pipe_process_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "affe_backend_producer_start_logs_and_builds_exact_pipe_process_contract",
        r##"(let ((affe-backend--client 'client)
                    process-arguments writes)
               (cl-letf
                   (((symbol-function 'make-process)
                     (lambda (&rest arguments)
                       (setq process-arguments
                             arguments)
                       'producer-process))
                    ((symbol-function
                      'process-send-string)
                     (lambda (process string)
                       (push
                        (list process string)
                        writes))))
                 (let ((result
                        (affe-backend--producer-start
                         '("rg" "--files" "src"))))
                   (list
                    result
                    (plist-get process-arguments
                               :name)
                    (plist-get process-arguments
                               :noquery)
                    (plist-get process-arguments
                               :command)
                    (plist-get process-arguments
                               :connection-type)
                    (plist-get process-arguments
                               :stderr)
                    (plist-get process-arguments
                               :sentinel)
                    (plist-get process-arguments
                               :filter)
                    (nreverse writes)))))"##,
        expect![[
            r#"OK (producer-process "rg" t ("rg" "--files" "src") pipe "*producer stderr*" affe-backend--producer-sentinel affe-backend--producer-filter ((client "(log \"Starting (\\\"rg\\\" \\\"--files\\\" \\\"src\\\")\\n\")\n")))"#
        ]],
    )
}

pub(super) fn backend_producer_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        affe_backend_producer_filter_accumulates_fragments_lines_totals_and_tail_links(),
        affe_backend_producer_filter_restricts_match_region_and_retains_prefix_suffix_properties(),
        affe_backend_producer_sentinel_logs_stderr_marks_done_and_appends_final_fragment_once(),
        affe_backend_producer_start_logs_and_builds_exact_pipe_process_contract(),
    ]
}
