//! Shared merge-aware ceiling ratchet for the reviewed-baseline guards.
//!
//! The promotion ledger (`promotion_ledger_guard_test.rs`) and the plain-pub
//! surface (`plain_pub_surface_guard_test.rs`) both freeze a per-file row
//! ceiling that may only DECREASE, except through a named ceiling-only bump
//! commit. Git history is walked along the first-parent chain. A merge needs
//! care: its own resolution may not raise the ceiling over its first parent,
//! while a side branch that legitimately earned an increase through a judged
//! named bump may carry that value. `judge_merge` judges every commit a merge
//! brought in and then the merge resolution; `side_authorized_value` decides
//! whether the side actually decided an increase (N1/N2/N3 rules, batch 6-8).
//!
//! The owning guard supplies its own review rules: the bump-subject predicate
//! and error tag, plus any historical (path, revision) exemptions.

use std::path::Path;
use std::process::Command;

/// How one guard names its ceiling file, bump commits and errors.
pub(crate) struct Rules<'a> {
    pub ceiling_path: &'a str,
    pub bump_subject: &'a str,
    pub error_tag: &'a str,
    /// Exact historical (path, revision) pairs that may raise this ceiling.
    /// Only pre-convention history is listed here; never a live bypass.
    pub exempt: &'a [(&'static str, &'static str)],
}

impl Rules<'_> {
    fn named_bump(&self, subject: &str, changed: &[String]) -> bool {
        subject.starts_with(self.bump_subject)
            && !subject[self.bump_subject.len()..].trim().is_empty()
            && changed.len() == 1
            && changed[0] == self.ceiling_path
    }

    fn error(&self, body: impl Into<String>) -> String {
        let body = body.into();
        format!("{} {body}", self.error_tag)
    }
}

pub(crate) fn ceiling(text: &str, rules: &Rules) -> Result<usize, String> {
    let text = text.trim();
    if text.is_empty() || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(rules.error("expected one nonnegative integer"));
    }
    text.parse().map_err(|_| rules.error("count overflow"))
}

pub(crate) fn at_most(rows: usize, limit: usize, rules: &Rules) -> Result<(), String> {
    if rows <= limit {
        Ok(())
    } else {
        Err(rules.error(format!("{rows} baseline rows exceed ceiling {limit}")))
    }
}

pub(crate) fn monotonic(
    previous: usize,
    next: usize,
    subject: &str,
    changed: &[String],
    rules: &Rules,
    revision: &str,
) -> Result<(), String> {
    if next <= previous {
        return Ok(());
    }
    if rules
        .exempt
        .iter()
        .any(|(path, sha)| *path == rules.ceiling_path && *sha == revision)
    {
        return Ok(());
    }
    if rules.named_bump(subject, changed) {
        return Ok(());
    }
    Err(rules.error(format!(
        "silent ceiling increase {previous} -> {next}; \
         requires a named ceiling-only bump commit"
    )))
}

fn git(workspace: &Path, args: &[&str], rules: &Rules) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(workspace)
        .args(args)
        .output()
        .map_err(|error| rules.error(format!("git: {error}")))?;
    if !output.status.success() {
        return Err(rules.error(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        )));
    }
    String::from_utf8(output.stdout)
        .map_err(|error| rules.error(format!("non-UTF8 history: {error}")))
}

/// The ceiling value at `revision`, or `None` when the file does not exist
/// there: a revision without the file has no prior value and so acts as a
/// history anchor, like the first revision of the walk.
fn ceiling_at(workspace: &Path, revision: &str, rules: &Rules) -> Result<Option<usize>, String> {
    let listed = git(
        workspace,
        &["ls-tree", "--name-only", revision, "--", rules.ceiling_path],
        rules,
    )?;
    if listed.trim().is_empty() {
        return Ok(None);
    }
    let text = git(
        workspace,
        &["show", &format!("{revision}:{}", rules.ceiling_path)],
        rules,
    )?;
    ceiling(&text, rules).map(Some)
}

fn parents_of(workspace: &Path, revision: &str, rules: &Rules) -> Result<Vec<String>, String> {
    let line = git(
        workspace,
        &["rev-list", "--parents", "-n1", revision],
        rules,
    )?;
    Ok(line
        .split_whitespace()
        .skip(1)
        .map(str::to_string)
        .collect())
}

/// Judge one non-merge commit by the monotonic rule against its own first
/// parent, using its own subject and changed files.
fn judge_commit(
    workspace: &Path,
    revision: &str,
    parents: &[String],
    rules: &Rules,
) -> Result<(), String> {
    let Some(first) = parents.first() else {
        return Ok(());
    };
    let Some(previous) = ceiling_at(workspace, first, rules)? else {
        return Ok(());
    };
    let value = ceiling_at(workspace, revision, rules)?
        .ok_or_else(|| rules.error(format!("{revision} deletes {}", rules.ceiling_path)))?;
    let subject = git(workspace, &["show", "-s", "--format=%s", revision], rules)?;
    let files = git(
        workspace,
        &["diff-tree", "--no-commit-id", "--name-only", "-r", revision],
        rules,
    )?;
    let files: Vec<_> = files.lines().map(str::to_string).collect();
    monotonic(previous, value, subject.trim(), &files, rules, revision)
}

