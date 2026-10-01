use std::fs;
use std::time::Duration;

use expect_test::expect;

use super::{CachedMelpaOracle, HELM_CORE_MELPA_PIN, HELM_PYDOC_MELPA_PIN};

use super::scenario::{PackageTuiScenario, PairTimeout, ReadinessCheckpoint};

mod harness;
mod prelude;

use harness::*;
use prelude::*;

#[test]
fn helm_pydoc_real_helm_workflows_match_gnu_terminal_and_filesystem() {
    let oracle = CachedMelpaOracle::new(HELM_PYDOC_MELPA_PIN, "helm-pydoc.el")
        .expect("prepare revision-pinned Helm Pydoc source")
        .with_melpa_dependency(HELM_CORE_MELPA_PIN)
        .expect("prepare exact Helm Core dependency")
        .with_prelude(HELM_PYDOC_TUI_PRELUDE);
    let mut pair = PackageTuiScenario::new("helm-pydoc-workflows", oracle.prepared_packages())
        .spawn_when_ready(
            ReadinessCheckpoint::new(
                "Python module fixture",
                PairTimeout::same(Duration::from_secs(20)),
            ),
            |grid| grid.iter().any(|row| row.contains("candidate-42")),
        )
        .expect("spawn ready Helm Pydoc package TUI pair");
    let mut divergences = Vec::new();

    open_pydoc(&mut pair);
    assert_stage(
        &pair,
        "imported and installed module sources",
        &["Imported Modules", "json", "Installed Modules"],
        expect![[r#"
            02 |import json
            26 |Imported Modules
            27 |json
            30 |Installed Modules
            33 |json
        "#]],
        &mut divergences,
    );
    assert_exact_rows_stage(
        &pair,
        "imported and installed candidate membership",
        &[
            "Imported Modules",
            "json",
            "os",
            "Installed Modules",
            "analytics",
            "deploymentkit",
            "sys",
        ],
        expect![[r#"
            26 |Imported Modules
            27 |json
            28 |os
            30 |Installed Modules
            31 |analytics
            32 |deploymentkit
            33 |json
            34 |sys
        "#]],
        &mut divergences,
    );
    filter_module(&mut pair, "deploymentkit");

    assert_stage(
        &pair,
        "filtered module selection",
        &["pattern:", "Installed Modules", "deploymentkit"],
        expect![[r#"
            26 |Installed Modules
            27 |deploymentkit
            49 |pattern: deploymentkit
        "#]],
        &mut divergences,
    );

    send_to_both(&mut pair, |session| session.send_key("RET"));
    wait_for_both(&mut pair, Duration::from_secs(12), |grid| {
        grid.iter()
            .any(|row| row.contains("Help on package deploymentkit"))
            && grid
                .iter()
                .any(|row| row.contains("Promote one release after policy validation"))
    });

    assert_stage(
        &pair,
        "opened Python documentation",
        &[
            "Help on package deploymentkit",
            "deploymentkit - Release deployment helpers",
            "promote(release, region=\"prod\")",
            "Promote one release after policy validation",
            "*Pydoc deploymentkit*",
        ],
        expect![[r#"
            01 |# Release operations console                                                   |Help on package deploymentkit:
            04 |                                                                               |    deploymentkit - Release deployment helpers.
            07 |                                                                               |    promote(release, region="prod")
            08 |                                                                               |        Promote one release after policy validation.
            48 |-UU-:--- F1  release_console.py   All L6     (Python ElDoc) -------------------|-UUU:%*- F1  *Pydoc deploymentkit*   All L1     (Fundamental View) -------------
        "#]],
        &mut divergences,
    );
    capture_and_assert_pydoc_buffer(
        &mut pair,
        "successful documentation lookup",
        "successful-pydoc",
        expect![[r#"
            Help on package deploymentkit:

            NAME
                deploymentkit - Release deployment helpers.

            FUNCTIONS
                promote(release, region="prod")
                    Promote one release after policy validation.
        "#]],
        expect![r#"(:point 1 :view-mode t :read-only t :modified t)"#],
        &mut divergences,
    );

    send_to_both(&mut pair, |session| session.send_key("q"));
    wait_for_both(&mut pair, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("candidate-42"))
            && !grid
                .iter()
                .any(|row| row.contains("Help on package deploymentkit"))
    });

    open_and_filter_module(&mut pair, "deploymentkit");
    open_action_menu(&mut pair);
    assert_overlay_introspection(&pair, "action selection", &mut divergences);
    assert_stage(
        &pair,
        "action selection",
        &[
            "Pydoc Module",
            "View Source Code",
            "Import Module(import module)",
            "Import Module(from module import identifiers)",
            "Import Module(from module import identifiers as name)",
        ],
        expect![[r#"
            25 | C-j: DoNothing (keeping session)                                              | C-j: Pydoc Module (keeping session)
            27 |[f1]  Pydoc Module                                                             |deploymentkit
            28 |[f2]  View Source Code                                                         |
            29 |[f3]  Import Module(import module)                                             |
            30 |[f4]  Import Module(from module import identifiers)                            |
            31 |[f5]  Import Module(from module import identifiers as name)                    |
        "#]],
        &mut divergences,
    );

    send_to_both(&mut pair, |session| session.send_keys("C-n RET"));
    wait_for_both(&mut pair, Duration::from_secs(12), |grid| {
        grid.iter()
            .any(|row| row.contains("Release deployment helpers"))
            && grid
                .iter()
                .any(|row| row.contains("def promote(release, region=\"prod\")"))
            && grid.iter().any(|row| row.contains("deploymentkit.py"))
    });
    assert_stage(
        &pair,
        "read-only module source",
        &[
            "Release deployment helpers",
            "def promote(release, region=\"prod\")",
            "Promote one release after policy validation",
            "return release, region",
            "deploymentkit.py",
        ],
        expect![[r#"
            01 |# Release operations console                                                   |"""Release deployment helpers."""
            03 |from os import path                                                            |def promote(release, region="prod"):
            04 |                                                                               |    """Promote one release after policy validation."""
            05 |release = {"id": "candidate-42"}                                               |    return release, region
            48 |-UU-:--- F1  release_console.py   All L6     (Python ElDoc) -------------------|-UU-:%%- F1  deploymentkit.py   All L1     (Python ElDoc) ----------------------
        "#]],
        &mut divergences,
    );

    send_to_both(&mut pair, |session| session.send_keys("C-x k RET C-x 1"));
    wait_for_both(&mut pair, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("candidate-42"))
            && !grid.iter().any(|row| row.contains("deploymentkit.py"))
    });

    open_pydoc(&mut pair);
    send_to_both(&mut pair, |session| session.send(b"t"));
    wait_for_both(&mut pair, Duration::from_secs(12), |grid| {
        grid.iter()
            .any(|row| row.contains("pattern:") && row.contains('t'))
            && grid.iter().any(|row| row.contains("analytics"))
            && grid.iter().any(|row| row.contains("deploymentkit"))
    });
    send_to_both(&mut pair, |session| session.send_keys("C-SPC C-SPC"));
    open_action_menu(&mut pair);
    send_to_both(&mut pair, |session| session.send_keys("C-n C-n RET"));
    wait_for_both(&mut pair, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("import analytics"))
            && grid.iter().any(|row| row.contains("import deploymentkit"))
            && grid.iter().any(|row| row.contains("release_console.py"))
    });
    save_and_wait_for_release_console(&mut pair, "import deploymentkit\n");
    assert_release_console(
        &pair,
        "plain module import",
        expect![[r#"
            # Release operations console
            import json
            from os import path
            import analytics
            import deploymentkit
            release = {"id": "candidate-42"}
        "#]],
        &mut divergences,
    );

    open_and_filter_module(&mut pair, "json");
    open_action_menu(&mut pair);
    send_to_both(&mut pair, |session| session.send_keys("C-n C-n RET"));
    wait_for_both(&mut pair, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("import nil"))
            && grid.iter().any(|row| row.contains("release_console.py"))
    });
    save_and_wait_for_release_console(&mut pair, "import nil\n");
    assert_release_console(
        &pair,
        "already-imported module",
        expect![[r#"
            # Release operations console
            import json
            from os import path
            import analytics
            import deploymentkit
            import nil
            release = {"id": "candidate-42"}
        "#]],
        &mut divergences,
    );

    open_and_filter_module(&mut pair, "deploymentkit");
    open_action_menu(&mut pair);
    send_to_both(&mut pair, |session| session.send_keys("C-n C-n C-n RET"));
    wait_for_both(&mut pair, Duration::from_secs(8), |grid| {
        grid.iter()
            .any(|row| row.contains("Identifiers in deploymentkit:"))
    });
    send_to_both(&mut pair, |session| {
        session.send(b"promote, rollback");
        session.send_key("RET");
    });
    wait_for_both(&mut pair, Duration::from_secs(8), |grid| {
        grid.iter()
            .any(|row| row.contains("from deploymentkit import promote, rollback"))
    });
    save_and_wait_for_release_console(&mut pair, "from deploymentkit import promote, rollback\n");
    assert_release_console(
        &pair,
        "from-module import",
        expect![[r#"
            # Release operations console
            import json
            from os import path
            import analytics
            import deploymentkit
            import nil
            from deploymentkit import promote, rollback
            release = {"id": "candidate-42"}
        "#]],
        &mut divergences,
    );

    open_and_filter_module(&mut pair, "deploymentkit");
    open_action_menu(&mut pair);
    send_to_both(&mut pair, |session| {
        session.send_keys("C-n C-n C-n C-n RET");
    });
    wait_for_both(&mut pair, Duration::from_secs(8), |grid| {
        grid.iter()
            .any(|row| row.contains("Identifiers in deploymentkit:"))
    });
    send_to_both(&mut pair, |session| {
        session.send(b"promote");
        session.send_key("RET");
    });
    wait_for_both(&mut pair, Duration::from_secs(8), |grid| {
        grid.iter()
            .any(|row| row.contains("As name [deploymentkit]:"))
    });
    send_to_both(&mut pair, |session| {
        session.send(b"dk");
        session.send_key("RET");
    });
    wait_for_both(&mut pair, Duration::from_secs(8), |grid| {
        grid.iter()
            .any(|row| row.contains("from deploymentkit import promote as name"))
    });
    save_and_wait_for_release_console(&mut pair, "from deploymentkit import promote as name\n");
    assert_release_console(
        &pair,
        "aliased import",
        expect![[r#"
            # Release operations console
            import json
            from os import path
            import analytics
            import deploymentkit
            import nil
            from deploymentkit import promote, rollback
            from deploymentkit import promote as name
            release = {"id": "candidate-42"}
        "#]],
        &mut divergences,
    );

    send_to_both(&mut pair, |session| {
        session.send_key("M-:");
        session.send(br#"(setenv "NEOMACS_HELM_PYDOC_FAIL_DOCS" "deploymentkit")"#);
        session.send_key("RET");
    });
    open_and_filter_module(&mut pair, "deploymentkit");
    send_to_both(&mut pair, |session| session.send_key("RET"));
    wait_for_both(&mut pair, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("Failed:"))
    });
    assert_stage(
        &pair,
        "failed documentation lookup",
        &["Failed:"],
        expect![[r#"
            49 |Failed: ’pydoc’
        "#]],
        &mut divergences,
    );
    capture_and_assert_pydoc_buffer(
        &mut pair,
        "failed documentation lookup",
        "failed-pydoc",
        expect![[r#"
            No Python documentation found for deploymentkit
        "#]],
        expect![r#"(:point 49 :view-mode nil :read-only nil :modified t)"#],
        &mut divergences,
    );

    let gnu_log = fs::read_to_string(pair.gnu.home_dir().join("python-invocations.log"))
        .expect("read GNU fake-Python transcript");
    let neo_log = fs::read_to_string(pair.neo.home_dir().join("python-invocations.log"))
        .expect("read Neomacs fake-Python transcript");
    expect![[r#"
        collect|helm-pydoc.py
        pydoc|-m|pydoc|deploymentkit
        collect|helm-pydoc.py
        source|-c|import deploymentkit;print(deploymentkit.__file__)
        collect|helm-pydoc.py
        collect|helm-pydoc.py
        collect|helm-pydoc.py
        collect|helm-pydoc.py
        collect|helm-pydoc.py
        pydoc|-m|pydoc|deploymentkit
    "#]]
    .assert_eq(&gnu_log);
    if neo_log != gnu_log {
        divergences.push(format!(
            "Python argv transcript differs:\nGNU:\n{gnu_log}\nNeomacs:\n{neo_log}"
        ));
    }
    assert!(
        divergences.is_empty(),
        "Helm Pydoc GNU/Neomacs divergences:\n{}",
        divergences.join("\n\n")
    );
}

/// The live helm ingredient my outside-session probes never replicated:
/// the MINIBUFFER is active while the overlay renders (helm reads events
/// from the miniwindow).  This probe arms an overlay in a side window
/// during minibuffer-setup, holds the minibuffer open, and captures both
/// engines' screens in that state.
#[test]
fn overlay_face_renders_with_the_minibuffer_active() {
    let oracle = CachedMelpaOracle::new(HELM_PYDOC_MELPA_PIN, "helm-pydoc.el")
        .expect("prepare revision-pinned Helm Pydoc source")
        .with_melpa_dependency(HELM_CORE_MELPA_PIN)
        .expect("prepare exact Helm Core dependency")
        .with_prelude(HELM_PYDOC_TUI_PRELUDE);
    let mut pair =
        PackageTuiScenario::new("helm-minibuf-overlay-probe", oracle.prepared_packages())
            .spawn_when_ready(
                ReadinessCheckpoint::new(
                    "Python module fixture",
                    PairTimeout::same(Duration::from_secs(20)),
                ),
                |grid| grid.iter().any(|row| row.contains("candidate-42")),
            )
            .expect("spawn ready Helm Pydoc package TUI pair");

    // Open the side window with the overlay, then enter the minibuffer.
    // Long expressions go through M-: + bracketed paste (send_keys strips
    // spaces, which destroys elisp).  The overlay carries the real
    // helm-selection face, the ingredient the helm-pydoc screens render.
    let setup_form = r#"(progn (switch-to-buffer (get-buffer-create "probe-faces")) (fundamental-mode) (erase-buffer) (insert "SELECTED CANDIDATE") (let ((ov (make-overlay (point-min) (point-max) (current-buffer) t nil))) (overlay-put ov 'face 'helm-selection)) (delete-other-windows) (split-window-right) (other-window 1) (set-window-start (selected-window) (point-min) t))"#;
    let mini_form = r#"(minibuffer-with-setup-hook (lambda () (redisplay)) (read-from-minibuffer "MINIPROBE: "))"#;
    send_to_both(&mut pair, |session| {
        session.send_key("M-:");
        session.read_until(Duration::from_secs(8), |grid| {
            grid.iter().any(|row| row.contains("Eval:"))
        });
        session.paste(setup_form);
        session.send_key("RET");
    });
    wait_for_both(&mut pair, Duration::from_secs(10), |grid| {
        grid.iter().any(|row| row.contains("SELECTED CANDIDATE"))
    });
    send_to_both(&mut pair, |session| {
        session.send_key("M-:");
        session.read_until(Duration::from_secs(8), |grid| {
            grid.iter().any(|row| row.contains("Eval:"))
        });
        session.paste(mini_form);
        session.send_key("RET");
    });
    let mini_ready = |grid: &[String]| grid.iter().any(|row| row.contains("MINIPROBE"));
    wait_for_both(&mut pair, Duration::from_secs(10), mini_ready);
    std::thread::sleep(Duration::from_millis(500));
    // The probe state must actually be on screen: the overlay text in the
    // side window AND the open minibuffer.  A blank or debugger-poisoned
    // screen would otherwise satisfy the pairwise comparison below.
    for (label, session) in [("GNU", &pair.gnu), ("NEO", &pair.neo)] {
        let grid = session.text_grid();
        assert!(
            grid.iter().any(|row| row.contains("SELECTED CANDIDATE")),
            "{label} side window must show the overlay text"
        );
        assert!(mini_ready(&grid), "{label} minibuffer must stay open");
    }
    // Pairwise parity is the contract: GNU and Neomacs must render the
    // minibuffer-active state identically, styled cells included.
    let report = neomacs_tui_tests::compare_session_displays(&pair.gnu, &pair.neo);
    assert!(
        report.is_satisfied(),
        "minibuffer-active state must render identically:\n{report:#?}"
    );
    send_to_both(&mut pair, |session| session.send_key("RET"));
}
