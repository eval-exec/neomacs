#![cfg(unix)]
//! Issue #860: Display-table vectors containing embedded non-trailing `?\n`
//! glyphs (e.g. `[?\n ?X]`, `[?a ?\n ?b]`, `[?1 ?\n ?2 ?\n ?3]`) must split
//! across display rows like GNU Emacs 31.1 across unwrapped, wrapped,
//! truncated, hscrolled, `display-line-numbers-mode`, and per-glyph-face paths.
//!
//! GNU Emacs 31.1 (`a360712c9d272d950d8d8255ef74570f7e90b7d9`) source contracts:
//! - `src/xdisp.c:8488-8513` (`get_next_display_element`): `DISP_CHAR_VECTOR`
//!   switches `it->method = GET_FROM_DISPLAY_VECTOR`, initializes `it->dpvec`,
//!   `it->dpend`, `it->current.dpvec_index = 0`, and yields one glyph per step.
//! - `src/xdisp.c:9014-9060` (`set_iterator_to_next`): increments
//!   `it->current.dpvec_index` and advances buffer position only once the
//!   vector is exhausted.
//! - `src/xdisp.c:9258-9346` (`next_element_from_display_vector`): sets `it->c`
//!   and per-glyph face per element; an embedded `?\n` triggers
//!   `ITERATOR_AT_END_OF_LINE_P(it)` while leaving suffix elements at the same
//!   buffer charpos for the next display row.
//! - `src/xdisp.c:7661-7799` (`forward_to_next_line_start`): truncation skip
//!   steps through display-vector elements and stops at an embedded `?\n` with
//!   `dpvec_index` positioned at the remaining suffix.

use neomacs_tui_tests::{
    RawTerminalSnapshot, TuiLaunch, TuiSession, TuiTempDirectory, TuiTerminalConfig,
    assert_raw_terminal_snapshots_eq,
};
use std::{path::PathBuf, time::Duration};

fn run_display_table_probe(
    binary: PathBuf,
    setup_elisp: &str,
    pt_expr: &str,
    cols: u16,
    rows: u16,
) -> (String, RawTerminalSnapshot) {
    let directory = TuiTempDirectory::new("issue-860-dt-nl-");
    let init = directory.path().join("init.el");
    let out = directory.path().join("out.txt");
    let capture_rows = rows.saturating_sub(2);
    let elisp = format!(
        r#"(progn
  (menu-bar-mode -1)
  (switch-to-buffer (get-buffer-create "*dt-nl*"))
  (delete-other-windows)
  (erase-buffer)
  {setup_elisp}
  (goto-char {pt_expr})
  (redisplay t)
  (let* ((w (selected-window))
         (pinfo (posn-at-point (point) w))
         (xy (and pinfo (posn-col-row pinfo)))
         (report (format "ws=%d we=%d pm=%d pt=%d posn=%S\n"
                         (window-start w) (window-end w t) (point-max) (point) xy)))
    (with-temp-file "{out_path}"
      (insert report)))
  (message "ISSUE860-READY"))
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
        "ISSUE860-READY",
        TuiTerminalConfig::new("xterm-256color", rows, cols),
    );
    session.read_until_screen(Duration::from_secs(15), |_lines, screen| {
        screen.contents().contains("ISSUE860-READY")
    });
    let snap = RawTerminalSnapshot::capture_rows(session.screen(), 0..capture_rows);
    session.send_key("C-x");
    session.send_key("C-c");
    let _ = session.run_to_completion(Duration::from_secs(5));
    let report = std::fs::read_to_string(&out).expect("read out.txt");
    (report, snap)
}

fn assert_case_matches_gnu(label: &str, setup_elisp: &str, pt_expr: &str, cols: u16, rows: u16) {
    let (gnu_report, gnu_snap) = run_display_table_probe(
        neomacs_tui_tests::oracle::gnu_oracle_program(),
        setup_elisp,
        pt_expr,
        cols,
        rows,
    );
    let (neo_report, neo_snap) = run_display_table_probe(
        neomacs_tui_tests::neomacs_binary(),
        setup_elisp,
        pt_expr,
        cols,
        rows,
    );
    assert_eq!(
        neo_report, gnu_report,
        "{label}: Lisp window/cursor metrics differ from GNU Emacs 31.1"
    );
    assert_raw_terminal_snapshots_eq(label, &gnu_snap, &neo_snap);
}