/// A merge's own resolution may not raise the ceiling: its value must be at
/// most its first parent's, or equal to a non-first parent's value ONLY when
/// that side actually decided an INCREASE in `M^1..M^k`: a judged named
/// ceiling-only bump that raised the side's value at least to the result. A
/// side that forked before a main-line decrease has no such decision of its
/// own, so its stale fork-point value must not authorize an evil merge that
/// restores the pre-decrease ceiling. A side DECREASE (5 -> 4) is not an
/// increase decision either: resolving two conflicting decreases to the
/// higher one would silently raise the first parent's judged value. Parents
/// without the file carry no value; when no parent has it the merge is an
/// anchor.
fn judge_merge_result(
    workspace: &Path,
    revision: &str,
    parents: &[String],
    rules: &Rules,
) -> Result<(), String> {
    let value = ceiling_at(workspace, revision, rules)?
        .ok_or_else(|| rules.error(format!("{revision} deletes {}", rules.ceiling_path)))?;
    let mut values = Vec::with_capacity(parents.len());
    for parent in parents {
        values.push(ceiling_at(workspace, parent, rules)?);
    }
    if values.iter().all(Option::is_none) || values[0].is_some_and(|first| value <= first) {
        return Ok(());
    }
    for (index, parent_value) in values.iter().enumerate().skip(1) {
        if parent_value.is_some_and(|parent| parent == value)
            && side_authorized_value(
                workspace,
                &parents[0],
                &parents[index],
                value,
                values[0],
                rules,
            )?
        {
            return Ok(());
        }
    }
    // Measure the rejected increase from the highest parent value BELOW the
    // resolution: a parent above it (an octopus side with a judged increase)
    // is where the merge failed to carry, not the baseline it raised from.
    let previous = values
        .iter()
        .flatten()
        .copied()
        .filter(|parent| *parent < value)
        .max()
        .or(values[0])
        .unwrap_or(value);
    Err(rules.error(format!(
        "silent ceiling increase {previous} -> {value} in the resolution of merge {revision}; \
         a merge may only carry a parent's ceiling"
    )))
}

/// True when SIDE contains a judged ceiling decision authorizing `value`: a
/// named ceiling-only INCREASE commit that raised the side's ceiling at least
/// to `value`. Such a commit is itself judged by `judge_commit`, so the merge
/// merely carries a value the side legitimately earned. A plain decrease is
/// not an increase decision: on a side that forked before main's 5 -> 3, its
/// 5 -> 4 decrease must not authorize resolving the merge to 4, which would
/// raise the first parent's judged 3. When the first parent has no ceiling at
/// all, the side commit that anchors the file also authorizes values it
/// covers.
pub(crate) fn side_authorized_value(
    workspace: &Path,
    first: &str,
    side: &str,
    value: usize,
    first_value: Option<usize>,
    rules: &Rules,
) -> Result<bool, String> {
    let exclude = format!("^{first}");
    let args = [
        "log",
        "--format=%H",
        exclude.as_str(),
        side,
        "--",
        rules.ceiling_path,
    ];
    let listed = git(workspace, &args, rules)?;
    for commit in listed.lines().filter(|line| !line.is_empty()) {
        let parents = parents_of(workspace, commit, rules)?;
        let Some(parent) = parents.first() else {
            continue;
        };
        let Some(previous) = ceiling_at(workspace, parent, rules)? else {
            if first_value.is_none()
                && ceiling_at(workspace, commit, rules)?.is_some_and(|current| value <= current)
            {
                return Ok(true);
            }
            continue;
        };
        // A side commit that deletes the ceiling is never a silent skip: a
        // merge may not carry a value across a deletion, and the deletion
        // must be judged as such (the anchor rule only covers recreation
        // relative to a parent that has no file).
        let Some(current) = ceiling_at(workspace, commit, rules)? else {
            return Err(rules.error(format!(
                "side commit {commit} deletes {}; \
                 a merge may not carry a value across a deletion",
                rules.ceiling_path
            )));
        };
        if current <= previous || current < value {
            continue;
        }
        let subject = git(workspace, &["show", "-s", "--format=%s", commit], rules)?;
        let files = git(
            workspace,
            &["diff-tree", "--no-commit-id", "--name-only", "-r", commit],
            rules,
        )?;
        let files: Vec<_> = files.lines().map(str::to_string).collect();
        if monotonic(previous, current, subject.trim(), &files, rules, commit).is_ok() {
            return Ok(true);
        }
    }
    Ok(false)
}

