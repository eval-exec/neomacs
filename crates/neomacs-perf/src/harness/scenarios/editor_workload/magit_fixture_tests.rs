//! The Magit fixture repository must be a function of the scenario alone.

use std::path::Path;
use std::process::Command;

use super::{ScenarioId, prepare_magit_repository};

fn git_output(repository: &Path, arguments: &[&str]) -> String {
    let output = Command::new("git")
        .args(arguments)
        .current_dir(repository)
        .output()
        .expect("launch git");
    assert!(output.status.success(), "git {arguments:?} failed");
    String::from_utf8(output.stdout).expect("utf-8 git output")
}

/// Everything Magit renders from the object store: HEAD, the full log and the
/// stash. Two builds must agree byte for byte, or the two arms of an A/B (and
/// two hosts) render different status buffers.
fn object_identity(repository: &Path) -> String {
    [
        git_output(repository, &["rev-parse", "HEAD"]),
        git_output(repository, &["symbolic-ref", "HEAD"]),
        git_output(
            repository,
            &["log", "--format=%H %an <%ae> %ad %cn <%ce> %cd %s"],
        ),
        git_output(repository, &["stash", "list", "--format=%H %gs"]),
    ]
    .concat()
}

#[test]
fn magit_fixture_repositories_are_identical_across_builds() {
    for scenario in [ScenarioId::MagitStatus, ScenarioId::MagitStatusHeavy] {
        let first = tempfile::tempdir().expect("first run directory");
        let second = tempfile::tempdir().expect("second run directory");
        let a = prepare_magit_repository(first.path(), scenario).expect("first fixture");
        // A different wall-clock second must not change anything either.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        let b = prepare_magit_repository(second.path(), scenario).expect("second fixture");
        assert_eq!(
            object_identity(&a),
            object_identity(&b),
            "{scenario:?}: fixture history differs between two builds"
        );
    }
}
