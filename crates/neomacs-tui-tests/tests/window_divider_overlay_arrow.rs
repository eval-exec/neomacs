//! TUI comparison test: the overlay arrow.
//!
//! Written because the "window-divider / overlay-arrow TUI divergence
//! clusters" had NO tests in this crate at all — a green TUI suite said
//! nothing whatsoever about either. Writing them settled both:
//!
//! * **Window dividers are a GUI-only feature in GNU**, so there is nothing to
//!   compare on a TTY and no test here. `gui_set_right_divider_width`
//!   (src/frame.c) is registered only in the GUI frame-parameter tables
//!   (xfns/pgtkfns/w32fns/haikufns/androidfns), never for terminal frames, so
//!   `window-divider-mode` provably changes nothing in a GNU TTY frame — a
//!   test asserting parity there would pass vacuously forever. The vertical
//!   line between side-by-side TTY windows is the *vertical border*, a
//!   different feature, already exercised elsewhere.
//!
//! * **The overlay arrow is genuinely unimplemented on our TTY**, which the
//!   test below caught immediately (see its own comment).

mod support;
use std::time::Duration;
use support::*;

/// `overlay-arrow-position` marks a line with `overlay-arrow-string`
/// (default "=>"). A terminal frame has no fringe, so GNU renders the string
/// into the leading columns of that line, OVERWRITING them — for a line
/// "beta" GNU displays "=>ta".
///
/// This was red when written — neomacs defined the variables but no display
/// code consumed them, so it drew "beta" where GNU drew "=>ta". Implemented in
/// `display_overlay_arrow.rs`.
#[test]
fn overlay_arrow_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"arrow\")) \
         (erase-buffer) (insert \"alpha\\nbeta\\ngamma\\n\") \
         (goto-char (point-min)) (forward-line 1) \
         (setq overlay-arrow-position (point-marker)) nil)",
    );

    // The arrow is the point of the test: wait for GNU to actually draw it
    // rather than sampling a half-rendered frame.
    let arrow_drawn = |grid: &[String]| grid.iter().any(|row| row.contains("=>"));
    gnu.read_until(Duration::from_secs(10), arrow_drawn);
    neo.read_until(Duration::from_secs(12), arrow_drawn);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !arrow_drawn(&gnu.text_grid()) {
        dump_pair_grids("overlay_arrow", &gnu, &neo);
    }
    assert!(
        arrow_drawn(&gnu.text_grid()),
        "GNU did not draw the overlay arrow, so this test would be vacuous"
    );

    assert_pair_nearly_matches("overlay_arrow", &gnu, &neo, 0);
}

/// An arrow on an EMPTY line. GNU draws it there too — the row's whole content
/// is its newline — and this is the case that exposed the row test's bound:
/// our `end_charpos` is the newline's own position, so `start == end` on an
/// empty line and an exclusive upper bound matched nothing. It also has no
/// glyphs to overwrite, so the arrow has to extend the row.
#[test]
fn overlay_arrow_on_empty_line_matches_gnu() {
    let (mut gnu, mut neo) = boot_pair("");
    wait_for_both(&mut gnu, &mut neo, Duration::from_secs(30), scratch_ready);

    eval_expression(
        &mut gnu,
        &mut neo,
        "(progn (switch-to-buffer (get-buffer-create \"arrow-empty\")) \
         (erase-buffer) (insert \"alpha\\n\\ngamma\\n\") \
         (goto-char (point-min)) (forward-line 1) \
         (setq overlay-arrow-position (point-marker)) nil)",
    );

    let arrow_drawn = |grid: &[String]| grid.iter().any(|row| row.contains("=>"));
    gnu.read_until(Duration::from_secs(10), arrow_drawn);
    neo.read_until(Duration::from_secs(12), arrow_drawn);
    read_both(&mut gnu, &mut neo, Duration::from_secs(1));

    if !arrow_drawn(&gnu.text_grid()) {
        dump_pair_grids("overlay_arrow_empty_line", &gnu, &neo);
    }
    assert!(
        arrow_drawn(&gnu.text_grid()),
        "GNU did not draw the overlay arrow, so this test would be vacuous"
    );

    assert_pair_nearly_matches("overlay_arrow_empty_line", &gnu, &neo, 0);
}
