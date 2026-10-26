use super::super::scenario::{
    PackageTuiPair, PackageTuiScenario, PairTimeout, ReadinessCheckpoint,
};
use super::super::{CachedMelpaOracle, HELM_GITIGNORE_MELPA_PIN};
use super::prelude::*;
use expect_test::expect;
use neomacs_infra::packages::elisp_string;
use neomacs_tui_tests::{Snapshot, TuiSession};
use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

pub(super) fn send_to_both<F>(pair: &mut PackageTuiPair, operation: F)
where
    F: Fn(&mut TuiSession),
{
    operation(&mut pair.gnu);
    operation(&mut pair.neo);
}

pub(super) fn wait_for_editor<F>(session: &mut TuiSession, timeout: Duration, predicate: F) -> bool
where
    F: Fn(&[String]) -> bool,
{
    session.read_until(timeout, |grid| predicate(grid));
    predicate(&session.text_grid())
}

pub(super) fn wait_for_progress<F>(
    pair: &mut PackageTuiPair,
    stage: &str,
    timeout: Duration,
    predicate: F,
    divergences: &mut Vec<String>,
) -> bool
where
    F: Fn(&[String]) -> bool + Copy,
{
    assert!(
        wait_for_editor(&mut pair.gnu, timeout, predicate),
        "GNU {stage} screen did not reach expected state:\n{}",
        pair.gnu.text_grid().join("\n")
    );
    let neo_reached = wait_for_editor(&mut pair.neo, timeout, predicate);
    if !neo_reached {
        divergences.push(format!(
            "Neomacs {stage} screen did not reach the GNU state:\n{}",
            pair.neo.text_grid().join("\n")
        ));
    }
    neo_reached
}

/// Quit the failed helm session the way the state says, not the clock.
///
/// `C-g C-g' sent blind reaches GNU's emergency-escape path when the two
/// keys straddle an unconsumed quit: the first C-g sets `quit-flag' inside
/// code still running with `inhibit-quit', and the second (50ms later)
/// finds it non-nil and suspends the editor — "Emacs is resuming after an
/// emergency escape. Auto-save? (y or n)", which struck this test twice.
/// The timing-safe fix is to observe each key's effect before sending the
/// next: settle the sentinel-error state, send ONE C-g, and wait for the
/// helm session to close (its buffer leaves the grid) before sending the
/// second.  Both editors then receive the same keys, each gated on that
/// editor's own state.
pub(super) fn quit_failure_session(pair: &mut PackageTuiPair) {
    settle_pair(pair);
    send_to_both(pair, |session| session.send_key("C-g"));
    let session_closed = |grid: &[String]| !grid.iter().any(|row| row.contains("*helm-gitignore*"));
    for (name, session) in [("GNU", &mut pair.gnu), ("Neomacs", &mut pair.neo)] {
        let deadline = Instant::now() + STAGE_TIMEOUT;
        loop {
            session.read(Duration::from_millis(20));
            if session_closed(&session.text_grid()) {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "{name} did not close the helm session after C-g:\n{}",
                session.text_grid().join("\n")
            );
        }
    }
    settle_pair(pair);
    send_to_both(pair, |session| session.send_key("C-g"));
}

pub(super) fn settle_pair(pair: &mut PackageTuiPair) {
    let _ = neomacs_tui_tests::pair::settle_session(&mut pair.gnu);
    let _ = neomacs_tui_tests::pair::settle_session(&mut pair.neo);
}

pub(super) fn open_helm_gitignore(
    pair: &mut PackageTuiPair,
    divergences: &mut Vec<String>,
) -> bool {
    send_to_both(pair, |session| {
        session.send_key("M-x");
        session.send(b"helm-gitignore");
        session.send_key("RET");
    });
    wait_for_progress(
        pair,
        "Helm startup",
        STAGE_TIMEOUT,
        |grid| {
            grid.iter().any(|row| row.contains("*helm-gitignore*"))
                && grid.iter().any(|row| row.contains("pattern:"))
        },
        divergences,
    )
}

pub(super) fn type_query_and_wait(
    pair: &mut PackageTuiPair,
    query: &str,
    candidate: &str,
    divergences: &mut Vec<String>,
) -> bool {
    send_to_both(pair, |session| session.send(query.as_bytes()));
    wait_for_progress(
        pair,
        &format!("query {query:?}"),
        STAGE_TIMEOUT,
        |grid| {
            grid.iter()
                .any(|row| row.contains("pattern:") && row.contains(query))
                && grid.iter().any(|row| row.trim().contains(candidate))
        },
        divergences,
    )
}

