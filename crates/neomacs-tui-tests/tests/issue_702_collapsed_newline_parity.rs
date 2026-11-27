#![cfg(unix)]
//! Issue #702 Round 2 (Correctness): newlines collapsed by `buffer-display-table`
//! (entries without a trailing `?\n`, e.g. `[]` or `[? ]`) or by `composition`
//! properties spanning `\n` must NOT be counted as display row breaks by
//! `find_nth_preserved_newline_within` or stopped at by truncation skip.
//!
//! GNU Emacs 31.1 (`a360712c9d`) source contracts:
//! - `src/xdisp.c:8488-8513` (`get_next_display_element`): display-table vector
//!   for `'\n'` without a trailing `?\n` joins lines instead of ending the row.
//! - `src/xdisp.c:6837-6894` (`handle_composition_prop`): a composition spanning
//!   `'\n'` consumes the composed characters as one display element without
//!   breaking the display row.
//! - `src/xdisp.c:7661-7799` (`forward_to_next_line_start`): truncated-line
//!   skipping stops only at a displayed `'\n'` (`ITERATOR_AT_END_OF_LINE_P`).

use neomacs_tui_tests::{
    RawTerminalSnapshot, TuiLaunch, TuiSession, TuiTempDirectory, TuiTerminalConfig,
    assert_raw_terminal_snapshots_eq,
};
use std::{path::PathBuf, time::Duration};

fn run_collapsed_newline_probe(
    binary: PathBuf,
    setup_elisp: &str,
) -> (String, RawTerminalSnapshot, Vec<String>) {
    let directory = TuiTempDirectory::new("issue-702-collapsed-nl-");
    let init = directory.path().join("init.el");
    let out = directory.path().join("out.txt");
    let elisp = format!(
        r#"(progn
  (menu-bar-mode -1)
  (switch-to-buffer (get-buffer-create "*collapsed-nl*"))
  (delete-other-windows)
  (erase-buffer)
  {setup_elisp}
  (goto-char (point-min))
  (redisplay t)
  (let ((report (format "window-end=%d point-max=%d\n" (window-end nil t) (point-max))))
    (with-temp-file "{out_path}"
      (insert report)))
  (message "ISSUE702-COLLAPSE-READY"))
"#,
        out_path = out.display()
    );
    std::fs::write(&init, elisp).expect("write init.el");

    let launch = TuiLaunch::new(binary.as_os_str())
        .arg("-nw")
        .arg("-Q")
        .arg("--load")
        .arg(&init)
        .env("HOME", directory.path());
    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        "ISSUE702-COLLAPSE-READY",
        TuiTerminalConfig::new("xterm-256color", 50, 200),
    );
    session.read_until_screen(Duration::from_secs(15), |_lines, screen| {
        screen.contents().contains("ISSUE702-COLLAPSE-READY")
    });
    let snap = RawTerminalSnapshot::capture_rows(session.screen(), 0..47);
    let rows: Vec<String> = (0..47).map(|r| session.row_text(r)).collect();
    session.send_key("C-x");
    session.send_key("C-c");
    let _ = session.run_to_completion(Duration::from_secs(5));
    let report = std::fs::read_to_string(&out).expect("read out.txt");
    (report, snap, rows)
}

#[test]
fn display_table_empty_newline_matches_gnu() {
    let setup = r#"(let ((dt (make-display-table)))
                     (aset dt ?\n [])
                     (setq buffer-display-table dt))
                   (setq-local truncate-lines nil)
                   (dotimes (i 200) (insert (format "L%03d-" i) "\n"))"#;
    let (gnu_report, gnu_snap, _gnu_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::oracle::gnu_oracle_program(), setup);
    let (neo_report, neo_snap, _neo_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::neomacs_binary(), setup);

    assert_eq!(
        gnu_report.trim(),
        "window-end=1201 point-max=1201",
        "GNU Emacs 31.1 must join all 200 lines into wrapped rows reaching point-max"
    );
    assert_eq!(
        neo_report, gnu_report,
        "neomacs window-end under empty display-table ?\\n must match GNU Emacs 31.1"
    );
    assert_raw_terminal_snapshots_eq("display-table-empty-nl", &gnu_snap, &neo_snap);
}

