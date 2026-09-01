use std::time::Duration;

use crate::{AUTO_HIGHLIGHT_SYMBOL_MELPA_PIN, CachedMelpaOracle, DASH_MELPA_PIN, HT_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod editing;
mod highlighting;
mod lifecycle;
mod navigation;
mod plugins;
mod predicates;
mod registry;
mod workflows;

const AUTO_HIGHLIGHT_SYMBOL_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_HIGHLIGHT_SYMBOL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defvar auto-highlight-symbol-test-events nil)

(defun auto-highlight-symbol-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))

(defun auto-highlight-symbol-test-overlays ()
  (mapcar
   (lambda (overlay)
     (list
      (overlay-start overlay)
      (overlay-end overlay)
      (overlay-get overlay 'ahs-symbol)
      (overlay-get overlay 'face)
      (overlay-get overlay 'priority)
      (overlay-get overlay 'evaporate)
      (eq
       (overlay-get overlay 'window)
       (selected-window))))
   (sort
    (seq-filter
     (lambda (overlay)
       (overlay-get overlay 'ahs-symbol))
     (overlays-in
      (point-min)
      (point-max)))
    (lambda (left right)
      (let ((left-start
             (overlay-start left))
            (right-start
             (overlay-start right)))
        (if (= left-start right-start)
            (eq
             (overlay-get left 'ahs-symbol)
             'current)
          (< left-start right-start)))))))

(defun auto-highlight-symbol-test-mode-state ()
  (list
   auto-highlight-symbol-mode
   ahs-current-range
   ahs-mode-line
   ahs-edit-mode-enable
   (memq
    'ahs-start-timer
    post-command-hook)
   (memq
    'ahs-start-timer
    after-change-functions)
   (length ahs-current-overlay)
   (length ahs-overlay-list)))
"##;

fn auto_highlight_symbol_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_HIGHLIGHT_SYMBOL_MELPA_PIN, source_file)
        .expect("prepare pinned auto-highlight-symbol source below ./tmp")
        .with_melpa_dependency(DASH_MELPA_PIN)
        .expect("prepare pinned dash transitive dependency below ./tmp")
        .with_melpa_dependency(HT_MELPA_PIN)
        .expect("prepare pinned ht dependency below ./tmp")
        .with_prelude(AUTO_HIGHLIGHT_SYMBOL_TEST_PRELUDE)
        .with_timeout(AUTO_HIGHLIGHT_SYMBOL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-highlight-symbol parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_highlight_symbol_autoload_parity` cases (2a).
pub(crate) fn assert_auto_highlight_symbol_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_highlight_symbol_oracle("auto-highlight-symbol-autoloads.el"),
        &name,
        "auto_highlight_symbol_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_highlight_symbol_parity` cases (2a).
pub(crate) fn assert_auto_highlight_symbol_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_highlight_symbol_oracle("auto-highlight-symbol.el"),
        &name,
        "auto_highlight_symbol_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_highlight_symbol_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_highlight_symbol_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_highlight_symbol_autoload_batch(&cases);
}

#[test]
fn auto_highlight_symbol_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        editing::editing_public_surface_batch_cases(),
        highlighting::highlighting_public_surface_batch_cases(),
        lifecycle::lifecycle_public_surface_batch_cases(),
        navigation::navigation_public_surface_batch_cases(),
        plugins::plugins_public_surface_batch_cases(),
        predicates::predicates_public_surface_batch_cases(),
        registry::registry_auto_highlight_symbol_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_highlight_symbol_batch(&cases);
}

// END generated package batch tests
