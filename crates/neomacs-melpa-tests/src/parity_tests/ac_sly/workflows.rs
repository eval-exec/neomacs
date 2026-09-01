use expect_test::expect;

use super::ParityBatchCase;

/// The package's headline workflow: connected to a Lisp, the user types a
/// symbol prefix in a lisp buffer and completes from what the Lisp reports.
/// `set-up-sly-ac' without an argument installs the simple source, whose
/// `match' function case-corrects the Lisp's lowercase symbols to the case the
/// user typed -- so the second half types an upper-case prefix and gets
/// upper-case candidates back for the same Lisp answer.
fn ac_sly_completes_a_typed_symbol_in_a_lisp_buffer_from_the_live_connection() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_sly_completes_a_typed_symbol_in_a_lisp_buffer_from_the_live_connection",
        r##"(ac-sly-test-session
 (ac-sly-test-connect)
 (ac-sly-test-lisp-buffer "(")
 (ac-set-trigger-key "TAB")
 (set-up-sly-ac)
 (auto-complete-mode 1)
 (let ((installed (list :sources ac-sources
                        :source ac-source-sly-simple
                        :connected (and (sly-connected-p) t)
                        :implementation (sly-lisp-implementation-name))))
   (execute-kbd-macro (kbd "c a TAB"))
   (let ((offered (list (ac-sly-test-session-state) (ac-sly-test-menu))))
     (execute-kbd-macro (kbd "M-n"))
     (let ((moved (ac-sly-test-session-state)))
       (execute-kbd-macro (kbd "RET"))
       (let ((completed (ac-sly-test-buffer-state)))
         (erase-buffer)
         (insert "(")
         (execute-kbd-macro (kbd "C A TAB"))
         (let ((upper (list (ac-sly-test-session-state) (ac-sly-test-menu))))
           (execute-kbd-macro (kbd "RET"))
           (list :installed installed
                 :offered offered
                 :moved moved
                 :completed completed
                 :upper-case upper
                 :after (ac-sly-test-buffer-state)
                 :rpcs (ac-sly-test-rpcs))))))))"##,
        expect![[
            r#"OK (:installed (:sources #1=(ac-source-sly-simple) :source ((init . ac-sly-init) (candidates . ac-source-sly-simple-candidates) (candidate-face . ac-sly-menu-face) (selection-face . ac-sly-selection-face) (prefix . sly-symbol-start-pos) (symbol . "l") (document . ac-sly-documentation) (match . ac-source-sly-case-correcting-completions)) :connected t :implementation "sbcl") :offered ((:prefix "ca" :prefix-start 1 :common "ca" :menu-live t :selected "car") (("car" "l" nil nil) ("cadr" "l" nil completions-first-difference) ("case" "l" nil completions-first-difference) ("catch" "l" nil completions-first-difference))) :moved (:prefix "ca" :prefix-start 1 :common "ca" :menu-live t :selected "cadr") :completed (:text "(cadr" :point 5 :mode lisp-mode :connected t :auto-complete t :sources #1#) :upper-case ((:prefix "CA" :prefix-start 1 :common "CA" :menu-live t :selected "CAr") (("CAr" "l" nil nil) ("CAdr" "l" nil completions-first-difference) ("CAse" "l" nil completions-first-difference) ("CAtch" "l" nil completions-first-difference))) :after (:text "(CAr" :point 4 :mode lisp-mode :connected t :auto-complete t :sources #1#) :rpcs ((slynk:connection-info) (slynk-completion:simple-completions "ca" 'nil) (slynk-completion:simple-completions "CA" 'nil)))"#
        ]],
    )
}

fn ac_sly_fuzzy_source_shows_the_lisps_flags_unless_the_option_is_off() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_sly_fuzzy_source_shows_the_lisps_flags_unless_the_option_is_off",
        r##"(ac-sly-test-session
 (ac-sly-test-connect)
 (ac-sly-test-lisp-buffer "(str")
 (set-up-sly-ac t)
 (auto-complete-mode 1)
 (auto-complete)
 (let ((with-flags (list :sources ac-sources
                         :show-flags ac-sly-show-flags
                         :session (ac-sly-test-session-state)
                         :menu (ac-sly-test-menu))))
   (ac-next)
   (ac-complete)
   (let ((completed (ac-sly-test-buffer-state)))
     (setq ac-sly-show-flags nil)
     (erase-buffer)
     (insert "(str")
     (auto-complete)
     (let ((without-flags (ac-sly-test-menu)))
       (ac-abort)
       (list :with-flags with-flags
             :completed completed
             :without-flags without-flags
             :after (ac-sly-test-buffer-state)
             :rpcs (ac-sly-test-rpcs))))))"##,
        expect![[
            r#"OK (:with-flags (:sources #1=(ac-source-sly-fuzzy) :show-flags t :session (:prefix "string" :prefix-start 1 :common "string" :menu-live t :selected "string") :menu (("string" "l" "-f---- 87.50%" nil) ("string=" "l" "-f---- 80.00%" nil) ("stringp" "l" "-f---- 72.50%" nil))) :completed (:text "(string=" :point 8 :mode lisp-mode :connected t :auto-complete t :sources #1#) :without-flags (("string" "l" nil nil) ("string=" "l" nil nil) ("stringp" "l" nil nil)) :after (:text "(string" :point 7 :mode lisp-mode :connected t :auto-complete t :sources #1#) :rpcs ((slynk:connection-info) (slynk-completion:flex-completions "str" 'nil) (slynk-completion:flex-completions "str" 'nil)))"#
        ]],
    )
}

