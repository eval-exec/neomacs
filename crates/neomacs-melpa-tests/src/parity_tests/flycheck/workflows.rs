use expect_test::expect;

use super::ParityBatchCase;

fn checking_a_trusted_elisp_project_reports_and_navigates_byte_compiler_warnings() -> ParityBatchCase
{
    ParityBatchCase::value(
        "checking_a_trusted_elisp_project_reports_and_navigates_byte_compiler_warnings",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name "flycheck-project"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "inventory.el" root))
       (trusted-content (list (abbreviate-file-name root)))
       (flycheck-check-syntax-automatically nil)
       (flycheck-disabled-checkers '(emacs-lisp-checkdoc))
       buffer)
  (neomacs-flycheck-test-write-file
   source
   (concat
    ";;; inventory.el --- Inventory totals -*- lexical-binding: t; -*-\n"
    ";;; Commentary:\n"
    ";; Compute totals for an inventory report.\n"
    ";;; Code:\n"
    "(defun inventory-total (prices)\n"
    "  \"Return the sum of PRICES.\"\n"
    "  (let ((unused-currency \"USD\"))\n"
    "    (+ (apply #'+ prices) missing-tax)))\n"
    "(provide 'inventory)\n"
    ";;; inventory.el ends here\n"))
  (setq buffer (find-file-noselect source))
  (unwind-protect
      (with-current-buffer buffer
        (setq-local flycheck-checker 'emacs-lisp)
        (let ((finished (neomacs-flycheck-test-run-to-completion)))
          (goto-char (point-min))
          (flycheck-next-error 1 t)
          (let ((first-navigation
                 (list
                  :line (line-number-at-pos)
                  :column (current-column)
                  :errors (mapcar #'flycheck-error-message
                                  (flycheck-overlay-errors-at (point))))))
            (flycheck-next-error)
            (let ((second-navigation
                   (list
                    :line (line-number-at-pos)
                    :column (current-column)
                    :errors (mapcar #'flycheck-error-message
                                    (flycheck-overlay-errors-at (point))))))
              (flycheck-previous-error)
              (list
               :trusted (trusted-content-p)
               :finished finished
               :status flycheck-last-status-change
               :mode-line (substring-no-properties
                           (flycheck-mode-line-status-text))
               :diagnostics (neomacs-flycheck-test-diagnostics root)
               :first-navigation first-navigation
               :second-navigation second-navigation
               :previous-line (line-number-at-pos)
               :previous-column (current-column))))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"##,
        expect![[
            r#"OK (:trusted t :finished t :status finished :mode-line " FlyC:0|2|0" :diagnostics ((:file "inventory.el" :line 7 :column 10 :level warning :checker emacs-lisp :message "Unused lexical variable ‘unused-currency’") (:file "inventory.el" :line 8 :column 27 :level warning :checker emacs-lisp :message "reference to free variable ‘missing-tax’")) :first-navigation (:line 7 :column 9 :errors ("Unused lexical variable ‘unused-currency’")) :second-navigation (:line 8 :column 26 :errors ("reference to free variable ‘missing-tax’")) :previous-line 7 :previous-column 9)"#
        ]],
    )
}

fn a_normal_elisp_check_chains_compiler_and_documentation_diagnostics() -> ParityBatchCase {
    ParityBatchCase::value(
        "a_normal_elisp_check_chains_compiler_and_documentation_diagnostics",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name "flycheck-checker-chain"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "shipping.el" root))
       (trusted-content (list (abbreviate-file-name root)))
       (flycheck-check-syntax-automatically nil)
       buffer)
  (neomacs-flycheck-test-write-file
   source
   (concat
    ";;; shipping.el --- Shipping labels -*- lexical-binding: t; -*-\n"
    ";;; Commentary:\n"
    ";; Format labels for fulfilled orders.\n"
    ";;; Code:\n"
    "(defun shipping-label (address)\n"
    "  \"Format ADDRESS for a shipment\"\n"
    "  (format \"Deliver to: %s\" address))\n"
    "(shipping-label)\n"
    "(provide 'shipping)\n"
    ";;; shipping.el ends here\n"))
  (setq buffer (find-file-noselect source))
  (unwind-protect
      (with-current-buffer buffer
        (let ((finished (neomacs-flycheck-test-run-to-completion)))
          (list
           :finished finished
           :status flycheck-last-status-change
           :mode-line (substring-no-properties
                       (flycheck-mode-line-status-text))
           :checker-order (mapcar #'flycheck-error-checker
                                  flycheck-current-errors)
           :diagnostics (neomacs-flycheck-test-diagnostics root))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"##,
        expect![[
            r#"OK (:finished t :status finished :mode-line " FlyC:0|1|1" :checker-order (emacs-lisp-checkdoc emacs-lisp) :diagnostics ((:file "shipping.el" :line 6 :column nil :level info :checker emacs-lisp-checkdoc :message "First sentence should end with punctuation") (:file "shipping.el" :line 8 :column 2 :level warning :checker emacs-lisp :message "‘shipping-label’ called with 0 arguments, but requires 1")))"#
        ]],
    )
}

fn correcting_a_file_and_rechecking_removes_stale_diagnostics_and_overlays() -> ParityBatchCase {
    ParityBatchCase::value(
        "correcting_a_file_and_rechecking_removes_stale_diagnostics_and_overlays",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name "flycheck-correction"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "pricing.el" root))
       (trusted-content (list (abbreviate-file-name root)))
       (flycheck-check-syntax-automatically nil)
       (flycheck-disabled-checkers '(emacs-lisp-checkdoc))
       buffer)
  (neomacs-flycheck-test-write-file
   source
   (concat
    ";;; pricing.el --- Pricing helpers -*- lexical-binding: t; -*-\n"
    ";;; Commentary:\n"
    ";; Price complete orders.\n"
    ";;; Code:\n"
    "(defun pricing-total (prices)\n"
    "  \"Return the total for PRICES.\"\n"
    "  (let ((unused-currency \"USD\"))\n"
    "    (+ (apply #'+ prices) missing-tax)))\n"
    "(provide 'pricing)\n"
    ";;; pricing.el ends here\n"))
  (setq buffer (find-file-noselect source))
  (unwind-protect
      (with-current-buffer buffer
        (setq-local flycheck-checker 'emacs-lisp)
        (neomacs-flycheck-test-run-to-completion)
        (let ((before
               (list
                :status flycheck-last-status-change
                :mode-line (substring-no-properties
                            (flycheck-mode-line-status-text))
                :messages (mapcar #'flycheck-error-message
                                  flycheck-current-errors)
                :overlays (length (flycheck-overlays-in
                                   (point-min) (point-max))))))
          (goto-char (point-min))
          (search-forward "unused-currency")
          (replace-match "_currency" t t)
          (search-forward "missing-tax")
          (replace-match "0" t t)
          (save-buffer)
          (neomacs-flycheck-test-run-to-completion)
          (list
           :before before
           :after
           (list
            :status flycheck-last-status-change
            :mode-line (substring-no-properties
                        (flycheck-mode-line-status-text))
            :diagnostics (neomacs-flycheck-test-diagnostics root)
            :overlays (length (flycheck-overlays-in
                               (point-min) (point-max))))
           :saved (not (buffer-modified-p))
           :buffer-contains-fix
           (and (save-excursion
                  (goto-char (point-min))
                  (search-forward "_currency" nil t))
                (save-excursion
                  (goto-char (point-min))
                  (search-forward "(+ (apply #'+ prices) 0)" nil t))
                t))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"##,
        expect![[
            r#"OK (:before (:status finished :mode-line " FlyC:0|2|0" :messages ("Unused lexical variable ‘unused-currency’" "reference to free variable ‘missing-tax’") :overlays 2) :after (:status finished :mode-line " FlyC:0" :diagnostics nil :overlays 0) :saved t :buffer-contains-fix t)"#
        ]],
    )
}

fn trusting_a_project_and_resetting_eligibility_runs_the_byte_compiler() -> ParityBatchCase {
    ParityBatchCase::value(
        "trusting_a_project_and_resetting_eligibility_runs_the_byte_compiler",
        r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name "flycheck-trust-boundary"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "fulfillment.el" root))
       (trusted-content nil)
       (flycheck-check-syntax-automatically nil)
       buffer)
  (neomacs-flycheck-test-write-file
   source
   (concat
    ";;; fulfillment.el --- Fulfillment helpers -*- lexical-binding: t; -*-\n"
    ";;; Commentary:\n"
    ";; Prepare orders for fulfillment.\n"
    ";;; Code:\n"
    "(defun fulfillment-ready-p (order)\n"
    "  \"Return non-nil when ORDER can ship\"\n"
    "  (and order t))\n"
    "(fulfillment-ready-p)\n"
    "(provide 'fulfillment)\n"
    ";;; fulfillment.el ends here\n"))
  (setq buffer (find-file-noselect source))
  (unwind-protect
      (with-current-buffer buffer
        (neomacs-flycheck-test-run-to-completion)
        (let ((before-trusting
               (list
                :trusted (trusted-content-p)
                :checkers (mapcar #'flycheck-error-checker
                                  flycheck-current-errors)
                :diagnostics (neomacs-flycheck-test-diagnostics root))))
          (setq-local trusted-content (list (abbreviate-file-name root)))
          (neomacs-flycheck-test-reset-to-completion 'emacs-lisp)
          (list
           :before-trusting before-trusting
           :after-trusting
           (list
            :trusted (trusted-content-p)
            :compiler-enabled (flycheck-may-use-checker 'emacs-lisp)
            :status flycheck-last-status-change
            :mode-line (substring-no-properties
                        (flycheck-mode-line-status-text))
            :checkers (mapcar #'flycheck-error-checker
                              flycheck-current-errors)
            :diagnostics (neomacs-flycheck-test-diagnostics root)))))
    (when (buffer-live-p buffer)
      (with-current-buffer buffer (set-buffer-modified-p nil))
      (kill-buffer buffer))))
"##,
        expect![[
            r#"OK (:before-trusting (:trusted nil :checkers (emacs-lisp-checkdoc) :diagnostics ((:file "fulfillment.el" :line 6 :column nil :level info :checker emacs-lisp-checkdoc :message "First sentence should end with punctuation"))) :after-trusting (:trusted t :compiler-enabled t :status finished :mode-line " FlyC:0|1|1" :checkers (emacs-lisp-checkdoc emacs-lisp) :diagnostics ((:file "fulfillment.el" :line 6 :column nil :level info :checker emacs-lisp-checkdoc :message "First sentence should end with punctuation") (:file "fulfillment.el" :line 8 :column 2 :level warning :checker emacs-lisp :message "‘fulfillment-ready-p’ called with 0 arguments, but requires 1"))))"#
        ]],
    )
}

pub(super) fn workflows_public_surface_batch_cases() -> Vec<ParityBatchCase> {
    vec![
        checking_a_trusted_elisp_project_reports_and_navigates_byte_compiler_warnings(),
        a_normal_elisp_check_chains_compiler_and_documentation_diagnostics(),
        correcting_a_file_and_rechecking_removes_stale_diagnostics_and_overlays(),
        trusting_a_project_and_resetting_eligibility_runs_the_byte_compiler(),
    ]
}
