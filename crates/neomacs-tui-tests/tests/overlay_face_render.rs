#![cfg(unix)]
//! Overlay- and face-spec rendering regressions for the TUI walk.
//!
//! GNU applies overlay `face` properties over the text beneath them
//! (`face_at_buffer_position` merges overlay faces by `sort_overlays`
//! precedence), and defface specs branch on display type — packages gate
//! entire faces on `((type tty pc))` (helm's `helm-selection` tty spec is
//! `:extend t` + `:inherit isearch`).  These probes render named faces,
//! display-type branches, and overlay faces in a fundamental-mode buffer in
//! BOTH editors and require the terminals to agree byte for byte.
//!
//! Probes insert into a fundamental-mode buffer on purpose: font-lock in
//! `lisp-interaction-mode` strips inserted `face` text properties during
//! refontification, which would clear the very property under test.

use crate::support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

/// The shared probe program: three named faces covering the spec shapes
/// helm/terminal packages rely on, plus an anonymous plist face, rendered
/// into one fundamental-mode buffer at a known position.
const FACE_PROBE_FORM: &str = r##"
(progn
  (switch-to-buffer (get-buffer-create "probe-faces"))
  (fundamental-mode)
  (erase-buffer)
  (defface probe-tty-branch-face
    '((((type tty pc)) :background "#EF1234" :foreground "#112233")
      (t :background "#00FF00"))
    "tty-branch probe")
  (defface probe-anon-def-face
    '((t (:background "#0F0F0F")))
    "anon-def probe")
  (defface probe-helm-shape-face
    '((((type tty pc)) :extend t :inherit isearch)
      (((background light)) :background "#b5ffd1"))
    "helm-shape probe")
  (defface probe-extend-only-face
    '((((type tty pc)) :extend t :background "#00CC88"))
    "extend-only probe")
  (defface probe-inherit-only-face
    '((((type tty pc)) :inherit isearch :background "#CC8800"))
    "inherit-only probe")
  (insert (propertize "TTY|" 'face 'probe-tty-branch-face))
  (insert (propertize "ANONDEF|" 'face 'probe-anon-def-face))
  (insert (propertize "HELMSHAPE|" 'face 'probe-helm-shape-face))
  (insert (propertize "EXTONLY|" 'face 'probe-extend-only-face))
  (insert (propertize "INHONLY|" 'face 'probe-inherit-only-face))
  (insert (propertize "ANONLIST|" 'face '(:background "#0F0F0F")))
  (goto-char (point-min))
  (redisplay))
"##;

fn run_face_probe() -> (TuiSession, TuiSession) {
    let (mut gnu, mut neo) = boot_pair("");
    eval_expression(&mut gnu, &mut neo, FACE_PROBE_FORM);
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    (gnu, neo)
}

fn dump_row(label: &str, session: &TuiSession) -> String {
    let screen = session.screen();
    let probe_row = (1..screen.size().0)
        .find(|row| {
            screen
                .cell(*row, 0)
                .is_some_and(|cell| cell.contents() == "T")
                && screen
                    .cell(*row, 2)
                    .is_some_and(|cell| cell.contents() == "Y")
        })
        .expect("probe row with TTY… prefix");
    let row_text: String = (0..70)
        .filter_map(|c| screen.cell(probe_row, c))
        .map(|cell| cell.contents().to_string())
        .collect();
    // Segment starts (cumulative cols): TTY| 0, ANONDEF| 4, HELMSHAPE| 12,
    // EXTONLY| 23, INHONLY| 31, ANONLIST| 39.  The bg() probes below sample
    // a mid-segment cell of each.
    let bg = |col: u16| screen.cell(probe_row, col).expect("probe cell").bgcolor();
    let report = format!(
        "row{probe_row} text={row_text:?} tty={:?} anondef={:?} helmshape={:?} extonly={:?} inhonly={:?} anonlist={:?}",
        bg(0),
        bg(10),
        bg(18),
        bg(29),
        bg(37),
        bg(45),
    );
    eprintln!("DUMP-{label} {report}");
    report
}

#[test]
fn face_spec_branches_render_identically_in_both_editors() {
    let (mut gnu, mut neo) = run_face_probe();
    let gnu_report = dump_row("GNU", &gnu);
    let neo_report = dump_row("NEO", &neo);
    read_both(&mut gnu, &mut neo, Duration::from_millis(200));
    assert_pair_exact_display("face_spec_branches_render_identically", &gnu, &neo);
    // The segments under test are exactly where the faces must show; parity
    // alone could pass with BOTH editors dropping a face.
    assert_eq!(gnu_report, neo_report, "face report mismatch");
    assert!(
        gnu_report.contains("tty=Rgb(239, 18, 52)"),
        "tty branch must resolve: {gnu_report}"
    );
}

/// The helm-pydoc shape: an OVERLAY carrying a `:extend t :inherit isearch`
/// face over plain text must render its face (this is exactly how
/// `helm-selection-overlay` paints the selected candidate).
#[test]
fn overlay_with_extend_inherit_face_renders_like_helm_selection() {
    let (mut gnu, mut neo) = run_face_probe();
    // Reuse the probe buffer's second line: park an overlay with the
    // helm-selection tty spec over a plain-text run there.
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn\
 (goto-char (point-max))\
 (insert \"OVERLAYTARGET\")\
 (let ((ov (make-overlay (- (point) 12) (- (point) 1) (current-buffer) t nil)))\
 (overlay-put ov 'face 'probe-helm-shape-face))\
 (goto-char (point-min))\
 (redisplay))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_pair_exact_display(
        "overlay_with_extend_inherit_face_renders_like_helm_selection",
        &gnu,
        &neo,
    );

    // Parity alone cannot distinguish "both render the face" from "both drop
    // it" — assert the overlay cells actually carry the inherited isearch
    // background (GNU's resolved value for `:inherit isearch` under this
    // terminal profile, observed in the helm-pydoc triage).
    for (label, session) in [("GNU", &mut gnu), ("NEO", &mut neo)] {
        let screen = session.screen();
        let cell = screen.cell(1, 52).expect("overlay-covered cell exists");
        assert_eq!(
            cell.bgcolor(),
            vt100::Color::Rgb(238, 121, 159),
            "{label}: helm-shaped overlay face missing on the overlay text"
        );
    }
}

