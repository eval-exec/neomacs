use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline workflow, set up exactly as its commentary
/// prescribes: `inf-ruby-mode' added to `ac-modes', TAB rebound to
/// `auto-complete' in `inf-ruby-mode-map', and `ac-inf-ruby-enable' on the mode
/// hook -- called twice here, because a hook that runs again must not add the
/// source twice.  The user then types at the REPL prompt and completes from
/// what the live Ruby process reports, and a second, narrower prefix is asked
/// of the process again and is unique, so auto-complete's dwim expansion
/// inserts it straight away.
fn ac_inf_ruby_completes_a_repl_expression_from_the_live_ruby_process() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_inf_ruby_completes_a_repl_expression_from_the_live_ruby_process",
        r##"(ac-inf-ruby-test-with-repl
 (setq ac-modes (cons 'inf-ruby-mode ac-modes))
 (define-key inf-ruby-mode-map (kbd "TAB") #'auto-complete)
 (setq-local ac-sources nil)
 (ac-inf-ruby-enable)
 (ac-inf-ruby-enable)
 (auto-complete-mode 1)
 (let ((installed (list :sources ac-sources
                        :buffer-local (local-variable-p 'ac-sources)
                        :source ac-source-inf-ruby
                        :in-ac-modes (and (memq 'inf-ruby-mode ac-modes) t)
                        :auto-complete auto-complete-mode
                        :tab (lookup-key inf-ruby-mode-map (kbd "TAB")))))
   (goto-char (point-max))
   (execute-kbd-macro (kbd "S t r TAB"))
   (let ((offered (list (ac-inf-ruby-test-session) (ac-inf-ruby-test-menu))))
     (execute-kbd-macro (kbd "M-n"))
     (let ((moved (ac-inf-ruby-test-session)))
       (execute-kbd-macro (kbd "RET"))
       (let ((completed (ac-inf-ruby-test-buffer-state)))
         (comint-send-input)
         (ac-inf-ruby-test-wait-for-prompt)
         (execute-kbd-macro (kbd "S t r u TAB"))
         (list :installed installed
               :offered offered
               :moved moved
               :completed completed
               :unique (ac-inf-ruby-test-buffer-state)
               :requests (ac-inf-ruby-test-requests)))))))"##,
        expect![[
            r#"OK (:installed (:sources #1=(ac-source-inf-ruby) :buffer-local t :source ((available . ac-inf-ruby-available) (candidates . ac-inf-ruby-candidates) (symbol . "r") (prefix . ac-inf-ruby-prefix)) :in-ac-modes t :auto-complete t :tab auto-complete) :offered ((:prefix "Str" :prefix-start 17 :common "Str" :menu-live t :selected "Str") (("Str" "r") ("String" "r") ("Struct" "r") ("StringIO" "r"))) :moved (:prefix "Str" :prefix-start 17 :common "Str" :menu-live t :selected "String") :completed (:text "irb(main):001:0> String" :point 23 :mode inf-ruby-mode :top-level 0 :auto-complete t :sources #1#) :unique (:text "irb(main):001:0> String\n=> nil\nirb(main):003:0> Struct" :point 54 :mode inf-ruby-mode :top-level 0 :auto-complete t :sources #1#) :requests ("Str" "Stru"))"#
        ]],
    )
}

fn ac_inf_ruby_declines_to_complete_at_a_continuation_prompt() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_inf_ruby_declines_to_complete_at_a_continuation_prompt",
        r##"(ac-inf-ruby-test-with-repl
 (define-key inf-ruby-mode-map (kbd "TAB") #'auto-complete)
 (setq-local ac-sources nil)
 (ac-inf-ruby-enable)
 (auto-complete-mode 1)
 (let ((opened (ac-inf-ruby-test-submit "def greet")))
   (goto-char (point-max))
   (insert "Str")
   (let ((blocked (list :prompt opened
                        :top-level inf-ruby-at-top-level-prompt-p
                        :prefix (ac-inf-ruby-prefix)
                        :started (auto-complete)
                        :menu (ac-inf-ruby-test-menu)
                        :requests (ac-inf-ruby-test-requests))))
     (delete-region (- (point) 3) (point))
     (let ((closed (ac-inf-ruby-test-submit "end")))
       (goto-char (point-max))
       (execute-kbd-macro (kbd "S t r TAB"))
       (let ((offered (list (ac-inf-ruby-test-session) (ac-inf-ruby-test-menu))))
         (ac-abort)
         (list :blocked blocked
               :closed closed
               :top-level inf-ruby-at-top-level-prompt-p
               :offered offered
               :requests (ac-inf-ruby-test-requests)
               :after (ac-inf-ruby-test-buffer-state)))))))"##,
        expect![[
            r#"OK (:blocked (:prompt "irb(main):002:1* " :top-level nil :prefix nil :started nil :menu nil :requests nil) :closed "irb(main):003:0> " :top-level 0 :offered ((:prefix "Str" :prefix-start 74 :common "Str" :menu-live t :selected "Str") (("Str" "r") ("String" "r") ("Struct" "r") ("StringIO" "r"))) :requests ("Str") :after (:text "irb(main):001:0> def greet\nirb(main):002:1* end\n=> :done\nirb(main):003:0> Str" :point 77 :mode inf-ruby-mode :top-level 0 :auto-complete t :sources (ac-source-inf-ruby)))"#
        ]],
    )
    .fresh_process()
}

fn ac_inf_ruby_asks_the_repl_for_a_doubled_receiver_on_a_dotted_expression() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_inf_ruby_asks_the_repl_for_a_doubled_receiver_on_a_dotted_expression",
        r##"(ac-inf-ruby-test-with-repl
 (define-key inf-ruby-mode-map (kbd "TAB") #'auto-complete)
 (setq-local ac-sources nil)
 (ac-inf-ruby-enable)
 (auto-complete-mode 1)
 (goto-char (point-max))
 (execute-kbd-macro (kbd "s t r . t o _ s TAB"))
 (let ((dotted (list (ac-inf-ruby-test-session)
                     (ac-inf-ruby-test-menu)
                     (inf-ruby-completion-target-at-point))))
   (ac-abort)
   (let ((package-requests (ac-inf-ruby-test-requests))
         (control (inf-ruby-completions "to_s")))
     (list :dotted dotted
           :package-requests package-requests
           :inf-ruby-completions control
           :all-requests (ac-inf-ruby-test-requests)
           :after (ac-inf-ruby-test-buffer-state)))))"##,
        expect![[
            r#"OK (:dotted ((:prefix nil :prefix-start nil :common nil :menu-live nil :selected nil) nil "str.") :package-requests ("str.str.to_s") :inf-ruby-completions ("to_s" "to_str" "to_sym") :all-requests ("str.str.to_s" "str.to_s") :after (:text "irb(main):001:0> str.to_s" :point 25 :mode inf-ruby-mode :top-level 0 :auto-complete t :sources (ac-source-inf-ruby)))"#
        ]],
    )
    .fresh_process()
}

