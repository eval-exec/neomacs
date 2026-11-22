#![cfg(unix)]
//! GNU authority for the TTY painting of the reporter's R2L rows (issue #785
//! follow-up).
//!
//! GNU's terminal glyph production prepends the glyphs of a reversed row
//! (`src/term.c:1563-1585`), and the blanks that push a short paragraph to the
//! right margin are the end-of-line extend blanks produced by the terminal
//! branch of `extend_face_to_end_of_line` (`src/xdisp.c:24698-24827`); in a
//! reversed row each of those blanks is prepended too, so the LEFT side of a
//! short R2L row carries the extend face.
//!
//! This test runs the reporter's exact bytes (`d8 ac d8 a7 0a d8 ac 0a`) under
//! GNU 31.1 and Neomacs for all four settings (nil/auto, right-to-left,
//! left-to-right, and a nil `bidi-display-reordering`), a TAB-inside-the-line
//! case with point after the tab, and two zero-width bidi-control cases (an
//! RLM before the letters with point on it, and an LRM under an explicit
//! right-to-left paragraph), seven settings in total, and compares:
//!   - the body row cell for cell, including the exact rightmost column;
//!   - the cursor cell with point on the R2L line;
//!   - an overlay with `(face (:background "red" :extend t))` over the whole
//!     buffer, so the left blanks' extend face must match GNU.

use std::path::Path;
use std::time::Duration;

use neomacs_tui_tests::{TuiLaunch, TuiSession, TuiTempDirectory, TuiTerminalConfig};

