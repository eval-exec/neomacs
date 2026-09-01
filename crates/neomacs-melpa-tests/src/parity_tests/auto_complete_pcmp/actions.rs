use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_pcmp_action_appends_termination_for_sole_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_action_appends_termination_for_sole_completion",
        r##"(with-temp-buffer
         (insert "git checkout")
         (let ((ac-pcmp--status 'sole)
               (ac-pcmp--point 5)
               (pcomplete-stub "ch")
               (pcomplete-last-completion-raw nil)
               (pcomplete-termination-string " ")
               (pcomplete-suffix-list '(?/ ?:)))
           (ac-pcmp/do-ac-action)
           (list
            (buffer-string)
            (point)
            pcomplete-last-completion-length
            pcomplete-last-completion-stub)))"##,
        expect![[r#"OK ("git checkout " 14 9 "ch")"#]],
    )
}

fn auto_complete_pcmp_action_appends_termination_for_shortest_completion() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_action_appends_termination_for_shortest_completion",
        r##"(with-temp-buffer
         (insert "cargo ch")
         (let ((ac-pcmp--status 'shortest)
               (ac-pcmp--point 7)
               (pcomplete-stub "c")
               (pcomplete-last-completion-raw nil)
               (pcomplete-termination-string "::")
               (pcomplete-suffix-list '(?/ ?:)))
           (ac-pcmp/do-ac-action)
           (list
            (buffer-string)
            pcomplete-last-completion-length
            pcomplete-last-completion-stub)))"##,
        expect![[r#"OK ("cargo ch::" 4 "c")"#]],
    )
}

fn auto_complete_pcmp_action_does_not_append_for_partial_none_or_unknown_status() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_pcmp_action_does_not_append_for_partial_none_or_unknown_status",
        r##"(mapcar
         (lambda (status)
           (with-temp-buffer
             (insert "tool candidate")
             (let ((ac-pcmp--status status)
                   (ac-pcmp--point 6)
                   (pcomplete-stub "ca")
                   (pcomplete-last-completion-raw nil)
                   (pcomplete-termination-string " ")
                   (pcomplete-suffix-list '(?/ ?:)))
               (ac-pcmp/do-ac-action)
               (list
                status
                (buffer-string)
                pcomplete-last-completion-length
                pcomplete-last-completion-stub))))
         '(partial none exact nil custom))"##,
        expect![[
            r#"OK ((partial "tool candidate" 9 "ca") (none "tool candidate" 9 "ca") (exact "tool candidate" 9 "ca") (nil "tool candidate" 9 "ca") (custom "tool candidate" 9 "ca"))"#
        ]],
    )
}

fn auto_complete_pcmp_action_respects_existing_suffix_characters() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_action_respects_existing_suffix_characters",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (car case))
             (let ((ac-pcmp--status 'sole)
                   (ac-pcmp--point 1)
                   (pcomplete-stub "")
                   (pcomplete-last-completion-raw nil)
                   (pcomplete-termination-string " ")
                   (pcomplete-suffix-list '(?/ ?: ?=)))
               (ac-pcmp/do-ac-action)
               (list case
                     (buffer-string)
                     pcomplete-last-completion-length))))
         '(("directory/" . slash)
           ("host:" . colon)
           ("option=" . equals)
           ("word" . ordinary)))"##,
        expect![[
            r#"OK ((("directory/" . slash) "directory/" 10) (("host:" . colon) "host:" 5) (("option=" . equals) "option=" 7) (("word" . ordinary) "word " 5))"#
        ]],
    )
}

fn auto_complete_pcmp_action_measures_inserted_span_from_saved_candidate_point() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_pcmp_action_measures_inserted_span_from_saved_candidate_point",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (nth 0 case))
             (let ((ac-pcmp--status (nth 1 case))
                   (ac-pcmp--point (nth 2 case))
                   (pcomplete-stub (nth 3 case))
                   (pcomplete-last-completion-raw nil)
                   (pcomplete-termination-string " ")
                   (pcomplete-suffix-list '(?/ ?:)))
               (ac-pcmp/do-ac-action)
               (list
                case
                (point)
                pcomplete-last-completion-length
                pcomplete-last-completion-stub))))
         '(("abc" partial 1 "")
           ("prefix-value" partial 8 "val")
           ("x" sole 2 "x")
           ("command/" sole 4 "mand")))"##,
        expect![[
            r#"OK ((("abc" partial 1 "") 4 3 "") (("prefix-value" partial 8 "val") 13 5 "val") (("x" sole 2 "x") 3 1 "x") (("command/" sole 4 "mand") 9 5 "mand"))"#
        ]],
    )
}

