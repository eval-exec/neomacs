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
        grid.iter().any(|row| {
            row.contains('1') && row.contains(BODY) && row.find('1') < row.find(BODY)
        })
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
    support::assert_pair_exact_display(
        "global_display_line_numbers_lights_the_gutter",
        &gnu,
        &neo,
    );
}
