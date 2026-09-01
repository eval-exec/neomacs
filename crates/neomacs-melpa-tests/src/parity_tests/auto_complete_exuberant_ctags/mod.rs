use std::time::Duration;

use crate::{
    AUTO_COMPLETE_EXUBERANT_CTAGS_MELPA_PIN, AUTO_COMPLETE_MELPA_PIN, CachedMelpaOracle,
    POPUP_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod candidates;
mod discovery;
mod index;
mod lines;
mod registry;
mod workflows;

const AUTO_COMPLETE_EXUBERANT_CTAGS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_COMPLETE_EXUBERANT_CTAGS_TEST_PRELUDE: &str = r##"
(require 'cl)
(require 'cl-lib)

(defun auto-complete-exuberant-ctags-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))

(defun auto-complete-exuberant-ctags-test-root (name)
  (let ((root
         (file-name-as-directory
          (expand-file-name
           name
           (getenv "NEOMACS_TEST_SANDBOX_ROOT")))))
    (make-directory root t)
    root))

(defun auto-complete-exuberant-ctags-test-write (file content)
  (make-directory (file-name-directory file) t)
  (with-temp-file file
    (insert content))
  file)

(defun auto-complete-exuberant-ctags-test-relative (path root)
  (and path
       (file-relative-name path root)))

;; The package reads `candidates' from auto-complete's dynamically scoped
;; caller.  Define this probe helper under dynamic scope without making that
;; otherwise-unbound implementation detail global.
(eval
 '(defun auto-complete-exuberant-ctags-test-candidate (candidate-state)
    (let ((candidates candidate-state))
      (ac-exuberant-ctags-candidate)))
 nil)

;; popup.el normally asks the display engine for these coordinates.  Stable
;; batch coordinates keep the real auto-complete lifecycle deterministic.
(defun auto-complete-exuberant-ctags-test-posn-at-point (&rest _arguments)
  'auto-complete-exuberant-ctags-test-position)

(defun auto-complete-exuberant-ctags-test-posn-col-row (_position)
  (cons
   (current-column)
   (line-number-at-pos (point))))

(fset
 'posn-at-point
 #'auto-complete-exuberant-ctags-test-posn-at-point)
(fset
 'posn-col-row
 #'auto-complete-exuberant-ctags-test-posn-col-row)
"##;

fn auto_complete_exuberant_ctags_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_EXUBERANT_CTAGS_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete-exuberant-ctags source below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup transitive dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_EXUBERANT_CTAGS_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_EXUBERANT_CTAGS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-exuberant-ctags parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_exuberant_ctags_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_exuberant_ctags_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_exuberant_ctags_oracle("auto-complete-exuberant-ctags-autoloads.el"),
        &name,
        "auto_complete_exuberant_ctags_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_exuberant_ctags_parity` cases (2a).
pub(crate) fn assert_auto_complete_exuberant_ctags_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_exuberant_ctags_oracle("auto-complete-exuberant-ctags.el"),
        &name,
        "auto_complete_exuberant_ctags_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_exuberant_ctags_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_complete_exuberant_ctags_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_complete_exuberant_ctags_autoload_batch(&cases);
}

#[test]
fn auto_complete_exuberant_ctags_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        candidates::candidates_public_surface_batch_cases(),
        discovery::discovery_public_surface_batch_cases(),
        index::index_public_surface_batch_cases(),
        lines::lines_public_surface_batch_cases(),
        registry::registry_auto_complete_exuberant_ctags_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_exuberant_ctags_batch(&cases);
}

// END generated package batch tests
