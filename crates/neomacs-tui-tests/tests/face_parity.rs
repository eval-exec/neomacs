#![cfg(unix)]
//! Attribute-level face parity scenarios guarded by whole-screen
//! exact whole-display comparisons.
//!
//! Comparison discipline: both editors run with TERM=screen-256color
//! and -Q, so any color mismatch on a char-identical cell is a real
//! face-pipeline divergence, not terminal-capability noise. Each
//! assertion retries until the two screens agree or a deadline passes,
//! because face painting can land a frame later than the text.

mod support;
use neomacs_tui_tests::TuiTempDirectory;
use std::fs;
use std::time::Duration;
use support::*;

/// Font-lock over an Emacs Lisp buffer: keyword, function name, doc
/// string, and comment faces are the highest-traffic faces there are.
#[test]
fn font_lock_elisp_faces_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"fl.el\")) \
         (erase-buffer) \
         (insert \";; leading comment\\n\
                  (defun face-parity-probe (x)\\n\
                  \\\"Doc string face.\\\"\\n\
                  (let ((y (+ x 1))) (if (> y 0) 'positive nil)))\\n\") \
         (emacs-lisp-mode) (font-lock-ensure) (goto-char (point-min)) nil)",
    );

    // Wait until GNU has visibly fontified (the defun keyword row exists).
    let fontified = |grid: &[String]| grid.iter().any(|row| row.contains("face-parity-probe"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), fontified);

    assert_pair_exact_display("elisp font-lock", &gnu, &neo);
}

/// The active region highlight after C-SPC + motion.
#[test]
fn region_highlight_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"region\")) \
         (erase-buffer) (insert \"alpha beta gamma\\ndelta epsilon zeta\\n\") \
         (goto-char (point-min)) nil)",
    );
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("alpha beta gamma"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), ready);

    // Activate the mark and extend the region across a line boundary.
    send_both(&mut gnu, &mut neo, "C-SPC");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    send_both(&mut gnu, &mut neo, "C-n");
    send_both(&mut gnu, &mut neo, "C-e");
    read_both(&mut gnu, &mut neo, Duration::from_millis(500));

    assert_pair_exact_display("region highlight", &gnu, &neo);
}

/// Isearch: current-match face plus lazy highlight on other matches.
#[test]
fn isearch_highlight_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"search\")) \
         (erase-buffer) \
         (insert \"needle in a haystack\\nanother needle here\\nlast needle line\\n\") \
         (goto-char (point-min)) nil)",
    );
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("needle in a haystack"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), ready);

    send_both(&mut gnu, &mut neo, "C-s");
    read_both(&mut gnu, &mut neo, Duration::from_millis(300));
    for b in b"needle" {
        send_both_raw(&mut gnu, &mut neo, &[*b]);
    }
    // Wait for the echo area to show the search prompt in both, then let
    // lazy-highlight settle.
    let searching = |grid: &[String]| grid.iter().any(|row| row.contains("I-search: needle"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), searching);

    assert_pair_exact_display("isearch highlight", &gnu, &neo);
}

/// The mode line: on a TTY this is the highest-visibility face of all.
/// Only char-identical cells are compared, so the product-name segment
/// (GNU Emacs vs Neomacs) is excluded automatically.
///
/// Was RED when written (2026-08-05): neomacs painted X11 white
/// (255,255,255) where GNU paints the xterm palette entry (229,229,229)
/// that xterm-register-default-colors installs, because TTY face
/// realization resolved color names through the build-time X11 table and
/// never consulted the tty color table. Fixed by routing TTY-frame face
/// colors through tty-color-desc at face-sync time, mirroring GNU
/// realize_tty_face / map_tty_color (xfaces.c:6620).
#[test]
fn mode_line_face_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    assert_pair_exact_display("mode line", &gnu, &neo);
}

/// The minibuffer prompt face during M-x.
///
/// Was RED when written (2026-08-05), same root cause and fix as
/// mode_line_face_matches_gnu: xterm palette "cyan" (0,205,205) vs X11
/// cyan (0,255,255).
#[test]
fn minibuffer_prompt_face_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    send_both(&mut gnu, &mut neo, "M-x");
    let prompting = |grid: &[String]| grid.iter().any(|row| row.trim_start().starts_with("M-x"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(5), prompting);

    assert_pair_exact_display("minibuffer prompt", &gnu, &neo);

    // Leave the minibuffer so teardown is uniform.
    send_both(&mut gnu, &mut neo, "C-g");
}

/// show-paren-mode: paren-match highlight under the cursor.
#[test]
fn show_paren_highlight_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"parens.el\")) \
         (erase-buffer) (insert \"(defun outer ()\\n  (inner (deep 1 2) 3))\\n\") \
         (emacs-lisp-mode) (show-paren-mode 1) \
         (goto-char (point-min)) nil)",
    );
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("(defun outer ()"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), ready);

    // Sitting on the opening paren highlights its match on line 2.
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    assert_pair_exact_display("show-paren", &gnu, &neo);
}

/// hl-line-mode: the current-line background wash.
#[test]
fn hl_line_highlight_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"lines\")) \
         (erase-buffer) (insert \"first line\\nsecond line\\nthird line\\n\") \
         (goto-char (point-min)) (forward-line 1) (hl-line-mode 1) nil)",
    );
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("second line"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), ready);

    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    assert_pair_exact_display("hl-line", &gnu, &neo);
}

