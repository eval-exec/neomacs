#![cfg(unix)]
//! Reproduction: the Doom mode line orders `which-key` and `better-jumper`
//! oppositely in Neomacs and GNU.
//!
//! REPRODUCED (see the dump this test prints): with both modes enabled in both
//! editors and identical alist membership (41 entries), the ORDER differs --
//!
//!   GNU: (better-jumper-local-mode which-key-mode ...)   mode line "... better-jumper WK ..."
//!   NEO: (which-key-mode better-jumper-local-mode ...)   mode line "... WK better-jumper ..."
//!
//! WHY THE MODE LINE ANSWERS A LOAD-ORDER QUESTION.  The mode line's
//! minor-mode segment is `minor-mode-alist` order verbatim: `bindings.el`
//! `mode-line--minor-modes` takes `visible = minor-mode-alist` whenever
//! `mode-line-collapse-minor-modes` is nil (its default), and
//! `define-minor-mode` adds the lighter at DEFINITION time.  So the alist
//! order is reverse LOAD order, and reading it answers "which file loaded
//! first" with no grid diff.
//!
//! WHAT IS ALREADY RULED OUT (each checked against GNU, all identical):
//!   * `add-hook` default/append/depth, `add-to-list` default/append
//!   * `define-minor-mode :lighter` -> `minor-mode-alist` ordering
//!   * `mode-line--minor-modes` on all four `mode-line-collapse-minor-modes` branches
//! so the divergence is neither the list primitives nor the mode-line
//! construction -- it is WHICH FILE LOADS FIRST.
//!
//! WHY THIS NEEDS A PTY AND NOT `--batch`: Doom defers both modes to
//! `doom-first-input-hook`, which fires on the first *input*; batch has no
//! command loop.  Forcing the hook by hand in batch makes both editors agree
//! vacuously (verified: both report `which-key=nil better-jumper=nil` and
//! byte-identical alists), and reading the hook afterwards is too late --
//! it reports nil in both editors once the first input has been consumed.

use crate::support;
use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory};
use std::ffi::OsString;
use std::time::Duration;
use support::*;

fn doom_launch(
    doom: &neomacs_infra::DoomEnvironment,
    state: &TuiTempDirectory,
    program: &std::ffi::OsStr,
    extra: &[OsString],
) -> TuiLaunch {
    let mut launch = TuiLaunch::new(program)
        .arg("-nw")
        .args(doom.session_args())
        .args(extra.iter().cloned());
    for (name, value) in doom.session_env(state) {
        launch = launch.env(name, value);
    }
    launch
}

fn read_marker(session: &mut TuiSession, expr: &str, marker: &str) -> String {
    support::eval_expression_one(session, expr);
    session.read(Duration::from_millis(800));
    let (rows, _) = session.screen_size();
    for row in (0..rows).rev() {
        let text = session.row_text(row);
        if let Some(start) = text.find(marker) {
            return text[start..].trim_end().to_owned();
        }
    }
    String::from("<marker not found>")
}

fn mode_line_row(session: &TuiSession) -> String {
    session
        .text_grid()
        .iter()
        .find(|row| row.contains("(Doom Docs"))
        .cloned()
        .unwrap_or_default()
}

/// Read a file the editor was asked to write inside its session state.
fn read_state_file(state: &TuiTempDirectory, name: &str) -> String {
    std::fs::read_to_string(state.path().join(".doom-local").join(name))
        .unwrap_or_else(|_| "<unreadable>".to_owned())
}

