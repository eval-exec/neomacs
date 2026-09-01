use expect_test::expect;

use super::ParityBatchCase;

fn ast_grep_sync_search_runs_prompt_stream_completion_and_real_file_jump() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_sync_search_runs_prompt_stream_completion_and_real_file_jump",
        r##"(let* ((file
                (ast-grep-test-write-file
                 "sync/src/app.js"
                 "zero\n  console.log(value)\nlast\n"))
               (json
                (format
                 "{\"file\":%S,\"range\":{\"start\":{\"line\":1,\"column\":2}},\"text\":\"console.log(value)\"}\n"
                 file))
               calls)
          (unwind-protect
              (cl-letf (((symbol-function 'read-string)
                         (lambda (prompt initial history)
                           (push
                            (list :read prompt initial history)
                            calls)
                           "console.log($A)"))
                        ((symbol-function 'ast-grep--run-command)
                         (lambda (pattern directory)
                           (push
                            (list :run pattern directory)
                            calls)
                           json))
                        ((symbol-function 'completing-read)
                         (lambda (prompt collection predicate require-match
                                         initial history)
                           (let ((all (all-completions "" collection)))
                             (push
                              (list
                               :complete prompt predicate require-match
                               initial history
                               (mapcar #'substring-no-properties all)
                               (funcall collection "" nil 'metadata))
                              calls)
                             (car all)))))
                (ast-grep--search-sync "/fixture/project/")
                (list
                 (nreverse calls)
                 (equal (file-truename buffer-file-name)
                        (file-truename file))
                 (line-number-at-pos)
                 (- (point) (line-beginning-position))
                 (buffer-substring-no-properties
                  (point)
                  (min (+ (point) 11) (point-max)))
                 (hash-table-count ast-grep--candidate-table)))
            (ast-grep-test-kill-file-buffer file)))"##,
        expect![[
            r#"OK (((:read "ast-grep pattern: " nil ast-grep-history) (:run "console.log($A)" "/fixture/project/") (:complete "ast-grep [console.log($A)]: " nil t nil ast-grep-history ("[ORACLE-SANDBOX]/sync/src/app.js:2:2:console.log(value)") (metadata (affixation-function . ast-grep--affixation)))) t 2 2 "console.log" 1)"#
        ]],
    )
}

fn ast_grep_sync_search_no_matches_reports_pattern_and_never_prompts_for_selection()
-> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_sync_search_no_matches_reports_pattern_and_never_prompts_for_selection",
        r##"(let (messages completion-called)
          (cl-letf (((symbol-function 'read-string)
                     (lambda (&rest _) "$A && $A()"))
                    ((symbol-function 'ast-grep--run-command)
                     (lambda (&rest _) "\nmalformed\n"))
                    ((symbol-function 'completing-read)
                     (lambda (&rest _)
                       (setq completion-called t)))
                    ((symbol-function 'message)
                     (lambda (format-string &rest args)
                       (push
                        (apply #'format format-string args)
                        messages))))
            (list
             (ast-grep--search-sync "/fixture/")
             (nreverse messages)
             completion-called
             (hash-table-count ast-grep--candidate-table))))"##,
        expect![[r#"OK (nil ("No matches found for pattern: $A && $A()") nil 0)"#]],
    )
}

fn ast_grep_sync_new_session_discards_stale_registry_before_user_selection() -> ParityBatchCase {
    ParityBatchCase::value(
        "ast_grep_sync_new_session_discards_stale_registry_before_user_selection",
        r##"(let* ((stale
                (ast-grep--format-candidate
                 '(:file "stale.rs" :start-line 9 :start-column 9
                   :text "stale")))
               stale-visible selected)
          (cl-letf (((symbol-function 'read-string)
                     (lambda (&rest _) "fresh"))
                    ((symbol-function 'ast-grep--run-command)
                     (lambda (&rest _)
                       "{\"file\":\"fresh.rs\",\"range\":{\"start\":{\"line\":1,\"column\":2}},\"text\":\"fresh\"}\n"))
                    ((symbol-function 'completing-read)
                     (lambda (_prompt collection &rest _)
                       (setq stale-visible
                             (ast-grep-test-match-summary
                              (substring-no-properties stale)))
                       (car (all-completions "" collection))))
                    ((symbol-function 'ast-grep--goto-match)
                     (lambda (candidate)
                       (setq selected
                             (ast-grep-test-match-summary candidate)))))
            (ast-grep--search-sync "/fixture/")
            (list
             stale-visible
             selected
             (hash-table-count ast-grep--candidate-table))))"##,
        expect![[
            r#"OK (("stale.rs" 9 9 nil nil nil nil) ("fresh.rs" 1 2 nil nil "fresh" nil) 1)"#
        ]],
    )
}

pub(super) fn sync_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ast_grep_sync_search_runs_prompt_stream_completion_and_real_file_jump(),
        ast_grep_sync_search_no_matches_reports_pattern_and_never_prompts_for_selection(),
        ast_grep_sync_new_session_discards_stale_registry_before_user_selection(),
    ]
}