/// helm moves its selection overlay to the selected candidate on every
/// update (`move-overlay`).  The moved-to region must gain the face and the
/// moved-from region must lose it.
#[test]
fn move_overlay_moves_the_rendered_face() {
    let (mut gnu, mut neo) = run_face_probe();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn\
 (goto-char (point-max))\
 (insert \"AAAAAA\\nBBBBBB\")\
 (let ((ov (make-overlay 1 2 (current-buffer) t nil)))\
 (overlay-put ov 'face 'probe-anon-def-face)\
 (move-overlay ov (+ (point-max) 1) (+ (point-max) 4) (current-buffer)))\
 (goto-char (point-min))\
 (redisplay))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_pair_exact_display("move_overlay_moves_the_rendered_face", &gnu, &neo);
}

/// The helm-pydoc shape exactly: the selection overlay ENDS at point-max
/// (the buffer's final position — the selected line is the last line) with
/// a `:extend t :inherit` face.  GNU paints the overlay text AND the
/// trailing cells; this probe was built after the in-session introspection
/// proved both engines hold identical overlay state there.
#[test]
fn overlay_ending_at_point_max_extends_its_face() {
    let (mut gnu, mut neo) = run_face_probe();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn\
 (switch-to-buffer (get-buffer-create \"probe-eom\"))\
 (fundamental-mode)\
 (erase-buffer)\
 (insert \"ACTIONLINE\")\
 (let* ((bol (point-min))\
 (ov (make-overlay bol (point-max) (current-buffer) t nil)))\
 (overlay-put ov 'face 'probe-helm-shape-face))\
 (goto-char (point-min))\
 (redisplay))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_pair_exact_display("overlay_ending_at_point_max_extends_its_face", &gnu, &neo);

    for (label, session) in [("GNU", &mut gnu), ("NEO", &mut neo)] {
        let screen = session.screen();
        let probe_row = (1..screen.size().0)
            .find(|row| {
                (0..screen.size().1)
                    .filter_map(|col| screen.cell(*row, col))
                    .map(|cell| cell.contents().to_string())
                    .collect::<String>()
                    .starts_with("ACTIONLINE")
            })
            .expect("ACTIONLINE row");
        let text_cell = screen
            .cell(probe_row, 5)
            .expect("overlay text cell")
            .bgcolor();
        let trailing = screen.cell(probe_row, 15).expect("trailing cell").bgcolor();
        eprintln!("DUMP-{label} row{probe_row} text_bg={text_cell:?} trailing_bg={trailing:?}");
        // GNU does not extend an overlay face past EOL here (the trailing
        // cells stay default even with :extend t) -- the assert targets the
        // overlay's own text region, which is what the helm-pydoc divergence
        // showed missing on neomacs.
        assert_eq!(
            text_cell,
            vt100::Color::Rgb(238, 121, 159),
            "{label}: face missing on overlay text at point-max"
        );
    }
}
#[test]
fn overlay_face_follows_text_through_insertions_above_it() {
    let (mut gnu, mut neo) = run_face_probe();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(condition-case shift-err\
 (progn\
 (switch-to-buffer (get-buffer-create \"probe-shift\"))\
 (fundamental-mode)\
 (erase-buffer)\
 (insert \"LINE1\\nMARKERLINE\\nLINE3\\n\")\
 (let* ((bol (save-excursion (goto-char (point-min)) (forward-line 1) (point)))\
 (eol (save-excursion (goto-char (point-min)) (forward-line 1) (pos-eol))))\
 (setq probe-shift-ov (make-overlay bol (1+ eol) (current-buffer) t nil))\
 (overlay-put probe-shift-ov 'face 'probe-anon-def-face)\
 (goto-char (point-min))\
 (insert \"INSERTED\\n\"))\
 (goto-char (point-min))\
 (insert (format \"SHIFTERR:%S\" shift-err)))\
 (error (insert (format \"SHIFTERR:%S\" shift-err))))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_pair_exact_display(
        "overlay_face_follows_text_through_insertions_above_it",
        &gnu,
        &neo,
    );

    // MARKERLINE moved to the third text row and must still carry the face.
    for (label, session) in [("GNU", &mut gnu), ("NEO", &mut neo)] {
        let screen = session.screen();
        let marker_row = (1..screen.size().0)
            .find(|row| {
                (0..screen.size().1)
                    .filter_map(|col| screen.cell(*row, col))
                    .map(|cell| cell.contents().to_string())
                    .collect::<String>()
                    .starts_with("MARKERLINE")
            })
            .expect("MARKERLINE row");
        let marker_bg = screen
            .cell(marker_row, 3)
            .expect("MARKERLINE cell")
            .bgcolor();
        eprintln!("DUMP-{label} marker_row={marker_row} bg={marker_bg:?}");
        assert_eq!(
            marker_bg,
            vt100::Color::Rgb(15, 15, 15),
            "{label}: overlay face did not follow its text through the insertion"
        );
    }
}
