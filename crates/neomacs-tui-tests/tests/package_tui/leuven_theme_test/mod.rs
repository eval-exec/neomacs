use std::time::Duration;

use expect_test::{expect, expect_file};
use neomacs_tui_tests::Snapshot;

use super::{CachedMelpaOracle, LEUVEN_THEME_MELPA_PIN};

use super::scenario::{PackageTuiScenario, PairTimeout, ReadinessCheckpoint};

mod harness;
mod prelude;

use harness::*;
use prelude::*;

#[test]
fn leuven_theme_real_color_lifecycle_matches_gnu() {
    let oracle = CachedMelpaOracle::new(LEUVEN_THEME_MELPA_PIN, "leuven-theme.el")
        .expect("prepare exact Leuven Theme source below ./tmp")
        .with_prelude(LEUVEN_TUI_PRELUDE);
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("LEUVEN-TUI-READY"));
    let mut pair = PackageTuiScenario::new("leuven-theme-lifecycle", oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "Leuven lifecycle readiness marker",
                PairTimeout::per_editor(Duration::from_secs(20), Duration::from_secs(30)),
            ),
            ready,
        )
        .expect("spawn ready real Leuven Theme PTY pair");
    let mut neo_mismatches = Vec::new();

    let gnu_report = lifecycle_report(&pair, true);
    let neo_report = lifecycle_report(&pair, false);
    let expected = expect![[r##"
        CAP (:cells 16777216 :visual-class static-color :display-type color :graphic nil :gate t)
        SOURCE "leuven-theme-20260213.1052"
        REGISTERED (:result t :known t :enabled nil)
        BASELINE (:enabled nil :mode dark :direct ("unspecified-fg" "unspecified-bg") :resolved ("unspecified-fg" "unspecified-bg"))
        LIGHT (:enabled (leuven) :mode light :direct ("#333333" "#FFFFFF") :resolved ("#333333" "#FFFFFF"))
        DARK (:enabled (leuven-dark leuven) :mode dark :direct ("#cfccd2" "#25202a") :resolved ("#cfccd2" "#25202a"))
        LIGHT-RESTORED (:enabled (leuven) :mode light :direct ("#333333" "#FFFFFF") :resolved ("#333333" "#FFFFFF"))
        BASELINE-RESTORED (:enabled nil :mode dark :direct ("unspecified-fg" "unspecified-bg") :resolved ("unspecified-fg" "unspecified-bg"))
        RESTORATION (:light t :baseline t :second-disable t)
        LEUVEN-TUI-READY"##]];
    expected.assert_snapshot(&gnu_report);
    record_neo_mismatch(
        &mut neo_mismatches,
        "public theme lifecycle",
        &neo_report,
        &gnu_report,
    );

    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-use-light",
        ";; Publish release Ω",
    );
    let light_elisp = assert_rendered_rows(
        &pair,
        "light Elisp rows",
        &[
            ";; Publish release",
            "defconst",
            "defun",
            "Ship ARTIFACT",
            "when artifact",
        ],
        expect![[r#"
            [0;38;2;141;141;132;48;2;255;255;255m;; [0;2;38;2;160;161;167;48;2;255;255;255mPublish release Ω after review. [0;38;2;51;51;51;48;2;255;255;255m                                                                                                                             [0m
            [0;38;2;51;51;51;48;2;255;255;255m([0;38;2;0;0;255;48;2;255;255;255mdefconst[0;38;2;51;51;51;48;2;255;255;255m [0;38;2;186;54;165;48;2;255;255;255mrelease-limit[0;38;2;51;51;51;48;2;255;255;255m 42)                                                                                                                                     [0m
            [0;38;2;51;51;51;48;2;255;255;255m([0;38;2;0;0;255;48;2;255;255;255mdefun[0;38;2;51;51;51;48;2;255;255;255m [0;38;2;0;102;153;48;2;255;255;255mdeploy-release[0;38;2;51;51;51;48;2;255;255;255m (artifact)                                                                                                                                [0m
            [0;38;2;51;51;51;48;2;255;255;255m  [0;38;2;3;106;7;48;2;255;255;255m"Ship ARTIFACT safely."[0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                       [0m
            [0;38;2;51;51;51;48;2;255;255;255m  ([0;38;2;0;0;255;48;2;255;255;255mwhen[0;38;2;51;51;51;48;2;255;255;255m artifact (message [0;38;2;0;128;0;48;2;255;255;255m"ship %s"[0;38;2;51;51;51;48;2;255;255;255m artifact)))                                                                                                                 [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-show-org",
        "Release Control Ω",
    );
    let light_org = assert_rendered_rows(
        &pair,
        "light Org rows",
        &[
            "Release Control",
            "TODO Deploy",
            "DONE Verify",
            "runbook",
            "begin_src",
        ],
        expect![[r#"
            [0;38;2;0;142;209;48;2;234;234;255m#+title:[0;38;2;51;51;51;48;2;255;255;255m [0;1;38;2;0;0;0;48;2;255;255;255mRelease Control Ω [0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                     [0m
            [0;1;38;2;60;60;60;48;2;240;240;240m* [0;1;38;2;216;171;167;48;2;255;230;228mTODO[0;1;38;2;60;60;60;48;2;240;240;240m Deploy service[0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                           [0m
            [0;1;38;2;18;53;85;48;2;229;244;251m** [0;1;38;2;137;197;143;48;2;226;254;222mDONE[0;1;38;2;18;53;85;48;2;229;244;251m [0;38;2;173;173;173;48;2;229;244;251mVerify rollback[0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                         [0m
            [0;38;2;51;51;51;48;2;255;255;255mRead the [0;4;38;2;0;109;175;48;2;255;255;255mrunbook[0;38;2;51;51;51;48;2;255;255;255m.                                                                                                                                               [0m
            [0;4;38;2;85;85;85;48;2;226;225;213m#+begin_src emacs-lisp                                                                                                                                          [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-diff", "diff --git");
    let light_diff = assert_rendered_rows(
        &pair,
        "light Diff rows",
        &[
            "diff --git",
            "@@ -1,2",
            " context line",
            "-old release",
            "+new release",
        ],
        expect![[r#"
            [0;1;38;2;128;0;0;48;2;255;255;175mdiff --git a/release.el b/release.el                                                                                                                            [0m
            [0;38;2;153;0;153;48;2;255;238;255m@@ -1,2 +1,2 @@[0;38;2;51;51;51;48;2;255;255;255m                                                                                                                                                 [0m
            [0;38;2;160;161;167;48;2;255;255;255m context line                                                                                                                                                   [0m
            [0;38;2;204;51;51;48;2;255;220;224m-[0;38;2;51;51;51;48;2;255;182;186mold[0;38;2;51;51;51;48;2;254;232;233m release                                                                                                                                                    [0m
            [0;38;2;58;153;58;48;2;205;255;216m+[0;38;2;51;51;51;48;2;151;242;149mnew[0;38;2;51;51;51;48;2;221;255;221m release [0;38;2;51;51;51;48;2;151;242;149mΩ[0;38;2;51;51;51;48;2;221;255;221m                                                                                                                                                  [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-report", "PHASE light");
    let gnu_light_report = lifecycle_report(&pair, true);
    let neo_light_report = lifecycle_report(&pair, false);
    let light_report = expect_file!["snapshots/light_report.txt"];
    light_report.assert_eq(&gnu_light_report);
    record_neo_mismatch(
        &mut neo_mismatches,
        "light applied-face and property report",
        &neo_light_report,
        &gnu_light_report,
    );

    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-use-dark",
        ";; Publish release Ω",
    );
    assert_rendered_rows(
        &pair,
        "dark Elisp rows",
        &[
            ";; Publish release",
            "defconst",
            "defun",
            "Ship ARTIFACT",
            "when artifact",
        ],
        expect![[r#"
            [0;38;2;118;114;131;48;2;37;32;42m;; [0;2;38;2;118;114;131;48;2;37;32;42mPublish release Ω after review. [0;38;2;207;204;210;48;2;37;32;42m                                                                                                                             [0m
            [0;38;2;207;204;210;48;2;37;32;42m([0;38;2;255;255;11;48;2;37;32;42mdefconst[0;38;2;207;204;210;48;2;37;32;42m [0;38;2;74;201;100;48;2;37;32;42mrelease-limit[0;38;2;207;204;210;48;2;37;32;42m 42)                                                                                                                                     [0m
            [0;38;2;207;204;210;48;2;37;32;42m([0;38;2;255;255;11;48;2;37;32;42mdefun[0;38;2;207;204;210;48;2;37;32;42m [0;38;2;255;153;111;48;2;37;32;42mdeploy-release[0;38;2;207;204;210;48;2;37;32;42m (artifact)                                                                                                                                [0m
            [0;38;2;207;204;210;48;2;37;32;42m  [0;38;2;253;149;250;48;2;37;32;42m"Ship ARTIFACT safely."[0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                       [0m
            [0;38;2;207;204;210;48;2;37;32;42m  ([0;38;2;255;255;11;48;2;37;32;42mwhen[0;38;2;207;204;210;48;2;37;32;42m artifact (message [0;38;2;255;127;255;48;2;37;32;42m"ship %s"[0;38;2;207;204;210;48;2;37;32;42m artifact)))                                                                                                                 [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-show-org",
        "Release Control Ω",
    );
    assert_rendered_rows(
        &pair,
        "dark Org rows",
        &[
            "Release Control",
            "TODO Deploy",
            "DONE Verify",
            "runbook",
            "begin_src",
        ],
        expect![[r#"
            [0;38;2;255;113;56;48;2;56;51;42m#+title:[0;38;2;207;204;210;48;2;37;32;42m [0;1;38;2;255;255;255;48;2;37;32;42mRelease Control Ω [0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                     [0m
            [0;1;38;2;199;195;203;48;2;50;45;55m* [0;1;38;2;44;84;98;48;2;37;55;67mTODO[0;1;38;2;199;195;203;48;2;50;45;55m Deploy service[0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                           [0m
            [0;1;38;2;239;202;178;48;2;61;42;45m** [0;1;38;2;73;68;78;48;2;50;45;55mDONE[0;1;38;2;239;202;178;48;2;61;42;45m [0;38;2;87;82;92;48;2;61;42;45mVerify rollback[0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                         [0m
            [0;38;2;207;204;210;48;2;37;32;42mRead the [0;4;38;2;255;146;90;48;2;37;32;42mrunbook[0;38;2;207;204;210;48;2;37;32;42m.                                                                                                                                               [0m
            [0;4;38;2;174;170;178;48;2;34;30;52m#+begin_src emacs-lisp                                                                                                                                          [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-diff", "diff --git");
    assert_rendered_rows(
        &pair,
        "dark Diff rows",
        &[
            "diff --git",
            "@@ -1,2",
            " context line",
            "-old release",
            "+new release",
        ],
        expect![[r#"
            [0;1;38;2;131;255;255;48;2;37;32;115mdiff --git a/release.el b/release.el                                                                                                                            [0m
            [0;38;2;107;255;111;48;2;37;47;42m@@ -1,2 +1,2 @@[0;38;2;207;204;210;48;2;37;32;42m                                                                                                                                                 [0m
            [0;38;2;123;119;127;48;2;37;32;42m context line                                                                                                                                                   [0m
            [0;38;2;56;204;210;48;2;37;64;70m-[0;38;2;207;204;210;48;2;6;73;79mold[0;38;2;207;204;210;48;2;37;53;62m release                                                                                                                                                    [0m
            [0;38;2;201;102;204;48;2;83;32;78m+[0;38;2;207;204;210;48;2;109;13;115mnew[0;38;2;207;204;210;48;2;68;32;73m release [0;38;2;207;204;210;48;2;109;13;115mΩ[0;38;2;207;204;210;48;2;68;32;73m                                                                                                                                                  [0m
        "#]],
        &mut neo_mismatches,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-report", "PHASE dark");
    let gnu_dark_report = lifecycle_report(&pair, true);
    let neo_dark_report = lifecycle_report(&pair, false);
    let dark_report = expect_file!["snapshots/dark_report.txt"];
    dark_report.assert_eq(&gnu_dark_report);
    record_neo_mismatch(
        &mut neo_mismatches,
        "dark applied-face and property report",
        &neo_dark_report,
        &gnu_dark_report,
    );

    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-use-light",
        ";; Publish release Ω",
    );
    let restored_gnu_elisp = ansi_rows(
        &pair.gnu,
        &[
            ";; Publish release",
            "defconst",
            "defun",
            "Ship ARTIFACT",
            "when artifact",
        ],
    );
    assert_eq!(
        restored_gnu_elisp, light_elisp,
        "GNU light Elisp rendering was not restored"
    );
    let restored_neo_elisp = ansi_rows(
        &pair.neo,
        &[
            ";; Publish release",
            "defconst",
            "defun",
            "Ship ARTIFACT",
            "when artifact",
        ],
    );
    record_neo_mismatch(
        &mut neo_mismatches,
        "post-dark restored light Elisp rows",
        &restored_neo_elisp,
        &light_elisp,
    );
    invoke_both(
        &mut pair,
        "neomacs-leuven-tui-show-org",
        "Release Control Ω",
    );
    let restored_gnu_org = ansi_rows(
        &pair.gnu,
        &[
            "Release Control",
            "TODO Deploy",
            "DONE Verify",
            "runbook",
            "begin_src",
        ],
    );
    assert_eq!(
        restored_gnu_org, light_org,
        "GNU light Org rendering was not restored"
    );
    let restored_neo_org = ansi_rows(
        &pair.neo,
        &[
            "Release Control",
            "TODO Deploy",
            "DONE Verify",
            "runbook",
            "begin_src",
        ],
    );
    record_neo_mismatch(
        &mut neo_mismatches,
        "post-dark restored light Org rows",
        &restored_neo_org,
        &light_org,
    );
    invoke_both(&mut pair, "neomacs-leuven-tui-show-diff", "diff --git");
    let restored_gnu_diff = ansi_rows(
        &pair.gnu,
        &[
            "diff --git",
            "@@ -1,2",
            " context line",
            "-old release",
            "+new release",
        ],
    );
    assert_eq!(
        restored_gnu_diff, light_diff,
        "GNU light Diff rendering was not restored"
    );
    let restored_neo_diff = ansi_rows(
        &pair.neo,
        &[
            "diff --git",
            "@@ -1,2",
            " context line",
            "-old release",
            "+new release",
        ],
    );
    record_neo_mismatch(
        &mut neo_mismatches,
        "post-dark restored light Diff rows",
        &restored_neo_diff,
        &light_diff,
    );

    invoke_both(&mut pair, "neomacs-leuven-tui-finish", "LEUVEN-TUI-CLEAN");

    let gnu_clean = exact_row(&pair.gnu, "LEUVEN-TUI-CLEAN");
    let neo_clean = exact_row(&pair.neo, "LEUVEN-TUI-CLEAN");
    let clean = expect![[
        r#"LEUVEN-TUI-CLEAN (:enabled nil :mode dark :direct ("unspecified-fg" "unspecified-bg") :resolved ("unspecified-fg" "unspecified-bg"))"#
    ]];
    clean.assert_eq(&gnu_clean);
    record_neo_mismatch(
        &mut neo_mismatches,
        "final cleanup state",
        &neo_clean,
        &gnu_clean,
    );

    assert!(
        neo_mismatches.is_empty(),
        "Leuven Theme Neo divergences:\n{}",
        neo_mismatches.join("\n\n")
    );
}
