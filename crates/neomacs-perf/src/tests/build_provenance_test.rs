use std::fs;
use std::process::Command;

#[allow(dead_code)]
#[path = "../../build.rs"]
mod perf_build_script;

fn git(directory: &std::path::Path, arguments: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(arguments)
        .status()
        .expect("run git");
    assert!(status.success(), "git {arguments:?} failed");
}

#[test]
fn git_metadata_watch_paths_are_absolute_and_worktree_aware() {
    let workspace_tmp = crate::workspace_root().join("tmp");
    fs::create_dir_all(&workspace_tmp).expect("create workspace-local test scratch root");
    let scratch = tempfile::Builder::new()
        .prefix("neomacs-perf-build-provenance-")
        .tempdir_in(&workspace_tmp)
        .expect("create workspace-local Git repository");
    let repository = scratch.path().join("repository");
    let worktree = scratch.path().join("worktree");
    fs::create_dir(&repository).expect("create repository");
    git(&repository, &["init", "-q"]);
    git(
        &repository,
        &["config", "user.email", "tests@neomacs.invalid"],
    );
    git(&repository, &["config", "user.name", "Neomacs tests"]);
    fs::create_dir(repository.join("crates")).expect("create harness input directory");
    fs::create_dir(repository.join("docs")).expect("create unrelated tracked directory");
    fs::write(repository.join("crates/input"), "initial\n").expect("write harness input");
    fs::write(repository.join("docs/note"), "initial\n").expect("write unrelated input");
    git(&repository, &["add", "crates/input", "docs/note"]);
    git(&repository, &["commit", "-qm", "initial"]);
    git(
        &repository,
        &[
            "worktree",
            "add",
            "-qb",
            "benchmark-worktree",
            worktree.to_str().expect("UTF-8 test path"),
        ],
    );

    let paths = perf_build_script::git_metadata_watch_paths(&worktree)
        .expect("resolve worktree Git metadata");
    assert!(paths.len() >= 2);
    assert!(paths.iter().all(|path| path.is_absolute()));
    assert!(paths.iter().all(|path| path.is_file()));
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("worktrees/worktree/HEAD"))
    );
    assert!(
        paths
            .iter()
            .any(|path| path.ends_with("refs/heads/benchmark-worktree"))
    );

    fs::write(worktree.join("docs/note"), "changed\n").expect("change unrelated input");
    assert_eq!(
        perf_build_script::tracked_harness_inputs_dirty(&worktree),
        Some(false)
    );
    fs::write(worktree.join("crates/input"), "changed\n").expect("change harness input");
    assert_eq!(
        perf_build_script::tracked_harness_inputs_dirty(&worktree),
        Some(true)
    );
}
