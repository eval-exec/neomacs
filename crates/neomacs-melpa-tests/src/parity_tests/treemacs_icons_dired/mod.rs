use std::time::Duration;

use crate::{CachedMelpaOracle, TREEMACS_ICONS_DIRED_MELPA_PIN, TREEMACS_MELPA_PIN};

use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};

mod workflows;

const TREEMACS_ICONS_DIRED_TEST_TIMEOUT: Duration = Duration::from_secs(240);

const TREEMACS_ICONS_DIRED_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'treemacs)

(treemacs-create-theme "Neomacs Dired Test"
  :icon-directory (getenv "NEOMACS_TEST_SANDBOX_ROOT")
  :config
  (progn
    (treemacs-create-icon :icon "[FILE]" :fallback 'same-as-icon
                          :extensions (fallback))
    (treemacs-create-icon :icon "[DIR]" :fallback 'same-as-icon
                          :extensions (dir-closed))
    (treemacs-create-icon :icon "[SRC]" :fallback 'same-as-icon
                          :extensions ("src-closed"))
    (treemacs-create-icon :icon "[EL]" :fallback 'same-as-icon
                          :extensions ("el"))
    (treemacs-create-icon :icon "[MD]" :fallback 'same-as-icon
                          :extensions ("md"))
    (treemacs-create-icon :icon "[TXT]" :fallback 'same-as-icon
                          :extensions ("txt"))))
(treemacs-load-theme "Neomacs Dired Test")

(defun neomacs-treemacs-icons-dired-test-root (name)
  "Return a clean deterministic sandbox directory named NAME."
  (let ((root (file-name-as-directory
               (expand-file-name name (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (when (file-exists-p root)
      (delete-directory root t))
    (make-directory root t)
    root))

(defun neomacs-treemacs-icons-dired-test-entry (file)
  "Describe FILE's visible icon in the current Dired buffer."
  (save-excursion
    (if (not (dired-goto-file file))
        (list :name (file-name-nondirectory file) :present nil)
      (let ((filename-start (dired-move-to-filename t)))
        (list :name (file-name-nondirectory file)
              :present t
              :icon (get-text-property (1- filename-start) 'display))))))

(defun neomacs-treemacs-icons-dired-test-coverage (root)
  "Describe covered Dired subdirectories relative to ROOT."
  (mapcar (lambda (path) (file-relative-name path root))
          treemacs-icons-dired--covered-subdirs))

(defun neomacs-treemacs-icons-dired-test-registration ()
  "Describe the global mode's hook and advice registration."
  (list
   :after-readin (and (memq #'treemacs-icons-dired--display
                            dired-after-readin-hook)
                      t)
   :mode-select (and (memq #'treemacs--select-icon-set dired-mode-hook) t)
   :tab-width (and (memq #'treemacs-icons-dired--set-tab-width
                         dired-mode-hook)
                   t)
   :revert (and (advice-member-p #'treemacs-icons-dired--reset
                                 'dired-revert)
                t)
   :add-entry (and (advice-member-p
                    #'treemacs-icons-dired--add-icon-for-new-entry
                    'dired-add-entry)
                   t)))
"##;

fn treemacs_icons_dired_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(TREEMACS_ICONS_DIRED_MELPA_PIN, "treemacs-icons-dired.el")
        .expect("prepare exact shallow Treemacs Icons Dired source below ./tmp")
        .with_melpa_dependency(TREEMACS_MELPA_PIN)
        .expect("prepare exact shallow Treemacs dependency below ./tmp")
        .with_prelude(TREEMACS_ICONS_DIRED_TEST_PRELUDE)
        .with_timeout(TREEMACS_ICONS_DIRED_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed Treemacs Icons Dired parity test")
        .into()
}

fn assert_treemacs_icons_dired_batch(cases: &[ParityBatchCase]) {
    assert_oracle_batch_cases(
        treemacs_icons_dired_oracle(),
        &current_test_name(),
        "treemacs_icons_dired_parity",
        cases,
    );
}

#[test]
fn treemacs_icons_dired_package_batch() {
    assert_treemacs_icons_dired_batch(&workflows::workflow_batch_cases());
}
