#![cfg(unix)]
//! End-to-end exact-display comparisons of Neomacs vs GNU Emacs.

use crate::support;
use neomacs_tui_tests::*;
use std::time::Duration;
use support::*;

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

    assert_pair_exact_display("typed buffer", &gnu, &neo);
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

    assert_pair_exact_display("font-locked elisp", &gnu, &neo);
}
