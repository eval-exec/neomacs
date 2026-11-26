//! Issue #249: two "special key binding" reports, checked against GNU.
//!
//! 1. `C-x C-l` is a disabled command. GNU runs `disabled-command-function`
//!    (novice.el), which reads an answer through
//!    `read-multiple-choice--from-minibuffer`; `n` must decline the command
//!    and must not reach the buffer as a self-inserted character.
//! 2. `M-<down>` at a minibuffer completion must run
//!    `minibuffer-next-completion` (simple.el `completion-list-mode-map`
//!    `[M-down]`, minibuffer.el `M-<down>`), moving point in `*Completions*`
//!    to the first candidate line while point stays in the minibuffer.
//!
//! Both rows are paired: GNU's observed behaviour is the oracle, and the
//! Neomacs assertion is the parity contract.

use crate::support::*;
use std::time::Duration;

/// `C-x C-l` then `n`: the prompt is shown, the answer declines the command,
/// and the buffer keeps exactly the text it had before.
#[test]
fn disabled_command_declines_on_n_without_inserting_it() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "ABC");
    let typed = |grid: &[String]| grid.iter().any(|row| row.contains("ABC"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), typed);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            typed(&session.text_grid()),
            "{label} did not accept the setup text:\n{}",
            session.text_grid().join("\n")
        );
    }

    send_both(&mut gnu, &mut neo, "C-x C-l");
    let prompted = |grid: &[String]| grid.iter().any(|row| row.contains("Use this command?"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), prompted);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            prompted(&session.text_grid()),
            "{label} did not show the disabled-command prompt:\n{}",
            session.text_grid().join("\n")
        );
    }

    send_both(&mut gnu, &mut neo, "n");
    let declined = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*scratch*"))
            && !grid.iter().any(|row| row.contains("Use this command?"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), declined);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            declined(&session.text_grid()),
            "{label} did not return to *scratch* after declining:\n{}",
            session.text_grid().join("\n")
        );
    }

    let dir = tempfile::tempdir().expect("temporary directory");
    let gnu_out = dir.path().join("issue-249-gnu.txt");
    let neo_out = dir.path().join("issue-249-neomacs.txt");
    // Capture the buffer text BEFORE entering `with-temp-file`: inside it the
    // current buffer is the temp file, so a bare `(buffer-string)` there is
    // empty.
    let form = |path: &std::path::Path| {
        format!(
            r#"(let ((s (buffer-string))) (with-temp-file {path:?} (insert s)))"#,
            path = path
        )
    };
    eval_expression_one(&mut gnu, &form(&gnu_out));
    eval_expression_one(&mut neo, &form(&neo_out));

    let gnu_result = std::fs::read_to_string(&gnu_out).expect("GNU buffer contents");
    let neo_result = std::fs::read_to_string(&neo_out).expect("Neomacs buffer contents");
    assert!(
        gnu_result.ends_with("ABC"),
        "GNU must decline C-x C-l and leave the buffer text unchanged, got {gnu_result:?}"
    );
    assert!(
        !gnu_result.contains("ABCn") && !gnu_result.ends_with("\nn"),
        "GNU must not insert the declining `n`, got {gnu_result:?}"
    );
    assert_eq!(
        neo_result,
        gnu_result,
        "Neomacs must decline C-x C-l like GNU; GNU grid:\n{}\nNeomacs grid:\n{}",
        gnu.text_grid().join("\n"),
        neo.text_grid().join("\n")
    );
}

/// `M-x find TAB` then `M-<down>`: point in `*Completions*` lands on the first
/// candidate line, and the minibuffer still holds the typed input.
#[test]
fn meta_down_moves_completions_point_to_the_first_candidate() {
    let (mut gnu, mut neo) = boot_pair("");

    send_both(&mut gnu, &mut neo, "M-x find");
    let prompt = |grid: &[String]| grid.iter().any(|row| row.contains("M-x find"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), prompt);

    send_both(&mut gnu, &mut neo, "TAB");
    let completions = |grid: &[String]| {
        grid.iter().any(|row| row.contains("*Completions*"))
            && grid.iter().any(|row| row.contains("find-grep"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), completions);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        assert!(
            completions(&session.text_grid()),
            "{label} did not show the completion list:\n{}",
            session.text_grid().join("\n")
        );
    }

    // xterm's Alt-Down encoding, `CSI 1 ; 3 B`. The key helper has no
    // modified-arrow token; GNU maps this sequence to `[M-down]`.
    send_both_raw(&mut gnu, &mut neo, b"\x1b[1;3B");
    // GNU's *Completions* buffer has three header lines, so the first
    // candidate is line 5; the mode line reports the point line.
    let first_candidate = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains("*Completions*") && row.contains("All L5"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(8), first_candidate);
    for (label, session) in [("GNU", &gnu), ("Neomacs", &neo)] {
        let grid = session.text_grid();
        assert!(
            first_candidate(&grid),
            "{label} did not move point to the first completion candidate:\n{}",
            grid.join("\n")
        );
        assert!(
            grid.iter().any(|row| row.contains("M-x find")),
            "{label} left the minibuffer while moving in *Completions*:\n{}",
            grid.join("\n")
        );
    }
}
