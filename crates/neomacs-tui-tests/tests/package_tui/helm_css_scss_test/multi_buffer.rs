use super::super::scenario::{PackageTuiScenario, PairTimeout, ReadinessCheckpoint};
use super::super::{CachedMelpaOracle, HELM_CSS_SCSS_MELPA_PIN};
use super::harness::*;
use super::preludes::*;
use expect_test::expect_file;
use std::fs;
use std::time::Duration;

pub(super) fn helm_css_scss_named_display_adapter_drives_real_multi_buffer_helm() {
    let oracle = CachedMelpaOracle::new(HELM_CSS_SCSS_MELPA_PIN, "helm-css-scss.el")
        .expect("prepare exact revision-pinned helm-css-scss source")
        .with_prelude(HELM_CSS_SCSS_MULTI_TUI_PRELUDE);
    let mut pair = PackageTuiScenario::new("helm-css-scss-multi", oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "multi-buffer stylesheet fixture",
                PairTimeout::same(Duration::from_secs(20)),
            ),
            |grid| grid.iter().any(|row| row.contains("padding: 1rem")),
        )
        .expect("spawn ready configured multi-buffer GNU/Neomacs PTY pair");
    invoke(&mut pair.gnu, "neomacs-hcss-multi-start");
    invoke(&mut pair.neo, "neomacs-hcss-multi-start");
    wait_for(&mut pair.gnu, "GNU real multi-source Helm", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
            && grid.iter().any(|row| row.contains("13: .dashboard"))
            && grid
                .iter()
                .any(|row| row.contains("*Helm Css SCSS multi buffers*"))
    });
    wait_for(&mut pair.neo, "Neomacs real multi-source Helm", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
            && grid.iter().any(|row| row.contains("13: .dashboard"))
            && grid
                .iter()
                .any(|row| row.contains("*Helm Css SCSS multi buffers*"))
    });
    let gnu_initial = helm_grid(&pair.gnu);
    let neo_initial = helm_grid(&pair.neo);
    assert_eq!(
        neo_initial, gnu_initial,
        "full initial multi-source Helm grid differs"
    );

    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU initial multi observation", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-1"))
    });
    wait_for(&mut pair.neo, "Neomacs initial multi observation", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-1"))
    });
    pair.gnu.send_key("C-o");
    pair.neo.send_key("C-o");
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU component source observation", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-2"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs component source observation",
        |grid| grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-2")),
    );

    pair.gnu.send_key("C-n");
    pair.neo.send_key("C-n");
    wait_for(&mut pair.gnu, "GNU exact multi preview error", |grid| {
        grid.iter()
            .any(|row| row.contains("‘recenter’ing a window that does not display current-buffer"))
    });
    wait_for(&mut pair.neo, "Neomacs exact multi preview error", |grid| {
        grid.iter()
            .any(|row| row.contains("‘recenter’ing a window that does not display current-buffer"))
    });
    let gnu_error_grid = helm_grid(&pair.gnu);
    let neo_error_grid = helm_grid(&pair.neo);
    assert_eq!(
        neo_error_grid, gnu_error_grid,
        "full multi-preview error grid differs"
    );
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU partial preview observation", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-3"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs partial preview observation",
        |grid| grid.iter().any(|row| row.contains("HCSS-MULTI-OBSERVED-3")),
    );
    pair.gnu.send_key("RET");
    pair.neo.send_key("RET");
    wait_for(&mut pair.gnu, "GNU multi action returned", |grid| {
        grid.iter().any(|row| row.contains(".button:hover"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid
                .iter()
                .any(|row| row.contains("*Helm Css SCSS multi buffers*"))
    });
    wait_for(&mut pair.neo, "Neomacs multi action returned", |grid| {
        grid.iter().any(|row| row.contains(".button:hover"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid
                .iter()
                .any(|row| row.contains("*Helm Css SCSS multi buffers*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-multi-post-action");
    invoke(&mut pair.neo, "neomacs-hcss-multi-post-action");
    wait_for(&mut pair.gnu, "GNU multi action postcondition", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-MULTI-POST-ACTION"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs multi action postcondition",
        |grid| {
            grid.iter()
                .any(|row| row.contains("HCSS-MULTI-POST-ACTION"))
        },
    );
    invoke(&mut pair.gnu, "neomacs-hcss-multi-finish");
    invoke(&mut pair.neo, "neomacs-hcss-multi-finish");
    wait_for(&mut pair.gnu, "GNU multi cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-CLEAN"))
    });
    wait_for(&mut pair.neo, "Neomacs multi cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-MULTI-CLEAN"))
    });

    let gnu_report = fs::read_to_string(pair.gnu.home_dir().join("hcss-multi-report.sexp"))
        .expect("read GNU configured-multi report");
    let neo_report = fs::read_to_string(pair.neo.home_dir().join("hcss-multi-report.sexp"))
        .expect("read Neomacs configured-multi report");
    let report_expect = expect_file!["snapshots/multi_buffer_report.txt"];
    report_expect.assert_eq(&gnu_report);
    assert_eq!(neo_report, gnu_report, "configured multi report differs");
    let initial_expect = expect_file!["snapshots/multi_buffer_initial.txt"];
    initial_expect.assert_eq(&gnu_initial);
    let error_expect = expect_file!["snapshots/multi_buffer_error.txt"];
    error_expect.assert_eq(&gnu_error_grid);
}
