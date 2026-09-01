//! Prototype: strict, contract-level grid comparison of Neomacs vs GNU Emacs.
//!
//! Unlike the fuzzy/text-only helpers, this compares the *exact* character grid
//! plus face *identity* (a palette-independent colour-class partition — see
//! `compare_grids_strict`), over the text area, with chrome rows masked and an
//! explicit allow-list for known parity gaps.

use neomacs_tui_tests::*;
use std::time::Duration;

/// A fresh `-Q` buffer with deterministic typed text must render an *identical*
/// character grid in the text area (logical layout), and the same trivial
/// face-class partition (all default — plain text), as GNU Emacs.
///
/// This is the strongest, least-flaky form of strictness: the buffer content
/// is fully deterministic, chrome (mode-line + echo) is masked, and the only
/// thing under test is whether the two editors lay out the same characters at
/// the same cells with the same logical faces.
#[test]
fn strict_text_area_matches_gnu_for_typed_buffer() {
    let mut gnu = TuiSession::gnu_emacs("");
    let mut neo = TuiSession::neomacs("");
    gnu.read(Duration::from_secs(2));
    neo.read(Duration::from_secs(2));

    // Move to a fresh empty buffer (avoids the divergent *scratch* message),
    // type deterministic content, then return to the top of the buffer.
    for s in [&mut gnu, &mut neo] {
        s.send(b"\x18bstrict-grid\r"); // C-x b strict-grid RET
        s.read(Duration::from_millis(800));
        s.send(b"alpha\rbeta gamma\rdelta epsilon zeta\r"); // three lines
        s.read(Duration::from_millis(800));
        s.send(b"\x1b<"); // M-< : beginning of buffer
        s.read(Duration::from_millis(800));
    }

    // Mask the bottom two rows (mode-line + echo area): they legitimately
    // diverge (version string, buffer/position indicators) and are not part of
    // the logical-display contract for this fixture.
    let opts = StrictGridOptions {
        masked_rows: ((ROWS - 2)..ROWS).collect(),
        row_range: Some(0..(ROWS - 2)),
        compare_faces: true,
        // Calibrated below from the first real run, if needed.
        allow: Vec::new(),
    };

    assert_grids_strict("typed buffer text area", gnu.screen(), neo.screen(), &opts);
}

/// A small, deterministic Emacs-Lisp file exercising diverse font-lock faces:
/// comments, keywords (`require`/`defvar`/`defun`/`provide`), docstrings,
/// function/variable names, and ordinary symbols.
const SAMPLE_EL: &str = "\
;;; sample.el --- strict-grid fixture  -*- lexical-binding: t; -*-
(require 'cl-lib)

(defvar my-counter 0
  \"Docstring for the counter.\")

(defun my-add (a b)
  \"Add A and B together.\"
  (+ a b my-counter))

(provide 'sample)
;;; sample.el ends here
";

/// Open a font-locked Emacs-Lisp buffer in both editors and compare the text
/// area strictly. The *character* grid (logical layout) must match GNU exactly;
/// face-class (font-lock) divergences are surfaced and tracked via the explicit
/// allow-list — the precise, shrinking parity backlog.
#[test]
fn strict_font_locked_elisp_matches_gnu() {
    let path = TuiTempFile::new("neomacs-strict-fontlock-", "sample.el", SAMPLE_EL);
    let path_str = path.to_str().expect("utf8 path").to_string();

    // Open the file at startup (`-Q <file>`): emacs-lisp-mode + font-lock.
    let mut gnu = TuiSession::gnu_emacs(&path_str);
    let mut neo = TuiSession::neomacs(&path_str);
    let loaded = |g: &[String]| g.iter().any(|r| r.contains("ends here"));
    gnu.read_until(Duration::from_secs(10), loaded);
    neo.read_until(Duration::from_secs(15), loaded);
    // Let jit-lock fontify the visible region on both.
    gnu.read(Duration::from_secs(2));
    neo.read(Duration::from_secs(2));

    let masked: Vec<u16> = ((ROWS - 2)..ROWS).collect();
    let range = Some(0..(ROWS - 2));

    // Diagnostic pass with NO allow-list, so the full parity backlog is visible
    // in the test output (with the actual fg/bg at each divergent cell).
    let raw = compare_grids_strict(
        gnu.screen(),
        neo.screen(),
        &StrictGridOptions {
            masked_rows: masked.clone(),
            row_range: range.clone(),
            compare_faces: true,
            allow: Vec::new(),
        },
    );
    let n_char = raw
        .iter()
        .filter(|d| d.kind == StrictDiffKind::Char)
        .count();
    eprintln!(
        "font-locked elisp: {n_char} char diffs, {} face-class diffs",
        raw.len() - n_char
    );
    for d in &raw {
        let face = |s: &vt100::Screen| {
            s.cell(d.row, d.col)
                .map(|c| format!("{:?}/{:?}", c.fgcolor(), c.bgcolor()))
        };
        eprintln!(
            "  ({:>2},{:>3}) {:?}: GNU {:?} NEO {:?}",
            d.row,
            d.col,
            d.kind,
            face(gnu.screen()),
            face(neo.screen())
        );
    }
    // The logical-layout contract: the file must render char-for-char like GNU.
    assert_eq!(
        n_char, 0,
        "font-locked elisp text area must match GNU character-for-character"
    );

    // Known font-lock parity gaps on this fixture: GNU and Neomacs assign a
    // different face class to the trailing-whitespace cell past end-of-line on
    // two lines. Tracked explicitly (the shrinking parity backlog) rather than
    // fuzzed away — a NEW divergence (a real font-lock regression) fails below.
    let allow = vec![
        ExpectedDivergence {
            row: 1,
            col: 66,
            reason: "trailing-whitespace face class differs from GNU (require line)",
        },
        ExpectedDivergence {
            row: 12,
            col: 23,
            reason: "trailing-whitespace face class differs from GNU (ends-here comment)",
        },
    ];
    assert_grids_strict(
        "font-locked elisp text area",
        gnu.screen(),
        neo.screen(),
        &StrictGridOptions {
            masked_rows: masked,
            row_range: range,
            compare_faces: true,
            allow,
        },
    );
}
