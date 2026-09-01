use std::time::Duration;

use crate::{AC_HELM_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_HELM_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// A real helm session does run under `--batch' and `execute-kbd-macro' does
/// reach it, so every workflow drives the package's own command through a real
/// session in a window-displayed buffer and reads the rendered `*helm
/// auto-complete*' buffer back.
///
/// One thing helm cannot do here: narrow as the user types.  Its pattern is
/// only refreshed by the repeating idle timer `helm-read-from-minibuffer'
/// installs (helm-core.el), and a batch process driving a keyboard macro never
/// goes idle, so typed characters reach the minibuffer but `helm-pattern'
/// stays "".  `helm-refresh' does not help either -- it re-renders the
/// existing pattern rather than re-reading the minibuffer.  The workflows
/// therefore move the selection with `C-n'/`C-p' instead of filtering, and
/// claim nothing about narrowing.
const AC_HELM_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'auto-complete)
(require 'popup)

;; A realistic user configuration: one auto-complete source whose candidates
;; carry the per-candidate `document' and `action' text properties ac-helm
;; reads.  Nothing in ac-helm is replaced.
(defun ach-test-mark-settled ()
  (insert " ;; settled"))

(defun ach-test-mark-summarised ()
  (insert " ;; summarised"))

(defconst ach-test-api-candidates
  (list
   (propertize "ledger-settle"
               'document "ledger-settle (INVOICE)\n\nSettle INVOICE and return its new state."
               'action 'ach-test-mark-settled)
   (propertize "ledger-settle-all"
               'document "ledger-settle-all (INVOICES)\n\nSettle every invoice in INVOICES.")
   (propertize "ledger-summary"
               'document "ledger-summary ()\n\nReturn a summary alist for the open ledger."
               'action 'ach-test-mark-summarised)
   (propertize "ledger-reset"
               'document "ledger-reset ()\n\nDiscard every pending settlement.")))

(defvar ac-source-ach-api
  '((candidates . ach-test-api-candidates)
    (symbol . "f")
    (requires . 1)))

(defvar ach-test-result 'unset)

(defun ach-test-complete-with-helm ()
  "Run the package's own command and keep whatever it returns or signals."
  (interactive)
  (setq ach-test-result
        (condition-case err
            (ac-complete-with-helm)
          (error (list :error (car err) (cdr err))))))

(defmacro ach-test-in-buffer (&rest body)
  "Run BODY in a window-displayed buffer with auto-complete and ac-helm armed."
  `(let ((buffer (generate-new-buffer "*ac-helm-workflow*")))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (emacs-lisp-mode)
           (setq ac-sources '(ac-source-ach-api))
           (auto-complete-mode 1)
           (setq ach-test-result 'unset)
           (global-set-key (kbd "C-c :") #'ach-test-complete-with-helm)
           ,@body)
       (dolist (name '("*helm auto-complete*" "*Help*" "*Popup Help*"))
         (when (get-buffer name)
           (kill-buffer name)))
       (kill-buffer buffer))))

(defun ach-test-helm-lines ()
  "The candidate list helm is actually displaying."
  (let ((helm-buffer (get-buffer "*helm auto-complete*")))
    (and helm-buffer
         (with-current-buffer helm-buffer
           (split-string
            (buffer-substring-no-properties (point-min) (point-max))
            "\n" t)))))

(defun ach-test-state ()
  (list :buffer (buffer-substring-no-properties (point-min) (point-max))
        :point (point)
        :ac-completing ac-completing
        :ac-prefix ac-prefix
        :ac-candidates (mapcar #'substring-no-properties (or ac-candidates nil))))
"####;

fn ac_helm_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_HELM_MELPA_PIN, "ac-helm.el")
        .expect("prepare pinned ac-helm source below ./tmp")
        .with_prelude(AC_HELM_TEST_PRELUDE)
        .with_timeout(AC_HELM_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-helm parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_helm_parity` cases (2a).
pub(crate) fn assert_ac_helm_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(ac_helm_oracle(), &name, "ac_helm_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn ac_helm_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_helm_batch(&cases);
}

// END generated package batch tests