pub(super) fn helm_semantic_snapshot(
    session: &TuiSession,
    pattern: &str,
    candidates: &[&str],
) -> String {
    let grid = session.text_grid();
    let pattern_line = grid
        .iter()
        .find(|row| row.trim_start().starts_with("pattern:"))
        .map(|row| row.trim())
        .expect("Helm terminal snapshot has a pattern line");
    assert_eq!(pattern_line, format!("pattern: {pattern}"));
    let source = if grid.iter().any(|row| row.trim() == "gitignore.io") {
        r#""gitignore.io""#
    } else {
        "nil"
    };
    let status = grid
        .iter()
        .find(|row| row.contains("*helm-gitignore*"))
        .expect("Helm terminal snapshot has a status line");
    let selected = status
        .split_whitespace()
        .find_map(|word| word.strip_prefix('L')?.parse::<usize>().ok())
        .expect("Helm terminal snapshot reports the selected candidate index");
    let marked = status
        .split_whitespace()
        .find_map(|word| word.strip_prefix('M')?.parse::<usize>().ok())
        .unwrap_or(0);
    let candidate_count = status
        .split_whitespace()
        .find_map(|word| word.strip_prefix('[')?.parse::<usize>().ok())
        .unwrap_or(0);
    let visible = grid
        .iter()
        .filter_map(|row| {
            let trimmed = row.trim();
            let (is_marked, label) = match trimmed.strip_prefix('*') {
                Some(label) => (true, label),
                None => (false, trimmed),
            };
            candidates
                .contains(&label)
                .then(|| format!("({} {label:?})", if is_marked { "t" } else { "nil" }))
        })
        .collect::<Vec<_>>();
    assert_eq!(
        visible.len(),
        candidates.len(),
        "Helm terminal snapshot has every expected candidate exactly once"
    );
    assert_eq!(
        candidate_count,
        candidates.len(),
        "Helm terminal snapshot rejects unexpected extra candidates"
    );
    format!(
        "(:pattern {pattern:?} :source {source} :candidates ({}) :candidate-count {candidate_count} :selected-index {selected} :marked-count {marked})",
        visible.join(" ")
    )
}

pub(super) fn assert_helm_semantic_snapshot(
    pair: &PackageTuiPair,
    stage: &str,
    pattern: &str,
    candidates: &[&str],
    expected: impl Snapshot,
    divergences: &mut Vec<String>,
) {
    let gnu = helm_semantic_snapshot(&pair.gnu, pattern, candidates);
    expected.assert_snapshot(&gnu);
    let neo = helm_semantic_snapshot(&pair.neo, pattern, candidates);
    if neo != gnu {
        divergences.push(format!(
            "{stage} terminal snapshot differs:\nGNU:\n{gnu}\nNeomacs:\n{neo}"
        ));
    }
}

fn terminal_failure_text(grid: Vec<String>) -> String {
    grid.into_iter()
        .filter_map(|row| {
            let row = row.trim();
            (row.contains("Keyword argument")
                || row.contains("Wrong number of arguments")
                || row.contains("wrong-number-of-arguments"))
            .then(|| row.to_string())
        })
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn terminal_failure_snapshot(session: &TuiSession) -> String {
    terminal_failure_text(session.text_grid())
}

#[derive(Debug)]
enum TerminalFailureObservation {
    NotObserved,
    Observed(String),
}

/// Owns only this wait's exact observations. The exclusive pair borrow blocks
/// later input until the consuming assertion ends this observation attempt.
#[must_use = "consume the observed terminal failures before driving the pair"]
pub(super) struct PairedTerminalFailures<'pair> {
    pair: &'pair mut PackageTuiPair,
    gnu: TerminalFailureObservation,
    neo: TerminalFailureObservation,
}

impl std::fmt::Debug for PairedTerminalFailures<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PairedTerminalFailures")
            .field("gnu", &self.gnu)
            .field("neo", &self.neo)
            .finish_non_exhaustive()
    }
}

