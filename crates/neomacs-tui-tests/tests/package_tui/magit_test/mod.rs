use std::time::Duration;

use expect_test::expect_file;
use neomacs_tui_tests::RawTerminalSnapshot;
use neomacs_tui_tests::git_fixture::{GitCommitSpec, GitFixtureSpec};
use neomacs_tui_tests::package_harness::{both, invoke_with_prompt_timeout};

use super::{CachedMelpaOracle, MAGIT_MELPA_PIN};

use super::scenario::{DisplayCheckpoint, PackageTuiScenario, PairTimeout, ReadinessCheckpoint};

/// The repository the log screen is rendered from.
///
/// The harness creates it inside each peer's sandbox before that peer boots,
/// and the sandbox's drop removes it.  Branch, contents, identity and dates are
/// all pinned here: the expected grid pins the resulting commit hashes, so
/// nothing may come from the host's git configuration, clock, or timezone.
const MAGIT_LOG_FIXTURE: GitFixtureSpec = GitFixtureSpec {
    directory: "repo",
    branch: "main",
    file: "tracked.txt",
    commits: &[
        GitCommitSpec {
            subject: "short",
            timestamp: "2001-02-03T04:05:06+0000",
            contents: "one\n",
        },
        GitCommitSpec {
            subject: "a deliberately much longer subject",
            timestamp: "2002-03-04T05:06:07+0000",
            contents: "one\ntwo\n",
        },
        GitCommitSpec {
            subject: "medium subject",
            timestamp: "2003-04-05T06:07:08+0000",
            contents: "one\ntwo\nthree\n",
        },
    ],
    worktree: None,
};

/// The repository the status and diff screens are rendered from.
///
/// Same commits as the log fixture, but the working tree keeps one unstaged
/// change to the tracked file: that difference is what the status screen lists
/// and what the diff screen shows, so it comes from the fixture rather than
/// from either editor typing into the buffer.
const MAGIT_STATUS_FIXTURE: GitFixtureSpec = GitFixtureSpec {
    worktree: Some("one\ntwo\nthree\nfour\n"),
    ..MAGIT_LOG_FIXTURE
};

/// What both Magit screens need to render the fixture the same way in either
/// editor: Magit in the one window, a blank header and mode line, dates in the
/// margin rather than as relative ages, and the fixture's repository as the
/// default directory.
///
/// The mode line is blanked because it names the buffer's file with its
/// absolute path, and the two peers own different sandboxes; the dates are
/// absolute for the same reason the commits are pinned -- the screen may not
/// depend on when it was rendered.
const MAGIT_TUI_COMMON: &str = r#"
(require 'magit)
(defun neomacs-magit-tui-display-same-window (buffer)
  (display-buffer-same-window buffer nil))
(defun neomacs-magit-tui-stabilize-window ()
  (setq-local header-line-format nil
              mode-line-format nil))
(setq inhibit-message t
      byte-compile-verbose nil
      magit-display-buffer-function #'neomacs-magit-tui-display-same-window
      magit-log-margin
      '(t "%Y-%m-%d %a %H:%M" magit-log-margin-width t 18))
(let ((repo (getenv "NEOMACS_TUI_GIT_FIXTURE")))
  (unless (and repo (file-directory-p repo))
    (error "NEOMACS_TUI_GIT_FIXTURE is not a directory: %S" repo))
  (setq default-directory (file-name-as-directory repo)))
"#;

const MAGIT_LOG_SCREEN: &str = r#"
(add-hook 'magit-log-mode-hook #'neomacs-magit-tui-stabilize-window)
(find-file (expand-file-name "tracked.txt" default-directory))
(magit-log-buffer-file)
(when-let* ((warnings (get-buffer "*Warnings*")))
  (kill-buffer warnings))
(delete-other-windows)
"#;

const MAGIT_STATUS_SCREEN: &str = r#"
(add-hook 'magit-status-mode-hook #'neomacs-magit-tui-stabilize-window)
(add-hook 'magit-diff-mode-hook #'neomacs-magit-tui-stabilize-window)
(magit-status)
(when-let* ((warnings (get-buffer "*Warnings*")))
  (kill-buffer warnings))
(delete-other-windows)
"#;

/// The screens are set up with `inhibit-message` in effect, which keeps Magit's
/// progress messages out of the grid -- but it also stops GNU from *clearing*
/// the echo area (xdisp.c's `clear_message` returns early when the variable is
/// set), so whatever startup last printed would stay there and hide the
/// minibuffer prompt until the next keystroke redrew the row.  Clearing it
/// explicitly, with the inhibition off, leaves the screen with no echo-area
/// text to freeze and no stale row in front of a prompt.
const MAGIT_CLEAR_ECHO_AREA: &str = r#"
(let ((inhibit-message nil))
  (message nil))
"#;

fn magit_prelude(screen: &str) -> String {
    format!("{MAGIT_TUI_COMMON}{screen}{MAGIT_CLEAR_ECHO_AREA}")
}

#[test]
fn magit_log_buffer_file_margin_columns_match_gnu_full_screen() {
    let oracle = CachedMelpaOracle::new(MAGIT_MELPA_PIN, "magit.el")
        .expect("prepare revision-pinned Magit source")
        .with_prelude(magit_prelude(MAGIT_LOG_SCREEN));
    let ready = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("medium subject") && row.contains("A U Thor") && row.contains("2003-04-05")
        })
    };
    let mut pair = PackageTuiScenario::new("magit-log-margin", oracle.prepared_packages())
        .git_fixture(MAGIT_LOG_FIXTURE)
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "Magit log rows",
                PairTimeout::per_editor(Duration::from_secs(20), Duration::from_secs(30)),
            ),
            ready,
        )
        .expect("spawn ready package TUI pair");

    let gnu_snapshot = RawTerminalSnapshot::capture_full_screen(pair.gnu.screen());

    let expected_ansi_grid = expect_file!["snapshots/expected_ansi_grid.ansi"];
    expected_ansi_grid.assert_eq(&gnu_snapshot.ansi_grid());
    let expected_plain_grid = expect_file!["snapshots/expected_plain_grid.plain"];
    expected_plain_grid.assert_eq(&gnu_snapshot.plain_grid());

    pair.assert_display(DisplayCheckpoint::new(
        "Magit log full-screen terminal state",
    ));
    pair.assert_display(DisplayCheckpoint::raw_terminal(
        "Magit log full-screen terminal wire state",
    ));
}

