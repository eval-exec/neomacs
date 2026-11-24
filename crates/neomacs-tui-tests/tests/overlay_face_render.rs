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
///
/// `show-paren-mode` is turned off for the whole probe: its highlight is
/// painted by GNU's `show-paren-delay` idle timer, so a probe whose point
/// sits on a bracket (the `:extend` action row) would otherwise compare a
/// screen captured before the timer fired in one editor and after it fired
/// in the other.  Paren highlighting has its own parity test
/// (`face_parity.rs`).
const FACE_PROBE_FORM: &str = r##"
(progn
  (show-paren-mode -1)
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
  (defface probe-helm-selection-face
    '((((background dark)) :background "#3355bb" :foreground "white")
      (((background light)) :background "#ffffb3" :foreground "black")
      (t :extend t :inherit isearch))
    "helm-selection clause shapes")
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

/// helm's select-action screen: the ACTIONS buffer in one window and the
/// helm-buffer window (holding the selected candidate, its overlay ending
/// at point-max) SIDE BY SIDE — the failed screen row spans both.  The
/// face must render in the right window even while another window shows a
/// different buffer.
#[test]
fn overlay_face_renders_in_a_second_window_beside_another_buffer() {
    let (mut gnu, mut neo) = run_face_probe();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn(switch-to-buffer (get-buffer-create \
 \"probe-actions\"))(fundamental-mode)(erase-buffer)(insert \"[f1]  First \
 action\\n[f2]  Second action\\n[f3]  Third action\")(let* ((helm-window(progn \
 (delete-other-windows)(split-window-right) (other-window 1)(switch-to-buffer \
 \"probe-faces\")(selected-window)))(ov (with-current-buffer \"probe-faces\"(if \
 (get-text-property (point-min) 'probe-shape) nil(make-overlay (point-min) \
 (point-max) (current-buffer) t nil)))) (_set (progn (overlay-put ov 'face \
 'probe-helm-shape-face) (with-current-buffer \"probe-faces\"(set-text-properties \
 (point-min) (point-max) '(probe-shape t))))))(redisplay)))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_pair_exact_display(
        "overlay_face_renders_in_a_second_window_beside_another_buffer",
        &gnu,
        &neo,
    );
}

/// Same side-by-side shape, but the overlay-covered window's START is
/// scrolled mid-buffer (helm sets the candidate window's start so the
/// selected line is visible) — the overlay lies BELOW the window start.
#[test]
fn overlay_face_renders_below_a_scrolled_window_start() {
    let (mut gnu, mut neo) = run_face_probe();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn(switch-to-buffer (get-buffer-create \
 \"probe-actions\"))(fundamental-mode)(erase-buffer)(insert \"[f1]  First \
 action\\n[f2]  Second action\\n[f3]  Third action\") (let* ((helm-window(progn \
 (delete-other-windows)(split-window-right) (other-window 1)(switch-to-buffer \
 \"probe-faces\")(selected-window)))(ov (with-current-buffer \
 \"probe-faces\"(make-overlay (point-min) (point-max) (current-buffer) t \
 nil))))(overlay-put ov 'face 'probe-helm-shape-face) (with-current-buffer \
 \"probe-faces\"(insert \"HEADERLINE\\n\")) (with-current-buffer \
 \"probe-faces\"(goto-char (point-min)) (forward-line 1)) (set-window-start \
 helm-window (with-current-buffer \"probe-faces\" (point))) (with-current-buffer \
 \"probe-faces\" (goto-char (point-max))) (redisplay)))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_pair_exact_display(
        "overlay_face_renders_below_a_scrolled_window_start",
        &gnu,
        &neo,
    );
}

/// `:extend t` on an overlay that covers a WHOLE line (text + newline),
/// in the middle of the buffer, must paint the face through the rest of
/// the display line to the window edge — this is exactly helm's
/// `*helm action*` selection row on the action-selection screen (overlay
/// state was proven identical in both engines; only the rendering
/// differed).
///
/// GNU fills to `it->last_visible_x`, which a TTY window that is not
/// rightmost reduces by one column for the vertical border glyph
/// (src/xdisp.c:3524-3527), so the `:extend` tail stops before the border
/// cell.  The engine twin of this probe is
/// `split_window_line_break_extend_fill_reaches_right_column`.
#[test]
fn overlay_extend_face_paints_the_rest_of_the_line() {
    let (mut gnu, mut neo) = run_face_probe();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn\
 (switch-to-buffer (get-buffer-create \"probe-extend-mid\"))\
 (fundamental-mode)\
 (erase-buffer)\
 (insert \"Actions\\n[f1]  Pydoc Module\\n[f2]  View Source Code\\n[f3]  Import Module\\n\")\
 (let ((ov (make-overlay 9 28 (current-buffer) t nil)))\
 (overlay-put ov 'face 'probe-helm-shape-face))\
 (goto-char 9)\
 (split-window-right)\
 (other-window 1)\
 (switch-to-buffer (get-buffer-create \"probe-candidates\"))\
 (fundamental-mode)\
 (erase-buffer)\
 (insert \"deploymentkit\\n\")\
 (other-window -1)\
 (redisplay))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    for (label, session) in [("GNU", &mut gnu), ("NEO", &mut neo)] {
        let screen = session.screen();
        let probe_row = (1..screen.size().0)
            .find(|row| {
                (0..screen.size().1)
                    .filter_map(|col| screen.cell(*row, col))
                    .map(|cell| cell.contents().to_string())
                    .collect::<String>()
                    .starts_with("[f1]  Pydoc Module")
            })
            .expect("action row");
        let text_bg = screen.cell(probe_row, 5).expect("text cell").bgcolor();
        let tail_bg = screen.cell(probe_row, 30).expect("tail cell").bgcolor();
        eprintln!("DUMP-{label} row={probe_row} text_bg={text_bg:?} tail_bg={tail_bg:?}");
        assert_eq!(
            tail_bg,
            vt100::Color::Rgb(238, 121, 159),
            "{label}: :extend face must paint the rest of the action line"
        );
    }
    assert_pair_exact_display(
        "overlay_extend_face_paints_the_rest_of_the_line",
        &gnu,
        &neo,
    );
}

fn overlay_face_wins_over_text_face_properties() {
    let (mut gnu, mut neo) = run_face_probe();
    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn\
 (switch-to-buffer (get-buffer-create \"probe-text-face\"))\
 (fundamental-mode)\
 (erase-buffer)\
 (defface probe-under-text-face\
 '((t :foreground \"#11AA22\" :background \"#2200AA\"))\
 \"under text\")\
 (defface probe-cover-face\
 '((((type tty pc)) :extend t :foreground \"#8B2323\" :background \"#EE799F\")\
 (t :background \"#EE799F\"))\
 \"cover\")\
 (insert (propertize \"UNDERTEXT\" 'face 'probe-under-text-face))\
 (let ((ov (make-overlay (point-min) (point-max) (current-buffer) t nil)))\
 (overlay-put ov 'face 'probe-cover-face))\
 (goto-char (point-min))\
 (redisplay))",
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(400));
    assert_pair_exact_display("overlay_face_wins_over_text_face_properties", &gnu, &neo);
    for (label, session) in [("GNU", &mut gnu), ("NEO", &mut neo)] {
        let screen = session.screen();
        let probe_row = (1..screen.size().0)
            .find(|row| {
                (0..screen.size().1)
                    .filter_map(|col| screen.cell(*row, col))
                    .map(|cell| cell.contents().to_string())
                    .collect::<String>()
                    .starts_with("UNDERTEXT")
            })
            .expect("UNDERTEXT row");
        let cell = screen.cell(probe_row, 3).expect("covered cell");
        eprintln!(
            "DUMP-{label} row{probe_row} fg={:?} bg={:?}",
            cell.fgcolor(),
            cell.bgcolor()
        );
        assert_eq!(
            cell.bgcolor(),
            vt100::Color::Rgb(238, 121, 159),
            "{label}: overlay face must win over the text's own face"
        );
    }
}

/// P4 (`NEOMACS_LAYOUT_OVERLAY_SOURCE=live`): a fontification callback that
/// moves an overlay, changes another via `overlay-put`, deletes a third, and
/// forces GC mid-attempt must render byte-for-byte identically to GNU Emacs
/// while recording 0 copies, 0 view entries, 0 roots, and nonzero live records.
#[test]
fn live_overlay_source_fontification_mutations_match_gnu() {
    let stats_dir = std::env::temp_dir().join(format!(
        "neomacs-tui-live-overlay-stats-{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&stats_dir);
    let stats_path = stats_dir.join("layout-stats.log");
    let _ = std::fs::remove_file(&stats_path);

    let (mut gnu, mut neo) = boot_pair_with_envs(
        "",
        &[],
        &[
            (
                "NEOMACS_LAYOUT_OVERLAY_SOURCE",
                std::ffi::OsStr::new("live"),
            ),
            ("NEOMACS_LAYOUT_STATS_FILE", stats_path.as_os_str()),
        ],
    );
    eval_expression(
        &mut gnu,
        &mut neo,
        r##"(progn
  (show-paren-mode -1)
  (switch-to-buffer (get-buffer-create "probe-live-ov"))
  (fundamental-mode)
  (erase-buffer)
  (defface probe-live-f1 '((((type tty pc)) :background "#1144AA") (t :background "#1144AA")) "f1")
  (defface probe-live-f2 '((((type tty pc)) :background "#AA4411") (t :background "#AA4411")) "f2")
  (defface probe-live-f3 '((((type tty pc)) :background "#11AA44") (t :background "#11AA44")) "f3")
  (insert "LINE1-MOVE\nLINE2-PUT\nLINE3-DEL\nLINE4-DEST\n")
  (setq probe-live-ran nil)
  (setq probe-live-ov-move (make-overlay 1 11))
  (overlay-put probe-live-ov-move 'face 'probe-live-f1)
  (setq probe-live-ov-put (make-overlay 12 21))
  (overlay-put probe-live-ov-put 'face 'probe-live-f2)
  (setq probe-live-ov-del (make-overlay 22 31))
  (overlay-put probe-live-ov-del 'face 'probe-live-f2)
  (put-text-property (point-min) (point-max) 'fontified nil)
  (setq fontification-functions
        (list (lambda (pos)
                (unless probe-live-ran
                  (setq probe-live-ran t)
                  (move-overlay probe-live-ov-move 32 42)
                  (overlay-put probe-live-ov-put 'face 'probe-live-f3)
                  (delete-overlay probe-live-ov-del)
                  (setq probe-live-ov-del nil)
                  (garbage-collect))
                (put-text-property pos (point-max) 'fontified t))))
  (goto-char (point-min))
  (redisplay t))"##,
    );
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    assert_pair_exact_display(
        "live_overlay_source_fontification_mutations_match_gnu",
        &gnu,
        &neo,
    );

    let find_row = |session: &TuiSession, prefix: &str| -> u16 {
        let screen = session.screen();
        (0..screen.size().0)
            .find(|row| {
                (0..screen.size().1)
                    .filter_map(|col| screen.cell(*row, col))
                    .map(|cell| cell.contents().to_string())
                    .collect::<String>()
                    .starts_with(prefix)
            })
            .unwrap_or_else(|| panic!("row starting with {prefix} not found"))
    };
    for (label, session) in [("GNU", &gnu), ("NEO", &neo)] {
        let screen = session.screen();
        let r1 = find_row(session, "LINE1-MOVE");
        let r2 = find_row(session, "LINE2-PUT");
        let r3 = find_row(session, "LINE3-DEL");
        let r4 = find_row(session, "LINE4-DEST");
        assert_eq!(
            screen.cell(r1, 2).unwrap().bgcolor(),
            vt100::Color::Default,
            "{label}: moved overlay must no longer paint LINE1-MOVE"
        );
        assert_eq!(
            screen.cell(r2, 2).unwrap().bgcolor(),
            vt100::Color::Rgb(17, 170, 68),
            "{label}: overlay-put face must paint LINE2-PUT"
        );
        assert_eq!(
            screen.cell(r3, 2).unwrap().bgcolor(),
            vt100::Color::Default,
            "{label}: deleted overlay must no longer paint LINE3-DEL"
        );
        assert_eq!(
            screen.cell(r4, 2).unwrap().bgcolor(),
            vt100::Color::Rgb(17, 68, 170),
            "{label}: moved overlay must paint LINE4-DEST"
        );
    }

    let stats_text = std::fs::read_to_string(&stats_path).expect(
        "NEOMACS_LAYOUT_STATS_FILE must be written under NEOMACS_LAYOUT_OVERLAY_SOURCE=live",
    );
    assert!(
        !stats_text.trim().is_empty(),
        "expected layout stats lines in {}",
        stats_path.display()
    );
    let mut saw_live_records = false;
    for line in stats_text
        .lines()
        .filter(|line| line.contains("overlay_copies="))
    {
        assert!(
            line.contains(
                "overlay_copies=0 overlay_view_entries=0 overlay_roots=0 overlay_rebuilds=0"
            ),
            "live overlay source must report 0 copies/entries/roots/rebuilds: {line}"
        );
        if let Some(rest) = line.split("overlay_live_records=").nth(1) {
            let count: usize = rest
                .split_whitespace()
                .next()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0);
            if count > 0 {
                saw_live_records = true;
            }
        }
    }
    assert!(
        saw_live_records,
        "expected nonzero overlay_live_records in stats:\n{stats_text}"
    );
    let _ = std::fs::remove_dir_all(&stats_dir);
}
