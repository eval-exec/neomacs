#![cfg(unix)]
//! Regression coverage for issue #445: an underline face run starting ON a
//! zero-width variation selector (the shape behind ibuffer group headers
//! like "🛠\u{FE0F}\u{FE0F} Emacs Internal" under the issue's advice) rendered
//! an empty window body in Neomacs while GNU kept every line visible.
//!
//! The fixture drives the SAME startup-shaped render the issue hits: the
//! styled buffer is displayed by the FIRST frame layout, not by an
//! incremental re-render after the fact.

use crate::support;

use neomacs_tui_tests::TuiTempFile;
use std::time::Duration;

use support::{assert_pair_exact_display, boot_pair, dump_pair_grids, read_both};

/// The rendered line each case must keep visible.
const TAIL: &str = "tail tail tail";

fn render_fixture(span_from: usize, span_to: usize) -> TuiTempFile {
    TuiTempFile::new(
        "neomacs-issue-445",
        "render-setup.el",
        // Double the issue's exact shape at the character level: the underline
        // run starts ON a zero-width variation selector that follows a base
        // character. GNU renders the whole line; the layout bug blanked it.
        format!(
            "(with-current-buffer (get-buffer-create \"*render*\")\
             \n  (insert (concat \"a\" \"\\uFE0F\" \" {TAIL}\"))\
             \n  (put-text-property {span_from} {span_to} 'face '(:underline t))\
             \n  (goto-char (point-min))\
             \n  (set-window-buffer nil (current-buffer)))\n",
            TAIL = TAIL,
            span_from = span_from,
            span_to = span_to,
        ),
    )
}

fn assert_tail_visible(
    label: &str,
    gnu: &mut neomacs_tui_tests::TuiSession,
    neo: &mut neomacs_tui_tests::TuiSession,
    require_underline: bool,
) {
    read_both(gnu, neo, Duration::from_secs(1));
    let tail_visible = |grid: &[String]| grid.iter().any(|row| row.contains(TAIL));
    let gnu_ok = tail_visible(&gnu.text_grid());
    let neo_ok = tail_visible(&neo.text_grid());
    if !gnu_ok || !neo_ok {
        dump_pair_grids(label, gnu, neo);
    }
    assert!(gnu_ok, "{label}: GNU oracle must keep the line visible");
    assert!(
        neo_ok,
        "{label}: Neomacs must keep the line visible (issue #445 blanked the row)"
    );
    if require_underline {
        let tail_row = gnu
            .text_grid()
            .iter()
            .position(|row| row.contains(TAIL))
            .expect("GNU tail row present") as u16;
        let (_, cols) = gnu.screen_size();
        let gnu_has_underline = (0..cols).any(|col| {
            gnu.screen()
                .cell(tail_row, col)
                .is_some_and(|cell| cell.underline())
        });
        assert!(
            gnu_has_underline,
            "{label}: precondition failed — GNU row {tail_row} should carry underline attribute"
        );
    }
    assert_pair_exact_display(label, gnu, neo);
}

#[test]
fn face_boundary_on_zero_width_extender_keeps_line_visible() {
    let fixture = render_fixture(2, 3);
    let (mut gnu, mut neo) = boot_pair(&format!("-l {}", fixture.path().display()));
    assert_tail_visible(
        "issue #445/face-on-single-selector",
        &mut gnu,
        &mut neo,
        false,
    );
}

#[test]
fn face_boundary_on_second_extender_keeps_line_visible() {
    // The user's literal header shape: 🛠 + VS16 + VS16 + text, underline
    // starting on the SECOND selector — the advice's `[\uFE0F\u20E3]?` icon
    // match leaves it at the start of the styled run.
    let fixture = TuiTempFile::new(
        "neomacs-issue-445",
        "render-setup.el",
        format!(
            "(with-current-buffer (get-buffer-create \"*render*\")\
             \n  (insert (concat \"\\U0001F6E0\" \"\\uFE0F\" \"\\uFE0F\" \" Emacs Internal {TAIL}\"))\
             \n  (put-text-property 3 20 'face '(:underline t))\
             \n  (goto-char (point-min))\
             \n  (set-window-buffer nil (current-buffer)))\n",
            TAIL = TAIL,
        ),
    );
    let (mut gnu, mut neo) = boot_pair(&format!("-l {}", fixture.path().display()));
    assert_tail_visible(
        "issue #445/face-on-doubled-selector",
        &mut gnu,
        &mut neo,
        true,
    );
}
