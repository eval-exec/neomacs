use expect_test::expect;

use super::ParityBatchCase;

fn general_predicate_dispatch_selects_the_first_true_definition_then_fallback() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_predicate_dispatch_selects_the_first_true_definition_then_fallback",
        r##"(progn
               (defvar
                 neomacs-general-predicate-map
                 (make-sparse-keymap))
               (defvar
                 neomacs-general-first nil)
               (defvar
                 neomacs-general-second nil)
               (setq
                neomacs-general-predicate-map
                (make-sparse-keymap)
                neomacs-general-first nil
                neomacs-general-second nil)
               (general-define-key
                :keymaps
                'neomacs-general-predicate-map
                "x"
                (general-predicate-dispatch
                    #'beginning-of-line
                  neomacs-general-first
                  #'forward-char
                  neomacs-general-second
                  #'backward-char
                  :docstring "General dispatch"))
               (with-temp-buffer
                 (use-local-map
                  neomacs-general-predicate-map)
                 (let ((fallback
                        (key-binding (kbd "x"))))
                   (setq
                    neomacs-general-second t)
                   (let ((second
                          (key-binding (kbd "x"))))
                     (setq
                      neomacs-general-first t)
                     (list
                      fallback
                      second
                      (key-binding (kbd "x")))))))"##,
        expect![[r#"OK (beginning-of-line backward-char forward-char)"#]],
    )
}

fn general_key_dynamically_resolves_a_binding_and_runs_setup_and_teardown() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_key_dynamically_resolves_a_binding_and_runs_setup_and_teardown",
        r##"(progn
               (defvar
                 neomacs-general-key-map
                 (make-sparse-keymap))
               (defvar
                 neomacs-general-key-events nil)
               (setq
                neomacs-general-key-map
                (make-sparse-keymap)
                neomacs-general-key-events nil)
               (define-key
                neomacs-general-key-map
                (kbd "a")
                #'forward-char)
               (general-define-key
                :keymaps
                'neomacs-general-key-map
                "b"
                (general-key
                    "a"
                  :docstring "Mirror a"
                  :setup
                  (push 'setup
                        neomacs-general-key-events)
                  :teardown
                  (push 'teardown
                        neomacs-general-key-events)))
               (with-temp-buffer
                 (use-local-map
                  neomacs-general-key-map)
                 (let ((resolved
                        (key-binding (kbd "b"))))
                   (list
                    resolved
                    (nreverse
                     neomacs-general-key-events)))))"##,
        expect![[r#"OK (forward-char (setup teardown))"#]],
    )
}

