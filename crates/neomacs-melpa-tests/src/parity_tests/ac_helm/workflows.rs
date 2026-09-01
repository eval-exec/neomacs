use expect_test::expect;

use super::ParityBatchCase;

/// The package's whole story: `C-:' hands auto-complete's candidates to helm
/// and the chosen one replaces the prefix.  Pins the list helm renders --
/// including ac-helm's own annotation of candidates that carry an `action'
/// property -- and the buffer, point and text properties afterwards.
fn completing_an_api_symbol_through_a_real_helm_session() -> ParityBatchCase {
    ParityBatchCase::value(
        "completing_an_api_symbol_through_a_real_helm_session",
        r####"
(ach-test-in-buffer
 ;; helm's `helm-turn-on-show-completion' is documented as "Display candidate
 ;; in `current-buffer' while moving selection"; a user who turns it off gets
 ;; the working command.  The last workflow pins what its default value does.
 (setq helm-turn-on-show-completion nil)
 (insert "(ledger-s")
 (let ((before (ach-test-state)))
   (execute-kbd-macro (kbd "C-c : RET"))
   (list :before before
         :result ach-test-result
         :helm-lines (ach-test-helm-lines)
         :after (ach-test-state)
         :inserted-properties
         (list :document (get-text-property 2 'document)
               :action (get-text-property 2 'action)
               :symbol (get-text-property 2 'symbol))
         :source-attributes
         (list :name (assoc-default 'name helm-source-auto-complete-candidates)
               :persistent-action
               (assoc-default 'persistent-action
                              helm-source-auto-complete-candidates)
               :cached-candidates
               (assoc-default 'ac-candidates
                              helm-source-auto-complete-candidates)))))
"####,
        expect![[
            r#"OK (:before (:buffer "(ledger-s" :point 10 :ac-completing nil :ac-prefix nil :ac-candidates nil) :result nil :helm-lines ("Auto Complete" "ledger-settle        <ach-test-mark-settled>" "ledger-summary       <ach-test-mark-summarised>" "ledger-settle-all") :after (:buffer "(ledger-settle ;; settled" :point 26 :ac-completing nil :ac-prefix nil :ac-candidates nil) :inserted-properties (:document "ledger-settle (INVOICE)\n\nSettle INVOICE and return its new state." :action ach-test-mark-settled :symbol "f") :source-attributes (:name "Auto Complete" :persistent-action popup-item-show-help :cached-candidates nil))"#
        ]],
    )
}

fn choosing_a_later_candidate_runs_that_candidates_own_action() -> ParityBatchCase {
    ParityBatchCase::value(
        "choosing_a_later_candidate_runs_that_candidates_own_action",
        r####"
(list
 :second-candidate
 (ach-test-in-buffer
  (setq helm-turn-on-show-completion nil)
  (insert "(ledger-s")
  (execute-kbd-macro (kbd "C-c : C-n RET"))
  (list :result ach-test-result
        :helm-lines (ach-test-helm-lines)
        :state (ach-test-state)))
 :third-candidate-has-no-action
 (ach-test-in-buffer
  (setq helm-turn-on-show-completion nil)
  (insert "(ledger-s")
  (execute-kbd-macro (kbd "C-c : C-n C-n RET"))
  (list :result ach-test-result
        :state (ach-test-state)
        :inserted-action (get-text-property 2 'action)))
 :back-up-again
 (ach-test-in-buffer
  (setq helm-turn-on-show-completion nil)
  (insert "(ledger-s")
  (execute-kbd-macro (kbd "C-c : C-n C-n C-p RET"))
  (list :result ach-test-result
        :state (ach-test-state))))
"####,
        expect![[
            r#"OK (:second-candidate (:result nil :helm-lines ("Auto Complete" "ledger-settle        <ach-test-mark-settled>" "ledger-summary       <ach-test-mark-summarised>" "ledger-settle-all") :state (:buffer "(ledger-summary ;; summarised" :point 30 :ac-completing nil :ac-prefix nil :ac-candidates nil)) :third-candidate-has-no-action (:result nil :state (:buffer "(ledger-settle-all" :point 19 :ac-completing nil :ac-prefix nil :ac-candidates nil) :inserted-action nil) :back-up-again (:result nil :state (:buffer "(ledger-summary ;; summarised" :point 30 :ac-completing nil :ac-prefix nil :ac-candidates nil)))"#
        ]],
    )
}

fn the_persistent_action_shows_the_selected_candidates_documentation() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_persistent_action_shows_the_selected_candidates_documentation",
        r####"
(ach-test-in-buffer
 (setq helm-turn-on-show-completion nil)
 (insert "(ledger-s")
 (defvar ach-help-snapshots nil)
 (setq ach-help-snapshots nil)
 (defun ach-capture-help ()
   (interactive)
   (push (let ((help (get-buffer " *Popup Help*")))
           (list :exists (and help t)
                 :text (and help
                            (with-current-buffer help
                              (buffer-substring-no-properties
                               (point-min) (point-max))))
                 :point (and help (with-current-buffer help (point)))))
         ach-help-snapshots))
 (define-key helm-map (kbd "C-c s") #'ach-capture-help)
 (execute-kbd-macro (kbd "C-c : C-c s C-j C-c s C-n C-j C-c s RET"))
 (list :result ach-test-result
       :help-snapshots (nreverse ach-help-snapshots)
       :state (ach-test-state)))
"####,
        expect![[
            r#"OK (:result nil :help-snapshots ((:exists nil :text nil :point nil) (:exists t :text "ledger-settle (INVOICE)\n\nSettle INVOICE and return its new state." :point 1) (:exists t :text "ledger-summary ()\n\nReturn a summary alist for the open ledger." :point 1)) :state (:buffer "(ledger-summary ;; summarised" :point 30 :ac-completing nil :ac-prefix nil :ac-candidates nil))"#
        ]],
    )
}

fn aborting_the_helm_session_leaves_the_buffer_untouched() -> ParityBatchCase {
    ParityBatchCase::value(
        "aborting_the_helm_session_leaves_the_buffer_untouched",
        r####"
(ach-test-in-buffer
 (setq helm-turn-on-show-completion nil)
 (insert "(ledger-s")
 (let ((before (ach-test-state)))
   (execute-kbd-macro (kbd "C-c : C-g"))
   (let ((after (ach-test-state)))
     (execute-kbd-macro (kbd "C-c : RET"))
     (list :before before
           :aborted-result ach-test-result
           :after-abort after
           :buffer-untouched (equal (plist-get before :buffer)
                                    (plist-get after :buffer))
           :point-untouched (equal (plist-get before :point)
                                   (plist-get after :point))
           :recovered-result ach-test-result
           :after-recovery (ach-test-state)))))
"####,
        expect![[
            r#"OK (:before (:buffer "(ledger-s" :point 10 :ac-completing nil :ac-prefix nil :ac-candidates nil) :aborted-result nil :after-abort (:buffer "(ledger-s" :point 10 :ac-completing nil :ac-prefix nil :ac-candidates nil) :buffer-untouched t :point-untouched t :recovered-result nil :after-recovery (:buffer "(ledger-settle ;; settled" :point 26 :ac-completing nil :ac-prefix nil :ac-candidates nil))"#
        ]],
    )
}

fn an_already_running_auto_complete_session_is_handed_straight_to_helm() -> ParityBatchCase {
    ParityBatchCase::value(
        "an_already_running_auto_complete_session_is_handed_straight_to_helm",
        r####"
(ach-test-in-buffer
 (setq helm-turn-on-show-completion nil)
 (insert "(ledger-s")
 (ac-start :force-init t)
 (ac-update t)
 (let ((armed (list :ac-completing ac-completing
                    :ac-prefix ac-prefix
                    :ac-point ac-point
                    :ac-candidates
                    (mapcar #'substring-no-properties ac-candidates))))
   (execute-kbd-macro (kbd "C-c : RET"))
   (list :armed armed
         :result ach-test-result
         :helm-lines (ach-test-helm-lines)
         :state (ach-test-state))))
"####,
        expect![[
            r#"OK (:armed (:ac-completing t :ac-prefix "ledger-s" :ac-point 2 :ac-candidates ("ledger-settle" "ledger-summary" "ledger-settle-all")) :result nil :helm-lines ("Auto Complete" "ledger-settle        <ach-test-mark-settled>" "ledger-summary       <ach-test-mark-summarised>" "ledger-settle-all") :state (:buffer "(ledger-settle ;; settled" :point 26 :ac-completing nil :ac-prefix nil :ac-candidates nil))"#
        ]],
    )
}

fn a_prefix_with_a_single_candidate_hits_the_packages_exit_minibuffer_shortcut() -> ParityBatchCase
{
    ParityBatchCase::value(
        "a_prefix_with_a_single_candidate_hits_the_packages_exit_minibuffer_shortcut",
        r####"
(ach-test-in-buffer
 (setq helm-turn-on-show-completion nil)
 (insert "(ledger-r")
 (let ((before (ach-test-state)))
   (execute-kbd-macro (kbd "C-c :"))
   (list :before before
         :result ach-test-result
         :helm-lines (ach-test-helm-lines)
         :after (ach-test-state))))
"####,
        expect![[
            r#"OK (:before (:buffer "(ledger-r" :point 10 :ac-completing nil :ac-prefix nil :ac-candidates nil) :result (:error no-catch (exit nil)) :helm-lines nil :after (:buffer "(ledger-reset" :point 14 :ac-completing nil :ac-prefix nil :ac-candidates nil))"#
        ]],
    )
}

fn the_default_show_completion_setting_breaks_the_command_on_current_helm() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_default_show_completion_setting_breaks_the_command_on_current_helm",
        r####"
(ach-test-in-buffer
 (insert "(ledger-s")
 (let ((before (ach-test-state)))
   (execute-kbd-macro (kbd "C-c :"))
   (list :turn-on-show-completion helm-turn-on-show-completion
         :lexical-binding-in-package
         (with-temp-buffer
           (insert-file-contents (getenv "NEOMACS_PACKAGE_SOURCE") nil 0 200)
           (and (string-match-p "lexical-binding" (buffer-string)) t))
         :before before
         :result ach-test-result
         :helm-lines (ach-test-helm-lines)
         :after (ach-test-state))))
"####,
        expect![[
            r#"OK (:turn-on-show-completion t :lexical-binding-in-package nil :before (:buffer "(ledger-s" :point 10 :ac-completing nil :ac-prefix nil :ac-candidates nil) :result (:error void-variable (helm--hook)) :helm-lines ("Auto Complete" "ledger-settle        <ach-test-mark-settled>" "ledger-summary       <ach-test-mark-summarised>" "ledger-settle-all") :after (:buffer "(ledger-s" :point 10 :ac-completing nil :ac-prefix nil :ac-candidates nil))"#
        ]],
    )
    .fresh_process()
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        completing_an_api_symbol_through_a_real_helm_session(),
        choosing_a_later_candidate_runs_that_candidates_own_action(),
        the_persistent_action_shows_the_selected_candidates_documentation(),
        aborting_the_helm_session_leaves_the_buffer_untouched(),
        an_already_running_auto_complete_session_is_handed_straight_to_helm(),
        a_prefix_with_a_single_candidate_hits_the_packages_exit_minibuffer_shortcut(),
        the_default_show_completion_setting_breaks_the_command_on_current_helm(),
    ]
}
