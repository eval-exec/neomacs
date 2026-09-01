use std::time::Duration;

use crate::{
    AUCTEX_GNU_ELPA_PIN, AUTO_COMPLETE_AUCTEX_MELPA_PIN, AUTO_COMPLETE_MELPA_PIN,
    CachedMelpaOracle, POPUP_MELPA_PIN, YASNIPPET_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod actions;
mod arguments;
mod candidates;
mod registry;
mod workflows;

const AUTO_COMPLETE_AUCTEX_TEST_TIMEOUT: Duration = Duration::from_secs(180);
const AUTO_COMPLETE_AUCTEX_TEST_PRELUDE: &str = r##"
;; The 2014 source expects the old cl.el macros to have been expanded by
;; byte-compilation.  Loading cl preserves that source contract while keeping
;; the package payload itself unmodified.
(require 'cl)
(require 'seq)
(require 'auto-complete)
(require 'yasnippet)

(defvar candidate nil)

;; popup.el obtains these coordinates from the display engine.  Stable batch
;; coordinates retain the real completion lifecycle without workstation frame
;; geometry leaking into parity results.
(defun ac-auctex-test-posn-at-point (&rest _arguments)
  'ac-auctex-test-position)

(defun ac-auctex-test-posn-col-row (_position)
  (cons
   (current-column)
   (line-number-at-pos (point))))

(fset 'posn-at-point #'ac-auctex-test-posn-at-point)
(fset 'posn-col-row #'ac-auctex-test-posn-col-row)

(defun ac-auctex-test-error-data (thunk)
  (condition-case error-data
      (list :value
            (funcall thunk))
    (error
     (list :signal
           (car error-data)
           (cdr error-data)))))
"##;

fn auto_complete_auctex_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_AUCTEX_MELPA_PIN, "auto-complete-auctex.el")
        .expect("prepare pinned auto-complete-auctex source below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup dependency below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(YASNIPPET_MELPA_PIN)
        .expect("prepare pinned yasnippet dependency below ./tmp")
        .with_gnu_elpa_dependency(AUCTEX_GNU_ELPA_PIN)
        .expect("prepare pinned AUCTeX dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_AUCTEX_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_AUCTEX_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-auctex parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_auctex_parity` cases (2a).
pub(crate) fn assert_auto_complete_auctex_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_auctex_oracle(),
        &name,
        "auto_complete_auctex_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_auctex_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        actions::actions_public_surface_batch_cases(),
        arguments::arguments_public_surface_batch_cases(),
        candidates::candidates_public_surface_batch_cases(),
        registry::registry_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_auctex_batch(&cases);
}

// END generated package batch tests