/// The status screen lists the fixture's unstaged change, and the working-tree
/// diff screen shows it as a hunk.
///
/// Both screens read the same repository, so both are pinned: the status screen
/// by the commit hashes and the diff's stat line, the diff screen by the hunk
/// header and its added line.  The change itself is written by the fixture --
/// neither editor types into the buffer -- because the two peers must be showing
/// the same worktree state for their screens to be comparable at all.
#[test]
fn magit_status_and_working_tree_diff_match_gnu_full_screen() {
    let oracle = CachedMelpaOracle::new(MAGIT_MELPA_PIN, "magit.el")
        .expect("prepare revision-pinned Magit source")
        .with_prelude(magit_prelude(MAGIT_STATUS_SCREEN));
    // Magit inserts the status buffer section by section, running a git command
    // per section, so a row from the middle of the buffer says nothing about the
    // screen being finished: the unstaged change is inserted before the recent
    // commits below it.  The commits section is the last one this repository
    // produces anything for -- the two after it need a remote -- so waiting for
    // all three means the screen has stopped changing.
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Unstaged changes"))
            && grid
                .iter()
                .any(|row| row.contains("modified") && row.contains("tracked.txt"))
            && grid.iter().any(|row| row.contains("Recent commits"))
    };
    let mut pair = PackageTuiScenario::new("magit-status-diff", oracle.prepared_packages())
        .git_fixture(MAGIT_STATUS_FIXTURE)
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "Magit status rows",
                PairTimeout::per_editor(Duration::from_secs(20), Duration::from_secs(30)),
            ),
            ready,
        )
        .expect("spawn ready package TUI pair");

    let gnu_status = RawTerminalSnapshot::capture_full_screen(pair.gnu.screen());
    let expected_status_ansi = expect_file!["snapshots/expected_status_ansi_grid.ansi"];
    expected_status_ansi.assert_eq(&gnu_status.ansi_grid());
    let expected_status_plain = expect_file!["snapshots/expected_status_plain_grid.plain"];
    expected_status_plain.assert_eq(&gnu_status.plain_grid());

    pair.assert_display(DisplayCheckpoint::new(
        "Magit status full-screen terminal state",
    ));
    pair.assert_display(DisplayCheckpoint::raw_terminal(
        "Magit status full-screen terminal wire state",
    ));

    // The diff replaces the status buffer in the same window, so it is read
    // after the status screens are pinned.
    let hunk_header = "@@ -1,3 +1,4 @@";
    both(&mut pair, "working tree diff", |session| {
        invoke_with_prompt_timeout(
            session,
            "magit-diff-working-tree",
            hunk_header,
            Duration::from_secs(60),
        )
    })
    .expect("both editors show the working tree diff");

    let gnu_diff = RawTerminalSnapshot::capture_full_screen(pair.gnu.screen());
    let expected_diff_ansi = expect_file!["snapshots/expected_diff_ansi_grid.ansi"];
    expected_diff_ansi.assert_eq(&gnu_diff.ansi_grid());
    let expected_diff_plain = expect_file!["snapshots/expected_diff_plain_grid.plain"];
    expected_diff_plain.assert_eq(&gnu_diff.plain_grid());

    pair.assert_display(DisplayCheckpoint::new(
        "Magit working-tree diff full-screen terminal state",
    ));
    pair.assert_display(DisplayCheckpoint::raw_terminal(
        "Magit working-tree diff full-screen terminal wire state",
    ));
}
