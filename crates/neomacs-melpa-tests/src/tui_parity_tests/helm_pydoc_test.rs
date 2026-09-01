use std::fmt::Write as _;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use expect_test::expect;
use neomacs_tui_tests::{RawTerminalSnapshot, TuiSession};

use crate::{CachedMelpaOracle, HELM_CORE_MELPA_PIN, HELM_PYDOC_MELPA_PIN};

use super::support::PackageTuiPair;

const HELM_PYDOC_TUI_PRELUDE: &str = r####"
(defun neomacs-helm-pydoc-tui-write (path contents)
  (make-directory (file-name-directory path) t)
  (with-temp-file path
    (insert contents))
  path)

(defun neomacs-helm-pydoc-tui-setup ()
  (require 'helm-pydoc)
  (let* ((root (file-name-as-directory (getenv "HOME")))
         (project (expand-file-name "release workspace/" root))
         (python (expand-file-name "venv/bin/python" project))
         (python-fixture (expand-file-name "python-fixture/" project))
         (log (expand-file-name "python-invocations.log" root))
         (source (expand-file-name "release_console.py" project))
         (module-source (expand-file-name "deploymentkit.py" project)))
    (neomacs-helm-pydoc-tui-write
     python
     (mapconcat
      #'identity
      '("#!/bin/sh"
        "case \"$1\" in"
        "  */helm-pydoc.py)"
        "    printf 'collect|%s\\n' \"${1##*/}\" >> \"$NEOMACS_HELM_PYDOC_LOG\""
        "    if [ \"${NEOMACS_HELM_PYDOC_FAIL_COLLECT:-0}\" = 1 ]; then"
        "      printf '%s\\n' 'collector unavailable' >&2"
        "      exit 19"
        "    fi"
        "    PYTHONPATH=\"$NEOMACS_HELM_PYDOC_PYTHON_FIXTURE\" exec python3 -S \"$@\""
        "    ;;"
        "  -m)"
        "    printf 'pydoc|%s|%s|%s\\n' \"$1\" \"$2\" \"$3\" >> \"$NEOMACS_HELM_PYDOC_LOG\""
        "    if [ \"${NEOMACS_HELM_PYDOC_FAIL_DOCS:-}\" = \"$3\" ]; then"
        "      printf '%s\\n' \"No Python documentation found for $3\" >&2"
        "      exit 23"
        "    fi"
        "    printf 'Help on package %s:\\n\\nNAME\\n    %s - Release deployment helpers.\\n\\nFUNCTIONS\\n    promote(release, region=\"prod\")\\n        Promote one release after policy validation.\\n' \"$3\" \"$3\""
        "    ;;"
        "  -c)"
        "    printf 'source|%s|%s\\n' \"$1\" \"$2\" >> \"$NEOMACS_HELM_PYDOC_LOG\""
        "    PYTHONPATH=\"$NEOMACS_HELM_PYDOC_PROJECT\" exec python3 -S \"$@\""
        "    ;;"
        "  *)"
        "    printf 'unexpected' >&2"
        "    exit 97"
        "    ;;"
        "esac"
        "")
      "\n"))
    (set-file-modes python #o755)
    (neomacs-helm-pydoc-tui-write
     (expand-file-name "pkgutil.py" python-fixture)
     "def iter_modules():\n    return [(None, 'deploymentkit', False), (None, 'json', False), (None, 'analytics', False)]\n")
    (neomacs-helm-pydoc-tui-write log "")
    (neomacs-helm-pydoc-tui-write
     module-source
     "\"\"\"Release deployment helpers.\"\"\"\n\ndef promote(release, region=\"prod\"):\n    \"\"\"Promote one release after policy validation.\"\"\"\n    return release, region\n")
    (neomacs-helm-pydoc-tui-write
     source
     "# Release operations console\nimport json\nfrom os import path\n\nrelease = {\"id\": \"candidate-42\"}\n")
    (setenv "NEOMACS_HELM_PYDOC_LOG" log)
    (setenv "NEOMACS_HELM_PYDOC_PROJECT" project)
    (setenv "NEOMACS_HELM_PYDOC_PYTHON_FIXTURE" python-fixture)
    (setq helm-pydoc-virtualenv "venv"
          helm-input-idle-delay 0
          helm-candidate-number-limit 20)
    (find-file source)
    (goto-char (point-max))))

(add-hook 'emacs-startup-hook #'neomacs-helm-pydoc-tui-setup 100)
"####;

fn wait_for_both<F>(pair: &mut PackageTuiPair, timeout: Duration, predicate: F)
where
    F: Fn(&[String]) -> bool + Copy,
{
    pair.gnu.read_until(timeout, predicate);
    assert!(
        predicate(&pair.gnu.text_grid()),
        "GNU Helm Pydoc screen did not reach the expected state:\n{}",
        pair.gnu.text_grid().join("\n")
    );
    pair.neo.read_until(timeout, predicate);
    assert!(
        predicate(&pair.neo.text_grid()),
        "Neomacs Helm Pydoc screen did not reach the expected state:\n{}",
        pair.neo.text_grid().join("\n")
    );
}

fn send_to_both<F>(pair: &mut PackageTuiPair, operation: F)
where
    F: Fn(&mut TuiSession),
{
    operation(&mut pair.gnu);
    operation(&mut pair.neo);
}

fn open_pydoc(pair: &mut PackageTuiPair) {
    send_to_both(pair, |session| {
        session.send_key("M-x");
        session.send(b"helm-pydoc");
        session.send_key("RET");
    });
    wait_for_both(pair, Duration::from_secs(12), |grid| {
        grid.iter().any(|row| row.contains("Imported Modules"))
            && grid.iter().any(|row| row.contains("Installed Modules"))
    });
}

fn filter_module(pair: &mut PackageTuiPair, module: &str) {
    send_to_both(pair, |session| session.send(module.as_bytes()));
    wait_for_both(pair, Duration::from_secs(12), |grid| {
        grid.iter()
            .any(|row| row.contains("pattern:") && row.contains(module))
            && grid.iter().any(|row| row.trim_start().starts_with(module))
    });
}

fn open_and_filter_module(pair: &mut PackageTuiPair, module: &str) {
    open_pydoc(pair);
    filter_module(pair, module);
}

fn open_action_menu(pair: &mut PackageTuiPair) {
    send_to_both(pair, |session| session.send_key("TAB"));
    wait_for_both(pair, Duration::from_secs(8), |grid| {
        grid.iter().any(|row| row.contains("Pydoc Module"))
            && grid.iter().any(|row| row.contains("View Source Code"))
            && grid
                .iter()
                .any(|row| row.contains("Import Module(from module import identifiers as name)"))
    });
}

fn matching_rows(session: &TuiSession, needles: &[&str]) -> String {
    let mut output = String::new();
    for (row, contents) in session.text_grid().iter().enumerate() {
        if needles.iter().any(|needle| contents.contains(needle)) {
            let _ = writeln!(&mut output, "{row:02} |{}", contents.trim_end());
        }
    }
    output
}

fn matching_indices(session: &TuiSession, needles: &[&str]) -> Vec<u16> {
    session
        .text_grid()
        .iter()
        .enumerate()
        .filter_map(|(row, contents)| {
            needles
                .iter()
                .any(|needle| contents.contains(needle))
                .then_some(row as u16)
        })
        .collect()
}

fn exact_rows(session: &TuiSession, labels: &[&str]) -> String {
    let mut output = String::new();
    for (row, contents) in session.text_grid().iter().enumerate() {
        if labels.contains(&contents.trim()) {
            let _ = writeln!(&mut output, "{row:02} |{}", contents.trim_end());
        }
    }
    output
}

fn assert_exact_rows_stage(
    pair: &PackageTuiPair,
    stage: &str,
    labels: &[&str],
    expected_rows: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let gnu_rows = exact_rows(&pair.gnu, labels);
    let neo_rows = exact_rows(&pair.neo, labels);
    expected_rows.assert_eq(&gnu_rows);
    if neo_rows != gnu_rows {
        divergences.push(format!(
            "{stage} exact rows differ:\nGNU:\n{gnu_rows}\nNeomacs:\n{neo_rows}"
        ));
    }
}

fn assert_stage(
    pair: &PackageTuiPair,
    stage: &str,
    needles: &[&str],
    expected_rows: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let gnu_rows = matching_rows(&pair.gnu, needles);
    let neo_rows = matching_rows(&pair.neo, needles);
    expected_rows.assert_eq(&gnu_rows);
    if neo_rows != gnu_rows {
        divergences.push(format!(
            "{stage} semantic rows differ:\nGNU:\n{gnu_rows}\nNeomacs:\n{neo_rows}"
        ));
    }

    let gnu_indices = matching_indices(&pair.gnu, needles);
    let neo_indices = matching_indices(&pair.neo, needles);
    if neo_indices != gnu_indices {
        divergences.push(format!(
            "{stage} row indices differ: GNU {gnu_indices:?}, Neomacs {neo_indices:?}"
        ));
    }
    assert!(
        !gnu_indices.is_empty(),
        "at least one meaningful terminal row"
    );
    for row in gnu_indices {
        let gnu_snapshot = RawTerminalSnapshot::capture_rows(pair.gnu.screen(), row..row + 1);
        let neo_snapshot = RawTerminalSnapshot::capture_rows(pair.neo.screen(), row..row + 1);
        if gnu_snapshot != neo_snapshot {
            divergences.push(format!(
                "{stage} raw terminal row {row} differs:\nGNU:\n{}Neomacs:\n{}",
                gnu_snapshot.plain_grid(),
                neo_snapshot.plain_grid()
            ));
        }
    }
}

fn assert_release_console(
    pair: &PackageTuiPair,
    stage: &str,
    expected: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let relative = "release workspace/release_console.py";
    let gnu =
        fs::read_to_string(pair.gnu.home_dir().join(relative)).expect("read GNU release console");
    let neo = fs::read_to_string(pair.neo.home_dir().join(relative))
        .expect("read Neomacs release console");
    expected.assert_eq(&gnu);
    if neo != gnu {
        divergences.push(format!(
            "{stage} saved source differs:\nGNU:\n{gnu}\nNeomacs:\n{neo}"
        ));
    }
}

fn save_and_wait_for_release_console(pair: &mut PackageTuiPair, expected_fragment: &str) {
    send_to_both(pair, |session| session.send_keys("C-x C-s"));
    let relative = "release workspace/release_console.py";
    let deadline = Instant::now() + Duration::from_secs(8);
    loop {
        pair.gnu.read(Duration::from_millis(10));
        pair.neo.read(Duration::from_millis(10));
        let gnu_saved = fs::read_to_string(pair.gnu.home_dir().join(relative))
            .is_ok_and(|contents| contents.contains(expected_fragment));
        let neo_saved = fs::read_to_string(pair.neo.home_dir().join(relative))
            .is_ok_and(|contents| contents.contains(expected_fragment));
        if gnu_saved && neo_saved {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "GNU and Neomacs did not save release_console.py containing {expected_fragment:?}"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

fn capture_and_assert_pydoc_buffer(
    pair: &mut PackageTuiPair,
    stage: &str,
    file_stem: &str,
    expected_contents: expect_test::Expect,
    expected_state: expect_test::Expect,
    divergences: &mut Vec<String>,
) {
    let expression = format!(
        r##"(with-current-buffer "*Pydoc deploymentkit*" (let ((state (list :point (point) :view-mode view-mode :read-only buffer-read-only :modified (buffer-modified-p)))) (write-region (point-min) (point-max) (expand-file-name "{file_stem}-buffer.txt" (getenv "HOME")) nil 'silent) (with-temp-file (expand-file-name "{file_stem}-state.txt" (getenv "HOME")) (insert (prin1-to-string state)))))"##
    );
    send_to_both(pair, |session| {
        session.send_key("M-:");
        session.send(expression.as_bytes());
        session.send_key("RET");
    });

    let deadline = Instant::now() + Duration::from_secs(8);
    let (gnu_contents, neo_contents, gnu_state, neo_state) = loop {
        pair.gnu.read(Duration::from_millis(10));
        pair.neo.read(Duration::from_millis(10));
        let files = (
            fs::read_to_string(pair.gnu.home_dir().join(format!("{file_stem}-buffer.txt"))),
            fs::read_to_string(pair.neo.home_dir().join(format!("{file_stem}-buffer.txt"))),
            fs::read_to_string(pair.gnu.home_dir().join(format!("{file_stem}-state.txt"))),
            fs::read_to_string(pair.neo.home_dir().join(format!("{file_stem}-state.txt"))),
        );
        if let (Ok(gnu_contents), Ok(neo_contents), Ok(gnu_state), Ok(neo_state)) = files {
            break (gnu_contents, neo_contents, gnu_state, neo_state);
        }
        assert!(
            Instant::now() < deadline,
            "GNU and Neomacs did not capture the Pydoc output buffer"
        );
        thread::sleep(Duration::from_millis(10));
    };

    expected_contents.assert_eq(&gnu_contents);
    expected_state.assert_eq(&gnu_state);
    if neo_contents != gnu_contents {
        divergences.push(format!(
            "{stage} Pydoc output buffer differs:\nGNU:\n{gnu_contents}\nNeomacs:\n{neo_contents}"
        ));
    }
    if neo_state != gnu_state {
        divergences.push(format!(
            "{stage} Pydoc buffer state differs: GNU {gnu_state:?}, Neomacs {neo_state:?}"
        ));
    }
}

#[test]
fn helm_pydoc_real_helm_workflows_match_gnu_terminal_and_filesystem() {
    let oracle = CachedMelpaOracle::new(HELM_PYDOC_MELPA_PIN, "helm-pydoc.el")
        .expect("prepare revision-pinned Helm Pydoc source")
        .with_melpa_dependency(HELM_CORE_MELPA_PIN)
        .expect("prepare exact Helm Core dependency")
        .with_prelude(HELM_PYDOC_TUI_PRELUDE);
    let mut pair = PackageTuiPair::spawn("helm-pydoc-workflows", oracle.prepared_packages())
        .expect("spawn Helm Pydoc package TUI pair");
    let mut divergences = Vec::new();

    wait_for_both(&mut pair, Duration::from_secs(20), |grid| {
        grid.iter().any(|row| row.contains("candidate-42"))
    });

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
            48 |-UU-:--- F1  release_console.py   All   L6     (Python ElDoc) -----------------|-UUU:%*- F1  *Pydoc deploymentkit*   All   L1     (Fundamental View) -----------
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
            48 |-UU-:--- F1  release_console.py   All   L6     (Python ElDoc) -----------------|-UU-:%%- F1  deploymentkit.py   All   L1     (Python ElDoc) --------------------
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
