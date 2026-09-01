use std::time::Duration;

use crate::{AUDACIOUS_MELPA_PIN, CachedMelpaOracle, HELM_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod commands;
mod playlists;
mod registry;
mod songs;
mod workflows;

const AUDACIOUS_TEST_TIMEOUT: Duration = Duration::from_secs(120);
const AUDACIOUS_TEST_PRELUDE: &str = r##"
(require 'cl-lib)
(require 'seq)

;; The package only needs Helm's two public entry points.  Individual parity
;; cases install deterministic seams around both functions.
(provide 'helm)

(defun audacious-test-executable-find
    (command)
  (and
   (equal command "audtool")
   "/fixture/bin/audtool"))

(fset 'executable-find
      #'audacious-test-executable-find)

(defun audacious-test-reset-state ()
  (setq audacious-msg ""
        audacious-playlist-position nil
        audacious-playlist-length nil
        audacious-playlist-name nil
        audacious-song-title nil
        audacious-song-position nil
        audacious-song-length nil))

(defun audacious-test-error-data
    (thunk)
  (condition-case error-data
      (list :ok
            (funcall thunk))
    (error
     (list :error
           (car error-data)
           (cdr error-data)))))
"##;

fn audacious_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(AUDACIOUS_MELPA_PIN, "audacious.el")
        .expect("prepare pinned audacious source and dependencies below ./tmp")
        .with_melpa_dependency(HELM_MELPA_PIN)
        .expect("prepare pinned Helm dependency below ./tmp")
        .with_prelude(AUDACIOUS_TEST_PRELUDE)
        .with_timeout(AUDACIOUS_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed audacious parity test")
        .into()
}

/// Multi-probe batch for `assert_audacious_autoload_parity` cases (2a).
pub(crate) fn assert_audacious_autoload_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(
        audacious_oracle().with_installed_autoloads(),
        &name,
        "audacious_autoload_parity",
        cases,
    );
}

/// Multi-probe batch for `assert_audacious_parity` cases (2a).
pub(crate) fn assert_audacious_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(audacious_oracle(), &name, "audacious_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn audacious_autoload_package_batch() {
    let cases: Vec<ParityBatchCase> = [registry::registry_audacious_autoload_batch_cases()]
        .into_iter()
        .flatten()
        .collect();
    assert_audacious_autoload_batch(&cases);
}

#[test]
fn audacious_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        commands::commands_public_surface_batch_cases(),
        playlists::playlists_public_surface_batch_cases(),
        registry::registry_audacious_batch_cases(),
        songs::songs_public_surface_batch_cases(),
        workflows::workflows_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_audacious_batch(&cases);
}

// END generated package batch tests
