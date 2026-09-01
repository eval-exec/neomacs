use expect_test::expect;

use super::ParityBatchCase;

fn setup_prefers_helm_as_the_purpose_prompt() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_prefers_helm_as_the_purpose_prompt",
        r####"
(let ((old (and (boundp 'purpose-preferred-prompt)
                purpose-preferred-prompt)))
  (unwind-protect
      (progn
        ;; `purpose-preferred-prompt' is no longer a special variable in
        ;; current window-purpose, so setup must be observed via the global
        ;; assignment users get from their init file.
        (setq purpose-preferred-prompt 'default)
        (helm-purpose-setup)
        (list :preferred purpose-preferred-prompt
              :special (special-variable-p 'purpose-preferred-prompt)
              :version helm-purpose-version))
    (if old
        (setq purpose-preferred-prompt old)
      (makunbound 'purpose-preferred-prompt))))
"####,
        expect![[r#"OK (:preferred helm :special nil :version "0.1.1")"#]],
    )
}

fn switch_with_purpose_lists_peer_buffers_and_excludes_current() -> ParityBatchCase {
    ParityBatchCase::value(
        "switch_with_purpose_lists_peer_buffers_and_excludes_current",
        r####"
(neomacs-helm-purpose-test-with-configuration
  (with-current-buffer "edit-alpha"
    (let ((captured
           (neomacs-helm-purpose-test-capture-helm
            (lambda () (helm-purpose-switch-buffer-with-purpose)))))
      (list :classification
            (mapcar (lambda (name)
                      (list name (purpose-buffer-purpose (get-buffer name))))
                    '("edit-alpha" "edit-beta" "help-alpha" "term-alpha"))
            :session captured
            :source-name
            (helm-attr 'name helm-source-purpose-buffers-list)
            :not-found-source
            (and (memq 'helm-source-buffer-not-found
                       (plist-get captured :sources))
                 t)))))
"####,
        expect![[
            r#"OK (:classification (("edit-alpha" edit) ("edit-beta" edit) ("help-alpha" help) ("term-alpha" terminal)) :session (:sources (helm-source-purpose-buffers-list helm-source-buffer-not-found) :helm-buffer "*helm purpose*" :prompt "Buffer: " :purpose edit :candidates ("edit-beta")) :source-name "Purpose buffers" :not-found-source t)"#
        ]],
    )
}

fn explicit_purpose_argument_overrides_the_current_buffer_purpose() -> ParityBatchCase {
    ParityBatchCase::value(
        "explicit_purpose_argument_overrides_the_current_buffer_purpose",
        r####"
(neomacs-helm-purpose-test-with-configuration
  (with-current-buffer "edit-alpha"
    (neomacs-helm-purpose-test-capture-helm
     (lambda ()
       (helm-purpose-switch-buffer-with-purpose 'help)))))
"####,
        expect![[
            r#"OK (:sources (helm-source-purpose-buffers-list helm-source-buffer-not-found) :helm-buffer "*helm purpose*" :prompt "Buffer: " :purpose help :candidates ("help-alpha"))"#
        ]],
    )
}

fn switch_with_some_purpose_offers_only_occupied_purposes() -> ParityBatchCase {
    ParityBatchCase::value(
        "switch_with_some_purpose_offers_only_occupied_purposes",
        r####"
(neomacs-helm-purpose-test-with-configuration
  (let (prompt-collection session)
    (with-current-buffer "edit-alpha"
      (cl-letf (((symbol-function 'completing-read)
                 (lambda (prompt collection &rest _)
                   (setq prompt-collection
                         (list :prompt prompt
                               :choices (sort (copy-sequence collection)
                                              #'string<)))
                   "help")))
        (setq session
              (neomacs-helm-purpose-test-capture-helm
               (lambda ()
                 (helm-purpose-switch-buffer-with-some-purpose))))))
    (list :prompt prompt-collection
          :session session
          :all-purposes
          (sort (mapcar #'symbol-name (purpose-get-all-purposes))
                #'string<))))
"####,
        expect![[
            r#"OK (:prompt (:prompt "Purpose: " :choices ("edit" "general" "help" "terminal")) :session (:sources (helm-source-purpose-buffers-list helm-source-buffer-not-found) :helm-buffer "*helm purpose*" :prompt "Buffer: " :purpose help :candidates ("help-alpha")) :all-purposes ("edit" "general" "help" "terminal"))"#
        ]],
    )
}

fn purpose_source_buffer_list_updates_when_current_purpose_changes() -> ParityBatchCase {
    ParityBatchCase::value(
        "purpose_source_buffer_list_updates_when_current_purpose_changes",
        r####"
(neomacs-helm-purpose-test-with-configuration
  (with-current-buffer "help-alpha"
    (let ((before
           (progn
             (setq helm-purpose--current-purpose 'edit)
             (neomacs-helm-purpose-test-source-buffers)))
          (after
           (progn
             (setq helm-purpose--current-purpose 'terminal)
             (neomacs-helm-purpose-test-source-buffers)))
          (empty
           (progn
             (setq helm-purpose--current-purpose 'missing)
             (neomacs-helm-purpose-test-source-buffers))))
      (list :edit before :terminal after :missing empty))))
"####,
        expect![[r#"OK (:edit ("edit-alpha" "edit-beta") :terminal ("term-alpha") :missing nil)"#]],
    )
}

fn mini_ignore_purpose_runs_helm_mini_while_purpose_is_inactive() -> ParityBatchCase {
    ParityBatchCase::value(
        "mini_ignore_purpose_runs_helm_mini_while_purpose_is_inactive",
        r####"
(let ((purpose--active-p t)
      observed restored)
  (cl-letf (((symbol-function 'helm-mini)
             (lambda ()
               (setq observed purpose--active-p)
               'mini-opened)))
    (setq restored
          (list :return (helm-purpose-mini-ignore-purpose)
                :active-after purpose--active-p
                :active-during observed))))
"####,
        expect!["OK (:return mini-opened :active-after t :active-during nil)"],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        setup_prefers_helm_as_the_purpose_prompt(),
        switch_with_purpose_lists_peer_buffers_and_excludes_current(),
        explicit_purpose_argument_overrides_the_current_buffer_purpose(),
        switch_with_some_purpose_offers_only_occupied_purposes(),
        purpose_source_buffer_list_updates_when_current_purpose_changes(),
        mini_ignore_purpose_runs_helm_mini_while_purpose_is_inactive(),
    ]
}
