use std::time::Duration;

use crate::{ALECTRYON_MELPA_PIN, CachedMelpaOracle};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod configuration;
mod editing;
mod errors;
mod flycheck;
mod modes;
mod process;
mod registry;

const ALECTRYON_TEST_TIMEOUT: Duration = Duration::from_secs(180);

const MODE_FIXTURE_PRELUDE: &str = r##"
(progn
  (require 'rst)
  (unless (fboundp 'coq-mode)
    (define-derived-mode coq-mode prog-mode "Coq"
      (modify-syntax-entry ?\( "()1n")
      (modify-syntax-entry ?* ". 23n")
      (modify-syntax-entry ?\) ")(4n")
      (setq-local comment-start "(*"
                  comment-end "*)")))
  (unless (fboundp 'lean4-mode)
    (define-derived-mode lean4-mode prog-mode "Lean4"
      (modify-syntax-entry ?/ ". 14n")
      (modify-syntax-entry ?- ". 23n")
      (setq-local comment-start "/-"
                  comment-end "-/")))
  (unless (fboundp 'dafny-mode)
    (define-derived-mode dafny-mode prog-mode "Dafny"
      (modify-syntax-entry ?/ ". 124b")
      (modify-syntax-entry ?\n "> b")
      (setq-local comment-start "//"
                  comment-end "")))
  (unless (fboundp 'markdown-mode)
    (define-derived-mode markdown-mode text-mode "Markdown"))
  (unless (fboundp 'typst-ts-mode)
    (define-derived-mode typst-ts-mode text-mode "Typst")))"##;

fn alectryon_oracle(source_file: &str) -> CachedMelpaOracle {
    CachedMelpaOracle::new(ALECTRYON_MELPA_PIN, source_file)
        .expect("prepare pinned Alectryon source below ./tmp")
        .with_prelude(MODE_FIXTURE_PRELUDE)
        .with_timeout(ALECTRYON_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Alectryon parity test")
        .into()
}

/// Multi-probe batch for `assert_alectryon_autoload_parity` cases (2a).
pub(crate) fn assert_alectryon_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        alectryon_oracle("alectryon-autoloads.el"),
        &name,
        "alectryon_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_alectryon_parity` cases (2a).
pub(crate) fn assert_alectryon_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        alectryon_oracle("alectryon.el"),
        &name,
        "alectryon_parity",
        cases,
    );
}

// BEGIN generated package batch tests

#[test]
fn alectryon_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_alectryon_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_alectryon_autoload_batch(&cases);
}

#[test]
fn alectryon_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        configuration::configuration_public_surface_batch_cases(),
        editing::editing_public_surface_batch_cases(),
        errors::errors_public_surface_batch_cases(),
        flycheck::flycheck_public_surface_batch_cases(),
        modes::modes_public_surface_batch_cases(),
        process::process_public_surface_batch_cases(),
        registry::registry_alectryon_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_alectryon_batch(&cases);
}

// END generated package batch tests
