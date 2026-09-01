use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_pcmp_custom_command_dispatch_returns_real_pcomplete_choices() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_custom_command_dispatch_returns_real_pcomplete_choices",
        r##"(progn
         (fset
          'pcomplete/ac-pcmp-fixture-mode/tool
          (lambda ()
            (pcomplete-here
             '("build" "check" "clean" "clippy"))))
         (with-temp-buffer
           (setq major-mode 'ac-pcmp-fixture-mode)
           (insert "tool ")
           (list
            (ac-pcmp/get-ac-candidates)
            ac-pcmp--status
            ac-pcmp--point
            (buffer-string))))"##,
        expect![[r#"OK (("build" "check" "clean" "clippy") nil 6 "tool ")"#]],
    )
}

fn auto_complete_pcmp_custom_command_prefix_preserves_full_source_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_custom_command_prefix_preserves_full_source_candidates",
        r##"(progn
         (fset
          'pcomplete/ac-pcmp-fixture-mode/deploy
          (lambda ()
            (pcomplete-here
             '("production" "preview" "staging" "sandbox"))))
         (mapcar
          (lambda (prefix)
            (with-temp-buffer
              (setq major-mode 'ac-pcmp-fixture-mode)
              (insert "deploy " prefix)
              (list
               prefix
               (ac-pcmp/get-ac-candidates)
               ac-pcmp--status
               (buffer-string))))
          '("" "p" "pro" "s" "missing")))"##,
        expect![[
            r#"OK (("" #1=("production" "preview" "staging" "sandbox") nil "deploy ") ("p" #1# nil "deploy p") ("pro" #1# sole "deploy pro") ("s" #1# nil "deploy s") ("missing" #1# nil "deploy missing"))"#
        ]],
    )
}

fn auto_complete_pcmp_candidate_selection_action_completes_command_lifecycle() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_candidate_selection_action_completes_command_lifecycle",
        r##"(progn
         (fset
          'pcomplete/ac-pcmp-fixture-mode/git
          (lambda ()
            (pcomplete-here
             '("checkout" "cherry-pick" "cherry"))))
         (with-temp-buffer
           (setq major-mode 'ac-pcmp-fixture-mode)
           (insert "git ch")
           (let ((candidates (ac-pcmp/get-ac-candidates)))
             (delete-region ac-pcmp--point (point))
             (insert "checkout")
             (let ((pcomplete-stub "ch")
                   (pcomplete-last-completion-raw nil)
                   (pcomplete-termination-string " ")
                   (pcomplete-suffix-list '(?/ ?:)))
               (ac-pcmp/do-ac-action)
               (list
                candidates
                ac-pcmp--status
                (buffer-string)
                pcomplete-last-completion-length
                pcomplete-last-completion-stub)))))"##,
        expect![[r#"OK (("checkout" "cherry-pick" "cherry") nil "git chcheckout" 8 "ch")"#]],
    )
}

fn auto_complete_pcmp_real_file_completion_reads_deterministic_workspace_entries() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_pcmp_real_file_completion_reads_deterministic_workspace_entries",
        r##"(let* ((root
                                 (expand-file-name
                                  "auto-complete-pcmp-files"
                                  default-directory))
              (default-directory
               (file-name-as-directory root)))
         (when (file-exists-p root)
           (delete-directory root t))
         (unwind-protect
             (progn
               (make-directory (expand-file-name "alpha-dir" root) t)
               (with-temp-file (expand-file-name "alpha.txt" root)
                 (insert "alpha"))
               (with-temp-file (expand-file-name "beta.txt" root)
                 (insert "beta"))
               (fset
                'pcomplete/ac-pcmp-file-mode/open
                (lambda ()
                  (pcomplete-here
                   (pcomplete-entries))))
               (with-temp-buffer
                 (setq major-mode 'ac-pcmp-file-mode)
                 (insert "open al")
                 (let ((candidates
                        (ac-pcmp/get-ac-candidates)))
                   (list
                    (functionp candidates)
                    (sort
                     (all-completions "al" candidates)
                     #'string<)
                    ac-pcmp--status
                    (buffer-string)))))
           (when (file-exists-p root)
             (delete-directory root t))))"##,
        expect![[r#"OK (t ("alpha-dir/" "alpha.txt") nil "open al")"#]],
    )
}

fn auto_complete_pcmp_real_command_failure_is_contained_and_next_request_recovers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_real_command_failure_is_contained_and_next_request_recovers",
        r##"(progn
         (fset
          'pcomplete/ac-pcmp-fixture-mode/fail
          (lambda ()
            (error "completion backend unavailable")))
         (fset
          'pcomplete/ac-pcmp-fixture-mode/ok
          (lambda ()
            (pcomplete-here
             '("recovered" "ready"))))
         (with-temp-buffer
           (setq major-mode 'ac-pcmp-fixture-mode)
           (insert "fail ")
           (let ((failed (ac-pcmp/get-ac-candidates))
                 (failure-message (current-message)))
             (erase-buffer)
             (insert "ok ")
             (let ((recovered (ac-pcmp/get-ac-candidates)))
               (list
                failed
                failure-message
                recovered
                ac-pcmp--status
                ac-pcmp--point
                (buffer-string))))))"##,
        expect![[r#"OK (nil nil ("recovered" "ready") nil 4 "ok ")"#]],
    )
}

fn auto_complete_pcmp_repeated_commands_refresh_candidates_status_and_point() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_repeated_commands_refresh_candidates_status_and_point",
        r##"(progn
         (fset
          'pcomplete/ac-pcmp-fixture-mode/first
          (lambda ()
            (pcomplete-here
             '("one" "only"))))
         (fset
          'pcomplete/ac-pcmp-fixture-mode/second
          (lambda ()
            (pcomplete-here
             '("two" "three" "four"))))
         (with-temp-buffer
           (setq major-mode 'ac-pcmp-fixture-mode)
           (insert "first o")
           (let ((first-candidates
                  (ac-pcmp/get-ac-candidates))
                 (first-status ac-pcmp--status)
                 (first-point ac-pcmp--point))
             (erase-buffer)
             (insert "second t")
             (let ((second-candidates
                    (ac-pcmp/get-ac-candidates)))
               (list
                first-candidates
                first-status
                first-point
                second-candidates
                ac-pcmp--status
                ac-pcmp--point
                (buffer-string))))))"##,
        expect![[r#"OK (("one" "only") nil 8 ("two" "three" "four") nil 9 "second t")"#]],
    )
}

fn auto_complete_pcmp_unique_candidate_and_action_append_exact_termination() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_unique_candidate_and_action_append_exact_termination",
        r##"(progn
         (fset
          'pcomplete/ac-pcmp-fixture-mode/run
          (lambda ()
            (pcomplete-here
             '("release"))))
         (with-temp-buffer
           (setq major-mode 'ac-pcmp-fixture-mode)
           (insert "run rel")
           (let ((candidates
                  (ac-pcmp/get-ac-candidates)))
             (let ((pcomplete-stub "rel")
                   (pcomplete-last-completion-raw nil)
                   (pcomplete-termination-string " -> ")
                   (pcomplete-suffix-list '(?/ ?:)))
               (ac-pcmp/do-ac-action)
               (list
                candidates
                ac-pcmp--status
                (buffer-string)
                pcomplete-last-completion-length
                pcomplete-last-completion-stub)))))"##,
        expect![[r#"OK (("release") sole "run rel -> " 4 "rel")"#]],
    )
}

fn auto_complete_pcmp_self_insert_source_trigger_then_completion_pipeline() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_self_insert_source_trigger_then_completion_pipeline",
        r##"(let (events)
         (cl-letf (((symbol-function 'self-insert-command)
                    (lambda (count)
                      (insert (make-string count ?-))
                      (push (list :insert count (buffer-string)) events)))
                   ((symbol-function 'auto-complete-1)
                    (lambda (&rest arguments)
                      (push
                       (list :complete arguments (buffer-string))
                       events)
                      :completion-started)))
           (with-temp-buffer
             (insert "tool")
             (let ((result
                    (ac-pcmp/self-insert-command-with-ac-start 2)))
               (list
                result
                (buffer-string)
                (nreverse events))))))"##,
        expect![[
            r#"OK (:completion-started "tool--" ((:insert 2 "tool--") (:complete (:triggered trigger-key) "tool--")))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_pcmp_custom_command_dispatch_returns_real_pcomplete_choices(),
        auto_complete_pcmp_custom_command_prefix_preserves_full_source_candidates(),
        auto_complete_pcmp_candidate_selection_action_completes_command_lifecycle(),
        auto_complete_pcmp_real_file_completion_reads_deterministic_workspace_entries(),
        auto_complete_pcmp_real_command_failure_is_contained_and_next_request_recovers(),
        auto_complete_pcmp_repeated_commands_refresh_candidates_status_and_point(),
        auto_complete_pcmp_unique_candidate_and_action_append_exact_termination(),
        auto_complete_pcmp_self_insert_source_trigger_then_completion_pipeline(),
    ]
}
