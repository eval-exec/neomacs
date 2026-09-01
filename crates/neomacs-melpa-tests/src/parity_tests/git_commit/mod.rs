use std::time::Duration;

use crate::{CachedMelpaOracle, GIT_COMMIT_MELPA_PIN};

use super::batch_support::assert_oracle_batch_cases;

/// Case constructors in child modules use this via `super::ParityBatchCase`.
pub(crate) use super::batch_support::ParityBatchCase;

mod messages;
mod mode;
mod trailers;

const GIT_COMMIT_TEST_TIMEOUT: Duration = Duration::from_secs(180);

fn git_commit_oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GIT_COMMIT_MELPA_PIN, "git-commit.el")
        .expect("prepare pinned Git-Commit source and dependencies below ./tmp")
        .with_prelude(
            r##"(setq magit-git-global-arguments
                       (append
                        '("-c" "init.defaultBranch=master"
                          "-c" "user.name=A U Thor"
                          "-c" "user.email=a.u.thor@example.com")
                        (and (boundp 'magit-git-global-arguments)
                             magit-git-global-arguments)))"##,
        )
        .with_timeout(GIT_COMMIT_TEST_TIMEOUT)
}

fn current_test_name() -> String {
    let thread = std::thread::current();
    thread
        .name()
        .unwrap_or("unnamed Git-Commit parity test")
        .into()
}

/// Multi-probe batch for `assert_git_commit_parity` cases (2a).
pub(crate) fn assert_git_commit_batch(cases: &[ParityBatchCase]) {
    let name = current_test_name();
    assert_oracle_batch_cases(git_commit_oracle(), &name, "git_commit_parity", cases);
}

// BEGIN generated package batch tests

#[test]
fn git_commit_package_batch() {
    let cases: Vec<ParityBatchCase> = [
        messages::messages_public_surface_batch_cases(),
        mode::mode_public_surface_batch_cases(),
        trailers::trailers_public_surface_batch_cases(),
    ]
    .into_iter()
    .flatten()
    .collect();
    assert_git_commit_batch(&cases);
}

// END generated package batch tests
