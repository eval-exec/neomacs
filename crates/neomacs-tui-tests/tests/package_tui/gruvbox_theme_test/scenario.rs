use super::super::PreparedPackageSet;
use super::super::scenario::{PackageTuiPair, TerminalProfile};
use super::harness::*;
use expect_test::{Expect, ExpectFile, expect, expect_file};
use neomacs_tui_tests::Snapshot;

pub(super) fn exercise_rendering(
    pair: &mut PackageTuiPair,
    profile: &str,
    expected: RenderingExpectations,
    mismatches: &mut Vec<String>,
) -> RenderingSnapshots {
    let RenderingExpectations {
        dark_elisp,
        dark_org,
        dark_diff,
        dark_properties,
        dark_state,
        light_elisp,
        light_org,
        light_diff,
        light_properties,
        light_state,
    } = expected;
    invoke_both(pair, "gt357-use-dark-medium", "; comment Ω");
    let dark_elisp = record_grid(
        pair,
        &format!("{profile} dark Elisp"),
        &["; comment Ω", "defun greet", "\"Doc.\"", "Hello %s"],
        dark_elisp,
        mismatches,
    );
    invoke_both(pair, "gt357-show-org", "Plan Ω");
    let dark_org = record_grid(
        pair,
        &format!("{profile} dark Org"),
        &[
            "#+title: Plan Ω",
            "TODO Ship release",
            "DONE Verify rollback",
            "A link and =code=.",
            "#+begin_src",
            "message \"ship\"",
            "#+end_src",
        ],
        dark_org,
        mismatches,
    );
    invoke_both(pair, "gt357-show-diff", "diff --git");
    let dark_diff = record_grid(
        pair,
        &format!("{profile} dark Diff"),
        &[
            "diff --git",
            "--- a/a.el",
            "+++ b/a.el",
            "@@ -1 +1 @@",
            "-(old)",
            "+(new)",
        ],
        dark_diff,
        mismatches,
    );
    let dark_properties = record_properties(
        pair,
        &format!("{profile} dark property runs"),
        dark_properties,
        mismatches,
    );
    let dark_state = record_current_state(
        pair,
        &format!("{profile} dark state"),
        dark_state,
        mismatches,
    );

    invoke_both(pair, "gt357-use-light-medium", "; comment Ω");
    let light_elisp = record_grid(
        pair,
        &format!("{profile} light Elisp"),
        &["; comment Ω", "defun greet", "\"Doc.\"", "Hello %s"],
        light_elisp,
        mismatches,
    );
    invoke_both(pair, "gt357-show-org", "Plan Ω");
    let light_org = record_grid(
        pair,
        &format!("{profile} light Org"),
        &[
            "#+title: Plan Ω",
            "TODO Ship release",
            "DONE Verify rollback",
            "A link and =code=.",
            "#+begin_src",
            "message \"ship\"",
            "#+end_src",
        ],
        light_org,
        mismatches,
    );
    invoke_both(pair, "gt357-show-diff", "diff --git");
    let light_diff = record_grid(
        pair,
        &format!("{profile} light Diff"),
        &[
            "diff --git",
            "--- a/a.el",
            "+++ b/a.el",
            "@@ -1 +1 @@",
            "-(old)",
            "+(new)",
        ],
        light_diff,
        mismatches,
    );
    let light_properties = record_properties(
        pair,
        &format!("{profile} light property runs"),
        light_properties,
        mismatches,
    );
    let light_state = record_current_state(
        pair,
        &format!("{profile} light state"),
        light_state,
        mismatches,
    );

    RenderingSnapshots {
        dark_elisp,
        dark_org,
        dark_diff,
        dark_properties,
        dark_state,
        light_elisp,
        light_org,
        light_diff,
        light_properties,
        light_state,
    }
}

pub(super) struct StackRenderingExpectations {
    pub(super) light_elisp: Expect,
    pub(super) light_org: Expect,
    pub(super) light_diff: Expect,
    pub(super) light_properties: ExpectFile,
    pub(super) light_state: ExpectFile,
    pub(super) restored_elisp: Expect,
    pub(super) restored_org: Expect,
    pub(super) restored_diff: Expect,
    pub(super) restored_properties: ExpectFile,
    pub(super) restored_state: ExpectFile,
}

pub(super) fn record_current_state(
    pair: &mut PackageTuiPair,
    label: &str,
    expected: impl Snapshot,
    mismatches: &mut Vec<String>,
) -> String {
    let mut gnu = Vec::new();
    let mut neo = Vec::new();
    invoke_both(
        pair,
        "gt357-show-current-state",
        "GRUVBOX-THEME-PAGE-DONE 1/3",
    );
    gnu.push(report(&pair.gnu));
    neo.push(report(&pair.neo));
    for page in 2..=3 {
        let ready = if page == 3 {
            "GRUVBOX-THEME-READY".to_owned()
        } else {
            format!("GRUVBOX-THEME-PAGE-DONE {page}/3")
        };
        invoke_both(pair, "gt357-next-state-page", &ready);
        let gnu_page = report(&pair.gnu);
        let neo_page = report(&pair.neo);
        if page == 3 {
            assert!(
                gnu_page
                    .lines()
                    .any(|line| line == "GRUVBOX-THEME-PAGE 3/3"),
                "GNU final state page lacks the exact 3/3 header:\n{gnu_page}"
            );
            assert!(
                neo_page
                    .lines()
                    .any(|line| line == "GRUVBOX-THEME-PAGE 3/3"),
                "Neo final state page lacks the exact 3/3 header:\n{neo_page}"
            );
        }
        gnu.push(gnu_page);
        neo.push(neo_page);
    }
    let gnu = gnu.join("\n--\n");
    let neo = neo.join("\n--\n");
    if neo != gnu {
        mismatches.push(format!("{label} differs\nGNU:\n{gnu}\nNeo:\n{neo}"));
    }
    expected.assert_snapshot(&gnu);
    gnu
}

