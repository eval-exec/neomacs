use std::time::Duration;

use crate::{ACE_POPUP_MENU_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const ACE_POPUP_MENU_TEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Fixtures shared by the workflows.
///
/// ace-popup-menu is a global minor mode that advises `x-popup-menu' to render
/// the menu in a temporary window and let the user pick an entry with an avy
/// label.  Nothing here is stubbed: the workflows call the real (advised)
/// `x-popup-menu', avy reads real keys, and the menu is rendered into a real
/// window.
///
/// Two details make that observable and deterministic.  The menu window is
/// created by `with-current-buffer-window', which runs
/// `temp-buffer-window-show-hook' with the menu buffer current and its window
/// selected -- exactly after rendering and before the first key is read -- so
/// `apm-test-record-rendering' can capture what the user is shown before the
/// buffer is killed again.  And avy reads its selection key from `avy-keys',
/// which is pinned here: a key outside that alphabet is not a candidate, avy
/// only reports "No such candidate" and keeps reading, so every workflow feeds
/// a key that is either a real label or one of `avy-escape-chars'.
const ACE_POPUP_MENU_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar apm-test-renderings nil
  "Every menu rendering captured while a popup menu was displayed.")

(defun apm-test-face-runs ()
  "Return the current buffer's text as (TEXT . FACE) runs."
  (let ((position (point-min))
        (runs nil))
    (while (< position (point-max))
      (let* ((next (next-single-property-change position 'face nil (point-max)))
             (face (get-text-property position 'face)))
        (push (cons (buffer-substring-no-properties position next) face) runs)
        (setq position next)))
    (nreverse runs)))

(defun apm-test-record-rendering ()
  "Record the popup menu exactly as the user sees it when it appears."
  (push (list :buffer (buffer-name)
              :text (buffer-substring-no-properties (point-min) (point-max))
              :runs (apm-test-face-runs)
              :cursor cursor-type
              :window-buffer (buffer-name (window-buffer (selected-window))))
        apm-test-renderings))

(add-hook 'temp-buffer-window-show-hook #'apm-test-record-rendering)

(defun apm-test-renderings ()
  (reverse apm-test-renderings))

(defvar apm-test-menu
  '("Refactor"
    ("Rename"
     ("Rename symbol" . rename-symbol)
     ("Rename file" . rename-file))
    ("Extract"
     ("Extract function" . extract-function)
     ("Extract variable" . extract-variable)
     ("Inline variable" . inline-variable)))
  "A realistic two-pane menu in `x-popup-menu' format.")

(defvar apm-test-result nil
  "What the last `apm-test-popup' command selected.")

(defun apm-test-popup ()
  "Pop up `apm-test-menu' the way an ordinary command would."
  (interactive)
  (setq apm-test-result (x-popup-menu t apm-test-menu)))

(defvar apm-test-orig-calls nil
  "Every call ace-popup-menu forwarded to the ORIG-FUN it was given.")

(defun apm-test-orig-fun (&rest arguments)
  "Stand in for the original `x-popup-menu' and record ARGUMENTS."
  (push (cons :orig arguments) apm-test-orig-calls)
  'value-from-orig-fun)

(defun apm-test-orig-calls ()
  (reverse apm-test-orig-calls))

(defun apm-test-advice-count ()
  "Count how many times ace-popup-menu advises `x-popup-menu'."
  (let ((count 0))
    (advice-mapc (lambda (function _properties)
                   (when (eq function #'ace-popup-menu)
                     (setq count (1+ count))))
                 'x-popup-menu)
    count))

(defun apm-test-mode-state ()
  (list :advised (and (advice-member-p #'ace-popup-menu 'x-popup-menu) t)
        :advice-count (apm-test-advice-count)
        :mode ace-popup-menu-mode
        :global-value (default-value 'ace-popup-menu-mode)
        :buffer-local (local-variable-p 'ace-popup-menu-mode)))

(defun apm-test-setup ()
  "Create the work buffer the user is editing and pin avy's alphabet."
  (setq apm-test-renderings nil
        apm-test-result nil
        apm-test-orig-calls nil
        avy-keys '(?a ?s ?d ?f ?g ?h ?j ?k ?l)
        avy-style 'pre
        avy-all-windows nil)
  (global-set-key (kbd "C-c m") #'apm-test-popup)
  (switch-to-buffer (get-buffer-create "*apm-work*"))
  (erase-buffer)
  (insert "Editing buffer, untouched by the menu.\n")
  (goto-char (point-min))
  (current-buffer))
"##;

fn ace_popup_menu_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(ACE_POPUP_MENU_MELPA_PIN, "ace-popup-menu.el")
        .expect("prepare pinned ace-popup-menu source below ./tmp")
        .with_prelude(ACE_POPUP_MENU_TEST_PRELUDE)
        .with_timeout(ACE_POPUP_MENU_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed ace-popup-menu parity test")
        .into()
}

/// Multi-probe batch for `assert_ace_popup_menu_parity` cases (2a).
pub(crate) fn assert_ace_popup_menu_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        ace_popup_menu_oracle(),
        &name,
        "ace_popup_menu_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn ace_popup_menu_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_ace_popup_menu_batch(&cases);
}

// END generated package batch tests
