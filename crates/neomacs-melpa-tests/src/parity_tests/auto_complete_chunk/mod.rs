use std::time::Duration;

use crate::{
    AUTO_COMPLETE_CHUNK_MELPA_PIN, AUTO_COMPLETE_MELPA_PIN, CachedMelpaOracle, POPUP_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod boundaries;
mod candidates;
mod registry;
mod sources;
mod workflows;

const AUTO_COMPLETE_CHUNK_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_COMPLETE_CHUNK_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defvar auto-complete-chunk-test-events nil)

(defun auto-complete-chunk-test-error (thunk)
  (condition-case error-data
      (list :value
            (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))

(defun auto-complete-chunk-test-beginning (mode text &optional position)
  (with-temp-buffer
    (funcall mode)
    (insert text)
    (goto-char
     (or position
         (point-max)))
    (let ((beginning
           (ac-chunk-beginning)))
      (list
       mode
       text
       (point)
       beginning
       (and beginning
            (buffer-substring-no-properties
             beginning
             (point)))))))

;; popup.el normally obtains these coordinates from the display engine.
;; Deterministic batch coordinates preserve the real completion lifecycle.
(defun auto-complete-chunk-test-posn-at-point (&rest _arguments)
  'auto-complete-chunk-test-position)

(defun auto-complete-chunk-test-posn-col-row (_position)
  (cons
   (current-column)
   (line-number-at-pos
    (point))))

(fset
 'posn-at-point
 #'auto-complete-chunk-test-posn-at-point)
(fset
 'posn-col-row
 #'auto-complete-chunk-test-posn-col-row)
"##;

fn auto_complete_chunk_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_CHUNK_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete-chunk source below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup transitive dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_CHUNK_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_CHUNK_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-chunk parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_chunk_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_chunk_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_chunk_oracle("auto-complete-chunk-autoloads.el"),
        &name,
        "auto_complete_chunk_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_chunk_parity` cases (2a).
pub(crate) fn assert_auto_complete_chunk_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_chunk_oracle("auto-complete-chunk.el"),
        &name,
        "auto_complete_chunk_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_chunk_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_complete_chunk_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_complete_chunk_autoload_batch(&cases);
}

#[test]
fn auto_complete_chunk_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        boundaries::boundaries_public_surface_batch_cases(),
        candidates::candidates_public_surface_batch_cases(),
        registry::registry_auto_complete_chunk_batch_cases(),
        sources::sources_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_chunk_batch(&cases);
}

// END generated package batch tests