pub(super) fn exercise_stack_rendering(
    pair: &mut PackageTuiPair,
    ordinary: &RenderingSnapshots,
    expected: StackRenderingExpectations,
    mismatches: &mut Vec<String>,
) {
    let StackRenderingExpectations {
        light_elisp,
        light_org,
        light_diff,
        light_properties,
        light_state,
        restored_elisp,
        restored_org,
        restored_diff,
        restored_properties,
        restored_state,
    } = expected;

    invoke_both(pair, "gt357-use-light-over-dark", "; comment Ω");
    let light_elisp_actual = record_grid(
        pair,
        "truecolor stacked light Elisp",
        &["; comment Ω", "defun greet", "\"Doc.\"", "Hello %s"],
        light_elisp,
        mismatches,
    );
    invoke_both(pair, "gt357-show-org", "Plan Ω");
    let light_org_actual = record_grid(
        pair,
        "truecolor stacked light Org",
        &[
            "#+title: Plan Ω",
            "TODO Ship release",
            "DONE Verify rollback",
            "A link and =code=.",
            "#+begin_src",
            "message \"ship\"",
            "#+end_src",
        ],
        light_org,
        mismatches,
    );
    invoke_both(pair, "gt357-show-diff", "diff --git");
    let light_diff_actual = record_grid(
        pair,
        "truecolor stacked light Diff",
        &[
            "diff --git",
            "--- a/a.el",
            "+++ b/a.el",
            "@@ -1 +1 @@",
            "-(old)",
            "+(new)",
        ],
        light_diff,
        mismatches,
    );
    let light_properties_actual = record_properties(
        pair,
        "truecolor stacked light property runs",
        light_properties,
        mismatches,
    );
    let light_state_actual = record_current_state(
        pair,
        "truecolor stacked light state",
        light_state,
        mismatches,
    );

    invoke_both(pair, "gt357-disable-stack-light", "; comment Ω");
    let restored_elisp_actual = record_grid(
        pair,
        "truecolor restored dark Elisp",
        &["; comment Ω", "defun greet", "\"Doc.\"", "Hello %s"],
        restored_elisp,
        mismatches,
    );
    invoke_both(pair, "gt357-show-org", "Plan Ω");
    let restored_org_actual = record_grid(
        pair,
        "truecolor restored dark Org",
        &[
            "#+title: Plan Ω",
            "TODO Ship release",
            "DONE Verify rollback",
            "A link and =code=.",
            "#+begin_src",
            "message \"ship\"",
            "#+end_src",
        ],
        restored_org,
        mismatches,
    );
    invoke_both(pair, "gt357-show-diff", "diff --git");
    let restored_diff_actual = record_grid(
        pair,
        "truecolor restored dark Diff",
        &[
            "diff --git",
            "--- a/a.el",
            "+++ b/a.el",
            "@@ -1 +1 @@",
            "-(old)",
            "+(new)",
        ],
        restored_diff,
        mismatches,
    );
    let restored_properties_actual = record_properties(
        pair,
        "truecolor restored dark property runs",
        restored_properties,
        mismatches,
    );
    let restored_state_actual = record_current_state(
        pair,
        "truecolor restored dark state",
        restored_state,
        mismatches,
    );

    for (label, actual, ordinary) in [
        (
            "stacked light Elisp",
            &light_elisp_actual,
            &ordinary.light_elisp,
        ),
        ("stacked light Org", &light_org_actual, &ordinary.light_org),
        (
            "stacked light Diff",
            &light_diff_actual,
            &ordinary.light_diff,
        ),
        (
            "stacked light properties",
            &light_properties_actual,
            &ordinary.light_properties,
        ),
        (
            "restored dark Elisp",
            &restored_elisp_actual,
            &ordinary.dark_elisp,
        ),
        (
            "restored dark Org",
            &restored_org_actual,
            &ordinary.dark_org,
        ),
        (
            "restored dark Diff",
            &restored_diff_actual,
            &ordinary.dark_diff,
        ),
        (
            "restored dark properties",
            &restored_properties_actual,
            &ordinary.dark_properties,
        ),
    ] {
        if actual != ordinary {
            mismatches.push(format!(
                "truecolor {label} differs from its ordinary rendering\nORDINARY:\n{ordinary}\nSTACKED/RESTORED:\n{actual}"
            ));
        }
    }
    let without_enabled = |state: &str| {
        state
            .lines()
            .filter(|line| !line.starts_with("ENABLED "))
            .collect::<Vec<_>>()
            .join("\n")
    };
    for (label, actual, ordinary) in [
        (
            "stacked light state",
            &light_state_actual,
            &ordinary.light_state,
        ),
        (
            "restored dark state",
            &restored_state_actual,
            &ordinary.dark_state,
        ),
    ] {
        if without_enabled(actual) != without_enabled(ordinary) {
            mismatches.push(format!(
                "truecolor {label} differs beyond the intentional enabled stack\nORDINARY:\n{ordinary}\nSTACKED/RESTORED:\n{actual}"
            ));
        }
    }
}