/// Font-lock in C mode: a different major mode exercises different
/// font-lock faces (types, preprocessor) than the elisp scenario.
#[test]
fn font_lock_c_faces_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"probe.c\")) \
         (erase-buffer) \
         (insert \"#include <stdio.h>\\n\
                  /* block comment */\\n\
                  static int counter = 0;\\n\
                  int main(void) {\\n\
                  const char *msg = \\\"hello\\\";\\n\
                  return counter;\\n}\\n\") \
         (c-mode) (font-lock-ensure) (goto-char (point-min)) nil)",
    );
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("static int counter"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(15), ready);

    assert_pair_exact_display("c font-lock", &gnu, &neo);
}

/// Dired: directory-listing faces (dired-directory, dired-header,
/// permissions). Both sessions open the same test-owned directory, making the
/// absolute header path and filesystem metadata deterministic too.
#[test]
fn dired_faces_match_gnu() {
    // Dired includes `..` in its `-al` listing.  Keep that parent private too:
    // a direct child of the shared system temp directory observes unrelated
    // parallel tests changing the parent's link count and size between the GNU
    // and Neomacs snapshots.
    let directory = TuiTempDirectory::new_with_private_parent("neomacs-face-dired-", "listing");
    fs::create_dir(directory.join("sub")).expect("create dired fixture subdirectory");
    fs::write(directory.join("alpha.txt"), "a\n").expect("write alpha fixture");
    fs::write(directory.join("beta.el"), "b\n").expect("write beta fixture");

    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    send_both(&mut gnu, &mut neo, "C-x d");
    let path = format!("{}/", directory.display());
    send_both_raw(&mut gnu, &mut neo, path.as_bytes());
    send_both(&mut gnu, &mut neo, "RET");
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("alpha.txt"))
            && grid.iter().any(|row| row.contains("sub"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), ready);

    read_both(&mut gnu, &mut neo, Duration::from_millis(500));
    assert_pair_exact_display("dired", &gnu, &neo);
}

/// Split windows: the inactive window's mode line uses mode-line-inactive,
/// a different face from the selected window's mode-line.
///
/// Was RED when written (2026-08-05), for a reason unrelated to mode
/// lines: the differing cells were the end-of-line cells of the scratch
/// comment rows. GNU appends a space glyph carrying the NEWLINE's face
/// at every real TTY line end (append_space_for_newline, xdisp.c:24122,
/// called at xdisp.c:26530), so the comment face's foreground rides on
/// the EOL cell; neomacs's buffer-source row path emitted nothing
/// there. Fixed by porting the append to both row paths, including
/// GNU's merged handling with display-fill-column-indicator (the
/// indicator IS the appended glyph when the pen sits at the indicator
/// column) and the pen advance that keeps the :extend fill from
/// overlapping.
#[test]
fn inactive_mode_line_face_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    send_both(&mut gnu, &mut neo, "C-x 2");
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));
    send_both(&mut gnu, &mut neo, "C-x o");
    read_both(&mut gnu, &mut neo, Duration::from_millis(700));

    // Two mode lines now exist; compare every row -- the upper (now
    // inactive) mode line sits mid-screen, so scan the whole text area
    // plus the bottom mode-line row.
    assert_pair_exact_display("split mode lines", &gnu, &neo);
}

/// Prefix strings inherit the buffer-remapped default face, then merge their
/// own text-property face through the same buffer-local remapping table.  GNU
/// does both stages in face_at_pos/face_at_string_position; keeping the
/// explicit named-face merge buffer-aware is observable in the prefix cell.
#[test]
fn line_and_wrap_prefix_named_face_remapping_match_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"prefix-face\")) \
         (erase-buffer) (insert (make-string 220 ?x) \"\\n\") \
         (make-face 'prefix-face-probe) \
         (set-face-attribute 'prefix-face-probe nil \
                             :foreground \"#00ff00\" :background \"#0000ff\") \
         (setq-local face-remapping-alist \
                     '((default (:background \"#112233\") default) \
                       (prefix-face-probe (:foreground \"#ff0000\") \
                                          prefix-face-probe))) \
         (setq-local line-prefix \
                     (propertize \"L\" 'face 'prefix-face-probe)) \
         (setq-local wrap-prefix \
                     (propertize \"W\" 'face 'prefix-face-probe)) \
         (goto-char (point-min)) nil)",
    );
    let ready = |grid: &[String]| {
        grid.iter().any(|row| row.contains("Lxxxx")) && grid.iter().any(|row| row.contains("Wxxxx"))
    };
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), ready);
    assert!(
        ready(&gnu.text_grid()),
        "GNU did not render the prefix probe"
    );
    assert!(
        ready(&neo.text_grid()),
        "Neomacs did not render the prefix probe"
    );

    assert_pair_exact_display("line/wrap-prefix named-face remap", &gnu, &neo);
}

/// GNU `face_at_string_position' merges an overlay string's own text-property
/// face after resolving its anchor base face.  Minibuffer diagnostics use this
/// exact shape at EOB with `minibuffer-prompt'.
#[test]
fn eob_overlay_after_string_named_face_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"overlay-string-face\")) \
         (erase-buffer) (insert \"anchor\") \
         (overlay-put (make-overlay (point-max) (point-max)) 'after-string \
                      (copy-sequence \
                       (propertize \" [diagnostic]\" \
                                   'read-only t 'face 'minibuffer-prompt))) \
         (goto-char (point-min)) nil)",
    );
    let ready = |grid: &[String]| grid.iter().any(|row| row.contains("anchor [diagnostic]"));
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(10), ready);

    assert_pair_exact_display("EOB overlay after-string named face", &gnu, &neo);
}
