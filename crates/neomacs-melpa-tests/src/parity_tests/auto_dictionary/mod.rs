use std::time::Duration;

use crate::{AUTO_DICTIONARY_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod conditional;
mod detection;
mod mode;
mod registry;
mod workflows;

const AUTO_DICTIONARY_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_DICTIONARY_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)
(require 'flyspell)
(require 'ispell)

(defvar adict-test-valid-dictionaries
  '("en" "de" "fr" "es" "sv" "sl" "hu" "ro" "pt"
    "nb" "da" "grc" "el" "hi" "nn" "ca" "eo" "sk"))

(defun adict-test-valid-dictionary-list ()
  adict-test-valid-dictionaries)

(advice-add
 'ispell-valid-dictionary-list
 :override
 #'adict-test-valid-dictionary-list)

(defun adict-test-error (thunk)
  (condition-case error-data
      (list :ok (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))

(defun adict-test-overlay-state (overlay)
  (list
   (overlay-start overlay)
   (overlay-end overlay)
   (overlay-get overlay 'evaporate)
   (overlay-get overlay 'face)
   (overlay-get overlay
                'adict-conditional-list)
   (overlay-get overlay
                'modification-hooks)))
"##;

fn auto_dictionary_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_DICTIONARY_MELPA_PIN, source_file)
        .expect("prepare pinned auto-dictionary source below ./tmp")
        .with_prelude(AUTO_DICTIONARY_TEST_PRELUDE)
        .with_timeout(AUTO_DICTIONARY_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-dictionary parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_dictionary_autoload_parity` cases (2a).
pub(crate) fn assert_auto_dictionary_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_dictionary_oracle("auto-dictionary-autoloads.el"),
        &name,
        "auto_dictionary_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_dictionary_parity` cases (2a).
pub(crate) fn assert_auto_dictionary_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_dictionary_oracle("auto-dictionary.el"),
        &name,
        "auto_dictionary_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_dictionary_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_auto_dictionary_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_auto_dictionary_autoload_batch(&cases);
}

#[test]
fn auto_dictionary_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        conditional::conditional_public_surface_batch_cases(),
        detection::detection_public_surface_batch_cases(),
        mode::mode_public_surface_batch_cases(),
        registry::registry_auto_dictionary_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_dictionary_batch(&cases);
}

// END generated package batch tests