#[test]
fn display_table_embedded_newline_unwrapped_and_cursor_match_gnu() {
    // Issue #860 Probe B: `?\n -> [?\n ?X]` on "A\nB\n", testing point at A, \n, and B.
    let probe_b = r#"(let ((dt (make-display-table)))
                       (aset dt ?\n [?\n ?X])
                       (setq buffer-display-table dt))
                     (setq-local truncate-lines nil)
                     (insert "A\nB\n")"#;
    assert_case_matches_gnu("dt-nl-X-pt1", probe_b, "1", 80, 10);
    assert_case_matches_gnu("dt-nl-X-pt2-on-nl", probe_b, "2", 80, 10);
    assert_case_matches_gnu("dt-nl-X-pt3-on-B", probe_b, "3", 80, 10);

    // Consecutive embedded newlines: `?\n -> [?\n ?\n ?X]`.
    let double_nl = r#"(let ((dt (make-display-table)))
                         (aset dt ?\n [?\n ?\n ?X])
                         (setq buffer-display-table dt))
                       (setq-local truncate-lines nil)
                       (insert "A\nB\n")"#;
    assert_case_matches_gnu("dt-double-nl-X-pt2", double_nl, "2", 80, 12);
    assert_case_matches_gnu("dt-double-nl-X-pt3", double_nl, "3", 80, 12);

    // Embedded non-trailing `?\n` on an ordinary character: `?Z -> [?a ?\n ?b]`,
    // multi-break `?Z -> [?1 ?\n ?2 ?\n ?3]`, and trailing `?\n` on `?Z -> [?a ?\n]`.
    let mid_z = r#"(let ((dt (make-display-table)))
                     (aset dt ?Z [?a ?\n ?b])
                     (setq buffer-display-table dt))
                   (setq-local truncate-lines nil)
                   (insert "AZB\n")"#;
    assert_case_matches_gnu("dt-Z-a-nl-b-pt2", mid_z, "2", 80, 10);

    let multi_z = r#"(let ((dt (make-display-table)))
                       (aset dt ?Z [?1 ?\n ?2 ?\n ?3])
                       (setq buffer-display-table dt))
                     (setq-local truncate-lines nil)
                     (insert "AZB\n")"#;
    assert_case_matches_gnu("dt-Z-1-nl-2-nl-3-pt3", multi_z, "3", 80, 10);

    let trail_z = r#"(let ((dt (make-display-table)))
                       (aset dt ?Z [?a ?\n])
                       (setq buffer-display-table dt))
                     (setq-local truncate-lines nil)
                     (insert "AZB\n")"#;
    assert_case_matches_gnu("dt-Z-trail-nl-pt3", trail_z, "3", 80, 10);
}

#[test]
fn display_table_embedded_newline_truncated_matches_gnu() {
    // Issue #860 Probe A: truncated long line overflowing before `?\n -> [?\n ?X]`.
    let probe_a = r#"(let ((dt (make-display-table)))
                       (aset dt ?\n [?\n ?X])
                       (setq buffer-display-table dt))
                     (setq-local truncate-lines t auto-hscroll-mode nil)
                     (insert (make-string 220 ?a) "\nB\n")"#;
    assert_case_matches_gnu("dt-trunc-220a-nl-X-pt1", probe_a, "1", 80, 10);
    assert_case_matches_gnu("dt-trunc-220a-nl-X-pt221", probe_a, "221", 80, 10);

    // Truncated long line overflowing before ordinary char `?Z -> [?a ?\n ?b]`.
    let trunc_z = r#"(let ((dt (make-display-table)))
                       (aset dt ?Z [?a ?\n ?b])
                       (setq buffer-display-table dt))
                     (setq-local truncate-lines t auto-hscroll-mode nil)
                     (insert (make-string 220 ?a) "ZB\n")"#;
    assert_case_matches_gnu("dt-trunc-220a-Z-pt222", trunc_z, "222", 80, 10);
}