pub(super) fn run_profile(
    label: &str,
    packages: &PreparedPackageSet,
    terminal_profile: TerminalProfile,
    body: impl FnOnce(&mut PackageTuiPair, &mut Vec<String>),
) -> Result<(), String> {
    let mut pair = spawn_profile(label, packages, terminal_profile)?;
    let mut mismatches = Vec::new();
    let body_result = catch_phase(&format!("{label} body"), || {
        wait_for_boot_both(&mut pair);
        body(&mut pair, &mut mismatches);
    });
    let cleanup_result = catch_phase(&format!("{label} cleanup"), || {
        finish(&mut pair, &mut mismatches)
    });
    let mut failures = [body_result.err(), cleanup_result.err()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    failures.extend(mismatches);
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("\n\n"))
    }
}

pub(super) fn default_org_consumer(packages: &PreparedPackageSet) -> Result<(), String> {
    run_profile(
        "gruvbox-theme-default-org-consumer",
        packages,
        TerminalProfile::TrueColor,
        |pair, mismatches| {
            record_pair(
                pair,
                "default Org consumer capability",
                expect![[r#"
                    CAP TERM "dumb"
                    CAP COLORTERM "truecolor"
                    CAP CELLS 16777216
                    CAP VISUAL static-color
                    CAP DISPLAY color
                    CAP GRAPHIC nil
                    CAP TRUECOLOR t
                    CAP COLOR256 t
                    CAP ORG-COMPILED t
                    CAP GNUS-BEFORE nil
                    CAP LOAD-SUFFIXES (".el")
                    THEMES-KNOWN t
                    GRUVBOX-TUI-BOOT"#]],
                mismatches,
            );
            invoke_both(
                pair,
                "gt357-configure-default-org",
                "GRUVBOX-DEFAULT-ORG-READY",
            );
            record_pair(
                pair,
                "default Org module configuration",
                expect![[r#"
                    DEFAULT-ORG MODULES-1 (ol-doi ol-w3m ol-bbdb ol-bibtex ol-docview)
                    DEFAULT-ORG MODULES-2 (ol-gnus ol-info ol-irc ol-mhe ol-rmail ol-eww)
                    DEFAULT-ORG GNUS (nil nil nil)
                    GRUVBOX-DEFAULT-ORG-READY"#]],
                mismatches,
            );
            initialize_consumer(
                pair,
                "default Org",
                expect![[r#"
                CONSUMER MODULES-1 (ol-doi ol-w3m ol-bbdb ol-bibtex ol-docview)
                CONSUMER MODULES-2 (ol-gnus ol-info ol-irc ol-mhe ol-rmail ol-eww)
                CONSUMER BEFORE (nil nil nil)
                CONSUMER THEME (gruvbox-dark-medium)
                CONSUMER OUTCOME (:value returned)
                CONSUMER SOURCE "gnus-sum.elc" t
                CONSUMER AFTER (t t t)
                CONSUMER INHERIT (gnus-group-mail-1 gnus-group-news-low)
                CONSUMER SUFFIXES (".el")
                GRUVBOX-CONSUMER-READY"#]],
                mismatches,
            );
            invoke_both(pair, "gt357-show-org", "Plan Ω");
            record_grid(
                pair,
                "default Org rendered consumer",
                &[
                    "#+title: Plan Ω",
                    "TODO Ship release",
                    "DONE Verify rollback",
                    "A link and =code=.",
                    "#+begin_src",
                    "message \"ship\"",
                    "#+end_src",
                ],
                expect![[r#"
                    [0;38;2;124;111;100;48;2;40;40;40m#+title:[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;69;133;136;48;2;40;40;40mPlan Ω[0m
                    [0;38;2;131;165;152;48;2;40;40;40m* [0;1;38;2;251;73;51;48;2;40;40;40mTODO[0;38;2;131;165;152;48;2;40;40;40m Ship release[0m
                    [0;38;2;250;189;47;48;2;40;40;40m** [0;1;38;2;142;192;124;48;2;40;40;40mDONE[0;38;2;250;189;47;48;2;40;40;40m [0;38;2;142;192;124;48;2;40;40;40mVerify rollback[0m
                    [0;38;2;235;219;178;48;2;40;40;40mA [0;4;38;2;104;157;106;48;2;40;40;40mlink[0;38;2;235;219;178;48;2;40;40;40m and [0;38;2;124;111;100;48;2;40;40;40m=code=[0;38;2;235;219;178;48;2;40;40;40m.[0m
                    [0;38;2;235;219;178;48;2;60;56;54m#+begin_src emacs-lisp[0m
                    [0;38;2;235;219;178;48;2;50;48;47m(message [0;38;2;184;187;38;48;2;50;48;47m"ship"[0;38;2;235;219;178;48;2;50;48;47m)[0m
                    [0;38;2;235;219;178;48;2;60;56;54m#+end_src[0m
                "#]],
                mismatches,
            );
        },
    )
}

pub(super) fn truecolor(packages: &PreparedPackageSet) -> Result<(), String> {
    run_profile(
        "gruvbox-theme-truecolor",
        packages,
        TerminalProfile::TrueColor,
        |pair, mismatches| {
            record_pair(
                pair,
                "truecolor capability",
                expect![[r#"
                    CAP TERM "dumb"
                    CAP COLORTERM "truecolor"
                    CAP CELLS 16777216
                    CAP VISUAL static-color
                    CAP DISPLAY color
                    CAP GRAPHIC nil
                    CAP TRUECOLOR t
                    CAP COLOR256 t
                    CAP ORG-COMPILED t
                    CAP GNUS-BEFORE nil
                    CAP LOAD-SUFFIXES (".el")
                    THEMES-KNOWN t
                    GRUVBOX-TUI-BOOT"#]],
                mismatches,
            );
            invoke_both(pair, "gt357-configure-core-org", "GRUVBOX-CORE-ORG-READY");
            record_pair(
                pair,
                "truecolor core Org configuration",
                expect![[r#"
                    CORE-ORG BEFORE-1 (ol-doi ol-w3m ol-bbdb ol-bibtex ol-docview)
                    CORE-ORG BEFORE-2 (ol-gnus ol-info ol-irc ol-mhe ol-rmail ol-eww)
                    CORE-ORG AFTER nil
                    CORE-ORG GNUS (nil nil nil)
                    GRUVBOX-CORE-ORG-READY"#]],
                mismatches,
            );
            initialize_consumer(
                pair,
                "truecolor core",
                expect![[r#"
                CONSUMER MODULES-1 nil
                CONSUMER MODULES-2 nil
                CONSUMER BEFORE (nil nil nil)
                CONSUMER THEME (gruvbox-dark-medium)
                CONSUMER OUTCOME (:value returned)
                CONSUMER SOURCE nil nil
                CONSUMER AFTER (nil nil nil)
                CONSUMER INHERIT nil
                CONSUMER SUFFIXES (".el")
                GRUVBOX-CONSUMER-READY"#]],
                mismatches,
            );
            record_orderless_completion(
                pair,
                expect![[r#"
                [0;1;38;2;102;153;157;48;2;40;40;40malpha[0;38;2;235;219;178;48;2;40;40;40m [0;1;38;2;214;93;14;48;2;40;40;40mb[0;38;2;235;219;178;48;2;40;40;40meta [0;1;38;2;142;192;124;48;2;40;40;40mgam[0;38;2;235;219;178;48;2;40;40;40mma [0;1;38;2;215;153;33;48;2;40;40;40mdel[0;38;2;235;219;178;48;2;40;40;40mta[0m
            "#]],
                expect![[r##"
                ORDERLESS CHOICE "alpha beta gamma delta"
                ORDERLESS FINAL-INPUT "alpha beta gamma delta"
                ORDERLESS HISTORY-HEAD "alpha beta gamma delta"
                ORDERLESS MINIBUFFER nil
                ORDERLESS RUN ("alpha" orderless-match-face-0)
                ORDERLESS RUN (" " nil)
                ORDERLESS RUN ("b" orderless-match-face-1)
                ORDERLESS RUN ("eta " nil)
                ORDERLESS RUN ("gam" orderless-match-face-2)
                ORDERLESS RUN ("ma " nil)
                ORDERLESS RUN ("del" orderless-match-face-3)
                ORDERLESS RUN ("ta" nil)
                ORDERLESS FACE 0 "#66999D" "#66999D" bold bold
                ORDERLESS FACE 1 "#d65d0e" "#d65d0e" bold bold
                ORDERLESS FACE 2 "#8ec07c" "#8ec07c" bold bold
                ORDERLESS FACE 3 "#d79921" "#d79921" bold bold
                GRUVBOX-ORDERLESS-READY"##]],
                mismatches,
            );
            assert_matrix(
                pair,
                "truecolor seven-theme matrix",
                expect_file!["snapshots/truecolor_matrix.txt"],
                mismatches,
            );
            let ordinary = exercise_rendering(
                pair,
                "truecolor",
                RenderingExpectations {
                    dark_elisp: expect![[r#"
                        [0;38;2;124;111;100;48;2;40;40;40m; comment Ω[0m
                        [0;38;2;235;219;178;48;2;40;40;40m([0;38;2;251;73;51;48;2;40;40;40mdefun[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;250;189;47;48;2;40;40;40mgreet[0;38;2;235;219;178;48;2;40;40;40m (name)[0m
                        [0;38;2;235;219;178;48;2;40;40;40m  [0;38;2;184;187;38;48;2;40;40;40m"Doc."[0m
                        [0;38;2;235;219;178;48;2;40;40;40m  ([0;38;2;251;73;51;48;2;40;40;40mif[0;38;2;235;219;178;48;2;40;40;40m name (message [0;38;2;184;187;38;48;2;40;40;40m"Hello %s"[0;38;2;235;219;178;48;2;40;40;40m name) nil))[0m
                    "#]],
                    dark_org: expect![[r#"
                        [0;38;2;124;111;100;48;2;40;40;40m#+title:[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;69;133;136;48;2;40;40;40mPlan Ω[0m
                        [0;38;2;131;165;152;48;2;40;40;40m* [0;1;38;2;251;73;51;48;2;40;40;40mTODO[0;38;2;131;165;152;48;2;40;40;40m Ship release[0m
                        [0;38;2;250;189;47;48;2;40;40;40m** [0;1;38;2;142;192;124;48;2;40;40;40mDONE[0;38;2;250;189;47;48;2;40;40;40m [0;38;2;142;192;124;48;2;40;40;40mVerify rollback[0m
                        [0;38;2;235;219;178;48;2;40;40;40mA [0;4;38;2;104;157;106;48;2;40;40;40mlink[0;38;2;235;219;178;48;2;40;40;40m and [0;38;2;124;111;100;48;2;40;40;40m=code=[0;38;2;235;219;178;48;2;40;40;40m.[0m
                        [0;38;2;235;219;178;48;2;60;56;54m#+begin_src emacs-lisp[0m
                        [0;38;2;235;219;178;48;2;50;48;47m(message [0;38;2;184;187;38;48;2;50;48;47m"ship"[0;38;2;235;219;178;48;2;50;48;47m)[0m
                        [0;38;2;235;219;178;48;2;60;56;54m#+end_src[0m
                    "#]],
                    dark_diff: expect![[r#"
                        [0;38;2;235;219;178;48;2;60;56;54mdiff --git a/a.el b/a.el[0m
                        [0;38;2;235;219;178;48;2;60;56;54m--- [0;38;2;235;219;178;48;2;80;73;69ma/a.el[0m
                        [0;38;2;235;219;178;48;2;60;56;54m+++ [0;38;2;235;219;178;48;2;80;73;69mb/a.el[0m
                        [0;38;2;235;219;178;48;2;80;73;69m@@ -1 +1 @@[0m
                        [0;38;2;251;73;51;48;2;40;40;40m-([0;38;2;235;219;178;48;2;204;36;29mold[0;38;2;251;73;51;48;2;40;40;40m)[0m
                        [0;38;2;184;187;38;48;2;40;40;40m+([0;38;2;235;219;178;48;2;152;151;26mnew[0;38;2;184;187;38;48;2;40;40;40m)[0m
                    "#]],
                    dark_properties: expect_file!["snapshots/dark_properties.txt"],
                    dark_state: expect_file!["snapshots/dark_state.txt"],
                    light_elisp: expect![[r#"
                        [0;38;2;168;153;132;48;2;251;241;199m; comment Ω[0m
                        [0;38;2;60;56;54;48;2;251;241;199m([0;38;2;157;0;6;48;2;251;241;199mdefun[0;38;2;60;56;54;48;2;251;241;199m [0;38;2;181;118;20;48;2;251;241;199mgreet[0;38;2;60;56;54;48;2;251;241;199m (name)[0m
                        [0;38;2;60;56;54;48;2;251;241;199m  [0;38;2;121;116;14;48;2;251;241;199m"Doc."[0m
                        [0;38;2;60;56;54;48;2;251;241;199m  ([0;38;2;157;0;6;48;2;251;241;199mif[0;38;2;60;56;54;48;2;251;241;199m name (message [0;38;2;121;116;14;48;2;251;241;199m"Hello %s"[0;38;2;60;56;54;48;2;251;241;199m name) nil))[0m
                    "#]],
                    light_org: expect![[r#"
                        [0;38;2;168;153;132;48;2;251;241;199m#+title:[0;38;2;60;56;54;48;2;251;241;199m [0;38;2;69;133;136;48;2;251;241;199mPlan Ω[0m
                        [0;38;2;7;102;120;48;2;251;241;199m* [0;1;38;2;157;0;6;48;2;251;241;199mTODO[0;38;2;7;102;120;48;2;251;241;199m Ship release[0m
                        [0;38;2;181;118;20;48;2;251;241;199m** [0;1;38;2;66;123;88;48;2;251;241;199mDONE[0;38;2;181;118;20;48;2;251;241;199m [0;38;2;66;123;88;48;2;251;241;199mVerify rollback[0m
                        [0;38;2;60;56;54;48;2;251;241;199mA [0;4;38;2;104;157;106;48;2;251;241;199mlink[0;38;2;60;56;54;48;2;251;241;199m and [0;38;2;168;153;132;48;2;251;241;199m=code=[0;38;2;60;56;54;48;2;251;241;199m.[0m
                        [0;38;2;60;56;54;48;2;235;219;178m#+begin_src emacs-lisp[0m
                        [0;38;2;60;56;54;48;2;242;229;188m(message [0;38;2;121;116;14;48;2;242;229;188m"ship"[0;38;2;60;56;54;48;2;242;229;188m)[0m
                        [0;38;2;60;56;54;48;2;235;219;178m#+end_src[0m
                    "#]],
                    light_diff: expect![[r#"
                        [0;38;2;60;56;54;48;2;235;219;178mdiff --git a/a.el b/a.el[0m
                        [0;38;2;60;56;54;48;2;235;219;178m--- [0;38;2;60;56;54;48;2;213;196;161ma/a.el[0m
                        [0;38;2;60;56;54;48;2;235;219;178m+++ [0;38;2;60;56;54;48;2;213;196;161mb/a.el[0m
                        [0;38;2;60;56;54;48;2;213;196;161m@@ -1 +1 @@[0m
                        [0;38;2;157;0;6;48;2;251;241;199m-([0;38;2;60;56;54;48;2;204;36;29mold[0;38;2;157;0;6;48;2;251;241;199m)[0m
                        [0;38;2;121;116;14;48;2;251;241;199m+([0;38;2;60;56;54;48;2;152;151;26mnew[0;38;2;121;116;14;48;2;251;241;199m)[0m
                    "#]],
                    light_properties: expect_file!["snapshots/light_properties.txt"],
                    light_state: expect_file!["snapshots/light_state.txt"],
                },
                mismatches,
            );
            exercise_stack_rendering(
                pair,
                &ordinary,
                StackRenderingExpectations {
                    light_elisp: expect![[r#"
                        [0;38;2;168;153;132;48;2;251;241;199m; comment Ω[0m
                        [0;38;2;60;56;54;48;2;251;241;199m([0;38;2;157;0;6;48;2;251;241;199mdefun[0;38;2;60;56;54;48;2;251;241;199m [0;38;2;181;118;20;48;2;251;241;199mgreet[0;38;2;60;56;54;48;2;251;241;199m (name)[0m
                        [0;38;2;60;56;54;48;2;251;241;199m  [0;38;2;121;116;14;48;2;251;241;199m"Doc."[0m
                        [0;38;2;60;56;54;48;2;251;241;199m  ([0;38;2;157;0;6;48;2;251;241;199mif[0;38;2;60;56;54;48;2;251;241;199m name (message [0;38;2;121;116;14;48;2;251;241;199m"Hello %s"[0;38;2;60;56;54;48;2;251;241;199m name) nil))[0m
                    "#]],
                    light_org: expect![[r#"
                        [0;38;2;168;153;132;48;2;251;241;199m#+title:[0;38;2;60;56;54;48;2;251;241;199m [0;38;2;69;133;136;48;2;251;241;199mPlan Ω[0m
                        [0;38;2;7;102;120;48;2;251;241;199m* [0;1;38;2;157;0;6;48;2;251;241;199mTODO[0;38;2;7;102;120;48;2;251;241;199m Ship release[0m
                        [0;38;2;181;118;20;48;2;251;241;199m** [0;1;38;2;66;123;88;48;2;251;241;199mDONE[0;38;2;181;118;20;48;2;251;241;199m [0;38;2;66;123;88;48;2;251;241;199mVerify rollback[0m
                        [0;38;2;60;56;54;48;2;251;241;199mA [0;4;38;2;104;157;106;48;2;251;241;199mlink[0;38;2;60;56;54;48;2;251;241;199m and [0;38;2;168;153;132;48;2;251;241;199m=code=[0;38;2;60;56;54;48;2;251;241;199m.[0m
                        [0;38;2;60;56;54;48;2;235;219;178m#+begin_src emacs-lisp[0m
                        [0;38;2;60;56;54;48;2;242;229;188m(message [0;38;2;121;116;14;48;2;242;229;188m"ship"[0;38;2;60;56;54;48;2;242;229;188m)[0m
                        [0;38;2;60;56;54;48;2;235;219;178m#+end_src[0m
                    "#]],
                    light_diff: expect![[r#"
                        [0;38;2;60;56;54;48;2;235;219;178mdiff --git a/a.el b/a.el[0m
                        [0;38;2;60;56;54;48;2;235;219;178m--- [0;38;2;60;56;54;48;2;213;196;161ma/a.el[0m
                        [0;38;2;60;56;54;48;2;235;219;178m+++ [0;38;2;60;56;54;48;2;213;196;161mb/a.el[0m
                        [0;38;2;60;56;54;48;2;213;196;161m@@ -1 +1 @@[0m
                        [0;38;2;157;0;6;48;2;251;241;199m-([0;38;2;60;56;54;48;2;204;36;29mold[0;38;2;157;0;6;48;2;251;241;199m)[0m
                        [0;38;2;121;116;14;48;2;251;241;199m+([0;38;2;60;56;54;48;2;152;151;26mnew[0;38;2;121;116;14;48;2;251;241;199m)[0m
                    "#]],
                    light_properties: expect_file!["snapshots/truecolor_light_properties.txt"],
                    light_state: expect_file!["snapshots/truecolor_light_state.txt"],
                    restored_elisp: expect![[r#"
                        [0;38;2;124;111;100;48;2;40;40;40m; comment Ω[0m
                        [0;38;2;235;219;178;48;2;40;40;40m([0;38;2;251;73;51;48;2;40;40;40mdefun[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;250;189;47;48;2;40;40;40mgreet[0;38;2;235;219;178;48;2;40;40;40m (name)[0m
                        [0;38;2;235;219;178;48;2;40;40;40m  [0;38;2;184;187;38;48;2;40;40;40m"Doc."[0m
                        [0;38;2;235;219;178;48;2;40;40;40m  ([0;38;2;251;73;51;48;2;40;40;40mif[0;38;2;235;219;178;48;2;40;40;40m name (message [0;38;2;184;187;38;48;2;40;40;40m"Hello %s"[0;38;2;235;219;178;48;2;40;40;40m name) nil))[0m
                    "#]],
                    restored_org: expect![[r#"
                        [0;38;2;124;111;100;48;2;40;40;40m#+title:[0;38;2;235;219;178;48;2;40;40;40m [0;38;2;69;133;136;48;2;40;40;40mPlan Ω[0m
                        [0;38;2;131;165;152;48;2;40;40;40m* [0;1;38;2;251;73;51;48;2;40;40;40mTODO[0;38;2;131;165;152;48;2;40;40;40m Ship release[0m
                        [0;38;2;250;189;47;48;2;40;40;40m** [0;1;38;2;142;192;124;48;2;40;40;40mDONE[0;38;2;250;189;47;48;2;40;40;40m [0;38;2;142;192;124;48;2;40;40;40mVerify rollback[0m
                        [0;38;2;235;219;178;48;2;40;40;40mA [0;4;38;2;104;157;106;48;2;40;40;40mlink[0;38;2;235;219;178;48;2;40;40;40m and [0;38;2;124;111;100;48;2;40;40;40m=code=[0;38;2;235;219;178;48;2;40;40;40m.[0m
                        [0;38;2;235;219;178;48;2;60;56;54m#+begin_src emacs-lisp[0m
                        [0;38;2;235;219;178;48;2;50;48;47m(message [0;38;2;184;187;38;48;2;50;48;47m"ship"[0;38;2;235;219;178;48;2;50;48;47m)[0m
                        [0;38;2;235;219;178;48;2;60;56;54m#+end_src[0m
                    "#]],
                    restored_diff: expect![[r#"
                        [0;38;2;235;219;178;48;2;60;56;54mdiff --git a/a.el b/a.el[0m
                        [0;38;2;235;219;178;48;2;60;56;54m--- [0;38;2;235;219;178;48;2;80;73;69ma/a.el[0m
                        [0;38;2;235;219;178;48;2;60;56;54m+++ [0;38;2;235;219;178;48;2;80;73;69mb/a.el[0m
                        [0;38;2;235;219;178;48;2;80;73;69m@@ -1 +1 @@[0m
                        [0;38;2;251;73;51;48;2;40;40;40m-([0;38;2;235;219;178;48;2;204;36;29mold[0;38;2;251;73;51;48;2;40;40;40m)[0m
                        [0;38;2;184;187;38;48;2;40;40;40m+([0;38;2;235;219;178;48;2;152;151;26mnew[0;38;2;184;187;38;48;2;40;40;40m)[0m
                    "#]],
                    restored_properties: expect_file!["snapshots/restored_properties.txt"],
                    restored_state: expect_file!["snapshots/restored_state.txt"],
                },
                mismatches,
            );
            invoke_both(pair, "gt357-show-bold-cycle", "GRUVBOX-BOLD-READY");
            record_pair(
                pair,
                "truecolor bold reload",
                expect![[r#"
                BOLD PLAIN (normal normal)
                BOLD BEFORE-RELOAD (normal normal)
                BOLD RELOADED (bold bold)
                BOLD PLAIN-AGAIN (normal normal)
                BOLD ORG-RUN ("TODO" (org-todo org-level-1))
                GRUVBOX-BOLD-READY"#]],
                mismatches,
            );
        },
    )
}

pub(super) fn color256(packages: &PreparedPackageSet) -> Result<(), String> {
    run_profile(
        "gruvbox-theme-color256",
        packages,
        TerminalProfile::Indexed256,
        |pair, mismatches| {
            record_pair(
                pair,
                "256-color capability",
                expect![[r#"
                    CAP TERM "dumb"
                    CAP COLORTERM nil
                    CAP CELLS 256
                    CAP VISUAL static-color
                    CAP DISPLAY color
                    CAP GRAPHIC nil
                    CAP TRUECOLOR nil
                    CAP COLOR256 t
                    CAP ORG-COMPILED t
                    CAP GNUS-BEFORE nil
                    CAP LOAD-SUFFIXES (".el")
                    THEMES-KNOWN t
                    GRUVBOX-TUI-BOOT"#]],
                mismatches,
            );
            invoke_both(pair, "gt357-configure-core-org", "GRUVBOX-CORE-ORG-READY");
            record_pair(
                pair,
                "256-color core Org configuration",
                expect![[r#"
                    CORE-ORG BEFORE-1 (ol-doi ol-w3m ol-bbdb ol-bibtex ol-docview)
                    CORE-ORG BEFORE-2 (ol-gnus ol-info ol-irc ol-mhe ol-rmail ol-eww)
                    CORE-ORG AFTER nil
                    CORE-ORG GNUS (nil nil nil)
                    GRUVBOX-CORE-ORG-READY"#]],
                mismatches,
            );
            initialize_consumer(
                pair,
                "256-color core",
                expect![[r#"
                CONSUMER MODULES-1 nil
                CONSUMER MODULES-2 nil
                CONSUMER BEFORE (nil nil nil)
                CONSUMER THEME (gruvbox-dark-medium)
                CONSUMER OUTCOME (:value returned)
                CONSUMER SOURCE nil nil
                CONSUMER AFTER (nil nil nil)
                CONSUMER INHERIT nil
                CONSUMER SUFFIXES (".el")
                GRUVBOX-CONSUMER-READY"#]],
                mismatches,
            );
            record_orderless_completion(
                pair,
                expect![[r#"
                [0;1;38;5;73;48;5;235malpha[0;38;5;223;48;5;235m [0;1;38;5;166;48;5;235mb[0;38;5;223;48;5;235meta [0;1;38;5;108;48;5;235mgam[0;38;5;223;48;5;235mma [0;1;38;5;214;48;5;235mdel[0;38;5;223;48;5;235mta[0m
            "#]],
                expect![[r##"
                ORDERLESS CHOICE "alpha beta gamma delta"
                ORDERLESS FINAL-INPUT "alpha beta gamma delta"
                ORDERLESS HISTORY-HEAD "alpha beta gamma delta"
                ORDERLESS MINIBUFFER nil
                ORDERLESS RUN ("alpha" orderless-match-face-0)
                ORDERLESS RUN (" " nil)
                ORDERLESS RUN ("b" orderless-match-face-1)
                ORDERLESS RUN ("eta " nil)
                ORDERLESS RUN ("gam" orderless-match-face-2)
                ORDERLESS RUN ("ma " nil)
                ORDERLESS RUN ("del" orderless-match-face-3)
                ORDERLESS RUN ("ta" nil)
                ORDERLESS FACE 0 "#5fafaf" "#5fafaf" bold bold
                ORDERLESS FACE 1 "#d75f00" "#d75f00" bold bold
                ORDERLESS FACE 2 "#87af87" "#87af87" bold bold
                ORDERLESS FACE 3 "#ffaf00" "#ffaf00" bold bold
                GRUVBOX-ORDERLESS-READY"##]],
                mismatches,
            );
            assert_matrix(
                pair,
                "256-color seven-theme matrix",
                expect_file!["snapshots/color256_matrix.txt"],
                mismatches,
            );
            let _ordinary = exercise_rendering(
                pair,
                "256-color",
                RenderingExpectations {
                    dark_elisp: expect![[r#"
                        [0;38;5;243;48;5;235m; comment Ω[0m
                        [0;38;5;223;48;5;235m([0;38;5;167;48;5;235mdefun[0;38;5;223;48;5;235m [0;38;5;214;48;5;235mgreet[0;38;5;223;48;5;235m (name)[0m
                        [0;38;5;223;48;5;235m  [0;38;5;142;48;5;235m"Doc."[0m
                        [0;38;5;223;48;5;235m  ([0;38;5;167;48;5;235mif[0;38;5;223;48;5;235m name (message [0;38;5;142;48;5;235m"Hello %s"[0;38;5;223;48;5;235m name) nil))[0m
                    "#]],
                    dark_org: expect![[r#"
                        [0;38;5;243;48;5;235m#+title:[0;38;5;223;48;5;235m [0;38;5;109;48;5;235mPlan Ω[0m
                        [0;38;5;109;48;5;235m* [0;1;38;5;167;48;5;235mTODO[0;38;5;109;48;5;235m Ship release[0m
                        [0;38;5;214;48;5;235m** [0;1;38;5;108;48;5;235mDONE[0;38;5;214;48;5;235m [0;38;5;108;48;5;235mVerify rollback[0m
                        [0;38;5;223;48;5;235mA [0;4;38;5;108;48;5;235mlink[0;38;5;223;48;5;235m and [0;38;5;243;48;5;235m=code=[0;38;5;223;48;5;235m.[0m
                        [0;38;5;223;48;5;237m#+begin_src emacs-lisp[0m
                        [0;38;5;223;48;5;236m(message [0;38;5;142;48;5;236m"ship"[0;38;5;223;48;5;236m)[0m
                        [0;38;5;223;48;5;237m#+end_src[0m
                    "#]],
                    dark_diff: expect![[r#"
                        [0;38;5;223;48;5;237mdiff --git a/a.el b/a.el[0m
                        [0;38;5;223;48;5;237m--- [0;38;5;223;48;5;239ma/a.el[0m
                        [0;38;5;223;48;5;237m+++ [0;38;5;223;48;5;239mb/a.el[0m
                        [0;38;5;223;48;5;239m@@ -1 +1 @@[0m
                        [0;38;5;167;48;5;235m-([0;38;5;223;48;5;167mold[0;38;5;167;48;5;235m)[0m
                        [0;38;5;142;48;5;235m+([0;38;5;223;48;5;142mnew[0;38;5;142;48;5;235m)[0m
                    "#]],
                    dark_properties: expect_file!["snapshots/color256_dark_properties.txt"],
                    dark_state: expect_file!["snapshots/color256_dark_state.txt"],
                    light_elisp: expect![[r#"
                        [0;38;5;145;48;5;230m; comment Ω[0m
                        [0;38;5;237;48;5;230m([0;38;5;88;48;5;230mdefun[0;38;5;237;48;5;230m [0;38;5;136;48;5;230mgreet[0;38;5;237;48;5;230m (name)[0m
                        [0;38;5;237;48;5;230m  [0;38;5;100;48;5;230m"Doc."[0m
                        [0;38;5;237;48;5;230m  ([0;38;5;88;48;5;230mif[0;38;5;237;48;5;230m name (message [0;38;5;100;48;5;230m"Hello %s"[0;38;5;237;48;5;230m name) nil))[0m
                    "#]],
                    light_org: expect![[r#"
                        [0;38;5;145;48;5;230m#+title:[0;38;5;237;48;5;230m [0;38;5;109;48;5;230mPlan Ω[0m
                        [0;38;5;24;48;5;230m* [0;1;38;5;88;48;5;230mTODO[0;38;5;24;48;5;230m Ship release[0m
                        [0;38;5;136;48;5;230m** [0;1;38;5;66;48;5;230mDONE[0;38;5;136;48;5;230m [0;38;5;66;48;5;230mVerify rollback[0m
                        [0;38;5;237;48;5;230mA [0;4;38;5;108;48;5;230mlink[0;38;5;237;48;5;230m and [0;38;5;145;48;5;230m=code=[0;38;5;237;48;5;230m.[0m
                        [0;38;5;237;48;5;229m#+begin_src emacs-lisp[0m
                        [0;38;5;237;48;5;230m(message [0;38;5;100;48;5;230m"ship"[0;38;5;237;48;5;230m)[0m
                        [0;38;5;237;48;5;229m#+end_src[0m
                    "#]],
                    light_diff: expect![[r#"
                        [0;38;5;237;48;5;229mdiff --git a/a.el b/a.el[0m
                        [0;38;5;237;48;5;229m--- [0;38;5;237;48;5;187ma/a.el[0m
                        [0;38;5;237;48;5;229m+++ [0;38;5;237;48;5;187mb/a.el[0m
                        [0;38;5;237;48;5;187m@@ -1 +1 @@[0m
                        [0;38;5;88;48;5;230m-([0;38;5;237;48;5;167mold[0;38;5;88;48;5;230m)[0m
                        [0;38;5;100;48;5;230m+([0;38;5;237;48;5;142mnew[0;38;5;100;48;5;230m)[0m
                    "#]],
                    light_properties: expect_file!["snapshots/color256_light_properties.txt"],
                    light_state: expect_file!["snapshots/color256_light_state.txt"],
                },
                mismatches,
            );
        },
    )
}