/// A first-parent merge is not judged by its own subject and (empty) diff:
/// every commit it brought in that touches the ceiling is judged on its own,
/// in topological order, and then the merge result itself.
fn judge_merge(
    workspace: &Path,
    revision: &str,
    parents: &[String],
    rules: &Rules,
) -> Result<(), String> {
    let mut args = vec![
        "log".to_string(),
        "--reverse".into(),
        "--topo-order".into(),
        // Without it, a nested merge TREESAME to one parent hides the
        // ceiling commits reachable only through its other parents.
        "--full-history".into(),
        "--format=%H".into(),
        format!("^{}", parents[0]),
    ];
    args.extend(parents[1..].iter().cloned());
    args.extend(["--".into(), rules.ceiling_path.into()]);
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let brought = git(workspace, &args, rules)?;
    for commit in brought.lines().filter(|line| !line.is_empty()) {
        let commit_parents = parents_of(workspace, commit, rules)?;
        // A nested merge's own side commits are in this same range, so only
        // its resolution needs judging here.
        if commit_parents.len() > 1 {
            judge_merge_result(workspace, commit, &commit_parents, rules)
        } else {
            judge_commit(workspace, commit, &commit_parents, rules)
        }
        .map_err(|error| format!("{error} (brought in by merge {revision} at {commit})"))?;
    }
    judge_merge_result(workspace, revision, parents, rules)
}

/// Walk the first-parent ceiling history and enforce the rules. `rows` is the
/// current reviewed-baseline row count; the on-disk ceiling may not be below
/// it, and an uncommitted increase over the last committed value is refused.
pub(crate) fn ratchet(workspace: &Path, rows: usize, rules: &Rules) -> Result<(), String> {
    let text = std::fs::read_to_string(workspace.join(rules.ceiling_path))
        .map_err(|error| rules.error(format!("read ceiling: {error}")))?;
    let current = ceiling(&text, rules)?;
    at_most(rows, current, rules)?;
    let log = git(
        workspace,
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%H",
            "HEAD",
            "--",
            rules.ceiling_path,
        ],
        rules,
    )?;
    let revisions: Vec<_> = log.lines().filter(|line| !line.is_empty()).collect();
    if revisions.is_empty() {
        return Err(rules.error("ceiling must have a tracked history anchor"));
    }
    let mut previous = None;
    for revision in revisions {
        let revision_parents = parents_of(workspace, revision, rules)?;
        if revision_parents.len() > 1 {
            judge_merge(workspace, revision, &revision_parents, rules)?;
        }
        let value = ceiling_at(workspace, revision, rules)?
            .ok_or_else(|| rules.error(format!("{revision} deletes {}", rules.ceiling_path)))?;
        // A merge was judged above; a non-merge keeps the walk's own rule.
        if let Some(previous) = previous.filter(|_| revision_parents.len() <= 1) {
            let subject = git(workspace, &["show", "-s", "--format=%s", revision], rules)?;
            let files = git(
                workspace,
                &["diff-tree", "--no-commit-id", "--name-only", "-r", revision],
                rules,
            )?;
            let files: Vec<_> = files.lines().map(str::to_string).collect();
            monotonic(previous, value, subject.trim(), &files, rules, revision)?;
        }
        previous = Some(value);
    }
    let previous = previous.ok_or_else(|| rules.error("missing history anchor"))?;
    if current > previous {
        return Err(rules.error("uncommitted ceiling increase"));
    }
    Ok(())
}

/// A throwaway repository for the merge-aware ratchet tests: `commit` writes
/// the given files and commits them with `subject`.
#[cfg(test)]
pub(crate) struct TestRepo(tempfile::TempDir);

#[cfg(test)]
impl TestRepo {
    pub(crate) fn new() -> Self {
        let repo = TestRepo(tempfile::tempdir().unwrap());
        repo.git(&["-c", "init.defaultBranch=main", "init", "-q"]);
        repo
    }

    pub(crate) fn path(&self) -> &Path {
        self.0.path()
    }

    fn run_git(&self, args: &[&str]) -> std::process::Output {
        Command::new("git")
            .current_dir(self.path())
            .args([
                "-c",
                "user.name=t",
                "-c",
                "user.email=t@t",
                "-c",
                "commit.gpgsign=false",
            ])
            .args(args)
            .output()
            .unwrap()
    }

    pub(crate) fn git(&self, args: &[&str]) {
        let output = self.run_git(args);
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    pub(crate) fn write(&self, files: &[(&str, &str)]) {
        for (name, text) in files {
            std::fs::write(self.path().join(name), text).unwrap();
        }
    }

    pub(crate) fn commit(&self, subject: &str, files: &[(&str, &str)]) {
        self.write(files);
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", subject]);
    }

    pub(crate) fn checkout(&self, branch: &str) {
        self.git(&["checkout", "-q", branch]);
    }

    pub(crate) fn checkout_new(&self, branch: &str) {
        self.git(&["checkout", "-q", "-b", branch]);
    }

    pub(crate) fn merge(&self, branch: &str) {
        self.git(&["merge", "-q", "--no-ff", "--no-edit", branch]);
    }

    /// A conflicting merge: the caller resolves the ceiling and commits.
    pub(crate) fn merge_no_commit(&self, branch: &str) {
        let _ = self.run_git(&["merge", "-q", "--no-ff", "--no-commit", branch]);
    }
}