const PROBE_EL: &str = r##";;; -*- lexical-binding: t -*-
(defvar r2-file (getenv "R2_FILE"))
(defvar r2-dir (getenv "R2_DIR"))
(defvar r2-reorder (getenv "R2_REORDER"))
(defvar r2-extend (getenv "R2_EXTEND"))
(defvar r2-text (or (getenv "R2_TEXT") "arabic"))
(defvar r2-point (getenv "R2_POINT"))
(let ((buf (find-file-noselect r2-file)))
  (with-current-buffer buf
    (setq bidi-display-reordering (not (equal r2-reorder "nil")))
    (setq bidi-paragraph-direction
          (unless (equal r2-dir "nil") (intern r2-dir)))
    (erase-buffer)
    (insert
     (cond
      ((equal r2-text "ascii") "ab\n")
      ((equal r2-text "arabic-tab")
       (decode-coding-string
        (unibyte-string #xd8 #xac #xd8 #xa7 #x09 #xd8 #xa8 #x0a) 'utf-8))
      ((equal r2-text "arabic-controls")
       ;; JA, RLM, ALEF, LRM, newline: zero-width bidi controls around the
       ;; visible letters.  GNU paints no cell for a zero-width character, so
       ;; the right-aligned origin must not count them either.
       (decode-coding-string
        (unibyte-string #xd8 #xac #xe2 #x80 #x8f #xd8 #xa7 #xe2 #x80 #x8e #x0a)
        'utf-8))
      (t
       (decode-coding-string
        (unibyte-string #xd8 #xac #xd8 #xa7 #x0a #xd8 #xac #x0a) 'utf-8))))
    ;; The overlay must cover the INSERTED text: creating it before the
    ;; erase/insert pair leaves it collapsed at 1..1 (itree.c:1104-1114), and
    ;; GNU's row then never carries the extend face.
    (when (equal r2-extend "1")
      (let ((overlay (make-overlay (point-min) (point-max))))
        (overlay-put overlay 'face '(:background "red" :extend t))))
    (goto-char (if r2-point (string-to-number r2-point) (point-min))))
  (switch-to-buffer buf)
  (set-window-point nil (point))
  (redisplay t)
  (run-at-time 10 nil #'kill-emacs))
"##;

/// The reporter's file: `JA`, newline, `JIM`, newline.
const REPORTER_BYTES: [u8; 8] = [0xd8, 0xac, 0xd8, 0xa7, 0x0a, 0xd8, 0xac, 0x0a];

#[derive(Clone, Copy, Debug)]
enum Case {
    Auto,
    RightToLeft,
    LeftToRight,
    Off,
    /// The reporter's line with a TAB inside it and point after the tab.
    RightToLeftTab,
    /// The reporter's letters separated by an RLM, with point on the RLM.
    RlmControl,
    /// The same text with an explicit right-to-left paragraph and point on
    /// the trailing LRM.
    LrmControl,
}

impl Case {
    const ALL: [Case; 7] = [
        Case::Auto,
        Case::RightToLeft,
        Case::LeftToRight,
        Case::Off,
        Case::RightToLeftTab,
        Case::RlmControl,
        Case::LrmControl,
    ];

    fn label(self) -> &'static str {
        match self {
            Case::Auto => "direction-nil",
            Case::RightToLeft => "direction-right-to-left",
            Case::LeftToRight => "direction-left-to-right",
            Case::Off => "reordering-nil",
            Case::RightToLeftTab => "right-to-left-tab",
            Case::RlmControl => "right-to-left-rlm",
            Case::LrmControl => "right-to-left-lrm",
        }
    }

    fn direction_env(self) -> &'static str {
        match self {
            Case::Auto | Case::Off | Case::RightToLeftTab | Case::RlmControl => "nil",
            Case::RightToLeft | Case::LrmControl => "right-to-left",
            Case::LeftToRight => "left-to-right",
        }
    }

    fn reordering_env(self) -> &'static str {
        match self {
            Case::Off => "nil",
            _ => "t",
        }
    }

    fn text_env(self) -> &'static str {
        match self {
            Case::RightToLeftTab => "arabic-tab",
            Case::RlmControl | Case::LrmControl => "arabic-controls",
            _ => "arabic",
        }
    }

    fn point_env(self) -> Option<&'static str> {
        match self {
            Case::RightToLeftTab => Some("4"),
            Case::RlmControl => Some("2"),
            Case::LrmControl => Some("4"),
            _ => None,
        }
    }

    /// Whether GNU right-aligns this row (R2L paragraph).
    fn right_aligned(self) -> bool {
        matches!(
            self,
            Case::Auto
                | Case::RightToLeft
                | Case::RightToLeftTab
                | Case::RlmControl
                | Case::LrmControl
        )
    }
}

fn is_arabic(ch: char) -> bool {
    ch == '\u{062C}' || ch == '\u{0627}'
}

/// vt100 renders an empty cell as an empty string; GNU and Neomacs both mean a
/// blank cell.  Compare them as one cell value.
fn displayed_contents(cell: &vt100::Cell) -> String {
    let contents = cell.contents();
    if contents.is_empty() {
        " ".to_string()
    } else {
        contents.to_string()
    }
}

#[derive(Debug)]
struct RowSnapshot {
    /// The body row's cells as (contents, background) pairs.
    cells: Vec<(String, vt100::Color)>,
    /// Cursor (row, column) in the terminal.
    cursor: (u16, u16),
    /// Terminal row of the Arabic line.
    row: u16,
}

fn capture(program: &Path, engine: &str, case: Case, extend: bool) -> RowSnapshot {
    let home = TuiTempDirectory::new("neomacs-bidi-tty-");
    let file = home.path().join("arabic_test.md");
    std::fs::write(&file, REPORTER_BYTES).expect("write reporter fixture bytes");
    let script = home.path().join("probe.el");
    std::fs::write(&script, PROBE_EL).expect("write TTY bidi probe");

    let mut launch = TuiLaunch::new(program.as_os_str()).arg("-nw").arg("-Q");
    if engine == "GNU" {
        launch = launch.arg("-no-comp-spawn").arg(
            "--eval=(setq native-comp-jit-compilation nil native-comp-async-report-warnings-errors 'silent)",
        );
    }
    launch = launch
        .arg("-l")
        .arg(&script)
        .env("HOME", home.path())
        .env("TMPDIR", home.path())
        .env("LC_ALL", "C.UTF-8")
        .env("R2_FILE", &file)
        .env("R2_DIR", case.direction_env())
        .env("R2_REORDER", case.reordering_env())
        .env("R2_TEXT", case.text_env())
        .env("R2_EXTEND", if extend { "1" } else { "0" });
    if let Some(point) = case.point_env() {
        launch = launch.env("R2_POINT", point);
    }

    let mut session = TuiSession::spawn_launch_on_terminal(
        launch,
        &format!("{engine} {}", case.label()),
        TuiTerminalConfig::new("xterm-256color", 24, 80),
    );
    session.read_until(Duration::from_secs(20), |grid| {
        grid.iter().any(|row| row.chars().any(is_arabic))
    });
    neomacs_tui_tests::pair::settle_session(&mut session);

    let snapshot = {
        let screen = session.screen();
        let (rows, cols) = screen.size();
        let arabic_row = (0..rows).find(|&row| {
            (0..cols).any(|col| {
                screen
                    .cell(row, col)
                    .is_some_and(|cell| cell.contents().chars().any(is_arabic))
            })
        });
        let row = arabic_row.unwrap_or_else(|| {
            panic!(
                "{engine} {}: no Arabic cell reached the terminal grid:\n{}",
                case.label(),
                session.text_grid().join("\n")
            )
        });
        let cells = (0..cols)
            .map(|col| {
                let cell = screen
                    .cell(row, col)
                    .expect("every column of a terminal row has a cell");
                (displayed_contents(cell), cell.bgcolor())
            })
            .collect();
        RowSnapshot {
            cells,
            cursor: screen.cursor_position(),
            row,
        }
    };

    session.send_keys("C-x C-c");
    let _ = session.run_to_completion(Duration::from_secs(10));
    snapshot
}

fn describe(cells: &[(String, vt100::Color)]) -> String {
    cells
        .iter()
        .enumerate()
        .filter(|(_, (contents, bg))| {
            (!contents.is_empty() && contents != " ") || *bg != vt100::Color::Default
        })
        .map(|(col, (contents, bg))| format!("{col}:{contents:?}/{bg:?}"))
        .collect::<Vec<_>>()
        .join(" ")
}

#[test]
fn tty_rtl_rows_match_gnu_cells_cursor_and_extend_face() {
    let gnu_program = neomacs_tui_tests::gnu_oracle_program();
    let neomacs_program = neomacs_tui_tests::neomacs_binary();
    neomacs_tui_tests::assert_neomacs_dumped_runtime(&neomacs_program);
    assert!(
        neomacs_program.exists(),
        "Neomacs binary missing: {}",
        neomacs_program.display()
    );

    for case in Case::ALL {
        let gnu = capture(&gnu_program, "GNU", case, false);
        let neo = capture(&neomacs_program, "Neomacs", case, false);
        eprintln!(
            "bidi-tty case={} row={} gnu.cursor={:?} neo.cursor={:?} gnu_row={}",
            case.label(),
            gnu.row,
            gnu.cursor,
            neo.cursor,
            describe(&gnu.cells)
        );

        // The row must be the same width and match cell for cell, including
        // the exact rightmost column.
        assert_eq!(
            neo.cells.len(),
            gnu.cells.len(),
            "{}: terminal row widths differ",
            case.label()
        );
        let mismatches: Vec<String> = gnu
            .cells
            .iter()
            .zip(neo.cells.iter())
            .enumerate()
            .filter(|(_, (g, n))| g.0 != n.0)
            .map(|(col, (g, n))| format!("col {col}: GNU {:?} vs Neo {:?}", g.0, n.0))
            .collect();
        assert!(
            mismatches.is_empty(),
            "{}: TTY cells differ from GNU:\n  GNU: {}\n  Neo: {}\n  {}",
            case.label(),
            describe(&gnu.cells),
            describe(&neo.cells),
            mismatches.join("\n  ")
        );

        // Anchor the exact margin.  GNU's R2L row ends in the last column;
        // the forward row starts in column zero.
        if case.right_aligned() {
            let last = gnu.cells.len() - 1;
            assert!(
                gnu.cells[last].0.chars().any(is_arabic),
                "{}: GNU's R2L row must reach the last column; row: {}",
                case.label(),
                describe(&gnu.cells)
            );
        } else {
            assert!(
                gnu.cells[0].0.chars().any(is_arabic),
                "{}: GNU's forward row must start in column zero; row: {}",
                case.label(),
                describe(&gnu.cells)
            );
        }

        // The cursor cell must agree with GNU's, and the scenario must really
        // have point on the Arabic line.
        assert_eq!(
            gnu.cursor.0,
            gnu.row,
            "{}: GNU's probe did not put the cursor on the Arabic line: {:?} vs row {}",
            case.label(),
            gnu.cursor,
            gnu.row
        );
        assert_eq!(
            (neo.cursor.0 as i64) - (neo.row as i64),
            (gnu.cursor.0 as i64) - (gnu.row as i64),
            "{}: Neo's cursor is on a different row from GNU's",
            case.label()
        );
        assert_eq!(
            neo.cursor.1,
            gnu.cursor.1,
            "{}: Neo's cursor cell differs from GNU's ({:?} vs {:?})",
            case.label(),
            neo.cursor,
            gnu.cursor
        );
    }

    // The extend face: GNU paints the reversed row's left blanks with the
    // end-of-line extend face.  Compare the whole row's attributes, and
    // require GNU itself to have colored the row's left edge.
    for case in [Case::Auto, Case::RightToLeft] {
        let gnu = capture(&gnu_program, "GNU", case, true);
        let neo = capture(&neomacs_program, "Neomacs", case, true);
        eprintln!(
            "bidi-tty extend case={} gnu.cell0.bg={:?} neo.cell0.bg={:?}",
            case.label(),
            gnu.cells[0].1,
            neo.cells[0].1
        );
        assert_ne!(
            gnu.cells[0].1,
            vt100::Color::Default,
            "{}: GNU did not paint the extend face on the left blanks; row: {}",
            case.label(),
            describe(&gnu.cells)
        );
        let mismatches: Vec<String> = gnu
            .cells
            .iter()
            .zip(neo.cells.iter())
            .enumerate()
            .filter(|(_, (g, n))| g != n)
            .map(|(col, (g, n))| format!("col {col}: GNU {:?} vs Neo {:?}", g, n))
            .collect();
        assert!(
            mismatches.is_empty(),
            "{} extend face: TTY cells/attrs differ from GNU:\n  {}\n  {}",
            case.label(),
            describe(&gnu.cells),
            mismatches.join("\n  ")
        );
    }
}