fn auto_complete_pcmp_action_preserves_stub_object_without_copying() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_action_preserves_stub_object_without_copying",
        r##"(let ((stub (propertize "ca" 'face 'bold 'origin '(1 2))))
         (with-temp-buffer
           (insert "candidate")
           (let ((ac-pcmp--status 'partial)
                 (ac-pcmp--point 1)
                 (pcomplete-stub stub)
                 (pcomplete-last-completion-raw t)
                 (pcomplete-termination-string " ")
                 (pcomplete-suffix-list '(?/ ?:)))
             (ac-pcmp/do-ac-action)
             (list
              (eq stub pcomplete-last-completion-stub)
              pcomplete-last-completion-stub
              (text-properties-at
               0 pcomplete-last-completion-stub)
              pcomplete-last-completion-length))))"##,
        expect![[r#"OK (t #("ca" 0 2 (face bold origin (1 2))) (face bold origin (1 2)) 9)"#]],
    )
}

fn auto_complete_pcmp_action_raw_flag_does_not_change_bookkeeping_contract() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_action_raw_flag_does_not_change_bookkeeping_contract",
        r##"(mapcar
         (lambda (raw)
           (with-temp-buffer
             (insert "quoted value")
             (let ((ac-pcmp--status 'partial)
                   (ac-pcmp--point 8)
                   (pcomplete-stub "va")
                   (pcomplete-last-completion-raw raw)
                   (pcomplete-termination-string " ")
                   (pcomplete-suffix-list '(?/ ?:)))
               (ac-pcmp/do-ac-action)
               (list
                raw
                (buffer-string)
                pcomplete-last-completion-length
                pcomplete-last-completion-stub))))
         '(nil t raw-marker))"##,
        expect![[
            r#"OK ((nil "quoted value" 5 "va") (t "quoted value" 5 "va") (raw-marker "quoted value" 5 "va"))"#
        ]],
    )
}

fn auto_complete_pcmp_action_errors_are_reported_without_partial_bookkeeping() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_action_errors_are_reported_without_partial_bookkeeping",
        r##"(with-temp-buffer
         (insert "candidate")
         (let ((ac-pcmp--status 'partial)
               (ac-pcmp--point "not-a-position")
               (pcomplete-stub "ca")
               (pcomplete-last-completion-raw nil)
               (pcomplete-last-completion-length :old-length)
               (pcomplete-last-completion-stub :old-stub)
               (pcomplete-termination-string " ")
               (pcomplete-suffix-list '(?/ ?:)))
           (let ((result (ac-pcmp/do-ac-action)))
             (list
              result
              (current-message)
              pcomplete-last-completion-length
              pcomplete-last-completion-stub
              (buffer-string)))))"##,
        expect![[r#"OK (nil nil :old-length :old-stub "candidate")"#]],
    )
}

fn auto_complete_pcmp_self_insert_command_forwards_count_and_trigger_metadata() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_self_insert_command_forwards_count_and_trigger_metadata",
        r##"(let (events)
         (cl-letf (((symbol-function 'self-insert-command)
                    (lambda (count)
                      (push (list :insert count) events)
                      (insert (make-string count ?x))))
                   ((symbol-function 'auto-complete-1)
                    (lambda (&rest arguments)
                      (push (cons :complete arguments) events)
                      :started)))
           (with-temp-buffer
             (let ((result
                    (ac-pcmp/self-insert-command-with-ac-start 4)))
               (list
                result
                (buffer-string)
                (nreverse events))))))"##,
        expect![[r#"OK (:started "xxxx" ((:insert 4) (:complete :triggered trigger-key)))"#]],
    )
}

fn auto_complete_pcmp_candidate_then_action_models_real_auto_complete_lifecycle() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_pcmp_candidate_then_action_models_real_auto_complete_lifecycle",
        r##"(with-temp-buffer
         (insert "git che")
         (let ((pcomplete-termination-string " ")
               (pcomplete-suffix-list '(?/ ?:))
               (pcomplete-stub "che")
               (pcomplete-last-completion-raw nil))
           (cl-letf (((symbol-function 'call-interactively)
                      (lambda (_command)
                        (pcomplete-stub
                         "che"
                         '("checkout")))))
             (let ((candidates (ac-pcmp/get-ac-candidates)))
               (goto-char (point-max))
               (ac-pcmp/do-ac-action)
               (list
                candidates
                ac-pcmp--status
                (buffer-string)
                pcomplete-last-completion-length
                pcomplete-last-completion-stub)))))"##,
        expect![[r#"OK (("checkout") sole "git che " 1 "che")"#]],
    )
}

pub(super) fn actions_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_pcmp_action_appends_termination_for_sole_completion(),
        auto_complete_pcmp_action_appends_termination_for_shortest_completion(),
        auto_complete_pcmp_action_does_not_append_for_partial_none_or_unknown_status(),
        auto_complete_pcmp_action_respects_existing_suffix_characters(),
        auto_complete_pcmp_action_measures_inserted_span_from_saved_candidate_point(),
        auto_complete_pcmp_action_preserves_stub_object_without_copying(),
        auto_complete_pcmp_action_raw_flag_does_not_change_bookkeeping_contract(),
        auto_complete_pcmp_action_errors_are_reported_without_partial_bookkeeping(),
        auto_complete_pcmp_self_insert_command_forwards_count_and_trigger_metadata(),
        auto_complete_pcmp_candidate_then_action_models_real_auto_complete_lifecycle(),
    ]
}