fn ac_sly_completes_at_the_sly_repl_prompt_after_set_up_sly_ac() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_sly_completes_at_the_sly_repl_prompt_after_set_up_sly_ac",
        r##"(ac-sly-test-session
 (setq ac-sly-test-contribs '(sly-mrepl))
 (ac-sly-test-connect)
 (ac-sly-test-repl-buffer)
 (setq-local ac-sources nil)
 (set-up-sly-ac)
 (auto-complete-mode 1)
 (goto-char (point-max))
 (let ((repl (list :buffer (buffer-name)
                   :mode major-mode
                   :sources ac-sources
                   :prompt (buffer-substring-no-properties (point-min) (point-max)))))
   (insert "(ca")
   (auto-complete)
   (let ((offered (list (ac-sly-test-session-state) (ac-sly-test-menu))))
     (ac-next)
     (ac-complete)
     (list :repl repl
           :offered offered
           :after (ac-sly-test-buffer-state)
           :rpcs (ac-sly-test-rpcs)))))"##,
        expect![[
            r#"OK (:repl (:buffer "*sly-mrepl for sbcl*" :mode sly-mrepl-mode :sources #1=(ac-source-sly-simple) :prompt "CL-USER> ") :offered ((:prefix "ca" :prefix-start 10 :common "ca" :menu-live t :selected "car") (("car" "l" nil nil) ("cadr" "l" nil completions-first-difference) ("case" "l" nil completions-first-difference) ("catch" "l" nil completions-first-difference))) :after (:text "CL-USER> (cadr" :point 14 :mode sly-mrepl-mode :connected t :auto-complete t :sources #1#) :rpcs ((slynk:connection-info) (slynk:slynk-add-load-paths :elided) (slynk:slynk-require '("slynk/mrepl" "slynk/arglists")) (slynk-mrepl:create-mrepl 1) (slynk-completion:simple-completions "ca" 'nil)))"#
        ]],
    )
}

fn ac_sly_documentation_asks_the_lisp_for_a_swank_symbol_it_cannot_read() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_sly_documentation_asks_the_lisp_for_a_swank_symbol_it_cannot_read",
        r##"(ac-sly-test-session
 (ac-sly-test-connect)
 (ac-sly-test-lisp-buffer "(ca")
 (set-up-sly-ac)
 (auto-complete-mode 1)
 (auto-complete)
 (let* ((candidate (car ac-candidates))
        (documented (list :candidate (substring-no-properties candidate)
                          :document (popup-item-document candidate)
                          :quick-help (condition-case failure
                                          (popup-item-documentation candidate)
                                        (error failure)))))
   (ac-abort)
   (let ((control (sly-eval '(slynk:documentation-symbol "car"))))
     (list :documented documented
           :slynk-documentation control
           :rpcs (ac-sly-test-rpcs)))))"##,
        expect![[
            r#"OK (:documented (:candidate "car" :document ac-sly-documentation :quick-help (error "Synchronous Lisp Evaluation aborted")) :slynk-documentation "Return the car of LIST.  Signals TYPE-ERROR otherwise." :rpcs ((slynk:connection-info) (slynk-completion:simple-completions "ca" 'nil) (swank:documentation-symbol "car") (slynk:documentation-symbol "car")))"#
        ]],
    )
    .fresh_process()
}

fn ac_sly_offers_nothing_until_a_lisp_connection_exists() -> ParityBatchCase {
    ParityBatchCase::value(
        "ac_sly_offers_nothing_until_a_lisp_connection_exists",
        r##"(ac-sly-test-session
 (ac-sly-test-lisp-buffer "(ca")
 (set-up-sly-ac)
 (auto-complete-mode 1)
 (let ((disconnected (list :connected (and (sly-connected-p) t)
                           :sources ac-sources
                           :started (auto-complete)
                           :menu (ac-sly-test-menu)
                           :state (ac-sly-test-buffer-state)
                           :rpcs (ac-sly-test-rpcs))))
   (ac-abort)
   (ac-sly-test-connect)
   (set-buffer "*ac-sly-workflow*")
   (set-window-buffer (selected-window) (current-buffer))
   (goto-char (point-max))
   (auto-complete)
   (let ((connected (list (ac-sly-test-session-state) (ac-sly-test-menu))))
     (ac-complete)
     (list :disconnected disconnected
           :connected connected
           :after (ac-sly-test-buffer-state)
           :rpcs (ac-sly-test-rpcs)))))"##,
        expect![[
            r#"OK (:disconnected (:connected nil :sources #1=(ac-source-sly-simple) :started t :menu nil :state (:text "(ca" :point 3 :mode lisp-mode :connected nil :auto-complete t :sources #1#) :rpcs nil) :connected ((:prefix "ca" :prefix-start 1 :common "ca" :menu-live t :selected "car") (("car" "l" nil nil) ("cadr" "l" nil completions-first-difference) ("case" "l" nil completions-first-difference) ("catch" "l" nil completions-first-difference))) :after (:text "(car" :point 4 :mode lisp-mode :connected t :auto-complete t :sources #1#) :rpcs ((slynk:connection-info) (slynk-completion:simple-completions "ca" 'nil)))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        ac_sly_completes_a_typed_symbol_in_a_lisp_buffer_from_the_live_connection(),
        ac_sly_fuzzy_source_shows_the_lisps_flags_unless_the_option_is_off(),
        ac_sly_completes_at_the_sly_repl_prompt_after_set_up_sly_ac(),
        ac_sly_documentation_asks_the_lisp_for_a_swank_symbol_it_cannot_read(),
        ac_sly_offers_nothing_until_a_lisp_connection_exists(),
    ]
}
