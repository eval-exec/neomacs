use super::super::scenario::{
    PackageTuiPair, PackageTuiScenario, PairTimeout, ReadinessCheckpoint, TerminalProfile,
};
use super::super::{
    COMPAT_GNU_ELPA_PIN, CachedMelpaOracle, GRUVBOX_THEME_MELPA_PIN, ORDERLESS_MELPA_PIN,
    PreparedPackageSet,
};
use super::prelude::GRUVBOX_TUI_PRELUDE;
use expect_test::{Expect, ExpectFile, expect};
// Re-exported so the sibling modules reach the shared vocabulary the same way
// they reach this module's own helpers.
pub(super) use neomacs_tui_tests::package_harness::{
    catch_phase, invoke_with_prompt_timeout, wait_for,
};
use neomacs_tui_tests::{RawTerminalSnapshot, Snapshot, TuiSession};
use std::time::Duration;

pub(super) const REPORT_PREFIXES: &[&str] = &[
    "CAP ",
    "THEMES-KNOWN ",
    "GRUVBOX-TUI-BOOT",
    "GRUVBOX-THEME-PAGE ",
    "GRUVBOX-THEME-PAGE-DONE ",
    "THEME ",
    "ENABLED ",
    "MODE ",
    "FACE ",
    "VAR ",
    "GRUVBOX-THEME-READY",
    "PROPERTIES ",
    "PROPERTY-COUNT ",
    "RUN ",
    "GRUVBOX-PROPERTIES-",
    "BOLD ",
    "GRUVBOX-BOLD-READY",
    "ORDERLESS ",
    "GRUVBOX-ORDERLESS-READY",
    "CONSUMER ",
    "GRUVBOX-CONSUMER-READY",
    "CORE-ORG ",
    "GRUVBOX-CORE-ORG-READY",
    "DEFAULT-ORG ",
    "GRUVBOX-DEFAULT-ORG-READY",
];

pub(super) fn oracle() -> CachedMelpaOracle {
    CachedMelpaOracle::new(GRUVBOX_THEME_MELPA_PIN, "gruvbox.el")
        .expect("prepare exact Gruvbox Theme source below ./tmp")
        .with_installed_autoloads()
        .with_melpa_dependency(ORDERLESS_MELPA_PIN)
        .expect("prepare exact Orderless optional-integration source below ./tmp")
        .with_gnu_elpa_dependency(COMPAT_GNU_ELPA_PIN)
        .expect("prepare exact Compat closure for Orderless below ./tmp")
        .with_prelude(GRUVBOX_TUI_PRELUDE)
}

/// Gruvbox allows every wait the same twenty seconds, the M-x prompt included.
pub(super) const WAIT_TIMEOUT: Duration = Duration::from_secs(20);

pub(super) fn invoke_both(pair: &mut PackageTuiPair, command: &str, ready: &str) {
    let gnu = catch_phase(&format!("GNU {command}"), || {
        invoke_with_prompt_timeout(&mut pair.gnu, command, ready, WAIT_TIMEOUT)
    });
    let neo = catch_phase(&format!("Neo {command}"), || {
        invoke_with_prompt_timeout(&mut pair.neo, command, ready, WAIT_TIMEOUT)
    });
    let errors = [gnu.err(), neo.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "dual-peer command failed:\n{}",
        errors.join("\n")
    );
}

pub(super) fn wait_for_boot_both(pair: &mut PackageTuiPair) {
    // Queue the public command into each real command loop.  GNU may display
    // an informational startup warning after the startup file finishes; the
    // explicit command makes the final visible state the owned report.
    invoke_both(pair, "gt357-show-boot", "GRUVBOX-TUI-BOOT");
}

pub(super) fn report(session: &TuiSession) -> String {
    session
        .text_grid()
        .into_iter()
        .map(|row| row.trim_end().to_owned())
        .filter(|row| REPORT_PREFIXES.iter().any(|prefix| row.starts_with(prefix)))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(super) fn record_pair(
    pair: &PackageTuiPair,
    label: &str,
    expected: impl Snapshot,
    mismatches: &mut Vec<String>,
) -> String {
    let gnu = report(&pair.gnu);
    let neo = report(&pair.neo);
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU:\n{gnu}\nNeo:\n{neo}"));
    }
    expected.assert_snapshot(&gnu);
    gnu
}

