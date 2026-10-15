#![cfg(unix)]
//! Regression coverage for issue #441: `global-display-line-numbers-mode`
//! with `display-line-numbers-type` set to `absolute` must light the gutter
//! in buffers created AFTER the global mode enabled — the file buffer the
//! issue visits — not only in the buffers that existed at init time (the
//! *Messages* the reporter saw).
//!
//! The fixture is the issue's exact init: enable the global mode, then visit
//! a file. GNU renders `N  text` gutters in the visited buffer; the Neomacs
//! divergence is either the globalized-minor-mode's new-buffer enabler never
//! firing (the local mode stays off) or the display engine ignoring the
//! mode's `display-line-numbers` variable — a session-side probe tells those
//! apart during the fix, and the exact-pair match pins the rendered geometry.

use crate::support;

use neomacs_tui_tests::TuiTempFile;
use std::time::Duration;

use support::{boot_pair, dump_pair_grids, read_both};

/// The issue's init, verbatim, plus a visit of a multi-line file so the
/// first frame after startup displays a buffer created after the global
/// mode turned on.
fn init_fixture(visited: &std::path::Path) -> TuiTempFile {
    TuiTempFile::new(
        "neomacs-issue-441",
        "init.el",
        format!(
            ";;; issue-441 init -*- lexical-binding: t; -*-\n\
             (global-display-line-numbers-mode 1)\n\
             (setq display-line-numbers-type 'absolute)\n\
             (find-file {visited:?})\n",
            visited = visited.display().to_string(),
        ),
    )
}

const BODY: &str = "alpha line";
const TAIL: &str = "last line";

#[test]
fn display_string_newlines_do_not_number_synthetic_rows_or_shift_buffer_lines() {
    let (mut gnu, mut neo) = boot_pair("");
    support::eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn
          (erase-buffer)
          (dotimes (i 5) (insert (format "line %d\n" (1+ i))))
          (display-line-numbers-mode 1)
          (goto-char (point-min))
          (let ((eol (line-end-position)))
            (overlay-put (make-overlay eol (1+ eol)) 'display "  [A]\n  [B]\n"))
          (forward-line 2)
          nil)"##,
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let rows = |grid: Vec<String>| {
        grid.into_iter()
            .filter(|row| row.contains("line ") || row.contains("[B]"))
            .map(|row| row.trim().to_owned())
            .collect::<Vec<_>>()
    };
    let expected = [
        "1 line 1  [A]",
        "[B]",
        "2 line 2",
        "3 line 3",
        "4 line 4",
        "5 line 5",
    ];
    assert_eq!(rows(gnu.text_grid()), expected, "GNU 31.1 oracle");
    assert_eq!(rows(neo.text_grid()), expected, "issue #574");

    support::eval_expression(&mut gnu, &mut neo, "(setq display-line-numbers 'relative)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let relative = [
        "2 line 1  [A]",
        "[B]",
        "1 line 2",
        "3 line 3",
        "1 line 4",
        "2 line 5",
    ];
    assert_eq!(rows(gnu.text_grid()), relative, "GNU relative-mode oracle");
    assert_eq!(
        rows(neo.text_grid()),
        relative,
        "relative display-string rows"
    );
}

#[test]
fn display_replacement_counts_the_buffer_newlines_it_hides() {
    let (mut gnu, mut neo) = boot_pair("");
    support::eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn
          (erase-buffer)
          (dotimes (i 5) (insert (format "line %d\n" (1+ i))))
          (display-line-numbers-mode 1)
          (goto-char (point-min))
          (forward-line 1)
          (put-text-property (point) (progn (forward-line 2) (point)) 'display "[hidden] ")
          (goto-char (point-max))
          (forward-line -1)
          nil)"##,
    );
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let rows = |grid: Vec<String>| {
        grid.into_iter()
            .filter(|row| row.contains("line "))
            .map(|row| row.trim().to_owned())
            .collect::<Vec<_>>()
    };
    let expected = ["1 line 1", "2 [hidden] line 4", "5 line 5"];
    assert_eq!(rows(gnu.text_grid()), expected, "GNU 31.1 oracle");
    assert_eq!(
        rows(neo.text_grid()),
        expected,
        "issue #574 hidden buffer lines"
    );

    support::eval_expression(&mut gnu, &mut neo, "(setq display-line-numbers 'relative)");
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));
    let relative = ["4 line 1", "3 [hidden] line 4", "5 line 5"];
    assert_eq!(rows(gnu.text_grid()), relative, "GNU relative-mode oracle");
    assert_eq!(
        rows(neo.text_grid()),
        relative,
        "relative hidden buffer lines"
    );
}

#[test]
fn global_display_line_numbers_lights_the_gutter_in_a_buffer_created_after_startup() {
    let visited = TuiTempFile::new(
        "neomacs-issue-441-visited-",
        "visited.txt",
        format!("{BODY}\nmiddle line\n{TAIL}\n"),
    );
    let init = init_fixture(visited.path());
    let (mut gnu, mut neo) = boot_pair(&format!("-l {}", init.display()));
    read_both(&mut gnu, &mut neo, Duration::from_secs(2));

    // The visited file's first row must carry the absolute line number 1 in
    // the gutter, before the text — GNU's right-aligned number column plus
    // one space.
    let gutter = |grid: &[String]| {
        grid.iter()
            .any(|row| row.contains('1') && row.contains(BODY) && row.find('1') < row.find(BODY))
    };
    let gnu_ok = gutter(&gnu.text_grid());
    let neo_ok = gutter(&neo.text_grid());
    if !gnu_ok || !neo_ok {
        dump_pair_grids("global_display_line_numbers_gutter", &gnu, &neo);
    }
    assert!(
        gnu_ok,
        "GNU oracle: the visited file's first line must show gutter number 1"
    );
    assert!(
        neo_ok,
        "issue #441: the file buffer created after the global mode enabled \
         must show line numbers"
    );

    // Fall through to the exact-pair match: it pins the full rendered grid —
    // gutters on every line, the mode line, everything.
    support::assert_pair_exact_display("global_display_line_numbers_lights_the_gutter", &gnu, &neo);
}