#[test]
fn display_table_embedded_newline_narrow_wrap_and_truncate_match_gnu() {
    // Window narrower than the multi-glyph display-table entry (9 body cols via right margin).
    let narrow_wrap_z = r#"(set-window-margins nil 0 71)
                           (let ((dt (make-display-table)))
                             (aset dt ?Z [?1 ?2 ?3 ?4 ?5 ?6 ?7 ?8 ?9 ?0 ?\n ?X ?Y])
                             (setq buffer-display-table dt))
                           (setq-local truncate-lines nil truncate-partial-width-windows nil)
                           (insert "ABZC\n")"#;
    assert_case_matches_gnu("dt-narrow-wrap-Z-pt3", narrow_wrap_z, "3", 80, 10);
    assert_case_matches_gnu("dt-narrow-wrap-Z-pt4", narrow_wrap_z, "4", 80, 10);

    let narrow_wrap_nl = r#"(set-window-margins nil 0 71)
                            (let ((dt (make-display-table)))
                              (aset dt ?\n [?1 ?2 ?3 ?4 ?5 ?6 ?7 ?8 ?9 ?0 ?\n ?X ?Y])
                              (setq buffer-display-table dt))
                            (setq-local truncate-lines nil truncate-partial-width-windows nil)
                            (insert "AB\nC\n")"#;
    assert_case_matches_gnu("dt-narrow-wrap-nl-pt4", narrow_wrap_nl, "4", 80, 10);

    // Same narrow window with `truncate-lines t`: clips mid-vector, skips to embedded `?\n`,
    // and resumes suffix `XY` on the next row.
    let narrow_trunc_z = r#"(set-window-margins nil 0 71)
                            (let ((dt (make-display-table)))
                              (aset dt ?Z [?1 ?2 ?3 ?4 ?5 ?6 ?7 ?8 ?9 ?0 ?\n ?X ?Y])
                              (setq buffer-display-table dt))
                            (setq-local truncate-lines t auto-hscroll-mode nil)
                            (insert "ABZC\n")"#;
    assert_case_matches_gnu("dt-narrow-trunc-Z-pt4", narrow_trunc_z, "4", 80, 10);
}

#[test]
fn display_table_embedded_newline_hscroll_dln_and_faces_match_gnu() {
    // Hscroll landing mid-vector before AND after an embedded `?\n`.
    let hscroll_z = r#"(let ((dt (make-display-table)))
                         (aset dt ?Z [?1 ?2 ?3 ?4 ?5 ?\n ?X ?Y ?Z ?1 ?2 ?3 ?4 ?5])
                         (setq buffer-display-table dt))
                       (setq-local truncate-lines t auto-hscroll-mode nil)
                       (insert "ABZCDEFG\nGHIJKLMN\n")
                       (set-window-hscroll nil 4)"#;
    assert_case_matches_gnu("dt-hscroll4-Z", hscroll_z, "1", 80, 10);

    // `display-line-numbers-mode` with both ordinary-char and newline embedded-`\n` vectors,
    // plus truncated long line with `?\n -> [?\n ?X]`.
    let dln_mixed = r#"(let ((dt (make-display-table)))
                         (aset dt ?Z [?a ?\n ?b])
                         (aset dt ?\n [?\n ?X])
                         (setq buffer-display-table dt))
                       (setq-local truncate-lines nil)
                       (display-line-numbers-mode 1)
                       (insert "AZB\nC\n")"#;
    assert_case_matches_gnu("dt-dln-mixed-pt5", dln_mixed, "5", 80, 10);

    let dln_trunc = r#"(let ((dt (make-display-table)))
                         (aset dt ?\n [?\n ?X])
                         (setq buffer-display-table dt))
                       (setq-local truncate-lines t auto-hscroll-mode nil)
                       (display-line-numbers-mode 1)
                       (insert (make-string 220 ?a) "\nB\n")"#;
    assert_case_matches_gnu("dt-dln-trunc-220a", dln_trunc, "1", 80, 10);

    // Per-glyph faces via `make-glyph-code` across an embedded `?\n` split.
    let faced_nl = r#"(let ((dt (make-display-table)))
                        (aset dt ?\n (vector (make-glyph-code ?$ 'warning)
                                             ?\n
                                             (make-glyph-code ?X 'error)))
                        (setq buffer-display-table dt))
                      (setq-local truncate-lines nil)
                      (insert "A\nB\n")"#;
    assert_case_matches_gnu("dt-faced-nl-split", faced_nl, "1", 80, 10);
}
