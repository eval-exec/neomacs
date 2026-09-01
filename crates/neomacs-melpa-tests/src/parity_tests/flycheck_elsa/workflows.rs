use expect_test::expect;

use super::ParityBatchCase;

fn backend_selects_config_file_and_command_argv() -> ParityBatchCase {
    ParityBatchCase::value(
        "backend_selects_config_file_and_command_argv",
        r####"
(list :cask
      (let ((flycheck-elsa-backend 'cask))
        (list :config (flycheck-elsa--config-file)
              :command (flycheck-elsa-command)))
      :eask
      (let ((flycheck-elsa-backend 'eask))
        (list :config (flycheck-elsa--config-file)
              :command (flycheck-elsa-command)))
      :unknown
      (condition-case err
          (let ((flycheck-elsa-backend 'nix))
            (flycheck-elsa-command)
            :ok)
        (error (car err))))
"####,
        expect![[
            r#"OK (:cask (:config "Cask" :command ("cask" "exec" "elsa")) :eask (:config "Eask" :command ("eask" "exec" "elsa")) :unknown user-error)"#
        ]],
    )
}

fn enable_predicate_requires_config_with_elsa_dependency() -> ParityBatchCase {
    ParityBatchCase::value(
        "enable_predicate_requires_config_with_elsa_dependency",
        r####"
(neomacs-flycheck-elsa-test-with-project
 'cask
 (lambda (root file buffer)
   (list :enabled (and (flycheck-elsa--enable-p) t)
         :config-dir
         (equal (directory-file-name
                 (file-truename (flycheck-elsa--locate-config-dir)))
                (directory-file-name (file-truename root)))
         :working-dir
         (equal (directory-file-name
                 (file-truename (flycheck-elsa--working-directory)))
                (directory-file-name (file-truename root)))
         :ignored-cask
         (let ((cask-buf (find-file-noselect (expand-file-name "Cask" root))))
           (unwind-protect
               (with-current-buffer cask-buf
                 (emacs-lisp-mode)
                 ;; Cask files themselves are ignored by default regexps.
                 (and (flycheck-elsa--enable-p) t))
             (let ((kill-buffer-hook nil)
                   (kill-buffer-query-functions nil))
               (kill-buffer cask-buf))))
         :without-elsa
         (progn
           (with-temp-file (expand-file-name "Cask" root)
             (insert "(source gnu)\n(depends-on \"dash\")\n"))
           ;; Re-read from disk for the source file buffer.
           (and (not (flycheck-elsa--enable-p)) t)))))
"####,
        expect!["OK (:enabled t :config-dir t :working-dir t :ignored-cask t :without-elsa t)"],
    )
}

fn setup_registers_checker_and_executable_hook() -> ParityBatchCase {
    ParityBatchCase::value(
        "setup_registers_checker_and_executable_hook",
        r####"
(let ((flycheck-checkers (copy-sequence flycheck-checkers))
      (flycheck-before-syntax-check-hook
       (copy-sequence flycheck-before-syntax-check-hook)))
  (flycheck-elsa-setup)
  (with-temp-buffer
    (emacs-lisp-mode)
    (let ((flycheck-elsa-backend 'eask))
      (flycheck-elsa--setup-executable)
      (list :checker-registered
            (and (memq 'emacs-lisp-elsa flycheck-checkers) t)
            :hook-registered
            (and (memq 'flycheck-elsa--setup-executable
                       flycheck-before-syntax-check-hook)
                 t)
            :executable flycheck-emacs-lisp-elsa-executable
            :checker-modes
            (flycheck-checker-get 'emacs-lisp-elsa 'modes)
            :error-filter
            (flycheck-checker-get 'emacs-lisp-elsa 'error-filter)))))
"####,
        expect![[
            r#"OK (:checker-registered t :hook-registered t :executable "eask" :checker-modes (emacs-lisp-mode) :error-filter flycheck-increment-error-columns)"#
        ]],
    )
}

fn ignored_files_regexps_skip_config_filenames() -> ParityBatchCase {
    ParityBatchCase::value(
        "ignored_files_regexps_skip_config_filenames",
        r####"
;; Regexps match the whole file basename (\\`Cask\\' / \\`Eask\\').
(list :cask
      (and (seq-find (lambda (f) (string-match-p f "Cask"))
                     flycheck-elsa-ignored-files-regexps)
           t)
      :eask
      (and (seq-find (lambda (f) (string-match-p f "Eask"))
                     flycheck-elsa-ignored-files-regexps)
           t)
      :source-ok
      (not (seq-find (lambda (f) (string-match-p f "foo.el"))
                     flycheck-elsa-ignored-files-regexps)))
"####,
        expect!["OK (:cask t :eask t :source-ok t)"],
    )
}

pub(super) fn workflow_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        backend_selects_config_file_and_command_argv(),
        enable_predicate_requires_config_with_elsa_dependency(),
        setup_registers_checker_and_executable_hook(),
        ignored_files_regexps_skip_config_filenames(),
    ]
}