pub(super) fn observe_terminal_failures(
    pair: &mut PackageTuiPair,
    timeout: Duration,
) -> PairedTerminalFailures<'_> {
    let deadline = Instant::now() + timeout;
    let mut gnu = TerminalFailureObservation::NotObserved;
    let mut neo = TerminalFailureObservation::NotObserved;
    while Instant::now() < deadline {
        // Neither editor waits behind the other's full deadline: the error
        // can disappear while the other process is still reaching its error.
        for (session, observation) in [(&mut pair.gnu, &mut gnu), (&mut pair.neo, &mut neo)] {
            match observation {
                TerminalFailureObservation::Observed(_) => {}
                TerminalFailureObservation::NotObserved => {
                    let observed = session
                        .observe_at_fence()
                        .expect("Helm Gitignore terminal-failure fence");
                    let text = terminal_failure_text(observed.text_grid());
                    if text.is_empty() {
                        session.read(
                            deadline
                                .saturating_duration_since(Instant::now())
                                .min(Duration::from_millis(50)),
                        );
                    } else {
                        *observation = TerminalFailureObservation::Observed(text);
                    }
                }
            }
        }
        match (&gnu, &neo) {
            (TerminalFailureObservation::Observed(_), TerminalFailureObservation::Observed(_)) => {
                break;
            }
            (TerminalFailureObservation::NotObserved, TerminalFailureObservation::NotObserved)
            | (TerminalFailureObservation::NotObserved, TerminalFailureObservation::Observed(_))
            | (TerminalFailureObservation::Observed(_), TerminalFailureObservation::NotObserved) => {
            }
        }
    }
    PairedTerminalFailures { pair, gnu, neo }
}

impl PairedTerminalFailures<'_> {
    pub(super) fn assert_parity(
        self,
        stage: &str,
        expected: impl Snapshot,
        divergences: &mut Vec<String>,
    ) {
        let gnu = match self.gnu {
            TerminalFailureObservation::Observed(text) => text,
            TerminalFailureObservation::NotObserved => panic!(
                "GNU {stage} screen did not reach expected state:\n{}",
                self.pair.gnu.text_grid().join("\n")
            ),
        };
        let neo = match self.neo {
            TerminalFailureObservation::Observed(text) => text,
            TerminalFailureObservation::NotObserved => {
                divergences.push(format!(
                    "Neomacs {stage} screen did not reach the GNU state:\n{}",
                    self.pair.neo.text_grid().join("\n")
                ));
                String::new()
            }
        };
        assert_terminal_failure_text_parity(stage, &gnu, &neo, expected, divergences);
    }
}

fn assert_terminal_failure_text_parity(
    stage: &str,
    gnu: &str,
    neo: &str,
    expected: impl Snapshot,
    divergences: &mut Vec<String>,
) {
    expected.assert_snapshot(gnu);
    if neo != gnu {
        divergences.push(format!(
            "{stage} terminal failure differs:\nGNU:\n{gnu}\nNeomacs:\n{neo}"
        ));
    }
}

pub(super) fn assert_terminal_failure_parity(
    pair: &PackageTuiPair,
    stage: &str,
    expected: impl Snapshot,
    divergences: &mut Vec<String>,
) {
    let gnu = terminal_failure_snapshot(&pair.gnu);
    let neo = terminal_failure_snapshot(&pair.neo);
    assert_terminal_failure_text_parity(stage, &gnu, &neo, expected, divergences);
}

pub(super) fn navigate_to_visual_studio_code(
    pair: &mut PackageTuiPair,
    stage: &str,
    divergences: &mut Vec<String>,
) -> bool {
    open_helm_gitignore(pair, divergences);
    type_query_and_wait(pair, "visual", "VisualStudioCode", divergences);
    assert_helm_semantic_snapshot(
        pair,
        &format!("{stage} candidates"),
        "visual",
        &[
            "VisualBasic",
            "VisualStudio",
            "KonyVisualizer",
            "VisualStudioCode",
            "OpenFrameworks+VisualStudio",
        ],
        expect![[
            r#"(:pattern "visual" :source "gitignore.io" :candidates ((nil "VisualBasic") (nil "VisualStudio") (nil "KonyVisualizer") (nil "VisualStudioCode") (nil "OpenFrameworks+VisualStudio")) :candidate-count 5 :selected-index 1 :marked-count 0)"#
        ]],
        divergences,
    );
    send_to_both(pair, |session| session.send_keys("C-n C-n C-n"));
    let selection_reached = wait_for_progress(
        pair,
        &format!("{stage} VisualStudioCode selection"),
        STAGE_TIMEOUT,
        |grid| {
            grid.iter().any(|row| {
                row.contains("*helm-gitignore*")
                    && row.contains(" L4 ")
                    && row.contains("[5 Candidate(s)]")
            })
        },
        divergences,
    );
    if selection_reached {
        assert_helm_semantic_snapshot(
            pair,
            &format!("{stage} selected candidate"),
            "visual",
            &[
                "VisualBasic",
                "VisualStudio",
                "KonyVisualizer",
                "VisualStudioCode",
                "OpenFrameworks+VisualStudio",
            ],
            expect![[
                r#"(:pattern "visual" :source "gitignore.io" :candidates ((nil "VisualBasic") (nil "VisualStudio") (nil "KonyVisualizer") (nil "VisualStudioCode") (nil "OpenFrameworks+VisualStudio")) :candidate-count 5 :selected-index 4 :marked-count 0)"#
            ]],
            divergences,
        );
    }
    selection_reached
}

