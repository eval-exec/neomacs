//! The git fixture's contract: pinned content, blind to the host's git
//! environment, and confined to the sandbox that owns it.

use std::path::Path;
use std::process::Command;

use crate::TuiTempDirectory;
use crate::git_fixture::{GIT_ENV_LEAKS, GitCommitSpec, GitFixture, GitFixtureSpec};

/// A two-commit spec, small enough to check by hand.
const SPEC: GitFixtureSpec = GitFixtureSpec {
    directory: "repo",
    branch: "main",
    file: "tracked.txt",
    commits: &[
        GitCommitSpec {
            subject: "first",
            timestamp: "2001-02-03T04:05:06+0000",
            contents: "one\n",
        },
        GitCommitSpec {
            subject: "second",
            timestamp: "2002-03-04T05:06:07+0000",
            contents: "one\ntwo\n",
        },
    ],
    worktree: None,
};

/// Run git in `repo`, clearing the host's redirects as the fixture does -- an
/// inspection that kept them would be redirected too.
fn git(repo: &Path, args: &[&str]) -> String {
    let mut command = Command::new("git");
    command.arg("-C").arg(repo).args(args);
    for leak in GIT_ENV_LEAKS {
        command.env_remove(leak);
    }
    let output = command.output().expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

fn commits(repo: &Path) -> Vec<String> {
    git(repo, &["log", "--format=%H %an %ad %s", "--date=raw"])
        .lines()
        .map(str::to_owned)
        .collect()
}

/// A host that exports redirects, identity and dates cannot reach the fixture.
///
/// This is the property the magit grid depends on: its expected screen pins the
/// commit hashes, so any host input that moved them would fail the test on a
/// machine that merely has `GIT_DIR` exported, a global `init.defaultBranch`,
/// or `commit.gpgsign` set.
#[test]
fn hostile_host_environment_cannot_change_the_fixture() {
    let clean_sandbox = TuiTempDirectory::new("tui-git-clean-");
    let clean = GitFixture::create(clean_sandbox.path(), &SPEC).expect("create fixture");
    let clean_commits = commits(clean.path());
    let clean_branch = git(clean.path(), &["symbolic-ref", "--short", "HEAD"]);

    // The repository the host's redirects point at, and the global config its
    // commands would read: another branch name, signed commits, another author.
    let scratch = TuiTempDirectory::new("tui-git-hostile-");
    let canary = scratch.path().join("canary");
    std::fs::create_dir_all(&canary).expect("create canary directory");
    git(&canary, &["init", "--quiet", "-b", "canary", "."]);
    let hostile_config = scratch.path().join("hostile.gitconfig");
    std::fs::write(
        &hostile_config,
        "[init]\n\tdefaultBranch = hostile\n[commit]\n\tgpgsign = true\n",
    )
    .expect("write hostile gitconfig");

    let hostile_sandbox = TuiTempDirectory::new("tui-git-hostile-fixture-");
    // SAFETY: nextest runs each test in its own process, so mutating the
    // process environment cannot race another test.
    unsafe {
        std::env::set_var("GIT_DIR", canary.join(".git"));
        std::env::set_var("GIT_WORK_TREE", &canary);
        std::env::set_var("GIT_CONFIG_GLOBAL", &hostile_config);
        std::env::set_var("GIT_AUTHOR_NAME", "Hostile Author");
        std::env::set_var("GIT_AUTHOR_EMAIL", "hostile@example.com");
        std::env::set_var("GIT_AUTHOR_DATE", "1999-01-01T00:00:00+0000");
        std::env::set_var("GIT_COMMITTER_DATE", "1999-01-01T00:00:00+0000");
    }
    let hostile = GitFixture::create(hostile_sandbox.path(), &SPEC)
        .expect("create fixture while host is hostile");
    for leak in GIT_ENV_LEAKS {
        // SAFETY: as above.
        unsafe { std::env::remove_var(leak) };
    }

    assert_eq!(
        clean_branch, "main",
        "the branch comes from the spec, not the host's default"
    );
    assert_eq!(
        commits(hostile.path()),
        clean_commits,
        "the host's identity, dates, config or redirects changed the commits"
    );
    assert_eq!(
        git(hostile.path(), &["symbolic-ref", "--short", "HEAD"]),
        "main",
        "the host's global config renamed the branch"
    );
    assert_eq!(
        git(&canary, &["rev-list", "--all", "--count"]),
        "0",
        "the fixture wrote into the repository the host pointed at"
    );
}

/// A spec that leaves a working-tree change starts dirty, and stays dirty in
/// exactly one place: the working tree.  What a status or diff screen reads is
/// this difference, so it has to be the fixture's own file contents and not the
/// index, the commit, or anything another commit would carry.
#[test]
fn worktree_contents_start_unstaged() {
    const DIRTY: GitFixtureSpec = GitFixtureSpec {
        worktree: Some("one\nchanged\n"),
        ..SPEC
    };
    let sandbox = TuiTempDirectory::new("tui-git-dirty-");
    let fixture = GitFixture::create(sandbox.path(), &DIRTY).expect("create dirty fixture");

    assert_eq!(
        std::fs::read_to_string(fixture.path().join("tracked.txt"))
            .expect("read working tree file"),
        "one\nchanged\n",
        "the working tree holds the spec's contents"
    );
    assert_eq!(
        git(fixture.path(), &["show", "HEAD:tracked.txt"]),
        "one\ntwo",
        "the commits are unaffected by the working-tree change"
    );
    assert_eq!(
        git(fixture.path(), &["diff", "--name-only"]),
        "tracked.txt",
        "the change is unstaged, so it is what a status screen lists"
    );
    assert_eq!(
        git(fixture.path(), &["diff", "--cached", "--name-only"]),
        "",
        "nothing is staged"
    );
    // `git` trims its output, so the index column's blank is gone: "M" here is
    // the *worktree* column, which is what makes the change unstaged.
    assert_eq!(
        git(
            fixture.path(),
            &["status", "--porcelain", "--untracked-files=no"]
        ),
        "M tracked.txt",
        "one unstaged modification and nothing else"
    );
}

/// The fixture lives below the sandbox, and the sandbox's drop removes it.
#[test]
fn fixture_is_confined_to_its_sandbox() {
    let fixture_path = {
        let sandbox = TuiTempDirectory::new("tui-git-fixture-");
        let root = sandbox.path().to_path_buf();
        let fixture = GitFixture::create(&root, &SPEC).expect("create fixture");
        assert!(
            fixture.path().starts_with(&root),
            "fixture {} escaped the sandbox {}",
            fixture.path().display(),
            root.display()
        );
        assert!(fixture.path().join(".git").is_dir());
        assert_eq!(git(fixture.path(), &["rev-list", "--all", "--count"]), "2");
        fixture.path().to_path_buf()
        // The sandbox drops here; the fixture only named it.
    };
    assert!(
        !fixture_path.exists(),
        "dropping the sandbox must remove the fixture"
    );
}
