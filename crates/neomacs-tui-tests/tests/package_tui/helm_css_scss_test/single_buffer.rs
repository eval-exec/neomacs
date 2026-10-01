use super::super::scenario::{PackageTuiScenario, PairTimeout, ReadinessCheckpoint};
use super::super::{CachedMelpaOracle, HELM_CSS_SCSS_MELPA_PIN};
use super::harness::*;
use super::preludes::*;
use expect_test::expect_file;
use std::fs;
use std::time::Duration;

pub(super) fn helm_css_scss_named_display_adapter_drives_real_single_buffer_helm() {
    let oracle = CachedMelpaOracle::new(HELM_CSS_SCSS_MELPA_PIN, "helm-css-scss.el")
        .expect("prepare exact revision-pinned helm-css-scss source")
        .with_prelude(HELM_CSS_SCSS_SINGLE_TUI_PRELUDE);
    let mut pair = PackageTuiScenario::new("helm-css-scss-single", oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "single-buffer stylesheet fixture",
                PairTimeout::same(Duration::from_secs(20)),
            ),
            |grid| grid.iter().any(|row| row.contains("padding: 1rem")),
        )
        .expect("spawn ready configured helm-css-scss GNU/Neomacs PTY pair");

    invoke(&mut pair.gnu, "neomacs-hcss-single-start");
    invoke(&mut pair.neo, "neomacs-hcss-single-start");
    wait_for(&mut pair.gnu, "GNU real Helm candidates", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
            && grid.iter().any(|row| row.contains("13: .dashboard"))
            && grid.iter().any(|row| row.contains("Selector:"))
    });
    wait_for(&mut pair.neo, "Neomacs real Helm candidates", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
            && grid.iter().any(|row| row.contains("13: .dashboard"))
            && grid.iter().any(|row| row.contains("Selector:"))
    });
    let gnu_initial = helm_grid(&pair.gnu);
    let neo_initial = helm_grid(&pair.neo);
    assert_eq!(
        neo_initial, gnu_initial,
        "initial real single-buffer Helm grid differs"
    );

    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU initial Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-1"))
    });
    wait_for(&mut pair.neo, "Neomacs initial Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-1"))
    });
    pair.gnu.send_key("C-n");
    pair.neo.send_key("C-n");
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU next-row Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-2"))
    });
    wait_for(&mut pair.neo, "Neomacs next-row Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-2"))
    });
    pair.gnu.send(b"footer");
    pair.neo.send(b"footer");
    wait_for(&mut pair.gnu, "GNU live Helm filter", |grid| {
        grid.iter().any(|row| row.contains("Selector: footer"))
            && grid.iter().any(|row| row.contains("20: .footer"))
            && !grid.iter().any(|row| row.contains("10: .dashboard"))
    });
    wait_for(&mut pair.neo, "Neomacs live Helm filter", |grid| {
        grid.iter().any(|row| row.contains("Selector: footer"))
            && grid.iter().any(|row| row.contains("20: .footer"))
            && !grid.iter().any(|row| row.contains("10: .dashboard"))
    });
    let gnu_filtered = helm_grid(&pair.gnu);
    let neo_filtered = helm_grid(&pair.neo);
    assert_eq!(
        neo_filtered, gnu_filtered,
        "filtered real Helm grid differs"
    );
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU filtered Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-3"))
    });
    wait_for(&mut pair.neo, "Neomacs filtered Helm observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-3"))
    });
    pair.gnu.send_key("C-g");
    pair.neo.send_key("C-g");
    wait_for(&mut pair.gnu, "GNU cancellation returned", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs cancellation returned", |grid| {
        grid.iter().any(|row| row.contains("padding: 1rem"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-cancel");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-cancel");
    wait_for(&mut pair.gnu, "GNU cancellation postcondition", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-POST-cancel"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs cancellation postcondition",
        |grid| {
            grid.iter()
                .any(|row| row.contains("HCSS-SINGLE-POST-cancel"))
        },
    );
    pair.gnu.send_key("C-x");
    pair.gnu.send(b"1");
    pair.neo.send_key("C-x");
    pair.neo.send(b"1");
    wait_for(&mut pair.gnu, "GNU controlled source window", |grid| {
        !grid.iter().any(|row| row.contains("*scratch*"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs controlled source window", |grid| {
        !grid.iter().any(|row| row.contains("*scratch*"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });

    // A second public session must work immediately after cancellation.  RET
    // executes the package's real default open-brace action.
    invoke(&mut pair.gnu, "neomacs-hcss-single-start");
    invoke(&mut pair.neo, "neomacs-hcss-single-start");
    wait_for(&mut pair.gnu, "GNU recovery Helm session", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
    });
    wait_for(&mut pair.neo, "Neomacs recovery Helm session", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
    });
    pair.gnu.send_key("RET");
    pair.neo.send_key("RET");
    wait_for(&mut pair.gnu, "GNU open action returned", |grid| {
        grid.iter().any(|row| row.contains(".card {"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs open action returned", |grid| {
        grid.iter().any(|row| row.contains(".card {"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-open");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-open");

    // Enter the real Helm action menu, choose its second public action with
    // navigation keys, and then exercise the public back-to-last-point toggle.
    invoke(&mut pair.gnu, "neomacs-hcss-single-start");
    invoke(&mut pair.neo, "neomacs-hcss-single-start");
    wait_for(&mut pair.gnu, "GNU close-action source session", |grid| {
        grid.iter().any(|row| row.contains("[4 Candidate(s)]"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs close-action source session",
        |grid| grid.iter().any(|row| row.contains("[4 Candidate(s)]")),
    );
    pair.gnu.send_key("TAB");
    pair.neo.send_key("TAB");
    wait_for(&mut pair.gnu, "GNU Helm action menu", |grid| {
        grid.iter().any(|row| row.contains("Goto open brace"))
            && grid.iter().any(|row| row.contains("Goto close brace"))
    });
    wait_for(&mut pair.neo, "Neomacs Helm action menu", |grid| {
        grid.iter().any(|row| row.contains("Goto open brace"))
            && grid.iter().any(|row| row.contains("Goto close brace"))
    });
    let gnu_actions = helm_grid(&pair.gnu);
    let neo_actions = helm_grid(&pair.neo);
    assert_eq!(
        exact_split_panes_from(&pair.neo, 25),
        exact_split_panes_from(&pair.gnu, 25),
        "exact real Helm action panes differ:\nGNU:\n{gnu_actions}\nNeomacs:\n{neo_actions}"
    );
    pair.gnu.send_key("C-n");
    pair.neo.send_key("C-n");
    wait_for(&mut pair.gnu, "GNU second Helm action selected", |grid| {
        grid.iter().any(|row| row.contains("*helm action* L2"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs second Helm action selected",
        |grid| grid.iter().any(|row| row.contains("*helm action* L2")),
    );
    pair.gnu.send_key("RET");
    pair.neo.send_key("RET");
    wait_for(&mut pair.gnu, "GNU close action returned", |grid| {
        grid.iter().any(|row| row.contains("color: black"))
            && !grid.iter().any(|row| row.contains("Select action:"))
            && !grid.iter().any(|row| row.contains("*helm action*"))
    });
    wait_for(&mut pair.neo, "Neomacs close action returned", |grid| {
        grid.iter().any(|row| row.contains("color: black"))
            && !grid.iter().any(|row| row.contains("Select action:"))
            && !grid.iter().any(|row| row.contains("*helm action*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-close");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-close");
    invoke(&mut pair.gnu, "neomacs-hcss-single-back-one");
    invoke(&mut pair.neo, "neomacs-hcss-single-back-one");
    invoke(&mut pair.gnu, "neomacs-hcss-single-back-two");
    invoke(&mut pair.neo, "neomacs-hcss-single-back-two");
    invoke(&mut pair.gnu, "neomacs-hcss-single-restore-test-fold");
    invoke(&mut pair.neo, "neomacs-hcss-single-restore-test-fold");

    // Real user editing invalidates the modified-buffer cache on the next
    // public session; a real C-x C-s then runs the package's after-save hook.
    pair.gnu.send_key("M->");
    pair.neo.send_key("M->");
    pair.gnu.send(b".new-release { color: teal; }\n");
    pair.neo.send(b".new-release { color: teal; }\n");
    invoke(&mut pair.gnu, "neomacs-hcss-single-start-new-release");
    invoke(&mut pair.neo, "neomacs-hcss-single-start-new-release");
    wait_for(&mut pair.gnu, "GNU unsaved selector rebuilt", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
            && grid.iter().any(|row| row.contains("Selector: new-release"))
    });
    wait_for(&mut pair.neo, "Neomacs unsaved selector rebuilt", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
            && grid.iter().any(|row| row.contains("Selector: new-release"))
    });
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU unsaved-cache observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-4"))
    });
    wait_for(&mut pair.neo, "Neomacs unsaved-cache observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-4"))
    });
    pair.gnu.send_key("C-g");
    pair.neo.send_key("C-g");
    wait_for(&mut pair.gnu, "GNU unsaved-cache cancel", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs unsaved-cache cancel", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
            && !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-unsaved");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-unsaved");
    pair.gnu.send_key("C-x");
    pair.gnu.send_key("C-s");
    pair.neo.send_key("C-x");
    pair.neo.send_key("C-s");
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-save");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-save");

    invoke(&mut pair.gnu, "neomacs-hcss-single-start-new-release");
    invoke(&mut pair.neo, "neomacs-hcss-single-start-new-release");
    wait_for(&mut pair.gnu, "GNU post-save selector rebuilt", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs post-save selector rebuilt",
        |grid| grid.iter().any(|row| row.contains(".new-release")),
    );
    send_single_observer(&mut pair.gnu);
    send_single_observer(&mut pair.neo);
    wait_for(&mut pair.gnu, "GNU rebuilt-cache observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-5"))
    });
    wait_for(&mut pair.neo, "Neomacs rebuilt-cache observation", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-OBSERVED-5"))
    });
    pair.gnu.send_key("RET");
    pair.neo.send_key("RET");
    wait_for(&mut pair.gnu, "GNU rebuilt-cache action", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
    });
    wait_for(&mut pair.neo, "Neomacs rebuilt-cache action", |grid| {
        grid.iter().any(|row| row.contains(".new-release"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-rebuilt");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-rebuilt");

    // Exercise the actual public isearch handoff.  Only the documented public
    // display option is configured; literal quoting and regexp preservation
    // are performed by `helm-css-scss-from-isearch' itself.
    wait_for(&mut pair.gnu, "GNU rebuilt action postcondition", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-POST-saved-cache-rebuilt-action"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs rebuilt action postcondition",
        |grid| {
            grid.iter()
                .any(|row| row.contains("HCSS-SINGLE-POST-saved-cache-rebuilt-action"))
        },
    );
    pair.gnu.send_key("C-x");
    pair.gnu.send(b"1");
    pair.neo.send_key("C-x");
    pair.neo.send(b"1");
    invoke(
        &mut pair.gnu,
        "neomacs-hcss-single-configure-public-display",
    );
    invoke(
        &mut pair.neo,
        "neomacs-hcss-single-configure-public-display",
    );
    wait_for(&mut pair.gnu, "GNU public display configured", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-PUBLIC-DISPLAY-CONFIGURED"))
    });
    wait_for(&mut pair.neo, "Neomacs public display configured", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-PUBLIC-DISPLAY-CONFIGURED"))
    });

    pair.gnu.send_key("M-<");
    pair.neo.send_key("M-<");
    pair.gnu.send_key("C-s");
    pair.neo.send_key("C-s");
    pair.gnu.send(b".footer");
    pair.neo.send(b".footer");
    wait_for(&mut pair.gnu, "GNU literal isearch", |grid| {
        grid.iter().any(|row| row.contains("I-search: .footer"))
    });
    wait_for(&mut pair.neo, "Neomacs literal isearch", |grid| {
        grid.iter().any(|row| row.contains("I-search: .footer"))
    });
    invoke(&mut pair.gnu, "helm-css-scss-from-isearch");
    invoke(&mut pair.neo, "helm-css-scss-from-isearch");
    wait_for(&mut pair.gnu, "GNU literal isearch Helm handoff", |grid| {
        grid.iter().any(|row| row.contains("Selector: \\.footer"))
            && grid.iter().any(|row| row.contains("20: .footer"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs literal isearch Helm handoff",
        |grid| {
            grid.iter().any(|row| row.contains("Selector: \\.footer"))
                && grid.iter().any(|row| row.contains("20: .footer"))
        },
    );
    let gnu_isearch_literal = helm_grid(&pair.gnu);
    let neo_isearch_literal = helm_grid(&pair.neo);
    assert_eq!(
        exact_grid_rows_from(&pair.neo, 25),
        exact_grid_rows_from(&pair.gnu, 25),
        "literal isearch exact Helm pane differs:\nGNU:\n{gnu_isearch_literal}\nNeomacs:\n{neo_isearch_literal}"
    );
    pair.gnu.send_key("C-g");
    pair.neo.send_key("C-g");
    wait_for(&mut pair.gnu, "GNU literal isearch cancel", |grid| {
        !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs literal isearch cancel", |grid| {
        !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-isearch-literal");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-isearch-literal");
    wait_for(&mut pair.gnu, "GNU literal isearch postcondition", |grid| {
        grid.iter()
            .any(|row| row.contains("HCSS-SINGLE-POST-isearch-literal-cancel"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs literal isearch postcondition",
        |grid| {
            grid.iter()
                .any(|row| row.contains("HCSS-SINGLE-POST-isearch-literal-cancel"))
        },
    );
    pair.gnu.send_key("C-x");
    pair.gnu.send(b"1");
    pair.neo.send_key("C-x");
    pair.neo.send(b"1");

    pair.gnu.send_key("M-<");
    pair.neo.send_key("M-<");
    pair.gnu.send_key("C-M-s");
    pair.neo.send_key("C-M-s");
    pair.gnu.send(b".footer");
    pair.neo.send(b".footer");
    wait_for(&mut pair.gnu, "GNU regexp isearch", |grid| {
        grid.iter()
            .any(|row| row.contains("Regexp I-search: .footer"))
    });
    wait_for(&mut pair.neo, "Neomacs regexp isearch", |grid| {
        grid.iter()
            .any(|row| row.contains("Regexp I-search: .footer"))
    });
    invoke(&mut pair.gnu, "helm-css-scss-from-isearch");
    invoke(&mut pair.neo, "helm-css-scss-from-isearch");
    wait_for(&mut pair.gnu, "GNU regexp isearch Helm handoff", |grid| {
        grid.iter().any(|row| row.contains("Selector: .footer"))
            && grid.iter().any(|row| row.contains("20: .footer"))
    });
    wait_for(
        &mut pair.neo,
        "Neomacs regexp isearch Helm handoff",
        |grid| {
            grid.iter().any(|row| row.contains("Selector: .footer"))
                && grid.iter().any(|row| row.contains("20: .footer"))
        },
    );
    let gnu_isearch_regexp = helm_grid(&pair.gnu);
    let neo_isearch_regexp = helm_grid(&pair.neo);
    assert_eq!(
        exact_grid_rows_from(&pair.neo, 25),
        exact_grid_rows_from(&pair.gnu, 25),
        "regexp isearch exact Helm pane differs:\nGNU:\n{gnu_isearch_regexp}\nNeomacs:\n{neo_isearch_regexp}"
    );
    pair.gnu.send_key("C-g");
    pair.neo.send_key("C-g");
    wait_for(&mut pair.gnu, "GNU regexp isearch cancel", |grid| {
        !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    wait_for(&mut pair.neo, "Neomacs regexp isearch cancel", |grid| {
        !grid.iter().any(|row| row.contains("Selector:"))
            && !grid.iter().any(|row| row.contains("*Helm Css SCSS*"))
    });
    invoke(&mut pair.gnu, "neomacs-hcss-single-post-isearch-regexp");
    invoke(&mut pair.neo, "neomacs-hcss-single-post-isearch-regexp");

    invoke(&mut pair.gnu, "neomacs-hcss-single-finish");
    invoke(&mut pair.neo, "neomacs-hcss-single-finish");
    wait_for(&mut pair.gnu, "GNU configured single cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-SINGLE-CLEAN"))
    });
    wait_for(&mut pair.neo, "Neomacs configured single cleanup", |grid| {
        grid.iter().any(|row| row.contains("HCSS-SINGLE-CLEAN"))
    });

    let gnu_report = fs::read_to_string(pair.gnu.home_dir().join("hcss-single-report.sexp"))
        .expect("read GNU configured-single report");
    let neo_report = fs::read_to_string(pair.neo.home_dir().join("hcss-single-report.sexp"))
        .expect("read Neomacs configured-single report");
    let report_expect = expect_file!["snapshots/single_buffer_report.txt"];
    report_expect.assert_eq(&gnu_report);
    assert_eq!(neo_report, gnu_report, "configured single report differs");

    let initial_expect = expect_file!["snapshots/single_buffer_initial.txt"];
    initial_expect.assert_eq(&gnu_initial);
    let filtered_expect = expect_file!["snapshots/single_buffer_filtered.txt"];
    filtered_expect.assert_eq(&gnu_filtered);
    let action_expect = expect_file!["snapshots/single_buffer_action.txt"];
    action_expect.assert_eq(&gnu_actions);
    let literal_isearch_expect = expect_file!["snapshots/single_buffer_literal_isearch.txt"];
    literal_isearch_expect.assert_eq(&gnu_isearch_literal);
    let regexp_isearch_expect = expect_file!["snapshots/single_buffer_regexp_isearch.txt"];
    regexp_isearch_expect.assert_eq(&gnu_isearch_regexp);
}
