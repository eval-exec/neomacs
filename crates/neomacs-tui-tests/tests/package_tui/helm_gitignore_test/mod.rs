use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use expect_test::{expect, expect_file};

mod harness;
mod prelude;

use harness::*;
use prelude::*;

/// The failure bound every checkpoint in this file waits against.
///
/// Checkpoints exit the moment the editor *reaches* the stage's observable
/// state, so the bound is paid only by runs that are already failing; what
/// it must exceed is the worst-case scheduling lag of a loaded runner.  A
/// full-suite nextest run keeps ~24 editor processes and GNU oracles alive
/// at once, and under that load one lagging frame legitimately overshoots
/// the old 5-15s bounds: the harness observed Neomacs reach every later
/// stage exactly, only after the "list HTTP failure" bound had burned.
/// Generous bounds make that slow-but-correct case pass instead of flake;
/// they cost nothing when nothing is wrong.

#[test]
fn helm_gitignore_public_workflows_match_gnu() {
    let mut divergences = Vec::new();
    let mut pair = ready_helm_gitignore_pair("helm-gitignore-workflows");

    // A blank pattern is not a service request: the source requires one input
    // character before it contacts the dropdown API.
    open_helm_gitignore(&mut pair, &mut divergences);
    send_to_both(&mut pair, |session| session.send_key("C-g"));
    let gnu = capture_editor(&mut pair.gnu, "blank-pattern", "GNU");
    let neo = capture_editor(&mut pair.neo, "blank-pattern", "Neomacs");
    assert_gnu_literal_and_parity(
        "blank pattern",
        &gnu,
        Some(&neo),
        expect![
            "(:buffer nil :requests nil :request-urls nil :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"
        ],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" visual-list) '("/api/visualstudiocode" legacy-redirect) '("/developers/gitignore/api/visualstudiocode" vscode))"#,
    );

    // Select the human-facing “Visual Studio Code” row.  The server transcript
    // proves that Helm handed the distinct `visualstudiocode' ID to Request.
    navigate_to_visual_studio_code(&mut pair, "single selection", &mut divergences);
    send_to_both(&mut pair, |session| session.send_key("RET"));
    let (gnu, neo) = capture_reached_stage(
        &mut pair,
        "single-selection",
        STAGE_TIMEOUT,
        |grid| {
            grid.iter().any(|row| {
                row.contains("# Created by https://www.toptal.com/developers/gitignore/api/visual")
            }) && grid.iter().any(|row| row.contains("*gitignore*"))
        },
        &mut divergences,
    );
    assert_gnu_literal_and_parity(
        "single selection",
        &gnu,
        neo.as_deref(),
        expect_file!["snapshots/single_selection.txt"],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // Ordered multi-selection replaces the package's existing generated
    // buffer, matching the public workflow used to regenerate a project file.
    eval_both(
        &mut pair,
        "(neomacs-helm-gitignore-tui-seed-unsaved-buffer)",
    );
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=linux" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=linux" linux-list) '("/api/linux,archlinuxpackages" legacy-redirect) '("/developers/gitignore/api/linux,archlinuxpackages" linux-archlinuxpackages))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    type_query_and_wait(&mut pair, "linux", "ArchLinuxPackages", &mut divergences);
    assert_helm_semantic_snapshot(
        &pair,
        "multi-selection candidates",
        "linux",
        &["Linux", "ArchLinuxPackages"],
        expect![[
            r#"(:pattern "linux" :source "gitignore.io" :candidates ((nil "Linux") (nil "ArchLinuxPackages")) :candidate-count 2 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_keys("C-SPC C-SPC"));
    wait_for_progress(
        &mut pair,
        "ordered multi-selection marks",
        STAGE_TIMEOUT,
        |grid| {
            grid.iter().any(|row| {
                row.contains("*helm-gitignore*") && row.contains(" L2 ") && row.contains(" M2 ")
            })
        },
        &mut divergences,
    );
    assert_helm_semantic_snapshot(
        &pair,
        "multi-selection marks",
        "linux",
        &["Linux", "ArchLinuxPackages"],
        expect![[
            r#"(:pattern "linux" :source "gitignore.io" :candidates ((t "Linux") (t "ArchLinuxPackages")) :candidate-count 2 :selected-index 2 :marked-count 2)"#
        ]],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_key("RET"));
    let (gnu, neo) = capture_reached_stage(
        &mut pair,
        "ordered-multi-selection",
        STAGE_TIMEOUT,
        |grid| {
            grid.iter().any(|row| {
                row.contains("# Created by https://www.toptal.com/developers/gitignore/api/linux")
            }) && grid.iter().any(|row| row.contains("*gitignore*"))
        },
        &mut divergences,
    );
    assert_gnu_literal_and_parity(
        "ordered multi-selection",
        &gnu,
        neo.as_deref(),
        expect_file!["snapshots/ordered_multi_selection.txt"],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // Refine a live session from one real service result to another. The
    // second snapshot's exact total and rows prove that Helm dropped every
    // Python candidate instead of appending the Visual results to stale UI.
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=python" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=python" python-list) '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" visual-list))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    type_query_and_wait(&mut pair, "python", "PythonVanilla", &mut divergences);
    assert_helm_semantic_snapshot(
        &pair,
        "live refinement Python candidates",
        "python",
        &["Python", "CircuitPython", "PythonVanilla"],
        expect![[
            r#"(:pattern "python" :source "gitignore.io" :candidates ((nil "Python") (nil "CircuitPython") (nil "PythonVanilla")) :candidate-count 3 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_keys("C-a C-k"));
    type_query_and_wait(
        &mut pair,
        "visual",
        "OpenFrameworks+VisualStudio",
        &mut divergences,
    );
    assert_helm_semantic_snapshot(
        &pair,
        "live refinement Visual candidates",
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
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_key("C-g"));
    let gnu = capture_editor(&mut pair.gnu, "live-refinement", "GNU");
    let neo = capture_editor(&mut pair.neo, "live-refinement", "Neomacs");
    assert_gnu_literal_and_parity(
        "live query refinement",
        &gnu,
        Some(&neo),
        expect![[
            r#"(:buffer nil :requests ((:request-line "GET /dropdown/templates.json?term=python HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=python HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=python" "<origin>/dropdown/templates.json?term=visual") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"#
        ]],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // A response which arrives after abort populates the package-global cache
    // and is observable through a second public invocation.
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=python" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=python" held-python-list))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    send_to_both(&mut pair, |session| session.send(b"python"));

    let gnu_held =
        wait_for_state_file(&mut pair.gnu, "held-response.state", ":held-count 1", "GNU");
    expect![[r#"(:response held-python-list :request-line "GET /developers/gitignore/dropdown/templates.json?term=python HTTP/1.1" :held-count 1)"#]]
        .assert_eq(&gnu_held);
    let neo_held = wait_for_state_file(
        &mut pair.neo,
        "held-response.state",
        ":held-count 1",
        "Neomacs",
    );
    if neo_held != gnu_held {
        divergences.push(format!(
            "held Python response differs:\nGNU:\n{gnu_held}\nNeomacs:\n{neo_held}"
        ));
    }
    assert_helm_semantic_snapshot(
        &pair,
        "held Python query",
        "python",
        &[],
        expect![[
            r#"(:pattern "python" :source nil :candidates () :candidate-count 0 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );

    // Abort while Request still owns the response.  The pinned source's
    // success callback writes its package-global cache even though Helm is no
    // longer alive; only the subsequent `helm-update' is guarded.
    send_to_both(&mut pair, |session| session.send_key("C-g"));
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-release-held)");
    let gnu_late_cache =
        wait_for_state_file(&mut pair.gnu, "cache-events.state", ":count 1", "GNU");
    expect![[r#"(:count 1 :latest (("Python" . "python") ("CircuitPython" . "circuitpython") ("PythonVanilla" . "pythonvanilla")) :events ((("Python" . "python") ("CircuitPython" . "circuitpython") ("PythonVanilla" . "pythonvanilla"))))"#]]
        .assert_eq(&gnu_late_cache);
    let neo_late_cache =
        wait_for_state_file(&mut pair.neo, "cache-events.state", ":count 1", "Neomacs");
    if neo_late_cache != gnu_late_cache {
        divergences.push(format!(
            "late Python cache differs:\nGNU:\n{gnu_late_cache}\nNeomacs:\n{neo_late_cache}"
        ));
    }

    // A second public invocation consumes the late global cache.  Its new
    // `circuit' pattern deliberately has no route-plan entry: any fresh HTTP
    // query is therefore a strict fixture miss.
    open_helm_gitignore(&mut pair, &mut divergences);
    type_query_and_wait(&mut pair, "circuit", "CircuitPython", &mut divergences);
    assert_helm_semantic_snapshot(
        &pair,
        "stale cache in second session",
        "circuit",
        &["CircuitPython"],
        expect![[
            r#"(:pattern "circuit" :source "gitignore.io" :candidates ((nil "CircuitPython")) :candidate-count 1 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_key("C-g"));

    let gnu_consumed_cache =
        wait_for_state_file(&mut pair.gnu, "cache-events.state", ":count 2", "GNU");
    expect![[r#"(:count 2 :latest nil :events ((("Python" . "python") ("CircuitPython" . "circuitpython") ("PythonVanilla" . "pythonvanilla")) nil))"#]]
        .assert_eq(&gnu_consumed_cache);
    let neo_consumed_cache =
        wait_for_state_file(&mut pair.neo, "cache-events.state", ":count 2", "Neomacs");
    if neo_consumed_cache != gnu_consumed_cache {
        divergences.push(format!(
            "consumed late cache differs:\nGNU:\n{gnu_consumed_cache}\nNeomacs:\n{neo_consumed_cache}"
        ));
    }

    let gnu = capture_editor(&mut pair.gnu, "late-global-cache", "GNU");
    let neo = capture_editor(&mut pair.neo, "late-global-cache", "Neomacs");
    assert_gnu_literal_and_parity(
        "late global cache",
        &gnu,
        Some(&neo),
        expect![[
            r#"(:buffer nil :requests ((:request-line "GET /dropdown/templates.json?term=python HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=python HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=python") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"#
        ]],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // Exact empty JSON exercises nil's dual role as result and cache-miss
    // sentinel without allowing the real callback to create a request herd.
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=neomacsnomatch" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=neomacsnomatch" empty-list) '("/dropdown/templates.json?term=neomacsnomatch" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=neomacsnomatch" held-empty-list))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    send_to_both(&mut pair, |session| session.send(b"neomacsnomatch"));

    // The official dropdown's exact empty JSON decodes to nil.  Since nil is
    // also this package's cache-miss sentinel, the callback's `helm-update'
    // performs one identical retry.  Holding that retry makes the count exact
    // and prevents a test-induced request herd.
    let gnu_held =
        wait_for_state_file(&mut pair.gnu, "held-response.state", ":held-count 1", "GNU");
    expect![[r#"(:response held-empty-list :request-line "GET /developers/gitignore/dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :held-count 1)"#]]
        .assert_eq(&gnu_held);
    let neo_held = wait_for_state_file(
        &mut pair.neo,
        "held-response.state",
        ":held-count 1",
        "Neomacs",
    );
    if neo_held != gnu_held {
        divergences.push(format!(
            "held empty response differs:\nGNU:\n{gnu_held}\nNeomacs:\n{neo_held}"
        ));
    }
    assert_helm_semantic_snapshot(
        &pair,
        "empty-result retry",
        "neomacsnomatch",
        &[],
        expect![[
            r#"(:pattern "neomacsnomatch" :source nil :candidates () :candidate-count 0 :selected-index 1 :marked-count 0)"#
        ]],
        &mut divergences,
    );

    send_to_both(&mut pair, |session| session.send_key("C-g"));
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-release-held)");
    let gnu_cache_events =
        wait_for_state_file(&mut pair.gnu, "cache-events.state", ":count 2", "GNU");
    expect![[r#"(:count 2 :latest nil :events (nil nil))"#]].assert_eq(&gnu_cache_events);
    let neo_cache_events =
        wait_for_state_file(&mut pair.neo, "cache-events.state", ":count 2", "Neomacs");
    if neo_cache_events != gnu_cache_events {
        divergences.push(format!(
            "empty-result cache events differ:\nGNU:\n{gnu_cache_events}\nNeomacs:\n{neo_cache_events}"
        ));
    }

    let gnu = capture_editor(&mut pair.gnu, "empty-result", "GNU");
    let neo = capture_editor(&mut pair.neo, "empty-result", "Neomacs");
    assert_gnu_literal_and_parity(
        "empty result",
        &gnu,
        Some(&neo),
        expect![[
            r#"(:buffer nil :requests ((:request-line "GET /dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=neomacsnomatch HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=neomacsnomatch" "<origin>/dropdown/templates.json?term=neomacsnomatch") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"#
        ]],
        &mut divergences,
    );
    eval_both(&mut pair, "(neomacs-helm-gitignore-tui-reset)");

    // The generated buffer remains a normal editable gitignore-mode buffer and
    // can be saved through the real interactive write-file command.
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" visual-list) '("/api/visualstudiocode" legacy-redirect) '("/developers/gitignore/api/visualstudiocode" vscode))"#,
    );
    navigate_to_visual_studio_code(&mut pair, "save", &mut divergences);
    send_to_both(&mut pair, |session| session.send_key("RET"));
    let neo_generated = wait_for_progress(
        &mut pair,
        "generated buffer before save",
        STAGE_TIMEOUT,
        |grid| {
            grid.iter().any(|row| {
                row.contains("# Created by https://www.toptal.com/developers/gitignore/api/visual")
            }) && grid.iter().any(|row| row.contains("*gitignore*"))
        },
        &mut divergences,
    );
    let gnu_save_path = pair.gnu.home_dir().join(".gitignore");
    let neo_save_path = pair.neo.home_dir().join(".gitignore");
    pair.gnu.send_keys("M->");
    pair.gnu
        .send(b"\n# Project-local release artifacts\nrelease-output/\n");
    pair.gnu.send_keys("C-x C-w");
    pair.gnu.send(gnu_save_path.as_os_str().as_encoded_bytes());
    pair.gnu.send_key("RET");
    if neo_generated {
        pair.neo.send_keys("M->");
        pair.neo
            .send(b"\n# Project-local release artifacts\nrelease-output/\n");
        pair.neo.send_keys("C-x C-w");
        pair.neo.send(neo_save_path.as_os_str().as_encoded_bytes());
        pair.neo.send_key("RET");
    }
    let saved_relative = ".gitignore";
    let deadline = Instant::now() + STAGE_TIMEOUT;
    let gnu_file = loop {
        pair.gnu.read(Duration::from_millis(10));
        if let Ok(text) = fs::read_to_string(pair.gnu.home_dir().join(saved_relative))
            && text.ends_with("release-output/\n")
        {
            break text;
        }
        assert!(
            Instant::now() < deadline,
            "GNU did not save the edited .gitignore"
        );
        thread::yield_now();
    };
    let neo_file = if neo_generated {
        let deadline = Instant::now() + STAGE_TIMEOUT;
        loop {
            pair.neo.read(Duration::from_millis(10));
            if let Ok(text) = fs::read_to_string(pair.neo.home_dir().join(saved_relative))
                && text.ends_with("release-output/\n")
            {
                break Some(text);
            }
            if Instant::now() >= deadline {
                divergences.push("Neomacs did not save the edited .gitignore".to_string());
                break None;
            }
            thread::yield_now();
        }
    } else {
        None
    };
    assert_gnu_literal_and_parity(
        "edited and saved file",
        &gnu_file,
        neo_file.as_deref(),
        expect_file!["snapshots/edited_and_saved_file.txt"],
        &mut divergences,
    );
    let gnu = capture_editor(&mut pair.gnu, "edited-and-saved", "GNU");
    let neo = neo_generated.then(|| capture_editor(&mut pair.neo, "edited-and-saved", "Neomacs"));
    assert_gnu_literal_and_parity(
        "edited and saved buffer",
        &gnu,
        neo.as_deref(),
        expect_file!["snapshots/edited_and_saved_buffer.txt"],
        &mut divergences,
    );
    drop(pair);

    // Failure callbacks can leave asynchronous state behind, so failures use
    // scoped fresh editor pairs while still sharing this package-level test.
    let mut pair = ready_helm_gitignore_pair("helm-gitignore-list-failure");
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" connection-close))"#,
    );
    open_helm_gitignore(&mut pair, &mut divergences);
    send_to_both(&mut pair, |session| session.send(b"visual"));
    let failure = |grid: &[String]| {
        grid.iter().any(|row| {
            row.contains("Keyword argument")
                || row.contains("Wrong number of arguments")
                || row.contains("wrong-number-of-arguments")
        })
    };
    wait_for_progress(
        &mut pair,
        "list HTTP failure",
        STAGE_TIMEOUT,
        failure,
        &mut divergences,
    );
    assert_terminal_failure_parity(
        &pair,
        "list connection-close failure",
        expect![
            "pattern: visual [error in process sentinel: Keyword argument :data not one of (:error-thrown :&allow-other-keys&rest :)]"
        ],
        &mut divergences,
    );
    send_to_both(&mut pair, |session| session.send_key("C-g"));
    let gnu = capture_editor(&mut pair.gnu, "list-failure", "GNU");
    let neo = capture_editor(&mut pair.neo, "list-failure", "Neomacs");
    assert_gnu_literal_and_parity(
        "list HTTP failure",
        &gnu,
        Some(&neo),
        expect![[
            r#"(:buffer nil :requests ((:request-line "GET /dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=visual") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"#
        ]],
        &mut divergences,
    );
    drop(pair);

    let mut pair = ready_helm_gitignore_pair("helm-gitignore-generation-failure");
    eval_both(
        &mut pair,
        "(neomacs-helm-gitignore-tui-seed-unsaved-buffer)",
    );
    eval_both(
        &mut pair,
        r#"(neomacs-helm-gitignore-tui-expect '("/dropdown/templates.json?term=visual" legacy-redirect) '("/developers/gitignore/dropdown/templates.json?term=visual" visual-list))"#,
    );
    navigate_to_visual_studio_code(&mut pair, "generation failure", &mut divergences);
    send_to_both(&mut pair, |session| session.send_keys("C-c C-z"));
    let gnu_stopped = wait_for_state_file(
        &mut pair.gnu,
        "server-stopped.state",
        ":server-live nil",
        "GNU",
    );
    expect![[r#"(:server-live nil)"#]].assert_eq(&gnu_stopped);
    let neo_stopped = wait_for_state_file(
        &mut pair.neo,
        "server-stopped.state",
        ":server-live nil",
        "Neomacs",
    );
    if neo_stopped != gnu_stopped {
        divergences.push(format!(
            "stopped generation fixture differs:\nGNU:\n{gnu_stopped}\nNeomacs:\n{neo_stopped}"
        ));
    }
    send_to_both(&mut pair, |session| session.send_key("RET"));
    let gnu_attempted = wait_for_state_file(
        &mut pair.gnu,
        "request-urls.state",
        "<origin>/api/visualstudiocode",
        "GNU",
    );
    expect![[
        r#"("<origin>/dropdown/templates.json?term=visual" "<origin>/api/visualstudiocode")"#
    ]]
    .assert_eq(&gnu_attempted);
    let neo_attempted = wait_for_state_file(
        &mut pair.neo,
        "request-urls.state",
        "<origin>/api/visualstudiocode",
        "Neomacs",
    );
    if neo_attempted != gnu_attempted {
        divergences.push(format!(
            "generation request URL differs:\nGNU:\n{gnu_attempted}\nNeomacs:\n{neo_attempted}"
        ));
    }
    wait_for_progress(
        &mut pair,
        "generation connection-refused failure",
        STAGE_TIMEOUT,
        |grid| {
            grid.iter().any(|row| {
                row.contains("Keyword argument")
                    || row.contains("Wrong number of arguments")
                    || row.contains("wrong-number-of-arguments")
            })
        },
        &mut divergences,
    );
    assert_terminal_failure_parity(
        &pair,
        "generation connection-refused failure",
        expect![
            "error in process sentinel: Keyword argument :data not one of (:error-thrown :&allow-other-keys&rest :)"
        ],
        &mut divergences,
    );
    quit_failure_session(&mut pair);
    wait_for_progress(
        &mut pair,
        "generation failure cleanup",
        STAGE_TIMEOUT,
        |grid| {
            grid.iter()
                .any(|row| row.contains("Release engineering scratchpad"))
                && !grid.iter().any(|row| row.contains("pattern: visual"))
        },
        &mut divergences,
    );
    let gnu = capture_editor(&mut pair.gnu, "generation-failure", "GNU");
    let neo = capture_editor(&mut pair.neo, "generation-failure", "Neomacs");
    assert_gnu_literal_and_parity(
        "generation failure preserves unsaved buffer",
        &gnu,
        Some(&neo),
        expect![[r##"
            (:buffer (:text "# Unsaved incident-specific exclusions
            secret-release-token.txt
            " :point 3 :mode gitignore-mode :modified t :file nil :selected nil) :requests ((:request-line "GET /dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0) (:request-line "GET /developers/gitignore/dropdown/templates.json?term=visual HTTP/1.1" :headers ("Accept-encoding: gzip" "Accept: */*" "Connection: close" "Host: 127.0.0.1:<port>" "MIME-Version: 1.0" "User-Agent: <editor>") :body-bytes 0)) :request-urls ("<origin>/dropdown/templates.json?term=visual" "<origin>/api/visualstudiocode") :remaining-plan nil :misses nil :live-clients nil :response-buffers nil)"##]],
        &mut divergences,
    );
    assert_no_divergences(&divergences);
}