fn ac_inf_ruby_source_stays_disabled_once_it_was_compiled_outside_the_repl() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_inf_ruby_source_stays_disabled_once_it_was_compiled_outside_the_repl",
        r##"(ac-inf-ruby-test-with-repl
 (define-key inf-ruby-mode-map (kbd "TAB") #'auto-complete)
 (setq-local ac-sources nil)
 (ac-inf-ruby-enable)
 (auto-complete-mode 1)
 (let ((editor (generate-new-buffer "*project*"))
       elsewhere)
   (with-current-buffer editor
     (set-window-buffer (selected-window) editor)
     (fundamental-mode)
     (setq-local ac-sources (list 'ac-source-inf-ruby))
     (auto-complete-mode 1)
     (insert "Str")
     (setq elsewhere (list :mode major-mode
                           :started (auto-complete)
                           :menu (ac-inf-ruby-test-menu)
                           :cached (get 'ac-source-inf-ruby 'available))))
   (set-buffer "*ruby*")
   (set-window-buffer (selected-window) (current-buffer))
   (goto-char (point-max))
   (execute-kbd-macro (kbd "S t r TAB"))
   (let ((in-repl (list (ac-inf-ruby-test-session) (ac-inf-ruby-test-menu))))
     (ac-abort)
     (list :elsewhere elsewhere
           :cached (get 'ac-source-inf-ruby 'available)
           :in-repl in-repl
           :requests (ac-inf-ruby-test-requests)
           :after (ac-inf-ruby-test-buffer-state)))))"##,
        expect![[
            r#"OK (:elsewhere (:mode fundamental-mode :started nil :menu nil :cached no) :cached no :in-repl ((:prefix nil :prefix-start nil :common nil :menu-live nil :selected nil) nil) :requests nothing-recorded :after (:text "irb(main):001:0> Str" :point 20 :mode inf-ruby-mode :top-level 0 :auto-complete t :sources (ac-source-inf-ruby)))"#
        ]],
    )
    .fresh_process()
}

fn ac_inf_ruby_reports_a_dead_repl_out_of_the_public_command() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_inf_ruby_reports_a_dead_repl_out_of_the_public_command",
        r##"(ac-inf-ruby-test-with-repl
 (define-key inf-ruby-mode-map (kbd "TAB") #'auto-complete)
 (setq-local ac-sources nil)
 (ac-inf-ruby-enable)
 (auto-complete-mode 1)
 (ac-inf-ruby-test-stop-repl)
 (goto-char (point-max))
 (insert "Str")
 (list :process (and (get-buffer-process (current-buffer)) t)
       :outcome (condition-case failure (auto-complete) (error failure))
       :menu (ac-inf-ruby-test-menu)
       :requests (ac-inf-ruby-test-requests)
       :after (ac-inf-ruby-test-buffer-state)))"##,
        expect![[
            r#"OK (:process nil :outcome (error "No current process. See variable inf-ruby-buffers") :menu nil :requests nothing-recorded :after (:text "irb(main):001:0> \nProcess ruby killed\nStr\n" :point 41 :mode inf-ruby-mode :top-level 0 :auto-complete t :sources (ac-source-inf-ruby)))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ac_inf_ruby_completes_a_repl_expression_from_the_live_ruby_process(),
        ac_inf_ruby_declines_to_complete_at_a_continuation_prompt(),
        ac_inf_ruby_asks_the_repl_for_a_doubled_receiver_on_a_dotted_expression(),
        ac_inf_ruby_source_stays_disabled_once_it_was_compiled_outside_the_repl(),
        ac_inf_ruby_reports_a_dead_repl_out_of_the_public_command(),
    ]
}