pub(super) fn ansi_rows(session: &TuiSession, needles: &[&str]) -> String {
    let grid = session.text_grid();
    needles
        .iter()
        .map(|needle| {
            let row = grid
                .iter()
                .position(|contents| contents.contains(needle))
                .unwrap_or_else(|| {
                    panic!(
                        "{} never rendered {needle:?}:\n{}",
                        session.name,
                        grid.join("\n")
                    )
                }) as u16;
            let mut snapshot = RawTerminalSnapshot::capture_rows(session.screen(), row..row + 1);
            let meaningful_end = snapshot.rows[0]
                .cells
                .iter()
                .rposition(|cell| {
                    cell.contents()
                        .chars()
                        .any(|character| !character.is_whitespace())
                })
                .unwrap_or(0)
                + 1;
            snapshot.rows[0].cells.truncate(meaningful_end);
            snapshot.ansi_grid()
        })
        .collect::<Vec<_>>()
        .join("")
}

pub(super) fn record_grid(
    pair: &PackageTuiPair,
    label: &str,
    needles: &[&str],
    expected: impl Snapshot,
    mismatches: &mut Vec<String>,
) -> String {
    let gnu = catch_phase(&format!("GNU {label} grid"), || {
        ansi_rows(&pair.gnu, needles)
    });
    let neo = catch_phase(&format!("Neo {label} grid"), || {
        ansi_rows(&pair.neo, needles)
    });
    let errors = [gnu.as_ref().err(), neo.as_ref().err()]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "dual-peer grid capture failed:\n{}",
        errors.join("\n")
    );
    let gnu = gnu.expect("checked GNU grid result");
    let neo = neo.expect("checked Neo grid result");
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU: {gnu:?}\nNeo: {neo:?}"));
    }
    expected.assert_snapshot(&gnu);
    gnu
}

pub(super) fn record_properties(
    pair: &mut PackageTuiPair,
    label: &str,
    expected: impl Snapshot,
    mismatches: &mut Vec<String>,
) -> String {
    let mut gnu = Vec::new();
    let mut neo = Vec::new();
    for (command, tag) in [
        ("gt357-show-elisp-properties", "E"),
        ("gt357-show-org-properties", "O"),
        ("gt357-show-diff-properties", "D"),
    ] {
        invoke_both(
            pair,
            command,
            &format!("GRUVBOX-PROPERTIES-{tag}-PAGE-DONE 1/"),
        );
        let mut gnu_pages = vec![report(&pair.gnu)];
        let mut neo_pages = vec![report(&pair.neo)];
        let page_total = |editor: &str, report: &str| {
            let line = report
                .lines()
                .find(|line| line.contains(&format!(" {tag} PAGE 1/")))
                .unwrap_or_else(|| {
                    panic!(
                        "{editor} property report omitted the exact {tag} page header:\n{report}"
                    )
                });
            line.rsplit_once('/')
                .and_then(|(_, total)| total.parse::<usize>().ok())
                .filter(|total| *total > 0)
                .unwrap_or_else(|| {
                    panic!("{editor} property report has invalid {tag} page total:\n{report}")
                })
        };
        let gnu_total = page_total("GNU", &gnu_pages[0]);
        let neo_total = page_total("Neo", &neo_pages[0]);
        assert_eq!(
            neo_total, gnu_total,
            "{label} {tag} property page count differs before snapshots"
        );
        for page in 2..=gnu_total {
            let ready = if page == gnu_total {
                format!("GRUVBOX-PROPERTIES-{tag}-READY")
            } else {
                format!("GRUVBOX-PROPERTIES-{tag}-PAGE-DONE {page}/{gnu_total}")
            };
            invoke_both(pair, "gt357-next-property-page", &ready);
            let gnu_page = report(&pair.gnu);
            let neo_page = report(&pair.neo);
            for (editor, page_report) in [("GNU", &gnu_page), ("Neo", &neo_page)] {
                assert!(
                    page_report
                        .lines()
                        .any(|line| line.contains(&format!(" {tag} PAGE {page}/{gnu_total}"))),
                    "{editor} property report omitted exact {tag} page {page}/{gnu_total}:\n{page_report}"
                );
            }
            gnu_pages.push(gnu_page);
            neo_pages.push(neo_page);
        }
        let gnu_report = gnu_pages.join("\n--\n");
        let neo_report = neo_pages.join("\n--\n");
        for (editor, final_page) in [
            ("GNU", gnu_pages.last().expect("GNU property page")),
            ("Neo", neo_pages.last().expect("Neo property page")),
        ] {
            assert!(
                final_page
                    .lines()
                    .any(|line| line == format!("GRUVBOX-PROPERTIES-{tag}-READY")),
                "{editor} property report omitted the trailing {tag} ready marker:\n{final_page}"
            );
        }
        assert!(
            gnu_report.contains(&format!("PROPERTY-COUNT {tag} ")),
            "GNU property report omitted the exact {tag} run count:\n{gnu_report}"
        );
        assert!(
            neo_report.contains(&format!("PROPERTY-COUNT {tag} ")),
            "Neo property report omitted the exact {tag} run count:\n{neo_report}"
        );
        gnu.push(gnu_report);
        neo.push(neo_report);
    }
    let gnu = gnu.join("\n--\n");
    let neo = neo.join("\n--\n");
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU:\n{gnu}\nNeo:\n{neo}"));
    }
    expected.assert_snapshot(&gnu);
    gnu
}

