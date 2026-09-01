use std::time::Duration;

use crate::{
    AUTO_COMPLETE_MELPA_PIN, AUTO_COMPLETE_NXML_MELPA_PIN, CachedMelpaOracle, POPUP_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod actions;
mod candidates;
mod context;
mod documents;
mod registry;
mod workflows;

const AUTO_COMPLETE_NXML_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_COMPLETE_NXML_TEST_PRELUDE: &str = r##"
(require 'cl)
(require 'cl-lib)

(defun acnxml-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal (car error-data) (cdr error-data)))))

(defun acnxml-test-hash-alist (hash)
  (let (entries)
    (maphash
     (lambda (key value)
       (push (cons key value) entries))
     hash)
    (sort entries
          (lambda (left right)
            (string< (format "%S" (car left))
                     (format "%S" (car right)))))))

(defun acnxml-test-doc-value (doc)
  (when (auto-complete-nxml-doc-p doc)
    (list :name (auto-complete-nxml-doc-name doc)
          :ns (auto-complete-nxml-doc-ns doc)
          :comment (auto-complete-nxml-doc-comment doc)
          :note (auto-complete-nxml-doc-note doc))))

(defun acnxml-test-source-shape (source)
  (mapcar
   (lambda (entry)
     (let ((value (cdr entry)))
       (cons (car entry)
             (cond
              ((functionp value) :function)
              ((symbolp value) value)
              (t value)))))
   source))
"##;

fn auto_complete_nxml_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_NXML_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete-nxml source below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_NXML_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_NXML_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-nxml parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_nxml_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_nxml_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_nxml_oracle("auto-complete-nxml-autoloads.el"),
        &name,
        "auto_complete_nxml_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_nxml_parity` cases (2a).
pub(crate) fn assert_auto_complete_nxml_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_nxml_oracle("auto-complete-nxml.el"),
        &name,
        "auto_complete_nxml_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_nxml_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> =
        [registry::registry_auto_complete_nxml_autoload_batch_cases()]
            .into_iter()
            .flatten()
            .collect();
    assert_auto_complete_nxml_autoload_batch(&cases);
}

#[test]
fn auto_complete_nxml_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        actions::actions_public_surface_batch_cases(),
        candidates::candidates_public_surface_batch_cases(),
        context::context_public_surface_batch_cases(),
        documents::documents_public_surface_batch_cases(),
        registry::registry_auto_complete_nxml_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_nxml_batch(&cases);
}

// END generated package batch tests
