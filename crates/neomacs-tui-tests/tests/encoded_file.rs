//! Compiled file decoding reaches the real terminal command loop.
#![cfg(unix)]
use crate::support::*;
use neomacs_tui_tests::TuiSession;
use std::{fs, time::Duration};

const VALID: &[u8] = include_bytes!("../../neovm-oracle-tests/fixtures/encoded-file/valid.elc");
const OVERSIZED: &[u8] =
    include_bytes!("../../neovm-oracle-tests/fixtures/encoded-file/oversized.elc");

fn visible(grid: &[String], marker: &str) -> bool {
    grid.iter().any(|row| row.contains(marker))
}

fn assert_visible(session: &mut TuiSession, marker: &str) {
    session.read_until(Duration::from_secs(15), |grid| visible(grid, marker));
    assert!(
        visible(&session.text_grid(), marker),
        "{marker} absent from terminal:\n{}",
        session.text_grid().join("\n")
    );
}

#[test]
fn compiled_file_character_boundaries_survive_terminal_input() {
    let (mut gnu, mut neo) = boot_pair("");
    for session in [&gnu, &neo] {
        fs::write(session.home_dir().join("encoded-file.elc"), VALID).unwrap();
    }
    eval_expression(
        &mut gnu,
        &mut neo,
        r#"(progn
             (load (expand-file-name "encoded-file.elc" (getenv "HOME")) nil t t)
             (unless (equal (list neo-encoded-file-result neo-encoded-file-malformed
                                  neo-encoded-file-after)
                            '((233 30333 128512 2097152 4194175 4194176 4194303)
                              (195 40) loaded))
               (error "Encoded file character identity changed"))
             (switch-to-buffer (get-buffer-create "*encoded file*"))
             (erase-buffer)
             (insert "ENCODED-FILE-LOADED\n")
             (message nil))"#,
    );
    for session in [&mut gnu, &mut neo] {
        assert_visible(session, "ENCODED-FILE-LOADED");
        // Host PTY input after loading establishes continued command-loop
        // operation, rather than merely observing a successful Lisp callback.
        session.send(b"after-load");
        assert_visible(session, "after-load");
    }
    assert_pair_exact_display(
        "compiled_file_character_boundaries_survive_terminal_input",
        &gnu,
        &neo,
    );
}

#[test]
fn oversized_compiled_file_recovers_and_accepts_terminal_input() {
    // GNU's unchecked decoder assumes this payload is in range. Its behavior
    // outside that assumption is not a portable oracle for our recovery policy.
    let mut neo = TuiSession::neomacs("");
    assert_visible(&mut neo, "*scratch*");
    fs::write(neo.home_dir().join("oversized.elc"), OVERSIZED).unwrap();
    eval_expression_one(
        &mut neo,
        r#"(progn
             (load (expand-file-name "oversized.elc" (getenv "HOME")) nil t t)
             (unless (and (= (car neo-encoded-file-oversized) #x3ffff8)
                          (> (length neo-encoded-file-oversized) 1)
                          (eq neo-encoded-file-after 'loaded))
               (error "Oversized file did not preserve its lead and following form"))
             (dolist (code neo-encoded-file-oversized)
               (unless (<= 0 code #x3fffff) (error "Out of character domain")))
             (switch-to-buffer (get-buffer-create "*encoded file*"))
             (erase-buffer)
             (insert "OVERSIZED-FILE-RECOVERED\n")
             (message nil))"#,
    );
    assert_visible(&mut neo, "OVERSIZED-FILE-RECOVERED");
    neo.send(b"after-recovery");
    assert_visible(&mut neo, "after-recovery");
}
