use std::time::Duration;

use crate::{AC_EMMET_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AC_EMMET_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// ac-emmet is an auto-complete source, and its commentary's whole story is
/// "add `ac-emmet-html-setup' to `sgml-mode-hook' and `ac-emmet-css-setup' to
/// `css-mode-hook'".  The workflows therefore complete through `ac-start` /
/// `ac-update` / `ac-complete` in a window-displayed buffer, which is the only
/// place auto-complete's popup and its completion action can run.
const AC_EMMET_TEST_PRELUDE: &str = r####"
(require 'cl-lib)
(require 'auto-complete)

;; ac-emmet is from 2013 and builds its three candidate lists with cl.el's
;; `loop' while the file loads.  Modern Emacs no longer provides that macro, so
;; supplying it is the only way to load the package at all; one workflow pins
;; exactly what a user gets without it.
(defalias 'loop (symbol-function 'cl-loop))

(defmacro ac-emmet-test-in-buffer (mode name &rest body)
  "Run BODY in a window-displayed buffer NAME in MODE.
emmet-mode supplies the expansion `ac-emmet' delegates to, and
auto-complete's popup needs a real window."
  `(let ((buffer (generate-new-buffer ,name)))
     (unwind-protect
         (progn
           (set-window-buffer (selected-window) buffer)
           (set-buffer buffer)
           (funcall ,mode)
           (emmet-mode 1)
           (setq ac-sources nil)
           (auto-complete-mode 1)
           ,@body)
       (kill-buffer buffer))))

(defun ac-emmet-test-candidates ()
  "Start completion at point and return the plain candidate strings."
  (ac-start :force-init t)
  (ac-update t)
  (mapcar #'substring-no-properties ac-candidates))

(defun ac-emmet-test-items ()
  "Resolved popup metadata for the candidates auto-complete is offering."
  (mapcar
   (lambda (item)
     (list :candidate (substring-no-properties item)
           :documentation (popup-item-documentation item)
           :summary (popup-item-summary item)
           :symbol (get-text-property 0 'symbol item)
           :candidate-face (get-text-property 0 'popup-face item)
           :selection-face (get-text-property 0 'selection-face item)
           :expands-with (get-text-property 0 'action item)))
   ac-candidates))

(defun ac-emmet-test-offer (text)
  "Retype the buffer as TEXT, record what auto-complete offers, then abort."
  (erase-buffer)
  (insert text)
  (let* ((candidates (ac-emmet-test-candidates))
         (prefix ac-prefix))
    (ac-abort)
    (list :typed text :prefix prefix :candidates candidates)))

(defun ac-emmet-test-offer-with-metadata (text)
  "Like `ac-emmet-test-offer', but keep each candidate's popup metadata."
  (erase-buffer)
  (insert text)
  (let* ((candidates (ac-emmet-test-candidates))
         (prefix ac-prefix)
         (metadata (ac-emmet-test-items)))
    (ac-abort)
    (list :typed text :prefix prefix :candidates candidates :metadata metadata)))
"####;

fn ac_emmet_oracle(prelude: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AC_EMMET_MELPA_PIN, "ac-emmet.el")
        .expect("prepare pinned ac-emmet source below ./tmp")
        .with_prelude(prelude)
        .with_timeout(AC_EMMET_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ac-emmet parity test")
        .into()
}

/// Multi-probe batch for `assert_ac_emmet_parity` cases (2a).
pub(crate) fn assert_ac_emmet_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ac_emmet_oracle(AC_EMMET_TEST_PRELUDE),
        &name,
        "ac_emmet_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_unshimmed_ac_emmet_parity` cases (2a).
pub(crate) fn assert_unshimmed_ac_emmet_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ac_emmet_oracle(""),
        &name,
        "unshimmed_ac_emmet_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ac_emmet_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_ac_emmet_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ac_emmet_batch(&cases);
}

#[test]
fn unshimmed_ac_emmet_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_unshimmed_ac_emmet_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_unshimmed_ac_emmet_batch(&cases);
}

// END generated package batch tests
