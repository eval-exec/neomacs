use std::time::Duration;

use crate::{CachedMelpaOracle, LSP_MODE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod completion;
mod diagnostics;
mod positions;
mod transport;
mod uri;
mod workspace_edits;

const LSP_MODE_TEST_TIMEOUT: Duration = Duration::from_secs(240);
const LSP_MODE_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

(defun neomacs-lsp-test-json-get (object key)
  "Read KEY from either LSP Mode's hash-table or plist JSON representation."
  (if (hash-table-p object)
      (gethash key object)
    (plist-get object (intern (concat ":" key)))))

(defun neomacs-lsp-test-position-shape (position)
  "Return POSITION as a stable line/character pair."
  (if (hash-table-p position)
      (list (lsp:position-line position)
            (lsp:position-character position))
    (list (plist-get position :line)
          (plist-get position :character))))

(defun neomacs-lsp-test-copy-stats (path)
  "Return a snapshot of LSP diagnostic statistics for PATH."
  (and-let* ((stats (lsp-diagnostics-stats-for path)))
    (copy-sequence stats)))

(defun neomacs-lsp-test-range (start-line start-character end-line end-character)
  "Build an LSP range from its four wire coordinates."
  (lsp-make-range
   :start (lsp-make-position :line start-line :character start-character)
   :end (lsp-make-position :line end-line :character end-character)))
"##;

fn lsp_mode_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LSP_MODE_MELPA_PIN, "lsp-mode.el")
        .expect("prepare revision-pinned LSP Mode source below ./tmp")
        .with_prelude(LSP_MODE_TEST_PRELUDE)
        .with_timeout(LSP_MODE_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed LSP Mode parity test")
        .into()
}

pub(crate) fn assert_lsp_mode_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(lsp_mode_oracle(), &name, "lsp_mode_parity", cases);
}

#[test]
fn lsp_mode_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        transport::transport_public_surface_batch_cases(),
        positions::positions_public_surface_batch_cases(),
        workspace_edits::workspace_edits_public_surface_batch_cases(),
        completion::completion_public_surface_batch_cases(),
        diagnostics::diagnostics_public_surface_batch_cases(),
        uri::uri_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_lsp_mode_batch(&cases);
}