const PROJECT_RELEASE_TAIL: &str = "\n# Project-local release artifacts\nrelease-output/\n";
const WRITE_FILE_PROMPT: &str = "Write file: ";

#[derive(Clone, Copy, Debug)]
enum SaveStage<'filename> {
    EditedTail,
    WriteFilePrompt,
    EmptyFilename,
    Filename(&'filename str),
}

fn wait_for_save_stage(session: &mut TuiSession, deadline: Instant, stage: SaveStage<'_>) {
    loop {
        let observed = session
            .observe_at_fence()
            .expect("Helm Gitignore edit/save observation fence");
        let grid = observed.text_grid();
        let reached = match stage {
            SaveStage::EditedTail => PROJECT_RELEASE_TAIL
                .lines()
                .filter(|line| !line.is_empty())
                // The edited window occupies only the left pane of a row.
                .all(|line| grid.iter().any(|row| row.contains(line))),
            SaveStage::WriteFilePrompt => grid
                .last()
                .is_some_and(|row| row.starts_with(WRITE_FILE_PROMPT)),
            SaveStage::EmptyFilename => grid
                .last()
                .is_some_and(|row| row.trim_end() == WRITE_FILE_PROMPT.trim_end()),
            SaveStage::Filename(expected) => {
                grid.last().is_some_and(|row| row.trim_end() == expected)
            }
        };
        if reached {
            return;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        assert!(
            !remaining.is_zero(),
            "{} did not reach Helm Gitignore save stage {stage:?}:\n{}",
            session.name,
            grid.join("\n")
        );
        session.read(remaining.min(Duration::from_millis(50)));
    }
}

pub(super) fn edit_and_write_gitignore(session: &mut TuiSession, path: &Path) {
    let filename = path
        .to_str()
        .expect("owned Gitignore fixture path is UTF-8");
    let expected = format!("{WRITE_FILE_PROMPT}{filename}");
    let (_, columns) = session.screen_size();
    assert!(
        expected.is_ascii() && expected.len() < usize::from(columns),
        "owned Gitignore filename must fit completely on its prompt row"
    );
    // One observation budget covers this editor's edit and filename stages.
    // Transport operations retain their separate existing request deadlines.
    let deadline = Instant::now() + Duration::from_secs(8);
    session.send_keys("M->");
    session.send(PROJECT_RELEASE_TAIL.as_bytes());
    wait_for_save_stage(session, deadline, SaveStage::EditedTail);
    session.send_keys("C-x C-w");
    wait_for_save_stage(session, deadline, SaveStage::WriteFilePrompt);
    // Clear the default path so its shadow cannot obscure the requested name.
    session.send_keys("C-a C-k");
    wait_for_save_stage(session, deadline, SaveStage::EmptyFilename);
    session.send(filename.as_bytes());
    wait_for_save_stage(session, deadline, SaveStage::Filename(&expected));
    session.send_key("RET");
}

pub(super) fn eval_both(pair: &mut PackageTuiPair, form: &str) {
    send_to_both(pair, |session| {
        session.send_key("M-:");
        session.send(form.as_bytes());
        session.send_key("RET");
    });
}

pub(super) fn capture_editor(session: &mut TuiSession, stage: &str, editor: &str) -> String {
    let path = session.home_dir().join(format!("{stage}.state"));
    let pending_stage = format!("{stage}.pending");
    let pending_path = session.home_dir().join(format!("{pending_stage}.state"));
    for capture_path in [&path, &pending_path] {
        assert!(
            !capture_path
                .try_exists()
                .expect("inspect owned capture path"),
            "{editor} capture stage {stage:?} must use fresh paths: {capture_path:?}"
        );
    }
    // with-temp-file exposes its empty/truncated destination before writing
    // finishes. Publish the unchanged capture only after that helper returns
    // and closes the file; both paths share the editor's owned HOME directory.
    let pending = elisp_string(&pending_stage);
    let pending_file = elisp_string(&format!("{pending_stage}.state"));
    let final_file = elisp_string(&format!("{stage}.state"));
    session.send_key("M-:");
    session.send(
        format!(
            r#"(progn (neomacs-helm-gitignore-tui-await-http-idle) (prog1 (neomacs-helm-gitignore-tui-capture {pending}) (rename-file (expand-file-name {pending_file} (getenv "HOME")) (expand-file-name {final_file} (getenv "HOME")) nil)))"#
        )
        .as_bytes(),
    );
    session.send_key("RET");
    let deadline = Instant::now() + STAGE_TIMEOUT;
    loop {
        session.read(Duration::from_millis(10));
        if let Ok(state) = fs::read_to_string(&path) {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "{editor} did not capture helm-gitignore stage {stage:?}"
        );
        thread::yield_now();
    }
}

pub(super) fn wait_for_state_file(
    session: &mut TuiSession,
    name: &str,
    expected_fragment: &str,
    editor: &str,
) -> String {
    let path = session.home_dir().join(name);
    let deadline = Instant::now() + STAGE_TIMEOUT;
    loop {
        session.read(Duration::from_millis(10));
        if let Ok(state) = fs::read_to_string(&path)
            && state.contains(expected_fragment)
        {
            return state;
        }
        assert!(
            Instant::now() < deadline,
            "{editor} did not write {name:?} containing {expected_fragment:?}"
        );
        thread::yield_now();
    }
}

pub(super) fn capture_reached_stage<F>(
    pair: &mut PackageTuiPair,
    stage: &str,
    timeout: Duration,
    predicate: F,
    divergences: &mut Vec<String>,
) -> (String, Option<String>)
where
    F: Fn(&[String]) -> bool + Copy,
{
    assert!(
        wait_for_editor(&mut pair.gnu, timeout, predicate),
        "GNU {stage} screen did not reach expected state:\n{}",
        pair.gnu.text_grid().join("\n")
    );
    let gnu = capture_editor(&mut pair.gnu, stage, "GNU");

    if wait_for_editor(&mut pair.neo, timeout, predicate) {
        let neo = capture_editor(&mut pair.neo, stage, "Neomacs");
        (gnu, Some(neo))
    } else {
        divergences.push(format!(
            "Neomacs {stage} screen did not reach the GNU state:\n{}",
            pair.neo.text_grid().join("\n")
        ));
        (gnu, None)
    }
}

pub(super) fn assert_gnu_literal_and_parity(
    stage: &str,
    gnu: &str,
    neo: Option<&str>,
    expected: impl Snapshot,
    divergences: &mut Vec<String>,
) {
    expected.assert_snapshot(gnu);
    if let Some(neo) = neo.filter(|neo| *neo != gnu) {
        divergences.push(format!("{stage} differs:\nGNU:\n{gnu}\nNeomacs:\n{neo}"));
    }
}

pub(super) fn ready_helm_gitignore_pair(label: &str) -> PackageTuiPair {
    let oracle = CachedMelpaOracle::new(HELM_GITIGNORE_MELPA_PIN, "helm-gitignore.el")
        .expect("prepare exact revision-pinned helm-gitignore source")
        .with_prelude(HELM_GITIGNORE_TUI_PRELUDE);
    PackageTuiScenario::new(label, oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "release engineering fixture",
                PairTimeout::same(STAGE_TIMEOUT),
            ),
            |grid| {
                grid.iter()
                    .any(|row| row.contains("Release engineering scratchpad"))
            },
        )
        .expect("spawn ready helm-gitignore GNU/Neomacs TUI pair")
}

pub(super) fn assert_no_divergences(divergences: &[String]) {
    assert!(
        divergences.is_empty(),
        "helm-gitignore GNU/Neomacs divergences:\n{}",
        divergences.join("\n\n")
    );
}
