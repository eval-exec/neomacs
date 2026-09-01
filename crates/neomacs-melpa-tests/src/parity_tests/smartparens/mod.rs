use std::time::Duration;

use crate::{CachedMelpaOracle, MARKDOWN_MODE_MELPA_PIN, SMARTPARENS_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod insertion;
mod language_workflows;
mod strict_editing;
mod structural_editing;
mod wrapping;

const SMARTPARENS_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const SMARTPARENS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'smartparens-config)
(require 'markdown-mode)
(require 'smartparens-markdown)

(defun neomacs-smartparens-test-balanced-p ()
  "Return non-nil when the accessible buffer has balanced delimiters."
  (condition-case nil
      (progn
        (check-parens)
        t)
    (error nil)))

(defun neomacs-smartparens-test-state (label)
  "Capture a practical editor checkpoint under LABEL."
  (let ((parse-state (syntax-ppss)))
    (list
     :label label
     :buffer (buffer-substring-no-properties (point-min) (point-max))
     :point (point)
     :mark (and mark-active (mark))
     :depth (nth 0 parse-state)
     :string (nth 3 parse-state)
     :comment (nth 4 parse-state)
     :balanced (neomacs-smartparens-test-balanced-p))))

(defun neomacs-smartparens-test-sexp-shape (sexp)
  "Return the stable, user-visible shape of Smartparens SEXP."
  (when sexp
    (let ((beg (plist-get sexp :beg))
          (end (plist-get sexp :end)))
      (list
       :beg beg
       :end end
       :open (plist-get sexp :op)
       :close (plist-get sexp :cl)
       :text (and beg end
                  (buffer-substring-no-properties beg end))))))
"##;

fn smartparens_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(SMARTPARENS_MELPA_PIN, "smartparens.el")
        .expect("prepare revision-pinned Smartparens source below ./tmp")
        .with_melpa_dependency(MARKDOWN_MODE_MELPA_PIN)
        .expect("prepare pinned Markdown Mode integration dependency")
        .with_prelude(SMARTPARENS_TEST_PRELUDE)
        .with_timeout(SMARTPARENS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Smartparens parity test")
        .into()
}

pub(crate) fn assert_smartparens_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(smartparens_oracle(), &name, "smartparens_parity", cases);
}

#[test]
fn smartparens_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        insertion::insertion_public_surface_batch_cases(),
        structural_editing::structural_editing_public_surface_batch_cases(),
        wrapping::wrapping_public_surface_batch_cases(),
        strict_editing::strict_editing_public_surface_batch_cases(),
        language_workflows::language_workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_smartparens_batch(&cases);
}
