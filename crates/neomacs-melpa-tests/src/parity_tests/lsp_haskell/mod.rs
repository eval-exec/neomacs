use std::time::Duration;

use crate::{CachedMelpaOracle, LSP_HASKELL_MELPA_PIN, LSP_MODE_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod workflows;

const LSP_HASKELL_TEST_TIMEOUT: Duration = Duration::from_secs(180);

/// Every workflow enters the documented way: `require' the package, which
/// registers the haskell-language-server client with `lsp-mode' and extends
/// `lsp-language-id-configuration' with the five Haskell modes it serves.
/// A live language server never runs in batch, so the workflows pin the
/// whole client-side surface instead: the registered client's fields, the
/// language-id mapping, the 80 `lsp-haskell-*' customization variables with
/// their defaults and types, the pure server-command assembly (including
/// the documented nix-shell wrapper), and the code-action boolean filter
/// the client installs.
///
/// The dependency chain matters: lsp-mode is prepared through
/// `with_melpa_dependency' (which installs lsp-mode plus its own dash/f/ht/
/// lv/markdown-mode/s/spinner closure into the shared package directory),
/// and the suite pins both lsp-mode's and lsp-haskell's versions.
const LSP_HASKELL_TEST_PRELUDE: &str = r##"
(require 'cl-lib)

(defconst lsp-haskell-test-upstream-tree
  "75a53a7cef5d1e9d57bcc5369744784777c9ad87"
  "Git tree of the pinned upstream commit this suite installs.")

(defconst lsp-haskell-test-manifest
  '(("lsp-haskell-pkg.el"
     . "21f3a54f987e41641ac7e2ef2bce130bb901d804cdd004cb1cb2d174262c647b")
    ("lsp-haskell.el"
     . "d9a0dac08be000570bc187ec4c172bbf55aa06f3bf22803c2482f31e97bd7773"))
  "Per-file sha256 of the package-built sources the suite verifies.")

(defun lsp-haskell-test-source-state ()
  "Verify the installed payload is the pinned upstream build."
  (let* ((located (locate-library "lsp-haskell.el"))
         (main (and located (file-truename located)))
         (directory (and main (file-name-directory main))))
    (unless (and main directory
                 (string-suffix-p "/lsp-haskell.el" main)
                 (not (file-symlink-p main)))
      (error "Unexpected installed lsp-haskell location: %S" located))
    (dolist (entry lsp-haskell-test-manifest)
      (let ((file (expand-file-name (car entry) directory)))
        (with-temp-buffer
          (set-buffer-multibyte nil)
          (insert-file-contents-literally file)
          (unless (and (file-regular-p file)
                       (not (file-symlink-p file))
                       (equal (secure-hash 'sha256 (current-buffer))
                              (cdr entry)))
            (error "Unexpected installed lsp-haskell source: %S"
                   (car entry))))))
    (list :upstream-tree lsp-haskell-test-upstream-tree
          :feature (featurep 'lsp-haskell)
          :version (package-version-join
                    (package-desc-version
                     (cadr (assq 'lsp-haskell package-alist))))
          :lsp-mode (package-version-join
                     (package-desc-version
                      (cadr (assq 'lsp-mode package-alist))))
          :defcustom-count
          (let ((count 0))
            (mapatoms
             (lambda (symbol)
               (when (and (string-prefix-p "lsp-haskell-" (symbol-name symbol))
                          (custom-variable-p symbol))
                 (setq count (1+ count)))))
            count))))
"##;

fn lsp_haskell_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(LSP_HASKELL_MELPA_PIN, "lsp-haskell.el")
        .expect("prepare pinned lsp-haskell source below ./tmp")
        .with_melpa_dependency(LSP_MODE_MELPA_PIN)
        .expect("prepare pinned lsp-mode dependency")
        .with_prelude(LSP_HASKELL_TEST_PRELUDE)
        .with_timeout(LSP_HASKELL_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed lsp-haskell parity test")
        .into()
}

/// Multi-probe batch for `assert_lsp_haskell_parity` cases (2a).
pub(crate) fn assert_lsp_haskell_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(lsp_haskell_oracle(), &name, "lsp_haskell_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn lsp_haskell_package_batch() {
    let cases: Vec<ParityBatchCase> = [workflows::workflows_public_surface_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_lsp_haskell_batch(&cases);
}

// END generated package batch tests