fn general_translate_key_uses_the_original_backup_across_separate_calls() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_translate_key_uses_the_original_backup_across_separate_calls",
        r##"(progn
               (defvar
                 neomacs-general-translate-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-translate-map
                (make-sparse-keymap))
               (when
                   (boundp
                    'general-neomacs-general-translate-map-backup-map)
                 (makunbound
                  'general-neomacs-general-translate-map-backup-map))
               (define-key
                neomacs-general-translate-map
                (kbd "a") #'forward-char)
               (define-key
                neomacs-general-translate-map
                (kbd "b") #'backward-char)
               (define-key
                neomacs-general-translate-map
                (kbd "c") #'next-line)
               (general-translate-key
                   nil
                   'neomacs-general-translate-map
                 "a" "b")
               (general-translate-key
                   nil
                   'neomacs-general-translate-map
                 "b" "c")
               (general-translate-key
                   nil
                   'neomacs-general-translate-map
                 "c" "a")
               (list
                (lookup-key
                 neomacs-general-translate-map
                 (kbd "a"))
                (lookup-key
                 neomacs-general-translate-map
                 (kbd "b"))
                (lookup-key
                 neomacs-general-translate-map
                 (kbd "c"))
                (boundp
                 'general-neomacs-general-translate-map-backup-map)
                (lookup-key
                 general-neomacs-general-translate-map-backup-map
                 (kbd "a"))))"##,
        expect![[r#"OK (backward-char next-line forward-char t forward-char)"#]],
    )
}

fn general_translate_key_destructive_calls_observe_prior_mutations() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_translate_key_destructive_calls_observe_prior_mutations",
        r##"(progn
               (defvar
                 neomacs-general-destructive-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-destructive-map
                (make-sparse-keymap))
               (define-key
                neomacs-general-destructive-map
                (kbd "a") #'forward-char)
               (define-key
                neomacs-general-destructive-map
                (kbd "b") #'backward-char)
               (define-key
                neomacs-general-destructive-map
                (kbd "c") #'next-line)
               (general-translate-key
                   nil
                   'neomacs-general-destructive-map
                 :destructive t
                 "a" "b")
               (general-translate-key
                   nil
                   'neomacs-general-destructive-map
                 :destructive t
                 "b" "c")
               (general-translate-key
                   nil
                   'neomacs-general-destructive-map
                 :destructive t
                 "c" "a")
               (list
                (lookup-key
                 neomacs-general-destructive-map
                 (kbd "a"))
                (lookup-key
                 neomacs-general-destructive-map
                 (kbd "b"))
                (lookup-key
                 neomacs-general-destructive-map
                 (kbd "c"))))"##,
        expect![[r#"OK (backward-char next-line backward-char)"#]],
    )
}

fn general_swap_key_exchanges_both_definitions_in_one_operation() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_swap_key_exchanges_both_definitions_in_one_operation",
        r##"(progn
               (defvar
                 neomacs-general-swap-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-swap-map
                (make-sparse-keymap))
               (define-key
                neomacs-general-swap-map
                (kbd "a") #'forward-char)
               (define-key
                neomacs-general-swap-map
                (kbd "b") #'backward-char)
               (general-swap-key
                   nil
                   'neomacs-general-swap-map
                 :destructive t
                 "a" "b")
               (list
                (lookup-key
                 neomacs-general-swap-map
                 (kbd "a"))
                (lookup-key
                 neomacs-general-swap-map
                 (kbd "b"))))"##,
        expect![[r#"OK (backward-char forward-char)"#]],
    )
}

fn general_auto_unbind_replaces_a_non_prefix_before_installing_nested_keys() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_auto_unbind_replaces_a_non_prefix_before_installing_nested_keys",
        r##"(progn
               (defvar
                 neomacs-general-auto-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-auto-map
                (make-sparse-keymap))
               (define-key
                neomacs-general-auto-map
                (kbd "a") #'forward-char)
               (let ((before
                      (condition-case error
                          (progn
                            (general-define-key
                             :keymaps
                             'neomacs-general-auto-map
                             "a b" #'backward-char)
                            'no-error)
                        (error
                         (car error)))))
                 (unwind-protect
                     (progn
                       (general-auto-unbind-keys)
                       (general-define-key
                        :keymaps
                        'neomacs-general-auto-map
                        "a b" #'backward-char
                        "a b c" #'next-line)
                       (list
                        before
                        (keymapp
                         (lookup-key
                          neomacs-general-auto-map
                          (kbd "a")))
                        (lookup-key
                         neomacs-general-auto-map
                         (kbd "a b"))
                        (lookup-key
                         neomacs-general-auto-map
                         (kbd "a b c"))))
                   (general-auto-unbind-keys t))))"##,
        expect![[r#"OK (error t (keymap (99 . next-line)) next-line)"#]],
    )
}

fn general_simulate_key_generates_a_named_command_and_executes_the_target_binding()
-> ParityBatchCase {
    ParityBatchCase::value(
        "general_simulate_key_generates_a_named_command_and_executes_the_target_binding",
        r##"(progn
               (defvar
                 neomacs-general-simulate-map
                 (make-sparse-keymap))
               (setq
                neomacs-general-simulate-map
                (make-sparse-keymap))
               (define-key
                neomacs-general-simulate-map
                (kbd "a") #'forward-char)
               (let ((command
                      (general-simulate-key
                          "a"
                        :keymap
                        neomacs-general-simulate-map
                        :name
                        neomacs-general-simulate-a
                        :docstring
                        "Simulate general a.")))
                 (with-temp-buffer
                   (insert "abc")
                   (goto-char (point-min))
                   (call-interactively command)
                   (list
                    command
                    (commandp command)
                    (documentation command)
                    (point)
                    general--last-simulated-command))))"##,
        expect![[r#"OK (neomacs-general-simulate-a t "Simulate general a." 2 forward-char)"#]],
    )
}

fn general_key_dispatch_runs_a_matching_command_and_tracks_it() -> ParityBatchCase {
    ParityBatchCase::value(
        "general_key_dispatch_runs_a_matching_command_and_tracks_it",
        r##"(progn
               (defvar
                 neomacs-general-dispatch-events nil)
               (setq
                neomacs-general-dispatch-events nil)
               (defun neomacs-general-fallback ()
                 (interactive)
                 (push 'fallback
                       neomacs-general-dispatch-events))
               (defun neomacs-general-alternate ()
                 (interactive)
                 (push 'alternate
                       neomacs-general-dispatch-events))
               (let ((command
                      (general-key-dispatch
                          #'neomacs-general-fallback
                        "x"
                        #'neomacs-general-alternate
                        :name
                        neomacs-general-dispatch
                        :docstring
                        "General dispatch command.")))
                 (let ((unread-command-events
                        (list ?x)))
                   (call-interactively command))
                 (list
                  command
                  (documentation command)
                  (nreverse
                   neomacs-general-dispatch-events)
                  general--last-dispatch-command)))"##,
        expect![[
            r#"OK (neomacs-general-dispatch "General dispatch command." (alternate) neomacs-general-alternate)"#
        ]],
    )
}

pub(super) fn dispatch_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        general_predicate_dispatch_selects_the_first_true_definition_then_fallback(),
        general_key_dynamically_resolves_a_binding_and_runs_setup_and_teardown(),
        general_translate_key_uses_the_original_backup_across_separate_calls(),
        general_translate_key_destructive_calls_observe_prior_mutations(),
        general_swap_key_exchanges_both_definitions_in_one_operation(),
        general_auto_unbind_replaces_a_non_prefix_before_installing_nested_keys(),
        general_simulate_key_generates_a_named_command_and_executes_the_target_binding(),
        general_key_dispatch_runs_a_matching_command_and_tracks_it(),
    ]
}
