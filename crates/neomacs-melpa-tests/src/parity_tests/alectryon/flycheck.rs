use expect_test::expect;

use super::ParityBatchCase;

fn alectryon_parses_complete_realistic_json_diagnostics_into_flycheck_errors() -> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_parses_complete_realistic_json_diagnostics_into_flycheck_errors",
        r##"(with-temp-buffer
  (let* ((source (expand-file-name "proof_rst.v"
                                   (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
         (json
          (json-encode
           (list
            `((line . 3) (column . 7) (end_line . 4) (end_column . 12)
              (level . "warning") (message . "Unknown directive option")
              (source . ,source))
            `((line . 12) (column . 1) (end_line . nil) (end_column . nil)
              (level . "severe") (message . "Coq sentence failed")
              (source . ,source))
            `((line . 20) (column . 2) (level . "debug")
              (message . "Trace detail") (source . ,source))
            `((line . 30) (column . 5) (level . "novel")
              (message . "Fallback severity") (source . ,source)))))
         (errors (alectryon--parse-errors json 'alectryon (current-buffer))))
    (mapcar
     (lambda (error)
       (list
        (flycheck-error-line error)
        (flycheck-error-column error)
        (flycheck-error-end-line error)
        (flycheck-error-end-column error)
        (flycheck-error-level error)
        (flycheck-error-message error)
        (flycheck-error-filename error)
        (flycheck-error-checker error)
        (eq (flycheck-error-buffer error) (current-buffer))))
     errors)))"##,
        expect!["OK ((nil nil nil nil error nil nil alectryon t))"],
    )
}

fn alectryon_flycheck_verification_reports_mode_and_markup_lint_support_matrix() -> ParityBatchCase
{
    ParityBatchCase::value(
        "alectryon_flycheck_verification_reports_mode_and_markup_lint_support_matrix",
        r##"(let (records)
  (dolist (active '(nil t))
    (dolist (text '(rst-mode markdown-mode typst-ts-mode))
      (with-temp-buffer
        (setq-local alectryon-mode active
                    alectryon-prog-mode 'coq-mode
                    alectryon-text-mode text)
        (push
         (list active text
               (mapcar
                (lambda (result)
                  (list
                   (flycheck-verification-result-label result)
                   (flycheck-verification-result-message result)
                   (flycheck-verification-result-face result)))
                (alectryon--flycheck-verify-enabled)))
         records))))
  (nreverse records))"##,
        expect![[
            r#"OK ((nil rst-mode (("Mode selection" #("Use M-x alectryon-mode to enable" 4 22 (font-lock-face help-key-binding face help-key-binding)) #1=(bold error)) ("Linting support" "Yes." success))) (nil markdown-mode (("Mode selection" #("Use M-x alectryon-mode to enable" 4 22 (font-lock-face help-key-binding face help-key-binding)) #1#) ("Linting support" "Yes." success))) (nil typst-ts-mode (("Mode selection" #("Use M-x alectryon-mode to enable" 4 22 (font-lock-face help-key-binding face help-key-binding)) #1#) ("Linting support" "Not supported in typst-ts-mode" #2=(bold error)))) (t rst-mode (("Mode selection" "OK, using `alectryon-mode'" success) ("Linting support" "Yes." success))) (t markdown-mode (("Mode selection" "OK, using `alectryon-mode'" success) ("Linting support" "Yes." success))) (t typst-ts-mode (("Mode selection" "OK, using `alectryon-mode'" success) ("Linting support" "Not supported in typst-ts-mode" #2#))))"#
        ]],
    )
}

fn alectryon_flycheck_predicate_tracks_active_mode_and_lint_capability_in_real_buffers()
-> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_flycheck_predicate_tracks_active_mode_and_lint_capability_in_real_buffers",
        r##"(let ((predicate (flycheck-checker-get 'alectryon 'predicate))
      records)
  (dolist (case
           '((coq-mode rst-mode nil)
             (coq-mode rst-mode t)
             (lean4-mode markdown-mode t)
             (dafny-mode typst-ts-mode t)
             (rst-mode rst-mode t)
             (markdown-mode markdown-mode t)))
    (with-temp-buffer
      (let ((alectryon--winding-down t))
        (funcall (car case)))
      (setq-local alectryon-prog-mode
                  (if (memq (car case) '(coq-mode lean4-mode dafny-mode))
                      (car case) 'coq-mode)
                  alectryon-text-mode (cadr case)
                  alectryon-mode (caddr case))
      (push (list case (funcall predicate)
                  (alectryon--config-frontend)
                  (alectryon--config :lint 'text))
            records)))
  (nreverse records))"##,
        expect![[
            r#"OK (((coq-mode rst-mode nil) nil "coq+rst" t) ((coq-mode rst-mode t) t "coq+rst" t) ((lean4-mode markdown-mode t) t "lean4+md" t) ((dafny-mode typst-ts-mode t) nil "dafny+typst" nil) ((rst-mode rst-mode t) t "rst" t) ((markdown-mode markdown-mode t) t "md" t))"#
        ]],
    )
}

fn alectryon_flycheck_checker_expands_real_source_and_frontend_arguments() -> ParityBatchCase {
    ParityBatchCase::value(
        "alectryon_flycheck_checker_expands_real_source_and_frontend_arguments",
        r##"(let* ((source
         (expand-file-name "chapter_rst.v"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))
        (buffer (find-file-noselect source)))
  (unwind-protect
      (with-current-buffer buffer
        (let ((alectryon--winding-down t))
          (coq-mode))
        (setq-local alectryon-prog-mode 'coq-mode
                    alectryon-text-mode 'rst-mode
                    alectryon-mode t
                    flycheck-checker 'alectryon)
        (list
         (flycheck-checker-get 'alectryon 'command)
         (flycheck-substitute-argument
          'source-original 'alectryon)
         (alectryon--config-frontend)
         (flycheck-checker-supports-major-mode-p
          'alectryon major-mode)))
    (kill-buffer buffer)))"##,
        expect![[
            r#"OK (("alectryon" "--stdin-filename" source-original "--frontend" (eval (alectryon--config-frontend)) "--backend" "lint" "-") ("[ORACLE-SANDBOX]/chapter_rst.v") "coq+rst" (coq-mode lean4-mode dafny-mode rst-mode markdown-mode))"#
        ]],
    )
}

pub(super) fn flycheck_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        alectryon_parses_complete_realistic_json_diagnostics_into_flycheck_errors(),
        alectryon_flycheck_verification_reports_mode_and_markup_lint_support_matrix(),
        alectryon_flycheck_predicate_tracks_active_mode_and_lint_capability_in_real_buffers(),
        alectryon_flycheck_checker_expands_real_source_and_frontend_arguments(),
    ]
}
