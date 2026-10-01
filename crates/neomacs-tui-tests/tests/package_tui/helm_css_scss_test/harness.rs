use super::super::scenario::{PackageTuiScenario, PairTimeout, ReadinessCheckpoint};
use super::super::{CachedMelpaOracle, HELM_CSS_SCSS_MELPA_PIN};
use super::preludes::*;
use expect_test::expect;
use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};
use std::fs;
use std::time::Duration;

pub(super) fn wait_for<F>(session: &mut TuiSession, label: &str, predicate: F)
where
    F: Fn(&[String]) -> bool + Copy,
{
    session.read_until(Duration::from_secs(20), predicate);
    assert!(
        predicate(&session.text_grid()),
        "{label} did not reach the expected terminal state:\n{}",
        session.text_grid().join("\n")
    );
}

pub(super) fn send_default_probe(session: &mut TuiSession) {
    session.send_key("M-x");
    session.send(b"neomacs-hcss-default-run");
    session.send_key("RET");
}

pub(super) fn semantic_rows(session: &TuiSession) -> String {
    session
        .text_grid()
        .iter()
        .enumerate()
        .filter(|(_, row)| {
            row.contains("HCSS DEFAULT FAILURE") || row.contains("HCSS-DEFAULT-CLEAN")
        })
        .map(|(index, row)| format!("{index:02} |{}\n", row.trim_end()))
        .collect()
}

pub(super) fn invoke(session: &mut TuiSession, command: &str) {
    session.send_key("M-x");
    session.send(command.as_bytes());
    session.send_key("RET");
}

pub(super) fn helm_grid(session: &TuiSession) -> String {
    session
        .text_grid()
        .iter()
        .enumerate()
        .map(|(index, row)| format!("{index:02} |{}\n", row.trim_end()))
        .collect()
}

pub(super) fn exact_grid_rows_from(session: &TuiSession, first_row: usize) -> String {
    session
        .text_grid()
        .iter()
        .enumerate()
        .skip(first_row)
        .map(|(index, row)| format!("{index:02} |{}\n", row.trim_end()))
        .collect()
}

pub(super) fn exact_split_panes_from(session: &TuiSession, first_row: usize) -> String {
    session
        .text_grid()
        .iter()
        .enumerate()
        .skip(first_row)
        .map(|(index, row)| {
            let left = row.chars().take(79).collect::<String>();
            let right = row.chars().skip(80).collect::<String>();
            format!(
                "{index:02} L|{}\n{index:02} R|{}\n",
                left.trim_end(),
                right.trim_end()
            )
        })
        .collect()
}

pub(super) fn send_single_observer(session: &mut TuiSession) {
    session.send_key("C-c");
    session.send(b"t");
}

pub(super) fn helm_css_scss_unadapted_public_command_preserves_exact_helm_arity_failure() {
    let oracle = CachedMelpaOracle::new(HELM_CSS_SCSS_MELPA_PIN, "helm-css-scss.el")
        .expect("prepare exact revision-pinned helm-css-scss source")
        .with_prelude(HELM_CSS_SCSS_DEFAULT_TUI_PRELUDE);
    let mut pair =
        PackageTuiScenario::new("helm-css-scss-default-failure", oracle.prepared_packages())
            .spawn_when_ready(
                ReadinessCheckpoint::new(
                    "stylesheet fixture",
                    PairTimeout::same(Duration::from_secs(20)),
                ),
                |grid| grid.iter().any(|row| row.contains("padding: 1rem")),
            )
            .expect("spawn ready helm-css-scss GNU/Neomacs PTY pair");

    send_default_probe(&mut pair.gnu);
    send_default_probe(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU default failure cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-DEFAULT-CLEAN"))
    });
    wait_for(&mut pair.neo, "Neomacs default failure cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-DEFAULT-CLEAN"))
    });

    let gnu_report = fs::read_to_string(pair.gnu.home_dir().join("hcss-default-report.sexp"))
        .expect("read GNU default-failure report");
    let neo_report = fs::read_to_string(pair.neo.home_dir().join("hcss-default-report.sexp"))
        .expect("read Neomacs default-failure report");
    let expect = expect![[
        r#"(:post-public (:outcome (:error wrong-number-of-arguments :arity (1 . 1) :received 2) :buffer "tui-fixture.scss" :point 150 :line 11 :cache-count 4 :last-point (150 . "tui-fixture.scss") :last-query "" :fold-invisible neomacs-hcss-test-fold :recorded-invisible nil :package-overlay-buffer nil :session-advices (nil nil) :session-hook nil :helm-alive nil :helm-buffers ("*Helm Css SCSS*") :modified nil) :cleanup (:source-live nil :fold-buffer nil :package-overlay-buffer nil :session-advices (nil nil) :session-hook nil :helm-alive nil :helm-buffers nil :root-exists nil :cleanup-error nil))"#
    ]];
    expect.assert_eq(&gnu_report);
    assert_eq!(
        neo_report, gnu_report,
        "helm-css-scss default public failure diverged from GNU"
    );

    let gnu_rows = semantic_rows(&pair.gnu);
    let neo_rows = semantic_rows(&pair.neo);
    let rows_expect = expect![[r#"
        01 |HCSS DEFAULT FAILURE
        08 |HCSS-DEFAULT-CLEAN
    "#]];
    rows_expect.assert_eq(&gnu_rows);
    assert_eq!(neo_rows, gnu_rows, "default failure semantic rows differ");

    for (index, row) in pair.gnu.text_grid().iter().enumerate() {
        if row.contains("HCSS DEFAULT FAILURE") || row.contains("HCSS-DEFAULT-CLEAN") {
            let gnu = RawTerminalSnapshot::capture_rows(
                pair.gnu.screen(),
                index as u16..index as u16 + 1,
            );
            let neo = RawTerminalSnapshot::capture_rows(
                pair.neo.screen(),
                index as u16..index as u16 + 1,
            );
            assert_eq!(neo, gnu, "default failure raw row {index} differs");
        }
    }
}