pub(super) fn drive_orderless_completion(session: &mut TuiSession) -> (String, String) {
    session.send_keys("M-x");
    wait_for(
        session,
        WAIT_TIMEOUT,
        "M-x before Orderless completion",
        |grid| grid.iter().any(|row| row.contains("M-x")),
    );
    session.send(b"gt357-orderless-select");
    session.send_keys("RET");
    wait_for(
        session,
        WAIT_TIMEOUT,
        "real Orderless minibuffer prompt",
        |grid| grid.iter().any(|row| row.contains("Gruvbox Orderless:")),
    );
    session.send(b"alp b gam del");
    session.send_keys("TAB");
    wait_for(
        session,
        WAIT_TIMEOUT,
        "Orderless four-component completion row",
        |grid| {
            grid.iter()
                .any(|row| row.contains("alpha beta gamma delta"))
        },
    );
    let grid = ansi_rows(session, &["alpha beta gamma delta"]);
    session.send_keys("C-a");
    session.send_keys("C-k");
    session.send(b"alpha beta gamma delta");
    session.send_keys("RET");
    wait_for(
        session,
        WAIT_TIMEOUT,
        "completed Orderless selection",
        |grid| {
            grid.iter()
                .any(|row| row.contains("GRUVBOX-ORDERLESS-READY"))
        },
    );
    (grid, report(session))
}

pub(super) fn record_orderless_completion(
    pair: &mut PackageTuiPair,
    grid_expected: impl Snapshot,
    report_expected: impl Snapshot,
    mismatches: &mut Vec<String>,
) {
    let gnu = catch_phase("GNU real Orderless completion", || {
        drive_orderless_completion(&mut pair.gnu)
    });
    let neo = catch_phase("Neo real Orderless completion", || {
        drive_orderless_completion(&mut pair.neo)
    });
    let errors = [gnu.as_ref().err(), neo.as_ref().err()]
        .into_iter()
        .flatten()
        .cloned()
        .collect::<Vec<_>>();
    assert!(
        errors.is_empty(),
        "dual-peer Orderless completion failed:\n{}",
        errors.join("\n")
    );
    let (gnu_grid, gnu_report) = gnu.expect("checked GNU Orderless result");
    let (neo_grid, neo_report) = neo.expect("checked Neo Orderless result");
    if neo_grid != gnu_grid {
        mismatches.push(format!(
            "Orderless rendered completion differs\nGNU: {gnu_grid:?}\nNeo: {neo_grid:?}"
        ));
    }
    if neo_report != gnu_report {
        mismatches.push(format!(
            "Orderless completion report differs\nGNU:\n{gnu_report}\nNeo:\n{neo_report}"
        ));
    }
    grid_expected.assert_snapshot(&gnu_grid);
    report_expected.assert_snapshot(&gnu_report);
}

pub(super) fn initialize_consumer(
    pair: &mut PackageTuiPair,
    profile: &str,
    expected: impl Snapshot,
    mismatches: &mut Vec<String>,
) {
    invoke_both(pair, "gt357-use-dark-medium", "; comment Ω");
    invoke_both(pair, "gt357-show-consumer-state", "GRUVBOX-CONSUMER-READY");
    record_pair(
        pair,
        &format!("{profile} first lazy compiled consumer"),
        expected,
        mismatches,
    );
}