// Ignored on purpose: this FAILS while the Doom ordering bug is live, and a
// permanently-red test in the default run would hide real regressions behind
// it.  Run it explicitly to get the reproduction:
//
//   cargo nextest run -p neomacs-tui-tests --run-ignored \
//     -E 'test(doom_minor_mode_order_matches_gnu)' --success-output immediate
//
// Note it is timing-sensitive: adding any work before the first input (an
// extra `M-:` eval, say) can make both editors agree, because that eval is
// itself a first input and fires the load path early.  If it passes, do not
// conclude the bug is fixed -- remove the perturbation and run again.
#[test]
#[ignore = "reproduction aid for a live Doom mode-line ordering bug; see the module docs"]
fn doom_minor_mode_order_matches_gnu() {
    let Some(doom) = neomacs_infra::DoomEnvironment::open() else {
        eprintln!(
            "skipping: no sealed Doom fixture; run \
             `cargo run -p xtask -- infra materialize doom` to build one"
        );
        return;
    };
    let index = doom.tree().join("docs/index.org");
    let docs_library = doom.tree().join("lisp/lib/docs.el");
    assert!(index.is_file(), "fixture has no docs/index.org");

    let document_args = [
        OsString::from("--load"),
        docs_library.into_os_string(),
        index.into_os_string(),
        OsString::from("--eval=(goto-char(point-min))"),
    ];

    let gnu_state = TuiTempDirectory::new("mmo-gnu-");
    let neo_state = TuiTempDirectory::new("mmo-neo-");
    for state in [&gnu_state, &neo_state] {
        doom.prepare_session_state(state)
            .expect("seed Doom session state");
    }

    let mut gnu = TuiSession::spawn_launch(
        doom_launch(
            &doom,
            &gnu_state,
            std::ffi::OsStr::new("emacs"),
            &document_args,
        ),
        "GNU",
    );
    // Drain GNU to quiescence before the second costly startup: Doom's initial
    // repaint overruns a Linux PTY's output queue, and an undrained child can
    // lose the first byte of an escape sequence at the 4095-byte boundary.
    gnu.read(Duration::from_secs(5));
    let mut neo = TuiSession::spawn_launch(
        doom_launch(
            &doom,
            &neo_state,
            neomacs_tui_tests::neomacs_binary().as_os_str(),
            &document_args,
        ),
        "NEO",
    );

    let startup = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Doom Docs"))
            || grid.iter().any(|row| row.contains("Doom loaded"))
            || grid
                .iter()
                .any(|row| row.contains("Emoji images not available"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(45), startup);

    // Decline Doom's emojify download prompt so it cannot swallow the keys.
    for session in [&mut gnu, &mut neo] {
        session.read(Duration::from_secs(2));
        if session
            .text_grid()
            .iter()
            .any(|row| row.contains("Emoji images not available"))
        {
            session.send(b"n");
            session.read_until(Duration::from_secs(5), |grid| {
                !grid
                    .iter()
                    .any(|row| row.contains("Emoji images not available"))
            });
        }
    }

    // ARM THE LOAD TRACE BEFORE THE FIRST INPUT.  The modes are defined when
    // their files load, and that happens during first input, so the trace must
    // already be installed -- reading `doom-first-input-hook` afterwards is
    // too late (it reports nil in both editors).
    let arm = "(progn (setq mmo-loads nil) \
               (add-hook 'after-load-functions (lambda (f) (push (file-name-nondirectory f) mmo-loads))) 'armed)";
    support::eval_expression_one(&mut gnu, arm);
    support::eval_expression_one(&mut neo, arm);
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // THE STEP BATCH CANNOT DO: a real first input.
    send_both(&mut gnu, &mut neo, "C-g");
    read_both(&mut gnu, &mut neo, Duration::from_secs(5));

    let probe = "(message \"MMD%d%d%d\" \
                   (if (boundp 'which-key-mode) (if which-key-mode 1 0) 9) \
                   (if (boundp 'better-jumper-mode) (if better-jumper-mode 1 0) 9) \
                   (length minor-mode-alist))";
    let gnu_probe = read_marker(&mut gnu, probe, "MMD");
    let neo_probe = read_marker(&mut neo, probe, "MMD");

    let alist_probe = "(with-temp-file (expand-file-name \"alist.txt\" (getenv \"DOOMLOCALDIR\")) \
                        (insert (format \"%S\" (mapcar #'car minor-mode-alist))))";
    support::eval_expression_one(&mut gnu, alist_probe);
    support::eval_expression_one(&mut neo, alist_probe);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    let gnu_alist = read_state_file(&gnu_state, "alist.txt");
    let neo_alist = read_state_file(&neo_state, "alist.txt");

    // The decisive read: the order the two files actually loaded in.
    let dump_loads = "(with-temp-file (expand-file-name \"loads.txt\" (getenv \"DOOMLOCALDIR\")) \
         (insert (format \"%S\" (reverse mmo-loads))))";
    support::eval_expression_one(&mut gnu, dump_loads);
    support::eval_expression_one(&mut neo, dump_loads);
    read_both(&mut gnu, &mut neo, Duration::from_secs(3));
    let gnu_loads = read_state_file(&gnu_state, "loads.txt");
    let neo_loads = read_state_file(&neo_state, "loads.txt");

    let gnu_row = mode_line_row(&gnu);
    let neo_row = mode_line_row(&neo);

    // Report the position of each mode's file in the load sequence.
    let position = |loads: &str, name: &str| -> String {
        loads
            .find(name)
            .map(|byte| {
                let ordinal = loads[..byte].matches('"').count() / 2 + 1;
                format!("load #{ordinal}")
            })
            .unwrap_or_else(|| "not loaded".to_owned())
    };

    eprintln!("\n=== REPRODUCTION ===");
    eprintln!("probe(which-key,better-jumper,len): GNU={gnu_probe}  NEO={neo_probe}");
    eprintln!("GNU mode line: {gnu_row}");
    eprintln!("NEO mode line: {neo_row}");
    eprintln!("GNU alist: {gnu_alist}");
    eprintln!("NEO alist: {neo_alist}");
    eprintln!(
        "GNU load position: which-key {} | better-jumper {}",
        position(&gnu_loads, "which-key"),
        position(&gnu_loads, "better-jumper")
    );
    eprintln!(
        "NEO load position: which-key {} | better-jumper {}",
        position(&neo_loads, "which-key"),
        position(&neo_loads, "better-jumper")
    );
    eprintln!("GNU loads: {gnu_loads}");
    eprintln!("NEO loads: {neo_loads}");
    eprintln!("=====================\n");

    assert_eq!(
        neo_alist, gnu_alist,
        "minor-mode-alist order differs; see the dump above"
    );
}
