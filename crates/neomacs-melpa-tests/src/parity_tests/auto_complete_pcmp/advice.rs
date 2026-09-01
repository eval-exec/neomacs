use expect_test::expect;

use super::ParityBatchCase;

fn auto_complete_pcmp_completions_advice_captures_non_nil_return_once() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_completions_advice_captures_non_nil_return_once",
        r##"(let ((ac-pcmp--active-p t)
             (ac-pcmp--candidates nil)
             (pcomplete-index 0)
             (pcomplete-last 0)
             (pcomplete-command-completion-function
              (lambda () '("alpha" "beta" "gamma"))))
         (cl-letf (((symbol-function 'pcomplete-parse-arguments)
                    (lambda (&optional _expand) t)))
           (let ((first (pcomplete-completions)))
             (setq pcomplete-command-completion-function
                   (lambda () '("later" "ignored")))
             (let ((second (pcomplete-completions)))
               (list
                first
                second
                ac-pcmp--candidates)))))"##,
        expect![[r#"OK (#1=("alpha" "beta" "gamma") ("later" "ignored") #1#)"#]],
    )
}

fn auto_complete_pcmp_completions_advice_ignores_inactive_requests() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_completions_advice_ignores_inactive_requests",
        r##"(let ((ac-pcmp--active-p nil)
             (ac-pcmp--candidates '("preserved"))
             (pcomplete-index 0)
             (pcomplete-last 0)
             (pcomplete-command-completion-function
              (lambda () '("returned"))))
         (cl-letf (((symbol-function 'pcomplete-parse-arguments)
                    (lambda (&optional _expand) t)))
           (list
            (pcomplete-completions)
            ac-pcmp--candidates
            ac-pcmp--status)))"##,
        expect![[r#"OK (("returned") ("preserved") nil)"#]],
    )
    .fresh_process()
}

fn auto_complete_pcmp_show_completions_advice_suppresses_ui_and_captures_input() -> ParityBatchCase
{
    ParityBatchCase::value(
        "auto_complete_pcmp_show_completions_advice_suppresses_ui_and_captures_input",
        r##"(let ((ac-pcmp--active-p t)
             (ac-pcmp--candidates nil)
             (pcomplete-last-window-config :untouched)
             (pcomplete-window-restore-timer nil))
         (list
          (pcomplete-show-completions
           '("zeta" "alpha" "middle"))
          ac-pcmp--candidates
          pcomplete-last-window-config
          (get-buffer "*Completions*")))"##,
        expect![[r#"OK (nil ("zeta" "alpha" "middle") :untouched nil)"#]],
    )
}

fn auto_complete_pcmp_show_completions_advice_preserves_first_capture() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_show_completions_advice_preserves_first_capture",
        r##"(let ((ac-pcmp--active-p t)
             (ac-pcmp--candidates '("already")))
         (list
          (pcomplete-show-completions '("new" "ignored"))
          ac-pcmp--candidates
          (pcomplete-show-completions nil)
          ac-pcmp--candidates))"##,
        expect![[r#"OK (nil #1=("already") nil #1#)"#]],
    )
}

fn auto_complete_pcmp_stub_advice_captures_original_candidate_collection() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_stub_advice_captures_original_candidate_collection",
        r##"(let ((ac-pcmp--active-p t)
             (ac-pcmp--candidates nil)
             (ac-pcmp--status 'none))
         (with-temp-buffer
           (insert "ca")
           (let ((result
                  (pcomplete-stub
                   "ca"
                   '("cargo" "cache" "cat"))))
             (list
              result
              ac-pcmp--candidates
              ac-pcmp--status
              (buffer-string)
              (point)))))"##,
        expect![[r#"OK (nil ("cargo" "cache" "cat") nil "ca" 3)"#]],
    )
}

fn auto_complete_pcmp_stub_advice_maps_real_completion_outcomes_to_status() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_stub_advice_maps_real_completion_outcomes_to_status",
        r##"(mapcar
         (lambda (case)
           (with-temp-buffer
             (insert (nth 0 case))
             (let ((ac-pcmp--active-p t)
                   (ac-pcmp--candidates nil)
                   (ac-pcmp--status 'none)
                   (pcomplete-termination-string " ")
                   (pcomplete-suffix-list '(?/ ?:)))
               (list
                case
                (pcomplete-stub (nth 0 case) (nth 1 case))
                ac-pcmp--status
                ac-pcmp--candidates
                (buffer-string)))))
         '(("fo" ("foo"))
           ("fo" ("foobar" "foobaz"))
           ("foo" ("foo" "foobar"))
           ("zz" ("alpha" "beta"))))"##,
        expect![[
            r#"OK ((("fo" #1=("foo")) nil sole #1# "fo") (("fo" #2=("foobar" "foobaz")) nil nil #2# "fo") (("foo" #3=("foo" "foobar")) nil nil #3# "foo") (("zz" #4=("alpha" "beta")) nil nil #4# "zz"))"#
        ]],
    )
}

fn auto_complete_pcmp_stub_advice_does_not_overwrite_existing_candidates() -> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_stub_advice_does_not_overwrite_existing_candidates",
        r##"(with-temp-buffer
         (insert "al")
         (let ((ac-pcmp--active-p t)
               (ac-pcmp--candidates '("first" "capture"))
               (ac-pcmp--status 'none))
           (list
            (pcomplete-stub "al" '("alpha" "alpine"))
            ac-pcmp--candidates
            ac-pcmp--status
            (buffer-string))))"##,
        expect![[r#"OK (nil ("first" "capture") nil "al")"#]],
    )
}

fn auto_complete_pcmp_stub_advice_inactive_path_preserves_native_return_and_state()
-> ParityBatchCase {
    ParityBatchCase::value(
        "auto_complete_pcmp_stub_advice_inactive_path_preserves_native_return_and_state",
        r##"(with-temp-buffer
         (insert "fo")
         (let ((ac-pcmp--active-p nil)
               (ac-pcmp--candidates '("outer"))
               (ac-pcmp--status 'outer)
               (pcomplete-termination-string " ")
               (pcomplete-suffix-list '(?/ ?:)))
           (list
            (pcomplete-stub "fo" '("foo"))
            ac-pcmp--candidates
            ac-pcmp--status
            (buffer-string)
            (point))))"##,
        expect![[r#"OK ((sole . "foo") ("outer") outer "fo" 3)"#]],
    )
}

pub(super) fn advice_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        auto_complete_pcmp_completions_advice_captures_non_nil_return_once(),
        auto_complete_pcmp_completions_advice_ignores_inactive_requests(),
        auto_complete_pcmp_show_completions_advice_suppresses_ui_and_captures_input(),
        auto_complete_pcmp_show_completions_advice_preserves_first_capture(),
        auto_complete_pcmp_stub_advice_captures_original_candidate_collection(),
        auto_complete_pcmp_stub_advice_maps_real_completion_outcomes_to_status(),
        auto_complete_pcmp_stub_advice_does_not_overwrite_existing_candidates(),
        auto_complete_pcmp_stub_advice_inactive_path_preserves_native_return_and_state(),
    ]
}