#[test]
fn display_table_space_newline_truncated_matches_gnu() {
    // Full 200-line (1200-char) probe — NOT shrunk. Exercises GNU Emacs 31.1
    // `forward_to_next_line_start`'s `MAX_NEWLINE_DISTANCE = 500` shortcut past
    // column 200 (row 0 L000-..$, row 1 L117-..$, rows 2..46 blank, window-end=1201).
    let setup = r#"(let ((dt (make-display-table)))
                     (aset dt ?\n [? ])
                     (setq buffer-display-table dt))
                   (setq-local truncate-lines t)
                   (dotimes (i 200) (insert (format "L%03d-" i) "\n"))"#;
    let (gnu_report, gnu_snap, gnu_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::oracle::gnu_oracle_program(), setup);
    let (neo_report, neo_snap, _neo_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::neomacs_binary(), setup);

    assert_eq!(
        gnu_report.trim(),
        "window-end=1201 point-max=1201",
        "GNU Emacs 31.1 must reach point-max=1201 on the 200-line [? ] truncated probe"
    );
    assert!(
        gnu_rows[1].starts_with("L117-"),
        "GNU Emacs 31.1 must break onto row 1 at L117- via MAX_NEWLINE_DISTANCE=500, got: {:?}",
        gnu_rows[1]
    );
    assert_eq!(
        gnu_rows[2].trim(),
        "",
        "GNU Emacs 31.1 must leave row 2 blank after the remaining < 500 elements reach EOB"
    );
    assert_eq!(
        neo_report, gnu_report,
        "neomacs window-end under [? ] display-table ?\\n with truncate-lines must match GNU Emacs 31.1"
    );
    assert_raw_terminal_snapshots_eq("display-table-space-nl-trunc-200", &gnu_snap, &neo_snap);
}

#[test]
fn display_table_empty_newline_truncated_matches_gnu() {
    let setup = r#"(let ((dt (make-display-table)))
                     (aset dt ?\n [])
                     (setq buffer-display-table dt))
                   (setq-local truncate-lines t)
                   (dotimes (i 200) (insert (format "L%03d-" i) "\n"))"#;
    let (gnu_report, gnu_snap, _gnu_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::oracle::gnu_oracle_program(), setup);
    let (neo_report, neo_snap, _neo_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::neomacs_binary(), setup);

    assert_eq!(gnu_report.trim(), "window-end=1201 point-max=1201");
    assert_eq!(
        neo_report, gnu_report,
        "neomacs window-end under [] display-table ?\\n with truncate-lines must match GNU Emacs 31.1"
    );
    assert_raw_terminal_snapshots_eq("display-table-empty-nl-trunc-200", &gnu_snap, &neo_snap);
}