pub(super) fn spawn_profile(
    label: &str,
    packages: &PreparedPackageSet,
    terminal_profile: TerminalProfile,
) -> Result<PackageTuiPair, String> {
    PackageTuiScenario::new(label, packages)
        .terminal_profile(terminal_profile)
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "initial scratch buffer",
                PairTimeout::same(Duration::from_secs(20)),
            ),
            |grid| grid.iter().any(|row| row.contains("*scratch*")),
        )
}

pub(super) fn capture_matrix_pair(pair: &mut PackageTuiPair) -> (String, String) {
    let mut gnu = Vec::new();
    let mut neo = Vec::new();
    for _ in 0..7 {
        invoke_both(pair, "gt357-next-theme", "GRUVBOX-THEME-PAGE-DONE 1/3");
        gnu.push(report(&pair.gnu));
        neo.push(report(&pair.neo));
        invoke_both(pair, "gt357-next-state-page", "GRUVBOX-THEME-PAGE-DONE 2/3");
        gnu.push(report(&pair.gnu));
        neo.push(report(&pair.neo));
        invoke_both(pair, "gt357-next-state-page", "GRUVBOX-THEME-READY");
        let gnu_final = report(&pair.gnu);
        let neo_final = report(&pair.neo);
        assert!(
            gnu_final
                .lines()
                .any(|line| line == "GRUVBOX-THEME-PAGE 3/3"),
            "GNU final matrix page lacks the exact 3/3 header:\n{gnu_final}"
        );
        assert!(
            neo_final
                .lines()
                .any(|line| line == "GRUVBOX-THEME-PAGE 3/3"),
            "Neo final matrix page lacks the exact 3/3 header:\n{neo_final}"
        );
        gnu.push(gnu_final);
        neo.push(neo_final);
    }
    (gnu.join("\n--\n"), neo.join("\n--\n"))
}

pub(super) fn assert_matrix(
    pair: &mut PackageTuiPair,
    label: &str,
    expected: impl Snapshot,
    mismatches: &mut Vec<String>,
) {
    let (gnu, neo) = capture_matrix_pair(pair);
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU:\n{gnu}\nNeo:\n{neo}"));
    }
    expected.assert_snapshot(&gnu);
}

pub(super) fn finish(pair: &mut PackageTuiPair, mismatches: &mut Vec<String>) {
    invoke_both(pair, "gt357-finish", "GRUVBOX-TUI-CLEAN");
    let gnu = pair
        .gnu
        .text_grid()
        .into_iter()
        .find(|row| row.contains("GRUVBOX-TUI-CLEAN"))
        .unwrap_or_else(|| panic!("GNU did not report Gruvbox cleanup"));
    let neo = pair
        .neo
        .text_grid()
        .into_iter()
        .find(|row| row.contains("GRUVBOX-TUI-CLEAN"))
        .unwrap_or_else(|| panic!("Neo did not report Gruvbox cleanup"));
    if neo.trim_end() != gnu.trim_end() {
        mismatches.push(format!(
            "final cleanup differs\nGNU: {:?}\nNeo: {:?}",
            gnu.trim_end(),
            neo.trim_end()
        ));
    }
    expect!["GRUVBOX-TUI-CLEAN (:state t :errors nil)"].assert_eq(gnu.trim_end());
}

pub(super) struct RenderingExpectations {
    pub(super) dark_elisp: Expect,
    pub(super) dark_org: Expect,
    pub(super) dark_diff: Expect,
    pub(super) dark_properties: ExpectFile,
    pub(super) dark_state: ExpectFile,
    pub(super) light_elisp: Expect,
    pub(super) light_org: Expect,
    pub(super) light_diff: Expect,
    pub(super) light_properties: ExpectFile,
    pub(super) light_state: ExpectFile,
}

pub(super) struct RenderingSnapshots {
    pub(super) dark_elisp: String,
    pub(super) dark_org: String,
    pub(super) dark_diff: String,
    pub(super) dark_properties: String,
    pub(super) dark_state: String,
    pub(super) light_elisp: String,
    pub(super) light_org: String,
    pub(super) light_diff: String,
    pub(super) light_properties: String,
    pub(super) light_state: String,
}
