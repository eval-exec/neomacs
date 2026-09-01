use std::time::Duration;

use expect_test::expect;

use crate::{CachedMelpaOracle, FLYCHECK_POS_TIP_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

const FLYCHECK_POS_TIP_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const FLYCHECK_POS_TIP_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'flycheck)
(require 'flycheck-pos-tip)

(defun neomacs-flycheck-pos-tip-test-write-file (path contents)
  "Write CONTENTS to PATH, creating its parent directory."
  (make-directory (file-name-directory path) t)
  (write-region contents nil path nil 'silent)
  path)

(defun neomacs-flycheck-pos-tip-test-run-to-completion ()
  "Run the configured checker and wait for Flycheck's completion hook."
  (let (finished (rounds 0))
    (add-hook 'flycheck-after-syntax-check-hook
              (lambda () (setq finished t)) nil t)
    (flycheck-mode 1)
    (flycheck-buffer)
    (while (and (not finished) (< rounds 600))
      (accept-process-output nil 0.05)
      (setq rounds (1+ rounds)))
    (unless finished
      (error "Timed out waiting for Flycheck; status is %S"
             flycheck-last-status-change))
    finished))

(defun neomacs-flycheck-pos-tip-test-diagnostics (base)
  "Describe all current Flycheck diagnostics relative to BASE."
  (mapcar
   (lambda (diagnostic)
     (list
      :file (and (flycheck-error-filename diagnostic)
                 (file-relative-name
                  (flycheck-error-filename diagnostic) base))
      :line (flycheck-error-line diagnostic)
      :column (flycheck-error-column diagnostic)
      :level (flycheck-error-level diagnostic)
      :checker (flycheck-error-checker diagnostic)
      :message (flycheck-error-message diagnostic)))
   flycheck-current-errors))
"##;

fn flycheck_pos_tip_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(FLYCHECK_POS_TIP_MELPA_PIN, "flycheck-pos-tip.el")
        .expect("prepare pinned flycheck-pos-tip source below ./tmp")
        .with_prelude(FLYCHECK_POS_TIP_TEST_PRELUDE)
        .with_timeout(FLYCHECK_POS_TIP_TEST_TIMEOUT)
}

fn graphical_diagnostics_follow_point_and_buffer_changes() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((root
        (file-name-as-directory
         (expand-file-name "flycheck-pos-tip-gui"
                           (getenv "NEOMACS_TEST_SANDBOX_ROOT"))))
       (source (expand-file-name "invoice.el" root))
       (trusted-content (list (abbreviate-file-name root)))
       (flycheck-check-syntax-automatically nil)
       (flycheck-disabled-checkers '(emacs-lisp-checkdoc))
       (flycheck-pos-tip-timeout 7.25)
       (flycheck-pos-tip-max-width 44)
       (old-mode flycheck-pos-tip-mode)
       (old-display flycheck-display-errors-function)
       (old-old-display flycheck-pos-tip-old-display-function)
       (old-post-command-hook (copy-tree (default-value 'post-command-hook)))
       (old-focus-out-hook (copy-tree (default-value 'focus-out-hook)))
       (old-global-minor-modes (copy-sequence global-minor-modes))
       buffer result tooltip-calls hide-counts hide-calls)
  (neomacs-flycheck-pos-tip-test-write-file
   source
   (concat
    ";;; invoice.el --- Invoice totals -*- lexical-binding: t; -*-\n"
    ";;; Commentary:\n"
    ";; Compute totals for customer invoices.\n"
    ";;; Code:\n"
    "(defun invoice-total (amounts)\n"
    "  \"Return the total of AMOUNTS.\"\n"
    "  (let ((unused-currency \"EUR\"))\n"
    "    (+ (apply #'+ amounts) missing-tax)))\n"
    "(provide 'invoice)\n"
    ";;; invoice.el ends here\n"))
  (setq buffer (find-file-noselect source))
  (unwind-protect
      (with-current-buffer buffer
        (setq-local flycheck-checker 'emacs-lisp)
        (neomacs-flycheck-pos-tip-test-run-to-completion)
        (goto-char (point-min))
        (flycheck-next-error 1 t)
        (cl-letf
            (((symbol-function 'display-graphic-p)
              (lambda (&optional _display) t))
             ((symbol-function 'window-line-height)
              (lambda (&optional _line _window) '(18 0 0 0)))
             ((symbol-function 'pos-tip-show)
              (lambda (&rest arguments)
                (push
                 (list
                  :text (substring-no-properties (nth 0 arguments))
                  :tip-color (nth 1 arguments)
                  :position (nth 2 arguments)
                  :window (nth 3 arguments)
                  :timeout (nth 4 arguments)
                  :max-width (nth 5 arguments)
                  :frame-coordinates (nth 6 arguments)
                  :dx (nth 7 arguments)
                  :dy (nth 8 arguments))
                 tooltip-calls)
                'tooltip-shown))
             ((symbol-function 'pos-tip-hide)
              (lambda ()
                (setq hide-calls (1+ (or hide-calls 0)))
                'tooltip-hidden)))
          (flycheck-pos-tip-mode 1)
          (flycheck-display-error-at-point)
          (let ((display-point (point)))
            (let ((post-command-hook
                   (list #'flycheck-pos-tip-hide-messages)))
              (run-hooks 'post-command-hook)
              (push (or hide-calls 0) hide-counts)
              (forward-char 1)
              (run-hooks 'post-command-hook)
              (push (or hide-calls 0) hide-counts)
              (run-hooks 'post-command-hook)
              (push (or hide-calls 0) hide-counts)
              (insert "λ")
              (run-hooks 'post-command-hook)
              (push (or hide-calls 0) hide-counts))
            (setq
             result
             (list
              :finished flycheck-last-status-change
              :diagnostics
              (neomacs-flycheck-pos-tip-test-diagnostics root)
              :display-point
              (list :line (line-number-at-pos display-point)
                    :column
                    (save-excursion
                      (goto-char display-point)
                      (current-column)))
              :renderer flycheck-display-errors-function
              :hooks
              (list
               :post-command-count
               (cl-count
                #'flycheck-pos-tip-hide-messages
                (default-value 'post-command-hook)
                :test #'eq)
               :focus-out-count
               (cl-count
                #'flycheck-pos-tip-hide-messages
                (default-value 'focus-out-hook)
                :test #'eq))
              :tooltip-calls (nreverse tooltip-calls)
              :hide-counts (nreverse hide-counts))))))
    (when flycheck-pos-tip-mode
      (flycheck-pos-tip-mode -1))
    (setq flycheck-display-errors-function old-display
          flycheck-pos-tip-old-display-function old-old-display
          flycheck-pos-tip-mode old-mode
          global-minor-modes old-global-minor-modes)
    (setq-default post-command-hook old-post-command-hook
                  focus-out-hook old-focus-out-hook)
    (when (buffer-live-p buffer)
      (with-current-buffer buffer
        (flycheck-mode -1)
        (set-buffer-modified-p nil))
      (kill-buffer buffer)))
  result)
"##;
    let expect = expect![[
        r#"OK (:finished finished :diagnostics ((:file "invoice.el" :line 7 :column 10 :level warning :checker emacs-lisp :message "Unused lexical variable ‘unused-currency’") (:file "invoice.el" :line 8 :column 28 :level warning :checker emacs-lisp :message "reference to free variable ‘missing-tax’")) :display-point (:line 7 :column 9) :renderer flycheck-pos-tip-error-messages :hooks (:post-command-count 1 :focus-out-count 1) :tooltip-calls ((:text "Unused lexical variable ‘unused-currency’" :tip-color nil :position nil :window nil :timeout 7.25 :max-width 44 :frame-coordinates nil :dx nil :dy 23)) :hide-counts (0 1 1 2))"#
    ]];
    ParityBatchCase::value(
        "graphical_diagnostics_follow_point_and_buffer_changes",
        elisp_form,
        expect,
    )
}

fn tty_fallback_preserves_diagnostics_and_owns_text_presentation() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((old-mode flycheck-pos-tip-mode)
       (old-display flycheck-display-errors-function)
       (old-old-display flycheck-pos-tip-old-display-function)
       (old-tty-function flycheck-pos-tip-display-errors-tty-function)
       (old-last-displayed flycheck--last-displayed-message)
       (old-post-command-hook (copy-tree (default-value 'post-command-hook)))
       (old-focus-out-hook (copy-tree (default-value 'focus-out-hook)))
       (old-global-minor-modes (copy-sequence global-minor-modes))
       display-calls custom-calls tooltip-calls hide-calls result)
  (unwind-protect
      (with-temp-buffer
        (insert "subtotal + tax ; prepare λ invoice\n")
        (setq-local flycheck-highlighting-mode 'columns)
        (flycheck-mode 1)
        (let ((errors
               (list
                (flycheck-error-new-at
                 1 1 'error "Unknown subtotal" :id "E100"
                 :end-column 9 :checker 'invoice-linter)
                (flycheck-error-new-at
                 1 12 'warning "Tax table is stale" :id "W200"
                 :end-column 15 :checker 'invoice-linter))))
          (mapc #'flycheck-add-overlay errors)
          (goto-char (point-min))
          (cl-letf
              (((symbol-function 'display-graphic-p)
                (lambda (&optional _display) nil))
               ((symbol-function 'display-message-or-buffer)
                (lambda (message buffer action)
                  (push
                   (list
                    :message
                    (replace-regexp-in-string
                     "\u2069" "<PDI>"
                     (replace-regexp-in-string
                      "\u2068" "<FSI>"
                      (substring-no-properties message)
                      t t)
                     t t)
                    :buffer buffer
                    :action action)
                   display-calls)
                  'tty-window))
               ((symbol-function 'pos-tip-show)
                (lambda (&rest arguments)
                  (push arguments tooltip-calls)
                  'unexpected-tooltip))
               ((symbol-function 'flycheck-hide-error-buffer)
                (lambda ()
                  (setq hide-calls (1+ (or hide-calls 0)))
                  'tty-hidden)))
            (flycheck-pos-tip-mode 1)
            (let* ((default-return (flycheck-display-errors errors))
                   (flycheck-pos-tip-display-errors-tty-function
                    (lambda (received)
                      (push
                       (mapcar
                        (lambda (diagnostic)
                          (list
                           :level (flycheck-error-level diagnostic)
                           :id (flycheck-error-id diagnostic)
                           :message (flycheck-error-message diagnostic)))
                        received)
                       custom-calls)
                      'custom-rendered))
                   (custom-return (flycheck-display-errors (reverse errors))))
              (let ((post-command-hook
                     (list #'flycheck-pos-tip-hide-messages))
                    (focus-out-hook
                     (list #'flycheck-pos-tip-hide-messages)))
                (run-hooks 'post-command-hook)
                (let ((after-first-event (or hide-calls 0)))
                  (run-hooks 'post-command-hook)
                  (let ((after-unchanged-event (or hide-calls 0)))
                    (forward-char 1)
                    (run-hooks 'focus-out-hook)
                    (setq
                     result
                     (list
                      :returns (list default-return custom-return)
                      :default-display (nreverse display-calls)
                      :custom-display (nreverse custom-calls)
                      :tooltip-calls tooltip-calls
                      :hide-counts
                      (list after-first-event
                            after-unchanged-event
                            (or hide-calls 0)))))))))))
    (when flycheck-pos-tip-mode
      (flycheck-pos-tip-mode -1))
    (setq flycheck-display-errors-function old-display
          flycheck-pos-tip-old-display-function old-old-display
          flycheck-pos-tip-display-errors-tty-function old-tty-function
          flycheck--last-displayed-message old-last-displayed
          flycheck-pos-tip-mode old-mode
          global-minor-modes old-global-minor-modes)
    (setq-default post-command-hook old-post-command-hook
                  focus-out-hook old-focus-out-hook))
  result)
"##;
    let expect = expect![[
        r#"OK (:returns (tty-window custom-rendered) :default-display ((:message "‘<FSI>subtotal<PDI>’: Unknown subtotal [E100]\n‘<FSI>tax<PDI>’: Tax table is stale [W200]" :buffer "*Flycheck error messages*" :action not-this-window)) :custom-display (((:level warning :id "W200" :message "Tax table is stale") (:level error :id "E100" :message "Unknown subtotal"))) :tooltip-calls nil :hide-counts (1 1 2))"#
    ]];
    ParityBatchCase::value(
        "tty_fallback_preserves_diagnostics_and_owns_text_presentation",
        elisp_form,
        expect,
    )
}

fn graphical_multi_diagnostic_tooltip_preserves_context_and_order() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((old-mode flycheck-pos-tip-mode)
       (old-display flycheck-display-errors-function)
       (old-old-display flycheck-pos-tip-old-display-function)
       (old-post-command-hook (copy-tree (default-value 'post-command-hook)))
       (old-focus-out-hook (copy-tree (default-value 'focus-out-hook)))
       (old-global-minor-modes (copy-sequence global-minor-modes))
       (flycheck-pos-tip-timeout 12)
       (flycheck-pos-tip-max-width 64)
       (text-quoting-style 'straight)
       tooltip-calls result)
  (unwind-protect
      (with-temp-buffer
        (insert "alpha = beta + gamma\n")
        (goto-char 9)
        (set-buffer-modified-p nil)
        (let* ((relation
                (flycheck-related-location-new
                 :filename "/workspace/project/defs.el"
                 :line 7
                 :column 3
                 :message "defined here"))
               (errors
                (list
                 (flycheck-error-new-at
                  1 1 'error "Unknown symbol" :id "E100"
                  :end-column 6 :checker 'invoice-linter
                  :relations (list relation))
                 (flycheck-error-new-at
                  1 16 'warning "Suspicious λ call" :id "W200"
                  :end-column 21 :checker 'invoice-linter))))
          (cl-letf
              (((symbol-function 'display-graphic-p)
                (lambda (&optional _display) t))
               ((symbol-function 'window-line-height)
                (lambda (&optional _line _window) '(20 0 0 0)))
               ((symbol-function 'pos-tip-show)
                (lambda (&rest arguments)
                  (let* ((message (nth 0 arguments))
                         (relation-start
                          (string-match "defined here" message))
                         (related-location
                          (and relation-start
                               (get-text-property
                                relation-start
                                'flycheck-related-location
                                message))))
                    (push
                     (list
                      :text
                      (replace-regexp-in-string
                       "\u2069" "<PDI>"
                       (replace-regexp-in-string
                        "\u2068" "<FSI>"
                        (substring-no-properties message)
                        t t)
                       t t)
                      :relation
                      (and related-location
                           (list
                            :file
                            (flycheck-related-location-filename
                             related-location)
                            :line
                            (flycheck-related-location-line
                             related-location)
                            :column
                            (flycheck-related-location-column
                             related-location)
                            :message
                            (flycheck-related-location-message
                             related-location)))
                      :timeout (nth 4 arguments)
                      :max-width (nth 5 arguments)
                      :dy (nth 8 arguments))
                     tooltip-calls)
                    'tooltip-shown))))
            (flycheck-pos-tip-mode 1)
            (let ((display-return (flycheck-display-errors errors)))
              (setq
               result
               (list
                :return display-return
                :tooltip-calls (nreverse tooltip-calls)
                :buffer
                (list
                 :text (buffer-string)
                 :point (point)
                 :modified (buffer-modified-p))))))))
    (when flycheck-pos-tip-mode
      (flycheck-pos-tip-mode -1))
    (setq flycheck-display-errors-function old-display
          flycheck-pos-tip-old-display-function old-old-display
          flycheck-pos-tip-mode old-mode
          global-minor-modes old-global-minor-modes)
    (setq-default post-command-hook old-post-command-hook
                  focus-out-hook old-focus-out-hook))
  result)
"##;
    let expect = expect![[
        r#"OK (:return tooltip-shown :tooltip-calls ((:text "'<FSI>alpha<PDI>': Unknown symbol [E100]\n  ↳ defined here (defs.el:7:3)\n'<FSI>gamma<PDI>': Suspicious λ call [W200]" :relation (:file "/workspace/project/defs.el" :line 7 :column 3 :message "defined here") :timeout 12 :max-width 64 :dy 25)) :buffer (:text "alpha = beta + gamma\n" :point 9 :modified nil))"#
    ]];
    ParityBatchCase::value(
        "graphical_multi_diagnostic_tooltip_preserves_context_and_order",
        elisp_form,
        expect,
    )
}

fn mode_lifecycle_is_idempotent_and_restores_flychecks_frontend() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((old-mode flycheck-pos-tip-mode)
       (old-display flycheck-display-errors-function)
       (old-clear flycheck-clear-displayed-errors-function)
       (old-old-display flycheck-pos-tip-old-display-function)
       (old-post-command-hook (copy-tree (default-value 'post-command-hook)))
       (old-focus-out-hook (copy-tree (default-value 'focus-out-hook)))
       (old-global-minor-modes (copy-sequence global-minor-modes))
       snapshots mode-hook-observations result)
  (unwind-protect
      (progn
        (setq flycheck-pos-tip-mode nil
              flycheck-display-errors-function
              #'flycheck-display-errors-via-eldoc
              flycheck-pos-tip-old-display-function nil)
        (remove-hook 'post-command-hook #'flycheck-pos-tip-hide-messages)
        (remove-hook 'focus-out-hook #'flycheck-pos-tip-hide-messages)
        (cl-labels
            ((snapshot
              (phase)
              (list
               phase
               :mode flycheck-pos-tip-mode
               :display flycheck-display-errors-function
               :saved flycheck-pos-tip-old-display-function
               :clear flycheck-clear-displayed-errors-function
               :post-command-count
               (cl-count
                #'flycheck-pos-tip-hide-messages
                (default-value 'post-command-hook)
                :test #'eq)
               :focus-out-count
               (cl-count
                #'flycheck-pos-tip-hide-messages
                (default-value 'focus-out-hook)
                :test #'eq))))
          (let ((flycheck-pos-tip-mode-hook
                 (list
                  (lambda ()
                    (push
                     (list
                      :mode flycheck-pos-tip-mode
                      :display flycheck-display-errors-function
                      :saved flycheck-pos-tip-old-display-function)
                     mode-hook-observations)))))
            (push (snapshot :initial) snapshots)
            (flycheck-pos-tip-mode 1)
            (push (snapshot :enabled) snapshots)
            (flycheck-pos-tip-mode 1)
            (push (snapshot :reenabled) snapshots)
            (flycheck-pos-tip-mode -1)
            (push (snapshot :disabled) snapshots)
            (setq
             result
             (list
              :snapshots (nreverse snapshots)
              :mode-hook-observations
              (nreverse mode-hook-observations))))))
    (when flycheck-pos-tip-mode
      (flycheck-pos-tip-mode -1))
    (setq flycheck-display-errors-function old-display
          flycheck-clear-displayed-errors-function old-clear
          flycheck-pos-tip-old-display-function old-old-display
          flycheck-pos-tip-mode old-mode
          global-minor-modes old-global-minor-modes)
    (setq-default post-command-hook old-post-command-hook
                  focus-out-hook old-focus-out-hook))
  result)
"##;
    let expect = expect![
        "OK (:snapshots ((:initial :mode nil :display flycheck-display-errors-via-eldoc :saved nil :clear flycheck-clear-displayed-error-messages :post-command-count 0 :focus-out-count 0) (:enabled :mode t :display flycheck-pos-tip-error-messages :saved flycheck-display-errors-via-eldoc :clear flycheck-clear-displayed-error-messages :post-command-count 1 :focus-out-count 1) (:reenabled :mode t :display flycheck-pos-tip-error-messages :saved flycheck-display-errors-via-eldoc :clear flycheck-clear-displayed-error-messages :post-command-count 1 :focus-out-count 1) (:disabled :mode nil :display flycheck-display-errors-via-eldoc :saved nil :clear flycheck-clear-displayed-error-messages :post-command-count 0 :focus-out-count 0)) :mode-hook-observations ((:mode t :display flycheck-pos-tip-error-messages :saved flycheck-display-errors-via-eldoc) (:mode t :display flycheck-pos-tip-error-messages :saved flycheck-display-errors-via-eldoc) (:mode nil :display flycheck-display-errors-via-eldoc :saved nil)))"
    ];
    ParityBatchCase::value(
        "mode_lifecycle_is_idempotent_and_restores_flychecks_frontend",
        elisp_form,
        expect,
    )
}

fn preconfigured_pos_tip_renderer_enables_without_claiming_hooks() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((old-mode flycheck-pos-tip-mode)
       (old-display flycheck-display-errors-function)
       (old-old-display flycheck-pos-tip-old-display-function)
       (old-post-command-hook (copy-tree (default-value 'post-command-hook)))
       (old-focus-out-hook (copy-tree (default-value 'focus-out-hook)))
       (old-global-minor-modes (copy-sequence global-minor-modes))
       snapshots mode-hook-observations result)
  (unwind-protect
      (progn
        (setq flycheck-pos-tip-mode nil
              flycheck-display-errors-function
              #'flycheck-pos-tip-error-messages
              flycheck-pos-tip-old-display-function nil)
        (remove-hook 'post-command-hook #'flycheck-pos-tip-hide-messages)
        (remove-hook 'focus-out-hook #'flycheck-pos-tip-hide-messages)
        (cl-labels
            ((snapshot
              (phase)
              (list
               phase
               :mode flycheck-pos-tip-mode
               :display flycheck-display-errors-function
               :saved flycheck-pos-tip-old-display-function
               :listed
               (and (memq 'flycheck-pos-tip-mode global-minor-modes) t)
               :post-command-count
               (cl-count
                #'flycheck-pos-tip-hide-messages
                (default-value 'post-command-hook)
                :test #'eq)
               :focus-out-count
               (cl-count
                #'flycheck-pos-tip-hide-messages
                (default-value 'focus-out-hook)
                :test #'eq))))
          (let ((flycheck-pos-tip-mode-hook
                 (list
                  (lambda ()
                    (push
                     (list
                      :mode flycheck-pos-tip-mode
                      :display flycheck-display-errors-function
                      :saved flycheck-pos-tip-old-display-function)
                     mode-hook-observations)))))
            (push (snapshot :preconfigured) snapshots)
            (flycheck-pos-tip-mode 1)
            (push (snapshot :enabled) snapshots)
            (flycheck-pos-tip-mode -1)
            (push (snapshot :disabled) snapshots)
            (setq
             result
             (list
              :snapshots (nreverse snapshots)
              :mode-hook-observations
              (nreverse mode-hook-observations))))))
    (when flycheck-pos-tip-mode
      (flycheck-pos-tip-mode -1))
    (setq flycheck-display-errors-function old-display
          flycheck-pos-tip-old-display-function old-old-display
          flycheck-pos-tip-mode old-mode
          global-minor-modes old-global-minor-modes)
    (setq-default post-command-hook old-post-command-hook
                  focus-out-hook old-focus-out-hook))
  result)
"##;
    let expect = expect![
        "OK (:snapshots ((:preconfigured :mode nil :display flycheck-pos-tip-error-messages :saved nil :listed nil :post-command-count 0 :focus-out-count 0) (:enabled :mode t :display flycheck-pos-tip-error-messages :saved nil :listed t :post-command-count 0 :focus-out-count 0) (:disabled :mode nil :display nil :saved nil :listed nil :post-command-count 0 :focus-out-count 0)) :mode-hook-observations ((:mode t :display flycheck-pos-tip-error-messages :saved nil) (:mode nil :display nil :saved nil)))"
    ];
    ParityBatchCase::value(
        "preconfigured_pos_tip_renderer_enables_without_claiming_hooks",
        elisp_form,
        expect,
    )
}

fn disabling_after_another_frontend_takes_ownership_preserves_it() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((old-mode flycheck-pos-tip-mode)
       (old-display flycheck-display-errors-function)
       (old-old-display flycheck-pos-tip-old-display-function)
       (old-post-command-hook (copy-tree (default-value 'post-command-hook)))
       (old-focus-out-hook (copy-tree (default-value 'focus-out-hook)))
       (old-global-minor-modes (copy-sequence global-minor-modes))
       snapshots mode-hook-observations result)
  (unwind-protect
      (progn
        (setq flycheck-pos-tip-mode nil
              flycheck-display-errors-function
              #'flycheck-display-errors-via-eldoc
              flycheck-pos-tip-old-display-function nil)
        (remove-hook 'post-command-hook #'flycheck-pos-tip-hide-messages)
        (remove-hook 'focus-out-hook #'flycheck-pos-tip-hide-messages)
        (cl-labels
            ((snapshot
              (phase)
              (list
               phase
               :mode flycheck-pos-tip-mode
               :display flycheck-display-errors-function
               :saved flycheck-pos-tip-old-display-function
               :listed
               (and (memq 'flycheck-pos-tip-mode global-minor-modes) t)
               :post-command-count
               (cl-count
                #'flycheck-pos-tip-hide-messages
                (default-value 'post-command-hook)
                :test #'eq)
               :focus-out-count
               (cl-count
                #'flycheck-pos-tip-hide-messages
                (default-value 'focus-out-hook)
                :test #'eq))))
          (let ((flycheck-pos-tip-mode-hook
                 (list
                  (lambda ()
                    (push
                     (list
                      :mode flycheck-pos-tip-mode
                      :display flycheck-display-errors-function
                      :saved flycheck-pos-tip-old-display-function)
                     mode-hook-observations)))))
            (flycheck-pos-tip-mode 1)
            (push (snapshot :owned-by-pos-tip) snapshots)
            (setq flycheck-display-errors-function
                  #'flycheck-display-error-messages-unless-error-list)
            (push (snapshot :taken-over) snapshots)
            (flycheck-pos-tip-mode -1)
            (push (snapshot :disabled-after-takeover) snapshots)
            (setq
             result
             (list
              :snapshots (nreverse snapshots)
              :mode-hook-observations
              (nreverse mode-hook-observations))))))
    (remove-hook 'post-command-hook #'flycheck-pos-tip-hide-messages)
    (remove-hook 'focus-out-hook #'flycheck-pos-tip-hide-messages)
    (setq flycheck-display-errors-function old-display
          flycheck-pos-tip-old-display-function old-old-display
          flycheck-pos-tip-mode old-mode
          global-minor-modes old-global-minor-modes)
    (setq-default post-command-hook old-post-command-hook
                  focus-out-hook old-focus-out-hook))
  result)
"##;
    let expect = expect![
        "OK (:snapshots ((:owned-by-pos-tip :mode t :display flycheck-pos-tip-error-messages :saved flycheck-display-errors-via-eldoc :listed t :post-command-count 1 :focus-out-count 1) (:taken-over :mode t :display flycheck-display-error-messages-unless-error-list :saved flycheck-display-errors-via-eldoc :listed t :post-command-count 1 :focus-out-count 1) (:disabled-after-takeover :mode nil :display flycheck-display-error-messages-unless-error-list :saved flycheck-display-errors-via-eldoc :listed nil :post-command-count 1 :focus-out-count 1)) :mode-hook-observations ((:mode t :display flycheck-pos-tip-error-messages :saved flycheck-display-errors-via-eldoc) (:mode nil :display flycheck-display-error-messages-unless-error-list :saved flycheck-display-errors-via-eldoc)))"
    ];
    ParityBatchCase::value(
        "disabling_after_another_frontend_takes_ownership_preserves_it",
        elisp_form,
        expect,
    )
}

fn empty_input_is_inert_and_graphical_backend_failures_preserve_context() -> ParityBatchCase {
    let elisp_form = r##"
(let* ((old-mode flycheck-pos-tip-mode)
       (old-display flycheck-display-errors-function)
       (old-old-display flycheck-pos-tip-old-display-function)
       (old-post-command-hook (copy-tree (default-value 'post-command-hook)))
       (old-focus-out-hook (copy-tree (default-value 'focus-out-hook)))
       (old-global-minor-modes (copy-sequence global-minor-modes))
       (flycheck-pos-tip-timeout 0)
       (flycheck-pos-tip-max-width nil)
       tooltip-calls line-height-calls line-height backend-fails
       hide-calls result)
  (unwind-protect
      (with-temp-buffer
        (insert "deploy production after validation λ\n")
        (set-buffer-modified-p nil)
        (goto-char (point-min))
        (let ((diagnostic
               (flycheck-error-new-at
                1 1 'error "Deployment blocked" :id "DEPLOY-9"
                :end-column 7 :checker 'release-linter)))
          (cl-letf
              (((symbol-function 'display-graphic-p)
                (lambda (&optional _display) t))
               ((symbol-function 'window-line-height)
                (lambda (&optional _line _window)
                  (setq line-height-calls (1+ (or line-height-calls 0)))
                  line-height))
               ((symbol-function 'pos-tip-show)
                (lambda (&rest arguments)
                  (push
                   (list
                    :text (substring-no-properties (nth 0 arguments))
                    :timeout (nth 4 arguments)
                    :max-width (nth 5 arguments)
                    :dy (nth 8 arguments))
                   tooltip-calls)
                  (if backend-fails
                      (error "Native tooltip unavailable")
                    'tooltip-shown)))
               ((symbol-function 'pos-tip-hide)
                (lambda ()
                  (setq hide-calls (1+ (or hide-calls 0)))
                  'tooltip-hidden)))
            (flycheck-pos-tip-mode 1)
            (let* ((post-command-hook
                    (list #'flycheck-pos-tip-hide-messages))
                   (nil-return (flycheck-display-errors nil))
                   (calls-after-nil (length tooltip-calls))
                   (line-height-calls-after-nil (or line-height-calls 0)))
              (run-hooks 'post-command-hook)
              (let ((hide-after-nil (or hide-calls 0)))
                (setq line-height nil)
                (let ((success-return
                       (flycheck-display-errors (list diagnostic))))
                  (run-hooks 'post-command-hook)
                  (let ((hide-after-success (or hide-calls 0)))
                    (goto-char 12)
                    (setq backend-fails t
                          line-height '(11 0 0 0))
                    (let ((failure
                           (condition-case error-data
                               (flycheck-display-errors (list diagnostic))
                             (error error-data))))
                      (run-hooks 'post-command-hook)
                      (setq
                       result
                       (list
                        :nil-input
                        (list
                         :return nil-return
                         :tooltip-calls calls-after-nil
                         :line-height-calls line-height-calls-after-nil)
                        :success-return success-return
                        :failure failure
                        :tooltip-calls (nreverse tooltip-calls)
                        :line-height-calls line-height-calls
                        :hide-counts
                        (list hide-after-nil
                              hide-after-success
                              (or hide-calls 0))
                        :buffer
                        (list
                         :text (buffer-string)
                         :point (point)
                         :modified (buffer-modified-p))))))))))))
    (when flycheck-pos-tip-mode
      (flycheck-pos-tip-mode -1))
    (setq flycheck-display-errors-function old-display
          flycheck-pos-tip-old-display-function old-old-display
          flycheck-pos-tip-mode old-mode
          global-minor-modes old-global-minor-modes)
    (setq-default post-command-hook old-post-command-hook
                  focus-out-hook old-focus-out-hook))
  result)
"##;
    let expect = expect![[
        r#"OK (:nil-input (:return nil :tooltip-calls 0 :line-height-calls 0) :success-return tooltip-shown :failure (error "Native tooltip unavailable") :tooltip-calls ((:text "Deployment blocked [DEPLOY-9]" :timeout 0 :max-width nil :dy nil) (:text "Deployment blocked [DEPLOY-9]" :timeout 0 :max-width nil :dy 16)) :line-height-calls 2 :hide-counts (1 1 1) :buffer (:text "deploy production after validation λ\n" :point 12 :modified nil))"#
    ]];
    ParityBatchCase::value(
        "empty_input_is_inert_and_graphical_backend_failures_preserve_context",
        elisp_form,
        expect,
    )
}

#[test]
fn flycheck_pos_tip_package_batch() {
    let cases = vec![
        graphical_diagnostics_follow_point_and_buffer_changes(),
        tty_fallback_preserves_diagnostics_and_owns_text_presentation(),
        graphical_multi_diagnostic_tooltip_preserves_context_and_order(),
        mode_lifecycle_is_idempotent_and_restores_flychecks_frontend(),
        preconfigured_pos_tip_renderer_enables_without_claiming_hooks(),
        disabling_after_another_frontend_takes_ownership_preserves_it(),
        empty_input_is_inert_and_graphical_backend_failures_preserve_context(),
    ];
    let thread = std::thread::current();
    let test_name = thread
        .name()
        .unwrap_or("unnamed flycheck-pos-tip parity test");
    assert_oracle_batch_cases(
        flycheck_pos_tip_oracle(),
        test_name,
        "flycheck_pos_tip_parity",
        &cases,
    );
}
