//! The Magit fixture repository must be a function of the scenario alone.

use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::Path;
use std::process::Command;

use super::magit_repository::{
    MAGIT_FIXTURE_GIT_ENV, MAGIT_FIXTURE_GIT_ENV_LEAKS, configure_magit_fixture_git_command,
    magit_fixture_git_command,
};
use super::{ScenarioId, prepare_magit_repository};

fn git_output(repository: &Path, arguments: &[&str]) -> String {
    let output = magit_fixture_git_command(repository)
        .args(arguments)
        .output()
        .expect("launch git");
    assert!(
        output.status.success(),
        "git {arguments:?} failed ({}): {}",
        output.status,
        String::from_utf8_lossy(&output.stderr).trim()
    );
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

#[test]
fn magit_fixture_disables_auto_maintenance_and_leaves_no_background_packs() {
    let run_dir = tempfile::tempdir().expect("run directory");
    let repo = prepare_magit_repository(run_dir.path(), ScenarioId::MagitStatusHeavy)
        .expect("prepare heavy magit fixture");

    for (key, expected) in [
        ("maintenance.auto", "false"),
        ("maintenance.autoDetach", "false"),
        ("gc.auto", "0"),
        ("gc.autoDetach", "false"),
        ("core.hooksPath", "/dev/null"),
    ] {
        let actual = git_output(&repo, &["config", "--local", "--get", key]);
        assert_eq!(
            actual.trim(),
            expected,
            "local fixture config {key} must be pinned to {expected}"
        );
    }

    let pack_dir = repo.join(".git/objects/pack");
    let pack_entries: Vec<String> = if pack_dir.is_dir() {
        std::fs::read_dir(&pack_dir)
            .expect("read .git/objects/pack")
            .map(|entry| {
                entry
                    .expect("pack dir entry")
                    .file_name()
                    .to_string_lossy()
                    .into_owned()
            })
            .collect()
    } else {
        Vec::new()
    };
    assert!(
        pack_entries.is_empty(),
        "background git maintenance must not create packfiles in fixture repo: {pack_entries:?}"
    );
    assert!(
        !repo.join(".git/objects/maintenance.lock").exists(),
        "background git maintenance lock must not exist in fixture repo"
    );
    let bucket_17 = repo.join(".git/objects/17");
    let bucket_17_count = std::fs::read_dir(&bucket_17)
        .expect("read .git/objects/17")
        .count();
    assert!(
        bucket_17_count >= 2,
        "MagitStatusHeavy must cross Git 2.54's loose-object threshold (>= 2 in objects/17) \
         while staying unpacked; found {bucket_17_count}"
    );
}

#[test]
fn magit_fixture_git_command_strips_leaked_git_redirect_and_config_env() {
    let run_dir = tempfile::tempdir().expect("run directory");
    let repo = prepare_magit_repository(run_dir.path(), ScenarioId::MagitStatus)
        .expect("prepare minimal magit fixture");

    let configured = magit_fixture_git_command(&repo);
    let env_map: BTreeMap<&OsStr, Option<&OsStr>> = configured.get_envs().collect();
    for leak in MAGIT_FIXTURE_GIT_ENV_LEAKS {
        assert_eq!(
            env_map.get(OsStr::new(leak)),
            Some(&None),
            "magit_fixture_git_command must explicitly remove ambient {leak}"
        );
    }
    for (key, value) in MAGIT_FIXTURE_GIT_ENV {
        assert_eq!(
            env_map.get(OsStr::new(key)),
            Some(&Some(OsStr::new(value))),
            "magit_fixture_git_command must pin {key}={value}"
        );
    }

    // Seed a real git invocation with poisoned redirect/config variables and
    // verify `configure_magit_fixture_git_command` neutralizes them so git
    // still targets `repo` and sees `maintenance.auto=false`.
    let mut poisoned_toplevel = Command::new("git");
    poisoned_toplevel
        .env("GIT_DIR", "/nonexistent-magit-fixture-git-dir")
        .env("GIT_WORK_TREE", "/nonexistent-magit-fixture-work-tree")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "maintenance.auto")
        .env("GIT_CONFIG_VALUE_0", "true");
    configure_magit_fixture_git_command(&mut poisoned_toplevel, &repo);
    let toplevel_out = poisoned_toplevel
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .expect("launch git rev-parse");
    assert!(
        toplevel_out.status.success(),
        "poisoned git rev-parse failed: {}",
        String::from_utf8_lossy(&toplevel_out.stderr).trim()
    );
    let actual_toplevel = std::fs::canonicalize(
        String::from_utf8(toplevel_out.stdout)
            .expect("utf-8 toplevel")
            .trim(),
    )
    .expect("canonicalize actual toplevel");
    let expected_toplevel = std::fs::canonicalize(&repo).expect("canonicalize repo");
    assert_eq!(actual_toplevel, expected_toplevel);

    let mut poisoned_maint = Command::new("git");
    poisoned_maint
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "maintenance.auto")
        .env("GIT_CONFIG_VALUE_0", "true");
    configure_magit_fixture_git_command(&mut poisoned_maint, &repo);
    let maint_out = poisoned_maint
        .args(["config", "--get", "maintenance.auto"])
        .output()
        .expect("launch git config");
    assert!(
        maint_out.status.success(),
        "poisoned git config failed: {}",
        String::from_utf8_lossy(&maint_out.stderr).trim()
    );
    assert_eq!(
        String::from_utf8(maint_out.stdout)
            .expect("utf-8 maintenance.auto")
            .trim(),
        "false"
    );
}
