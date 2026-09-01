use expect_test::expect;

use super::ParityBatchCase;

/// Loading the package registers the haskell-language-server client with
/// lsp-mode and extends the language-id configuration with the five
/// Haskell modes it serves.  The registered client's fields are pinned:
/// the `lsp-haskell' server id, the six major modes, the "haskell"
/// language id, the synchronized sections, the completion-in-comments
/// default, and the action-filter hookup.
fn loading_registers_the_haskell_client_and_language_ids() -> ParityBatchCase {
    ParityBatchCase::value(
        "loading_registers_the_haskell_client_and_language_ids",
        r##"(let ((client (gethash 'lsp-haskell lsp-clients)))
  (list
   :source (lsp-haskell-test-source-state)
   :client
   (list :present (and client t)
         :server-id (and client (lsp--client-server-id client))
         :major-modes (and client (lsp--client-major-modes client))
         :language-id (and client (lsp--client-language-id client))
         :synchronize-sections
         (and client (lsp--client-synchronize-sections client))
         :completion-in-comments?
         (and client (lsp--client-completion-in-comments? client))
         :action-filter
         (and client (eq (lsp--client-action-filter client)
                         #'lsp-haskell--action-filter)))
   :language-ids
   (let (entries)
     (dolist (entry lsp-language-id-configuration)
       (when (eq (cdr entry) "haskell")
         (push (car entry) entries)))
     (nreverse entries))
   :mapped
   (list (and (assq 'haskell-literate-mode lsp-language-id-configuration) t)
         (and (assq 'haskell-tng-mode lsp-language-id-configuration) t)
         (and (assq 'haskell-cabal-mode lsp-language-id-configuration) t)
         (and (assq 'haskell-ts-mode lsp-language-id-configuration) t)
         (and (assq 'cabal-mode lsp-language-id-configuration) t))))"##,
        expect![[
            r#"OK (:source (:upstream-tree "75a53a7cef5d1e9d57bcc5369744784777c9ad87" :feature t :version "20260507.1745" :lsp-mode "20260716.755" :defcustom-count 80) :client (:present t :server-id lsp-haskell :major-modes (haskell-mode haskell-literate-mode haskell-tng-mode haskell-cabal-mode haskell-ts-mode cabal-mode) :language-id "haskell" :synchronize-sections ("haskell") :completion-in-comments? t :action-filter t) :language-ids nil :mapped (t t t t t))"#
        ]],
    )
}

/// The customization surface: every documented default of a representative
/// `lsp-haskell-*' variable is pinned together with its custom type, so a
/// wrong default, a renamed option, or a wrong widget all fail.
fn the_configuration_surface_carries_the_documented_defaults() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_configuration_surface_carries_the_documented_defaults",
        r##"(let ((options
       '(lsp-haskell-formatting-provider
         lsp-haskell-check-project
         lsp-haskell-max-completions
         lsp-haskell-session-loading
         lsp-haskell-server-path
         lsp-haskell-server-log-file
         lsp-haskell-server-args
         lsp-haskell-server-wrapper-function
         lsp-haskell-completion-in-comments
         lsp-haskell-plugin-import-lens-code-actions-on
         lsp-haskell-plugin-import-lens-code-lens-on
         lsp-haskell-plugin-hlint-code-actions-on
         lsp-haskell-plugin-hlint-diagnostics-on
         lsp-haskell-plugin-ghcide-completions-config-snippets-on
         lsp-haskell-plugin-ghcide-type-lenses-global-on
         lsp-haskell-plugin-tactics-global-on
         lsp-haskell-plugin-pragmas-completion-on
         lsp-haskell-plugin-cabal-code-actions-on
         lsp-haskell-plugin-refine-imports-global-on
         lsp-haskell-plugin-stan-global-on
         lsp-haskell-plugin-class-global-on
         lsp-haskell-plugin-splice-global-on
         lsp-haskell-plugin-module-name-global-on)))
  (list
   :options
   (mapcar
    (lambda (option)
      (list :option option
            :custom-variable-p (and (custom-variable-p option) t)
            :standard (eval (car (get option 'standard-value)))
            :type (get option 'custom-type)))
    options)))"##,
        expect![[
            r#"OK (:options ((:option lsp-haskell-formatting-provider :custom-variable-p t :standard "ormolu" :type (choice (const "brittany") (const "floskell") (const "fourmolu") (const "ormolu") (const "stylish-haskell") (const "none"))) (:option lsp-haskell-check-project :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-max-completions :custom-variable-p t :standard 40 :type number) (:option lsp-haskell-session-loading :custom-variable-p t :standard "singleComponent" :type (choice (const "singleComponent") (const "multipleComponents"))) (:option lsp-haskell-server-path :custom-variable-p t :standard "haskell-language-server-wrapper" :type string) (:option lsp-haskell-server-log-file :custom-variable-p t :standard "[ORACLE-TMPDIR]/hls.log" :type string) (:option lsp-haskell-server-args :custom-variable-p t :standard ("-l" "[ORACLE-TMPDIR]/hls.log") :type (repeat (string :tag "Argument"))) (:option lsp-haskell-server-wrapper-function :custom-variable-p t :standard identity :type (choice (function-item :tag "None" :value identity) (function :tag "Custom function"))) (:option lsp-haskell-completion-in-comments :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-import-lens-code-actions-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-import-lens-code-lens-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-hlint-code-actions-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-hlint-diagnostics-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-ghcide-completions-config-snippets-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-ghcide-type-lenses-global-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-tactics-global-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-pragmas-completion-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-cabal-code-actions-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-refine-imports-global-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-stan-global-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-class-global-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-splice-global-on :custom-variable-p t :standard t :type boolean) (:option lsp-haskell-plugin-module-name-global-on :custom-variable-p t :standard t :type boolean)))"#
        ]],
    )
}

/// The server-command assembly is a pure function of the customizable
/// server path, args, and wrapper: the default produces the documented
/// argument list, and a wrapper function (the nix-shell one from the
/// docstring) is applied around it.
fn the_server_command_assembly_runs_the_wrapper_and_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_server_command_assembly_runs_the_wrapper_and_arguments",
        r##"(let ((wrapper lsp-haskell-server-wrapper-function)
        (args lsp-haskell-server-args))
  (unwind-protect
      (progn
        (setq lsp-haskell-server-wrapper-function #'identity)
        (list
         :default
         (lsp-haskell--server-command)
         :wrapped
         (progn
           (setq lsp-haskell-server-wrapper-function
                 (lambda (argv)
                   (append
                    (append (list "nix-shell" "-I" "." "--command")
                            (list (mapconcat 'identity argv " ")))
                    (list (concat "/project/shell.nix")))))
           (lsp-haskell--server-command))))
    (setq lsp-haskell-server-wrapper-function wrapper
          lsp-haskell-server-args args)))"##,
        expect![[
            r#"OK (:default ("haskell-language-server-wrapper" "--lsp" "-l" "[ORACLE-TMPDIR]/hls.log") :wrapped ("nix-shell" "-I" "." "--command" "haskell-language-server-wrapper --lsp -l [ORACLE-TMPDIR]/hls.log" "/project/shell.nix"))"#
        ]],
    )
}

/// The client's action filter rewrites `:json-false' values in the pinned
/// non-nullable boolean argument positions and leaves other arguments
/// untouched -- the transform lsp-mode needs to send code actions back to
/// the server without corrupting the booleans.
fn the_action_filter_fixes_false_to_null_for_the_pinned_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "the_action_filter_fixes_false_to_null_for_the_pinned_arguments",
        r##"(let* ((a (make-hash-table :test 'equal))
       (b (make-hash-table :test 'equal))
       (c (make-hash-table :test 'equal))
       (command (make-hash-table :test 'equal)))
  (puthash "title" "a" a)
  (puthash :restrictToOriginatingFile nil a)
  (puthash "title" "b" b)
  (puthash :withSig nil b)
  (puthash "title" "c" c)
  (puthash :unrelated nil c)
  (puthash "command" "haskell.apply" command)
  (puthash "arguments" [a b c] command)
  (lsp-haskell--action-filter command)
  (list
   :restrict (gethash :restrictToOriginatingFile a)
   :with-sig (gethash :withSig b)
   :unrelated (gethash :unrelated c)
   :command-key (gethash "command" command)
   :absent-key
   (let ((ht (make-hash-table :test 'equal)))
     (puthash :other nil ht)
     (lsp--fix-nested-boolean ht '(:restrictToOriginatingFile))
     (list :other-stays-nil (and (not (gethash :other ht)) t)
           :pinned-absent (gethash :restrictToOriginatingFile ht)))))"##,
        expect![[
            r#"OK (:restrict nil :with-sig nil :unrelated nil :command-key "haskell.apply" :absent-key (:other-stays-nil t :pinned-absent nil))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        loading_registers_the_haskell_client_and_language_ids(),
        the_configuration_surface_carries_the_documented_defaults(),
        the_server_command_assembly_runs_the_wrapper_and_arguments(),
        the_action_filter_fixes_false_to_null_for_the_pinned_arguments(),
    ]
}
