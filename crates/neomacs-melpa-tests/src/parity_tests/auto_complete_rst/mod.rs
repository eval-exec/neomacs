use std::time::Duration;

use crate::{
    AUTO_COMPLETE_MELPA_PIN, AUTO_COMPLETE_RST_MELPA_PIN, CachedMelpaOracle, POPUP_MELPA_PIN,
};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod generation;
mod parsing;
mod registry;
mod sources;
mod workflows;

const AUTO_COMPLETE_RST_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUTO_COMPLETE_RST_TEST_PRELUDE: &str = r##"
(require 'cl)
(require 'cl-lib)
(require 'rst)

(defun auto-complete-rst-test-error (thunk)
  (condition-case error-data
      (list :value (funcall thunk))
    (error
     (list :signal (car error-data) (cdr error-data)))))

(defun auto-complete-rst-test-hash-alist (hash)
  (let (entries)
    (maphash
     (lambda (key value)
       (push (cons key value) entries))
     hash)
    (sort entries
          (lambda (left right)
            (string< (format "%S" (car left))
                     (format "%S" (car right)))))))

(defun auto-complete-rst-test-source-shape (source)
  (mapcar
   (lambda (entry)
     (let ((value (cdr entry)))
       (cons (car entry)
             (cond
              ((functionp value) :function)
              ((symbolp value) value)
              (t value)))))
   source))

(defun auto-complete-rst-test-count-eq (needle values)
  (let ((count 0))
    (dolist (value values count)
      (when (eq needle value)
        (setq count (1+ count))))))

(defun auto-complete-rst-test-insert-generated-source ()
  (insert
   "(defun auto-complete-rst-directives-candidates ()\n"
   "  '(\"note::\" \"code-block::\" \"image::\" \"py:function::\"))\n"
   "(defun auto-complete-rst-roles-candidates ()\n"
   "  '(\"ref:\" \"doc:\" \"py:class:\" \"emphasis:\"))\n"
   "(puthash \"note\" '(\"class:\" \"name:\") auto-complete-rst-directive-options-map)\n"
   "(puthash \"code-block\" '(\"class:\" \"linenos:\" \"caption:\") auto-complete-rst-directive-options-map)\n"
   "(puthash \"image\" '(\"alt:\" \"height:\" \"width:\") auto-complete-rst-directive-options-map)\n"
   "(puthash \"py:function\" '(\"module:\" \"noindex:\") auto-complete-rst-directive-options-map)\n"))

;; popup.el normally obtains these coordinates from the display engine.
(defun auto-complete-rst-test-posn-at-point (&rest _arguments)
  'auto-complete-rst-test-position)

(defun auto-complete-rst-test-posn-col-row (_position)
  (cons
   (current-column)
   (line-number-at-pos
    (point))))

(fset 'posn-at-point #'auto-complete-rst-test-posn-at-point)
(fset 'posn-col-row #'auto-complete-rst-test-posn-col-row)
"##;

fn auto_complete_rst_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUTO_COMPLETE_RST_MELPA_PIN, source_file)
        .expect("prepare pinned auto-complete-rst source below ./tmp")
        .with_melpa_dependency(AUTO_COMPLETE_MELPA_PIN)
        .expect("prepare pinned auto-complete dependency below ./tmp")
        .with_melpa_dependency(POPUP_MELPA_PIN)
        .expect("prepare pinned popup transitive dependency below ./tmp")
        .with_prelude(AUTO_COMPLETE_RST_TEST_PRELUDE)
        .with_timeout(AUTO_COMPLETE_RST_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed auto-complete-rst parity test")
        .into()
}

/// Multi-probe batch for `assert_auto_complete_rst_autoload_parity` cases (2a).
pub(crate) fn assert_auto_complete_rst_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_rst_oracle("auto-complete-rst-autoloads.el"),
        &name,
        "auto_complete_rst_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_auto_complete_rst_parity` cases (2a).
pub(crate) fn assert_auto_complete_rst_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        auto_complete_rst_oracle("auto-complete-rst.el"),
        &name,
        "auto_complete_rst_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn auto_complete_rst_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_auto_complete_rst_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_auto_complete_rst_autoload_batch(&cases);
}

#[test]
fn auto_complete_rst_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        generation::generation_public_surface_batch_cases(),
        parsing::parsing_public_surface_batch_cases(),
        registry::registry_auto_complete_rst_batch_cases(),
        sources::sources_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_auto_complete_rst_batch(&cases);
}

// END generated package batch tests
