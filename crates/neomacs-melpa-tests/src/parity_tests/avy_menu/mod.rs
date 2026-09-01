use std::time::Duration;

use crate::{AVY_MENU_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const AVY_MENU_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const AVY_MENU_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'avy-menu)

(defvar neomacs-avy-menu-test-spec nil)
(defvar neomacs-avy-menu-test-show-pane-header nil)
(defvar neomacs-avy-menu-test-result nil)
(defvar neomacs-avy-menu-test-observed nil)

(defun neomacs-avy-menu-test-face-runs ()
  "Return every non-nil face run in the current menu buffer."
  (let ((position (point-min))
        runs)
    (while (< position (point-max))
      (let* ((face (get-text-property position 'face))
             (next (next-single-property-change
                    position 'face nil (point-max))))
        (when face
          (push (list (buffer-substring-no-properties position next) face)
                runs))
        (setq position next)))
    (nreverse runs)))

(defun neomacs-avy-menu-test-capture (selection)
  "Capture the live rendered menu before Avy performs SELECTION."
  (setq neomacs-avy-menu-test-observed
        (list :text (buffer-substring-no-properties (point-min) (point-max))
              :faces (neomacs-avy-menu-test-face-runs)
              :cursor cursor-type
              :candidates (length avy-last-candidates)
              :menu-windows (length (get-buffer-window-list
                                     (current-buffer) nil t))))
  (avy-pre-action-default selection))

(defun neomacs-avy-menu-test-open ()
  "Open the configured test menu through the public interactive lifecycle."
  (interactive)
  (setq neomacs-avy-menu-test-result
        (avy-menu " *neomacs-avy-menu*"
                  neomacs-avy-menu-test-spec
                  neomacs-avy-menu-test-show-pane-header)))
"##;

fn avy_menu_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AVY_MENU_MELPA_PIN, "avy-menu.el")
        .expect("prepare pinned Avy Menu source below ./tmp")
        .with_prelude(AVY_MENU_TEST_PRELUDE)
        .with_timeout(AVY_MENU_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Avy Menu parity test")
        .into()
}

pub(crate) fn assert_avy_menu_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(avy_menu_oracle(), &name, "avy_menu_parity", cases);
}

#[test]
fn avy_menu_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_avy_menu_batch(&cases);
}