#[test]
fn hscroll_display_table_and_composition_collapsed_newline_matches_gnu() {
    // Blocker 1a: hscroll=10 with `(aset dt ?\n [? ])` on 100 lines.
    let setup_dt = r#"(let ((dt (make-display-table)))
                        (aset dt ?\n [? ])
                        (setq buffer-display-table dt))
                      (setq-local truncate-lines t auto-hscroll-mode nil)
                      (dotimes (i 100) (insert (format "L%03d-" i) "\n"))
                      (set-window-hscroll nil 10)"#;
    let (gnu_dt_report, gnu_dt_snap, _gnu_dt_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::oracle::gnu_oracle_program(), setup_dt);
    let (neo_dt_report, neo_dt_snap, _neo_dt_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::neomacs_binary(), setup_dt);
    assert_eq!(gnu_dt_report.trim(), "window-end=601 point-max=601");
    assert_eq!(
        neo_dt_report, gnu_dt_report,
        "neomacs window-end under hscroll=10 + [? ] display-table must match GNU Emacs 31.1"
    );
    assert_raw_terminal_snapshots_eq("hscroll-dt-space-nl", &gnu_dt_snap, &neo_dt_snap);

    // Blocker 1b: hscroll=10 with `compose-region` spanning `-\n` on 100 lines.
    let setup_comp = r#"(setq-local truncate-lines t auto-hscroll-mode nil)
                        (dotimes (i 100)
                          (let ((p (point)))
                            (insert (format "L%03d-\n" i))
                            (compose-region (+ p 4) (point) " ")))
                        (set-window-hscroll nil 10)"#;
    let (gnu_c_report, gnu_c_snap, _gnu_c_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::oracle::gnu_oracle_program(), setup_comp);
    let (neo_c_report, neo_c_snap, _neo_c_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::neomacs_binary(), setup_comp);
    assert_eq!(gnu_c_report.trim(), "window-end=601 point-max=601");
    assert_eq!(
        neo_c_report, gnu_c_report,
        "neomacs window-end under hscroll=10 + compose-region spanning ?\\n must match GNU Emacs 31.1"
    );
    assert_raw_terminal_snapshots_eq("hscroll-comp-span-nl", &gnu_c_snap, &neo_c_snap);
}

#[test]
fn truncation_skip_display_invisible_and_invalid_composition_matches_gnu() {
    // Blocker 2.1 + 2.2: `'display " "` and `'invisible t` collapse `\n` during
    // truncation skip, while invalid `'composition 'invalid` still breaks row 0.
    let setup = r#"(setq-local truncate-lines t)
                   (insert (make-string 220 ?a) "\nJOINED-BY-DISPLAY\n")
                   (put-text-property 221 222 'display " ")
                   (let ((p (point)))
                     (insert (make-string 220 ?b) "\nJOINED-BY-INVIS\n")
                     (put-text-property (+ p 220) (+ p 221) 'invisible t))
                   (let ((p (point)))
                     (insert (make-string 220 ?c) "\nNEXT-LINE-VISIBLE\n")
                     (put-text-property (+ p 220) (+ p 221) 'composition 'invalid))"#;
    let (gnu_report, gnu_snap, gnu_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::oracle::gnu_oracle_program(), setup);
    let (neo_report, neo_snap, _neo_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::neomacs_binary(), setup);

    assert!(
        gnu_rows[3].starts_with("NEXT-LINE-VISIBLE"),
        "GNU Emacs 31.1 must preserve NEXT-LINE-VISIBLE on row 3 after invalid composition, got: {:?}",
        &gnu_rows[0..5]
    );
    assert_eq!(neo_report, gnu_report);
    assert_raw_terminal_snapshots_eq("trunc-disp-invis-invalid-comp", &gnu_snap, &neo_snap);
}

#[test]
fn composition_spanning_newline_matches_gnu() {
    let setup = r#"(setq-local truncate-lines nil)
                   (dotimes (i 120)
                     (let ((p (point)))
                       (insert (format "C%03d\n" i))
                       (when (< i 80)
                         (compose-region (+ p 3) (point) "X"))))"#;
    let (gnu_report, gnu_snap, _gnu_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::oracle::gnu_oracle_program(), setup);
    let (neo_report, neo_snap, _neo_rows) =
        run_collapsed_newline_probe(neomacs_tui_tests::neomacs_binary(), setup);

    assert_eq!(
        gnu_report.trim(),
        "window-end=601 point-max=601",
        "GNU Emacs 31.1 must not count composed newlines as row breaks"
    );
    assert_eq!(
        neo_report, gnu_report,
        "neomacs window-end under composition spanning ?\\n must match GNU Emacs 31.1"
    );
    assert_raw_terminal_snapshots_eq("composition-span-nl", &gnu_snap, &neo_snap);
}
